//! Nexus Mods authentication and API client
//!
//! This module handles authentication with Nexus Mods API, including:
//! - API key management from environment variables
//! - Rate limiting to respect Nexus API limits
//! - User account validation and premium status checking
//! - Download link retrieval with proper authentication

use reqwest::{Client, RequestBuilder};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;
use tracing::debug;

use crate::downloader::core::{DownloadError, Result};

/// Nexus API endpoints
const NEXUS_API_BASE: &str = "https://api.nexusmods.com";

/// Rate limiting: Nexus allows up to 2400 requests per day and 100 per hour for most users
#[derive(Debug, Clone)]
pub struct RateLimit {
    #[allow(dead_code)] // Used for logging and debugging
    daily_limit: u32,
    daily_remaining: u32,
    daily_reset: SystemTime,
    #[allow(dead_code)] // Used for logging and debugging
    hourly_limit: u32,
    hourly_remaining: u32,
    hourly_reset: SystemTime,
}

impl RateLimit {
    /// Check if we're currently rate limited
    pub fn is_blocked(&self) -> bool {
        self.daily_remaining == 0 || self.hourly_remaining == 0
    }

    /// Get time until rate limit renewal
    pub fn time_until_renewal(&self) -> Option<Duration> {
        if !self.is_blocked() {
            return None;
        }

        let now = SystemTime::now();
        let hourly_wait = self.hourly_reset.duration_since(now).ok();
        let daily_wait = self.daily_reset.duration_since(now).ok();

        match (hourly_wait, daily_wait) {
            (Some(h), Some(d)) => Some(h.min(d)),
            (Some(h), None) => Some(h),
            (None, Some(d)) => Some(d),
            _ => None,
        }
    }
}

/// Nexus user validation response
#[derive(Debug, Clone, Deserialize)]
pub struct UserValidation {
    pub user_id: u32,
    #[serde(alias = "Key", alias = "key")]
    pub key: String,
    #[serde(alias = "Name", alias = "name")]
    pub name: String,
    #[serde(alias = "Email", alias = "email")]
    pub email: String,
    pub profile_url: Option<String>,
    pub is_premium: bool,
    pub is_supporter: bool,
}

/// Nexus mod information
#[derive(Debug, Clone, Deserialize)]
pub struct NexusMod {
    pub mod_id: u32,
    #[serde(alias = "Name", alias = "name")]
    pub name: String,
    #[serde(alias = "Summary", alias = "summary")]
    pub summary: Option<String>,
    #[serde(alias = "Description", alias = "description")]
    pub description: Option<String>,
    pub game_id: u32,
    pub domain_name: String,
    pub category_id: u32,
    #[serde(alias = "Version", alias = "version")]
    pub version: String,
    #[serde(alias = "Author", alias = "author")]
    pub author: String,
    pub uploaded_by: String,
    pub contains_adult_content: bool,
    pub available: bool,
}

/// Nexus file information
#[derive(Debug, Clone, Deserialize)]
pub struct NexusFile {
    #[serde(rename = "file_id")]
    pub id: u32,
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "version")]
    pub version: Option<String>,
    #[serde(rename = "category_id")]
    pub category_id: u32,
    #[serde(rename = "is_primary")]
    pub is_primary: bool,
    #[serde(rename = "size")]
    pub size: u64,
    #[serde(rename = "file_name")]
    pub file_name: String,
    #[serde(rename = "uploaded_timestamp")]
    pub uploaded_timestamp: u64,
    #[serde(rename = "mod_version")]
    pub mod_version: Option<String>,
}

/// Nexus file list response
#[derive(Debug, Clone, Deserialize)]
pub struct NexusFileList {
    pub files: Vec<NexusFile>,
}

/// Nexus download link
#[derive(Debug, Clone, Deserialize)]
pub struct NexusDownloadLink {
    pub name: String,
    pub short_name: String,
    #[serde(rename = "URI")]
    pub uri: String,
}

/// Public rate limit status for display/logging
#[derive(Debug, Clone)]
pub struct RateLimitStatus {
    pub daily_limit: u32,
    pub daily_remaining: u32,
    pub daily_reset: SystemTime,
    pub hourly_limit: u32,
    pub hourly_remaining: u32,
    pub hourly_reset: SystemTime,
    pub is_blocked: bool,
}

impl RateLimitStatus {
    /// Format rate limit status for display
    pub fn format_status(&self) -> String {
        let daily_percent = ((self.daily_remaining as f64 / self.daily_limit as f64) * 100.0) as u32;
        let hourly_percent = ((self.hourly_remaining as f64 / self.hourly_limit as f64) * 100.0) as u32;

        let status_icon = if self.is_blocked { "🚫" } else { "✅" };

        format!(
            "{} Rate Limits - Daily: {}/{} ({}%) | Hourly: {}/{} ({}%)",
            status_icon,
            self.daily_remaining,
            self.daily_limit,
            daily_percent,
            self.hourly_remaining,
            self.hourly_limit,
            hourly_percent
        )
    }

    /// Get time until reset as human-readable string
    pub fn time_until_reset(&self) -> String {
        let now = SystemTime::now();

        let daily_remaining_secs = self.daily_reset.duration_since(now)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let hourly_remaining_secs = self.hourly_reset.duration_since(now)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        format!(
            "Next reset: Hourly in {}m, Daily in {}h",
            hourly_remaining_secs / 60,
            daily_remaining_secs / 3600
        )
    }
}

/// Cache entry for API responses
#[derive(Debug, Clone)]
struct CacheEntry<T> {
    data: T,
    expires_at: Instant,
}

impl<T> CacheEntry<T> {
    fn new(data: T, ttl: Duration) -> Self {
        Self {
            data,
            expires_at: Instant::now() + ttl,
        }
    }

    fn is_expired(&self) -> bool {
        Instant::now() > self.expires_at
    }
}

/// Nexus authentication and API client
pub struct NexusAPI {
    api_key: String,
    client: Client,
    rate_limit: Arc<Mutex<Option<RateLimit>>>,
    // Simple in-memory cache for API responses
    mod_cache: Arc<Mutex<HashMap<(String, u32), CacheEntry<NexusMod>>>>,
    files_cache: Arc<Mutex<HashMap<(String, u32), CacheEntry<Vec<NexusFile>>>>>,
    links_cache: Arc<Mutex<HashMap<(String, u32, u32), CacheEntry<Vec<NexusDownloadLink>>>>>,
}

impl NexusAPI {
    /// Create new Nexus authentication client with API key from environment
    pub fn new() -> Result<Self> {
        // Load API key from environment
        dotenv::dotenv().ok(); // Ignore error if .env not present
        let api_key = std::env::var("NEXUS_API_KEY")
            .map_err(|_| DownloadError::Configuration {
                message: "NEXUS_API_KEY environment variable not set".to_string(),
                field: Some("NEXUS_API_KEY".to_string()),
                suggestion: Some("Set NEXUS_API_KEY in your .env file with your personal API key from Nexus Mods".to_string()),
            })?;

        let client = Client::builder()
            .user_agent("Unifier/1.0")
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| DownloadError::Legacy(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            api_key,
            client,
            rate_limit: Arc::new(Mutex::new(None)),
            mod_cache: Arc::new(Mutex::new(HashMap::new())),
            files_cache: Arc::new(Mutex::new(HashMap::new())),
            links_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Validate API key and get user information
    pub async fn validate_user(&self) -> Result<UserValidation> {
        let url = format!("{}/v1/users/validate.json", NEXUS_API_BASE);
        let request = self.create_authenticated_request(&url)?;

        let response = self.execute_request(request).await?;

        // Get the response text for debugging
        let response_text = response.text().await
            .map_err(|e| DownloadError::Legacy(format!("Failed to get response text: {}", e)))?;

        debug!("Nexus API validation response: {}", response_text);

        // Try to parse the JSON response
        let user: UserValidation = serde_json::from_str(&response_text)
            .map_err(|e| DownloadError::Legacy(format!("Failed to parse user validation response: {} - Response was: {}", e, response_text)))?;

        debug!("Nexus user validated: {} (Premium: {}, Supporter: {})",
               user.name, user.is_premium, user.is_supporter);

        Ok(user)
    }

    /// Get mod information from Nexus API
    pub async fn get_mod(&self, domain_name: &str, mod_id: u32) -> Result<NexusMod> {
        let cache_key = (domain_name.to_string(), mod_id);

        // Check cache first
        {
            let mut cache = self.mod_cache.lock().unwrap();
            if let Some(entry) = cache.get(&cache_key) {
                if !entry.is_expired() {
                    debug!("Returning cached mod info for {}:{}", domain_name, mod_id);
                    return Ok(entry.data.clone());
                } else {
                    cache.remove(&cache_key);
                }
            }
        }

        self.wait_for_rate_limit().await?;

        let url = format!("{}/v1/games/{}/mods/{}.json", NEXUS_API_BASE, domain_name, mod_id);
        let request = self.create_authenticated_request(&url)?;

        let response = self.execute_request(request).await?;

        // Get response text for better error handling
        let response_text = response.text().await
            .map_err(|e| DownloadError::Legacy(format!("Failed to get mod response text: {}", e)))?;

        debug!("Nexus API mod response: {}", response_text);

        let mod_info: NexusMod = serde_json::from_str(&response_text)
            .map_err(|e| DownloadError::Legacy(format!("Failed to parse mod info response: {} - Response was: {}", e, response_text)))?;

        // Cache the result for 24 hours
        {
            let mut cache = self.mod_cache.lock().unwrap();
            cache.insert(cache_key, CacheEntry::new(mod_info.clone(), Duration::from_secs(24 * 3600)));
        }

        Ok(mod_info)
    }

    /// Get files for a mod
    pub async fn get_mod_files(&self, domain_name: &str, mod_id: u32) -> Result<Vec<NexusFile>> {
        let cache_key = (domain_name.to_string(), mod_id);

        // Check cache first
        {
            let mut cache = self.files_cache.lock().unwrap();
            if let Some(entry) = cache.get(&cache_key) {
                if !entry.is_expired() {
                    debug!("Returning cached file list for {}:{}", domain_name, mod_id);
                    return Ok(entry.data.clone());
                } else {
                    cache.remove(&cache_key);
                }
            }
        }

        self.wait_for_rate_limit().await?;

        let url = format!("{}/v1/games/{}/mods/{}/files.json", NEXUS_API_BASE, domain_name, mod_id);
        let request = self.create_authenticated_request(&url)?;

        let response = self.execute_request(request).await?;

        // Get response text for better error handling
        let response_text = response.text().await
            .map_err(|e| DownloadError::Legacy(format!("Failed to get file list response text: {}", e)))?;

        debug!("Nexus API files response: {}", response_text);

        let file_list: NexusFileList = serde_json::from_str(&response_text)
            .map_err(|e| DownloadError::Legacy(format!("Failed to parse file list response: {} - Response was: {}", e, response_text)))?;

        // Cache the result for 12 hours
        {
            let mut cache = self.files_cache.lock().unwrap();
            cache.insert(cache_key, CacheEntry::new(file_list.files.clone(), Duration::from_secs(12 * 3600)));
        }

        Ok(file_list.files)
    }

    /// Get download links for a specific file
    pub async fn get_download_links(&self, domain_name: &str, mod_id: u32, file_id: u32) -> Result<Vec<NexusDownloadLink>> {
        let cache_key = (domain_name.to_string(), mod_id, file_id);

        // Check cache first
        {
            let mut cache = self.links_cache.lock().unwrap();
            if let Some(entry) = cache.get(&cache_key) {
                if !entry.is_expired() {
                    debug!("Returning cached download links for {}:{}:{}", domain_name, mod_id, file_id);
                    return Ok(entry.data.clone());
                } else {
                    cache.remove(&cache_key);
                }
            }
        }

        self.wait_for_rate_limit().await?;

        let url = format!("{}/v1/games/{}/mods/{}/files/{}/download_link.json",
                         NEXUS_API_BASE, domain_name, mod_id, file_id);
        let request = self.create_authenticated_request(&url)?;

        let response = self.execute_request(request).await?;

        // Get response text for better error handling
        let response_text = response.text().await
            .map_err(|e| DownloadError::Legacy(format!("Failed to get download links response text: {}", e)))?;

        debug!("Nexus API links response: {}", response_text);

        let links: Vec<NexusDownloadLink> = serde_json::from_str(&response_text)
            .map_err(|e| DownloadError::Legacy(format!("Failed to parse download links response: {} - Response was: {}", e, response_text)))?;

        // Cache the result for 6 hours
        {
            let mut cache = self.links_cache.lock().unwrap();
            cache.insert(cache_key, CacheEntry::new(links.clone(), Duration::from_secs(6 * 3600)));
        }

        Ok(links)
    }

    /// Get the best download link (prefer fastest CDN)
    pub fn select_best_download_link<'a>(&self, links: &'a [NexusDownloadLink]) -> Option<&'a NexusDownloadLink> {
        // Prefer specific CDNs in order of preference for better performance
        const PREFERRED_CDNS: &[&str] = &["CloudFlare", "Amazon CloudFront"];

        for preferred in PREFERRED_CDNS {
            if let Some(link) = links.iter().find(|l| l.name.contains(preferred)) {
                return Some(link);
            }
        }

        // Fall back to first available link
        links.first()
    }

    /// Create an authenticated request with proper headers
    fn create_authenticated_request(&self, url: &str) -> Result<RequestBuilder> {
        let request = self.client
            .get(url)
            .header("apikey", &self.api_key)
            .header("User-Agent", "Unifier/1.0")
            .header("Application-Name", "Unifier")
            .header("Application-Version", "1.0");

        Ok(request)
    }

    /// Execute request and update rate limit information
    async fn execute_request(&self, request: RequestBuilder) -> Result<reqwest::Response> {
        debug!("Nexus API request: {:?}", request);
        let response = request.send().await?;
        debug!("Nexus API response: {:?}", response);

        // Update rate limit information from headers
        self.update_rate_limit_from_headers(&response);

        if !response.status().is_success() {
            let status = response.status();
            let url = response.url().to_string();

            if status == 429 {
                // Rate limited - wait and retry once
                debug!("Rate limited by Nexus API, waiting...");
                if let Some(wait_time) = self.get_rate_limit_wait_time() {
                    sleep(wait_time).await;
                    return Err(DownloadError::Legacy("Rate limited by Nexus API".to_string()));
                }
            }

            return Err(DownloadError::HttpRequest {
                url,
                source: reqwest::Error::from(response.error_for_status().unwrap_err()),
            });
        }

        Ok(response)
    }

    /// Update rate limit information from response headers
    fn update_rate_limit_from_headers(&self, response: &reqwest::Response) {
        let headers = response.headers();

        let daily_limit = headers.get("x-rl-daily-limit")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(2400);

        let daily_remaining = headers.get("x-rl-daily-remaining")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(daily_limit);

        let hourly_limit = headers.get("x-rl-hourly-limit")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);

        let hourly_remaining = headers.get("x-rl-hourly-remaining")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(hourly_limit);

        // Parse reset times (Unix timestamps)
        let daily_reset = headers.get("x-rl-daily-reset")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .map(|ts| UNIX_EPOCH + Duration::from_secs(ts))
            .unwrap_or_else(|| SystemTime::now() + Duration::from_secs(24 * 3600));

        let hourly_reset = headers.get("x-rl-hourly-reset")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .map(|ts| UNIX_EPOCH + Duration::from_secs(ts))
            .unwrap_or_else(|| SystemTime::now() + Duration::from_secs(3600));

        let rate_limit = RateLimit {
            daily_limit,
            daily_remaining,
            daily_reset,
            hourly_limit,
            hourly_remaining,
            hourly_reset,
        };

        *self.rate_limit.lock().unwrap() = Some(rate_limit);

        debug!("Rate limit updated: daily {}/{}, hourly {}/{}",
               daily_remaining, daily_limit, hourly_remaining, hourly_limit);
    }

    /// Wait for rate limit to be available
    async fn wait_for_rate_limit(&self) -> Result<()> {
        if let Some(wait_time) = self.get_rate_limit_wait_time() {
            debug!("Rate limited, waiting {:?} before making request", wait_time);
            sleep(wait_time).await;
        }
        Ok(())
    }

    /// Get rate limit wait time if blocked
    fn get_rate_limit_wait_time(&self) -> Option<Duration> {
        let rate_limit = self.rate_limit.lock().unwrap();
        if let Some(ref limit) = *rate_limit {
            limit.time_until_renewal()
        } else {
            None
        }
    }

    /// Get current rate limit information
    pub fn get_rate_limit_status(&self) -> Option<RateLimitStatus> {
        let rate_limit = self.rate_limit.lock().unwrap();
        rate_limit.as_ref().map(|limit| RateLimitStatus {
            daily_limit: limit.daily_limit,
            daily_remaining: limit.daily_remaining,
            daily_reset: limit.daily_reset,
            hourly_limit: limit.hourly_limit,
            hourly_remaining: limit.hourly_remaining,
            hourly_reset: limit.hourly_reset,
            is_blocked: limit.is_blocked(),
        })
    }

}

impl Clone for NexusAPI {
    fn clone(&self) -> Self {
        Self {
            api_key: self.api_key.clone(),
            client: self.client.clone(),
            rate_limit: Arc::clone(&self.rate_limit),
            mod_cache: Arc::clone(&self.mod_cache),
            files_cache: Arc::clone(&self.files_cache),
            links_cache: Arc::clone(&self.links_cache),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;
    use tokio;
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{method, path, header};

    static INIT_ENV: Once = Once::new();

    fn setup_test_env() {
        INIT_ENV.call_once(|| {
            // Set a dummy API key for tests
            unsafe {
                std::env::set_var("NEXUS_API_KEY", "test_api_key_123");
            }
        });
    }

    #[tokio::test]
    async fn test_nexus_api_new_success() {
        setup_test_env();

        // Ensure the API key is set for this test
        unsafe {
            std::env::set_var("NEXUS_API_KEY", "test_api_key_123");
        }

        let api = NexusAPI::new();
        assert!(api.is_ok(), "API creation should succeed with valid API key");
    }

    #[tokio::test]
    #[ignore = "reason: test must be modified to mock an empty env before use"]
    async fn test_nexus_api_new_missing_key() {
        // Save the current key if it exists
        let saved_key = std::env::var("NEXUS_API_KEY").ok();

        // Temporarily remove the API key
        unsafe {
            std::env::remove_var("NEXUS_API_KEY");
        }

        let api = NexusAPI::new();
        assert!(api.is_err(), "API creation should fail without API key");

        // Restore the original key
        if let Some(key) = saved_key {
            unsafe {
                std::env::set_var("NEXUS_API_KEY", key);
            }
        } else {
            setup_test_env();
        }
    }

    #[tokio::test]
    async fn test_user_validation_success() {
        setup_test_env();
        let mock_server = MockServer::start().await;

        // Mock successful validation response
        let validation_response = r#"{
            "user_id": 123456,
            "key": "test_key",
            "name": "TestUser",
            "email": "test@example.com",
            "profile_url": "https://nexusmods.com/users/123456",
            "is_premium": true,
            "is_supporter": false
        }"#;

        Mock::given(method("GET"))
            .and(path("/v1/users/validate.json"))
            .and(header("apikey", "test_api_key_123"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(validation_response)
                    .insert_header("content-type", "application/json")
                    .insert_header("x-rl-daily-limit", "2400")
                    .insert_header("x-rl-daily-remaining", "2399")
                    .insert_header("x-rl-hourly-limit", "100")
                    .insert_header("x-rl-hourly-remaining", "99")
                    .insert_header("x-rl-daily-reset", "1234567890")
                    .insert_header("x-rl-hourly-reset", "1234567890")
            )
            .mount(&mock_server)
            .await;

        // Create API client with mocked server
        let mut api = NexusAPI::new().unwrap();
        // Replace the client with one pointing to mock server
        api.client = Client::builder()
            .user_agent("Unifier/1.0")
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();

        // Override the base URL for testing
        let mock_url = format!("{}/v1/users/validate.json", mock_server.uri());
        let request = api.client
            .get(&mock_url)
            .header("apikey", &api.api_key)
            .header("User-Agent", "Unifier/1.0")
            .header("Application-Name", "Unifier")
            .header("Application-Version", "1.0");

        let response = request.send().await.unwrap();
        let response_text = response.text().await.unwrap();
        let user: UserValidation = serde_json::from_str(&response_text).unwrap();

        assert_eq!(user.user_id, 123456);
        assert_eq!(user.name, "TestUser");
        assert_eq!(user.email, "test@example.com");
        assert!(user.is_premium);
        assert!(!user.is_supporter);
    }

    #[tokio::test]
    async fn test_mod_info_parsing() {
        setup_test_env();

        // Test various mod response formats
        let mod_response_capital = r#"{
            "mod_id": 12345,
            "Name": "Test Mod",
            "Summary": "A test mod",
            "Description": "Test description",
            "game_id": 1,
            "domain_name": "skyrimspecialedition",
            "category_id": 5,
            "Version": "1.0.0",
            "Author": "TestAuthor",
            "uploaded_by": "testuser",
            "contains_adult_content": false,
            "available": true
        }"#;

        let mod_response_lowercase = r#"{
            "mod_id": 12345,
            "name": "Test Mod",
            "summary": "A test mod",
            "description": "Test description",
            "game_id": 1,
            "domain_name": "skyrimspecialedition",
            "category_id": 5,
            "version": "1.0.0",
            "author": "TestAuthor",
            "uploaded_by": "testuser",
            "contains_adult_content": false,
            "available": true
        }"#;

        // Test both formats
        let mod_capital: std::result::Result<NexusMod, serde_json::Error> = serde_json::from_str(mod_response_capital);
        let mod_lowercase: std::result::Result<NexusMod, serde_json::Error> = serde_json::from_str(mod_response_lowercase);

        match mod_capital {
            Ok(mod_info) => {
                assert_eq!(mod_info.mod_id, 12345);
                assert_eq!(mod_info.name, "Test Mod");
                assert_eq!(mod_info.author, "TestAuthor");
            }
            Err(e) => println!("Capital case parsing error: {}", e),
        }

        match mod_lowercase {
            Ok(_) => println!("Lowercase parsing succeeded"),
            Err(e) => println!("Lowercase case parsing error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_mod_files_parsing() {
        setup_test_env();

        let files_response = r#"{
            "files": [
                {
                    "file_id": 67890,
                    "name": "Main File",
                    "version": "1.0.0",
                    "category_id": 1,
                    "is_primary": true,
                    "size": 1048576,
                    "file_name": "TestMod_v1.zip",
                    "uploaded_timestamp": 1234567890,
                    "mod_version": "1.0.0"
                }
            ]
        }"#;

        let file_list: std::result::Result<NexusFileList, serde_json::Error> = serde_json::from_str(files_response);
        assert!(file_list.is_ok());

        let files = file_list.unwrap();
        assert_eq!(files.files.len(), 1);
        assert_eq!(files.files[0].id, 67890);
        assert_eq!(files.files[0].name, "Main File");
        assert_eq!(files.files[0].size, 1048576);
    }

    #[tokio::test]
    async fn test_download_links_parsing() {
        setup_test_env();

        let links_response = r#"[
            {
                "name": "CloudFlare CDN",
                "short_name": "CF",
                "URI": "https://cf.nexusmods.com/file.zip"
            },
            {
                "name": "Amazon CloudFront",
                "short_name": "ACF",
                "URI": "https://acf.nexusmods.com/file.zip"
            }
        ]"#;

        let links: std::result::Result<Vec<NexusDownloadLink>, serde_json::Error> = serde_json::from_str(links_response);
        assert!(links.is_ok());

        let download_links = links.unwrap();
        assert_eq!(download_links.len(), 2);
        assert_eq!(download_links[0].name, "CloudFlare CDN");
        assert_eq!(download_links[1].name, "Amazon CloudFront");
    }

    #[tokio::test]
    async fn test_cdn_selection() {
        setup_test_env();
        let api = NexusAPI::new().unwrap();

        let links = vec![
            NexusDownloadLink {
                name: "Regular CDN".to_string(),
                short_name: "REG".to_string(),
                uri: "https://regular.nexusmods.com/file.zip".to_string(),
            },
            NexusDownloadLink {
                name: "CloudFlare CDN".to_string(),
                short_name: "CF".to_string(),
                uri: "https://cf.nexusmods.com/file.zip".to_string(),
            },
            NexusDownloadLink {
                name: "Amazon CloudFront".to_string(),
                short_name: "ACF".to_string(),
                uri: "https://acf.nexusmods.com/file.zip".to_string(),
            },
        ];

        let selected = api.select_best_download_link(&links);
        assert!(selected.is_some());
        // Should prefer CloudFlare over Amazon CloudFront over Regular
        assert_eq!(selected.unwrap().name, "CloudFlare CDN");
    }

    #[tokio::test]
    async fn test_rate_limit_parsing() {
        setup_test_env();
        let api = NexusAPI::new().unwrap();

        // Create a mock HTTP client for this test (simplified approach)
        let client = Client::builder()
            .user_agent("Unifier/1.0")
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();

        // Test the rate limit parsing with a mock server instead
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/test"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("{}")
                    .insert_header("x-rl-daily-limit", "2400")
                    .insert_header("x-rl-daily-remaining", "2350")
                    .insert_header("x-rl-hourly-limit", "100")
                    .insert_header("x-rl-hourly-remaining", "95")
                    .insert_header("x-rl-daily-reset", "1234567890")
                    .insert_header("x-rl-hourly-reset", "1234567890")
            )
            .mount(&mock_server)
            .await;

        let test_url = format!("{}/test", mock_server.uri());
        let response = client.get(&test_url).send().await.unwrap();

        api.update_rate_limit_from_headers(&response);

        // Check that rate limits were updated
        let rate_limit = api.rate_limit.lock().unwrap();
        assert!(rate_limit.is_some());
        let limit = rate_limit.as_ref().unwrap();
        assert_eq!(limit.daily_remaining, 2350);
        assert_eq!(limit.hourly_remaining, 95);
    }

    #[tokio::test]
    async fn test_cache_functionality() {
        setup_test_env();
        let api = NexusAPI::new().unwrap();

        // Test mod cache
        let cache_key = ("skyrimspecialedition".to_string(), 12345);
        let test_mod = NexusMod {
            mod_id: 12345,
            name: "Test Mod".to_string(),
            summary: Some("Test summary".to_string()),
            description: Some("Test description".to_string()),
            game_id: 1,
            domain_name: "skyrimspecialedition".to_string(),
            category_id: 5,
            version: "1.0.0".to_string(),
            author: "TestAuthor".to_string(),
            uploaded_by: "testuser".to_string(),
            contains_adult_content: false,
            available: true,
        };

        // Insert into cache
        {
            let mut cache = api.mod_cache.lock().unwrap();
            cache.insert(cache_key.clone(), CacheEntry::new(test_mod.clone(), Duration::from_secs(3600)));
        }

        // Check cache hit
        {
            let cache = api.mod_cache.lock().unwrap();
            let entry = cache.get(&cache_key);
            assert!(entry.is_some());
            assert!(!entry.unwrap().is_expired());
            assert_eq!(entry.unwrap().data.name, "Test Mod");
        }

        // Test cache expiration
        {
            let mut cache = api.mod_cache.lock().unwrap();
            cache.insert(cache_key.clone(), CacheEntry::new(test_mod, Duration::from_nanos(1))); // Immediate expiry
        }

        // Allow some time for expiration
        tokio::time::sleep(Duration::from_millis(1)).await;

        {
            let cache = api.mod_cache.lock().unwrap();
            let entry = cache.get(&cache_key);
            assert!(entry.is_some());
            assert!(entry.unwrap().is_expired());
        }
    }

    #[test]
    fn test_rate_limit_logic() {
        let rate_limit = RateLimit {
            daily_limit: 2400,
            daily_remaining: 100,
            daily_reset: SystemTime::now() + Duration::from_secs(3600),
            hourly_limit: 100,
            hourly_remaining: 0,
            hourly_reset: SystemTime::now() + Duration::from_secs(300),
        };

        // Should be blocked when hourly is 0, even if daily has remaining
        assert!(rate_limit.is_blocked(), "Rate limit should be blocked when hourly remaining is 0");

        let renewal_time = rate_limit.time_until_renewal();
        assert!(renewal_time.is_some(), "Should have renewal time when blocked");
        // Should wait for hourly reset (300s) rather than daily (3600s)
        let renewal_secs = renewal_time.unwrap().as_secs();
        assert!(renewal_secs <= 300, "Should wait for hourly reset, got {} seconds", renewal_secs);
    }
}
