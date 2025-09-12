//! Structured download source types
//!
//! This module defines type-safe representations of different download sources
//! that can be parsed from Wabbajack modlist JSON. Each source type contains
//! the specific data needed for that download method.

// use serde::{Deserialize, Serialize}; // Currently unused, but may be needed later
use std::collections::HashMap;
use async_trait::async_trait;
use crate::downloader::core::{Downloadable, DownloadRequest, DownloadResult, ProgressCallback, Result, DownloadError, ValidationType, ProgressEvent};
use futures::StreamExt;
use reqwest::Client;
use std::path::Path;
use tokio::fs;
use tokio::io::{AsyncWriteExt, AsyncSeekExt};
use tracing::{debug, warn};
use serde::Deserialize;
use flate2::read::GzDecoder;
use std::io::Read;
use base64::{Engine as _, engine::general_purpose};

/// Structured representation of a download source
///
/// This enum represents the different ways a file can be obtained,
/// providing type safety and avoiding the need to serialize/parse URLs.
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadSource {
    /// Direct HTTP/HTTPS download
    Http(HttpSource),
    /// Download from Nexus Mods via API
    Nexus(NexusSource),
    /// Copy from local game installation
    GameFile(GameFileSource),
    /// Manual download (user must provide file)
    Manual(ManualSource),
    /// Archive extraction from another archive
    Archive(ArchiveSource),
    /// Download from Wabbajack CDN
    WabbajackCDN(WabbajackCDNSource),
}

/// HTTP download source
#[derive(Debug, Clone, PartialEq)]
pub struct HttpSource {
    /// Primary download URL
    pub url: String,
    /// Optional HTTP headers to send with request
    pub headers: HashMap<String, String>,
    /// Optional fallback URLs if primary fails
    pub mirror_urls: Vec<String>,
}

/// Nexus Mods download source
#[derive(Debug, Clone, PartialEq)]
pub struct NexusSource {
    /// Nexus mod ID
    pub mod_id: u32,
    /// Nexus file ID
    pub file_id: u32,
    /// Game name (e.g., "SkyrimSpecialEdition")
    pub game_name: String,
    /// Mod name for display
    pub mod_name: String,
    /// Mod author
    pub author: String,
    /// Mod version
    pub version: String,
    /// Mod description
    pub description: String,
    /// Whether the mod is marked NSFW
    pub is_nsfw: bool,
}

/// Game file copy source
#[derive(Debug, Clone, PartialEq)]
pub struct GameFileSource {
    /// Game identifier (e.g., "SkyrimSpecialEdition")
    pub game: String,
    /// Relative path to file within game installation
    pub file_path: String,
    /// Expected game version
    pub game_version: String,
}

/// Manual download source (user must provide)
#[derive(Debug, Clone, PartialEq)]
pub struct ManualSource {
    /// Instructions for the user
    pub instructions: String,
    /// Optional URL where user can find the file
    pub url: Option<String>,
}

/// Archive extraction source
#[derive(Debug, Clone, PartialEq)]
pub struct ArchiveSource {
    /// Hash of the source archive
    pub archive_hash: String,
    /// Path within the archive to extract
    pub inner_path: String,
}

/// Wabbajack CDN download source
#[derive(Debug, Clone, PartialEq)]
pub struct WabbajackCDNSource {
    /// CDN URL for the file
    pub url: String,
}

impl HttpSource {
    pub fn new<S: Into<String>>(url: S) -> Self {
        Self {
            url: url.into(),
            headers: HashMap::new(),
            mirror_urls: Vec::new(),
        }
    }

    pub fn with_header<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    pub fn with_mirror<S: Into<String>>(mut self, mirror_url: S) -> Self {
        self.mirror_urls.push(mirror_url.into());
        self
    }
}

impl NexusSource {
    pub fn new(mod_id: u32, file_id: u32, game_name: String) -> Self {
        Self {
            mod_id,
            file_id,
            game_name,
            mod_name: String::new(),
            author: String::new(),
            version: String::new(),
            description: String::new(),
            is_nsfw: false,
        }
    }

    pub fn with_metadata<S: Into<String>>(
        mut self,
        name: S,
        author: S,
        version: S,
        description: S,
        is_nsfw: bool,
    ) -> Self {
        self.mod_name = name.into();
        self.author = author.into();
        self.version = version.into();
        self.description = description.into();
        self.is_nsfw = is_nsfw;
        self
    }
}

impl GameFileSource {
    pub fn new<S1: Into<String>, S2: Into<String>, S3: Into<String>>(
        game: S1,
        file_path: S2,
        game_version: S3,
    ) -> Self {
        Self {
            game: game.into(),
            file_path: file_path.into(),
            game_version: game_version.into(),
        }
    }
}

impl WabbajackCDNSource {
    pub fn new<S: Into<String>>(url: S) -> Self {
        Self {
            url: url.into(),
        }
    }
}

impl DownloadSource {
    /// Get a human-readable description of this download source
    pub fn description(&self) -> String {
        match self {
            DownloadSource::Http(http) => format!("HTTP download from {}", http.url),
            DownloadSource::Nexus(nexus) => {
                format!("Nexus download: {} by {} (mod {}, file {})",
                    nexus.mod_name, nexus.author, nexus.mod_id, nexus.file_id)
            },
            DownloadSource::GameFile(game) => {
                format!("Game file: {} from {}", game.file_path, game.game)
            },
            DownloadSource::Manual(manual) => format!("Manual download: {}", manual.instructions),
            DownloadSource::Archive(archive) => {
                format!("Extract {} from archive {}", archive.inner_path, archive.archive_hash)
            },
            DownloadSource::WabbajackCDN(wabbajack_cdn) => {
                format!("WabbajackCDN download from {}", wabbajack_cdn.url)
            },
        }
    }

    /// Check if this source requires user interaction
    pub fn requires_user_interaction(&self) -> bool {
        matches!(self, DownloadSource::Manual(_))
    }

    /// Check if this source requires external dependencies (API keys, game installations, etc.)
    pub fn requires_external_dependencies(&self) -> bool {
        matches!(self, DownloadSource::Nexus(_) | DownloadSource::GameFile(_))
    }
}

// Implement Downloadable trait for each source type
#[async_trait]
impl Downloadable for HttpSource {
    async fn download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<ProgressCallback>,
        config: &crate::downloader::config::DownloadConfig,
    ) -> Result<DownloadResult> {
        let filename = request.get_filename()?;
        let dest_path = request.destination.join(&filename);

        debug!("HTTP downloading {} to {}", self.url, dest_path.display());

        // Check existing file first
        if let Some(result) = self.check_existing_file(&dest_path, &request.validation, progress_callback.clone()).await? {
            return Ok(result);
        }

        let size = self.download_helper(&self.url, &dest_path, progress_callback.clone(), request.expected_size, config).await?;

        // Validate the downloaded file (only if validation is specified)
        if !request.validation.is_empty() {
            debug!("Validating downloaded file");
            if !request.validation.validate_file(&dest_path, progress_callback).await? {
                fs::remove_file(&dest_path).await?;
                return Err(DownloadError::ValidationFailed {
                    file: dest_path.clone(),
                    validation_type: ValidationType::Size,
                    expected: "valid file".to_string(),
                    actual: "invalid file".to_string(),
                    suggestion: "Check file integrity or download again".to_string(),
                });
            }
            debug!("File validation passed");
        }

        Ok(DownloadResult::Downloaded { size })
    }

    fn supports_resume(&self) -> bool {
        true
    }

    fn description(&self) -> String {
        format!("HTTP download from {}", self.url)
    }
}

impl HttpSource {
    /// Get file size from server
    async fn get_file_size(&self, config: &crate::downloader::config::DownloadConfig) -> Result<Option<u64>> {
        debug!("Getting file size for: {}", self.url);
        let client = self.create_client(config)?;
        let response = client.head(&self.url).send().await?;
        response.error_for_status_ref()?;

        Ok(response.content_length())
    }

    /// Create HTTP client with configuration
    fn create_client(&self, config: &crate::downloader::config::DownloadConfig) -> Result<Client> {
        let client = Client::builder()
            .timeout(config.timeout)
            .user_agent(&config.user_agent)
            .build()
            .map_err(|e| DownloadError::Legacy(format!("Failed to create HTTP client: {}", e)))?;

        Ok(client)
    }

    /// Download file with resume support and progress tracking
    async fn download_file(
        &self,
        url: &str,
        dest_path: &Path,
        progress_callback: Option<ProgressCallback>,
        expected_size: Option<u64>,
        config: &crate::downloader::config::DownloadConfig,
    ) -> Result<u64> {
        let client = self.create_client(config)?;

        // Check for existing partial file
        let temp_path = dest_path.with_extension("part");
        let start_byte = if config.allow_resume && temp_path.exists() {
            let size = fs::metadata(&temp_path).await?.len();
            debug!("Found partial file, resuming from byte {}", size);
            size
        } else {
            0
        };

        // Get file size for progress tracking
        let mut total_size = if let Some(expected) = expected_size {
            debug!("Using expected size from validation: {} bytes", expected);
            Some(expected)
        } else {
            debug!("No expected size provided, querying server");
            self.get_file_size(config).await?
        };

        // Build request with range header for resume
        let mut request = client.get(url);
        if start_byte > 0 {
            request = request.header("Range", format!("bytes={}-", start_byte));
            debug!("Requesting range: bytes={}-", start_byte);
        }

        let response = request.send().await?;
        response.error_for_status_ref()?;

        // If we didn't get size from HEAD request, try to get it from GET response
        if total_size.is_none() {
            total_size = response.content_length();
            debug!("Got content length from GET response: {:?}", total_size);
        }

        // Adjust total size if resuming
        if let Some(size) = total_size {
            if start_byte > 0 {
                total_size = Some(start_byte + size);
            }
        }

        if let Some(ref callback) = progress_callback {
            callback(ProgressEvent::DownloadStarted {
                url: url.to_string(),
                total_size,
            });
        }

        // Open file for writing
        let mut file = if start_byte > 0 {
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&temp_path)
                .await?
        } else {
            fs::File::create(&temp_path).await?
        };

        // Download with progress tracking
        let mut stream = response.bytes_stream();
        let mut downloaded = start_byte;
        let start_time = std::time::Instant::now();
        let mut last_progress_time = start_time;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;

            // Report progress at most every 100ms to avoid spam
            let now = std::time::Instant::now();
            if now.duration_since(last_progress_time).as_millis() >= 100 {
                if let Some(ref callback) = progress_callback {
                    let elapsed = start_time.elapsed().as_secs_f64();
                    let speed = if elapsed > 0.0 {
                        (downloaded - start_byte) as f64 / elapsed
                    } else {
                        0.0
                    };

                    callback(ProgressEvent::DownloadProgress {
                        url: url.to_string(),
                        downloaded,
                        total: total_size,
                        speed_bps: speed,
                    });
                }
                last_progress_time = now;
            }
        }

        file.flush().await?;

        // Move temp file to final destination
        fs::rename(&temp_path, dest_path).await?;

        if let Some(ref callback) = progress_callback {
            callback(ProgressEvent::DownloadComplete {
                url: url.to_string(),
                final_size: downloaded,
            });
        }

        debug!("Download completed: {} bytes", downloaded);
        Ok(downloaded)
    }

    /// Download helper method
    async fn download_helper(
        &self,
        url: &str,
        dest_path: &Path,
        progress_callback: Option<ProgressCallback>,
        expected_size: Option<u64>,
        config: &crate::downloader::config::DownloadConfig,
    ) -> Result<u64> {
        debug!("Download: {} to {}", url, dest_path.display());

        // Create destination directory if it doesn't exist
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).await?;
            debug!("Created directory: {}", parent.display());
        }

        // Try primary URL first
        match self.download_file(url, dest_path, progress_callback.clone(), expected_size, config).await {
            Ok(size) => return Ok(size),
            Err(e) => {
                warn!("Primary URL failed: {}", e);
                // Try mirror URLs if primary fails
                for mirror_url in &self.mirror_urls {
                    debug!("Trying mirror URL: {}", mirror_url);
                    match self.download_file(mirror_url, dest_path, progress_callback.clone(), expected_size, config).await {
                        Ok(size) => return Ok(size),
                        Err(mirror_error) => {
                            warn!("Mirror URL {} failed: {}", mirror_url, mirror_error);
                            continue;
                        }
                    }
                }
                // If all URLs failed, return the original error
                return Err(e);
            }
        }
    }

    /// Check if file exists and handle validation if needed
    async fn check_existing_file(
        &self,
        dest_path: &Path,
        validation: &crate::downloader::core::FileValidation,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<Option<DownloadResult>> {
        if dest_path.exists() {
            let size = fs::metadata(dest_path).await?.len();

            if validation.is_empty() {
                // No validation needed, file exists
                debug!("File exists and no validation required");
                return Ok(Some(DownloadResult::AlreadyExists { size }));
            } else if validation.validate_file(dest_path, progress_callback).await? {
                debug!("File exists and is valid");
                return Ok(Some(DownloadResult::AlreadyExists { size }));
            } else {
                // Remove invalid file
                warn!("Existing file is invalid, removing: {}", dest_path.display());
                fs::remove_file(dest_path).await?;
            }
        }
        Ok(None)
    }
}

// WabbajackCDN specific types
#[derive(Debug, Deserialize)]
struct FileDefinition {
    #[serde(rename = "MungedName")]
    munged_name: String,
    #[serde(rename = "Hash")]
    hash: String,
    #[serde(rename = "Size")]
    size: u64,
    #[serde(rename = "Parts")]
    parts: Vec<PartDefinition>,
}

#[derive(Debug, Deserialize)]
struct PartDefinition {
    #[serde(rename = "Index")]
    index: u32,
    #[serde(rename = "Size")]
    size: u64,
    #[serde(rename = "Hash")]
    hash: String,
    #[serde(rename = "Offset")]
    offset: u64,
}

#[async_trait]
impl Downloadable for WabbajackCDNSource {
    async fn download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<ProgressCallback>,
        _config: &crate::downloader::config::DownloadConfig,
    ) -> Result<DownloadResult> {
        let filename = request.get_filename()?;
        let dest_path = request.destination.join(&filename);

        debug!("WabbajackCDN downloading {} to {}", self.url, dest_path.display());

        // Check if file already exists and is valid
        if let Some(result) = self.check_existing_file(&dest_path, &request.validation, progress_callback.clone()).await? {
            return Ok(result);
        }

        // Create destination directory if it doesn't exist
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Download the chunked file
        let final_size = self.download_chunked_file(&dest_path, progress_callback.clone(), request.expected_size).await?;

        // Validate the complete assembled file using custom WabbajackCDN validation
        self.validate_wabbajack_file(&dest_path, &request.validation, progress_callback).await?;

        Ok(DownloadResult::Downloaded { size: final_size })
    }

    fn description(&self) -> String {
        format!("WabbajackCDN download from {}", self.url)
    }
}

impl WabbajackCDNSource {
    /// Domain remapping for WabbajackCDN domains
    const DOMAIN_REMAPS: &'static [(&'static str, &'static str)] = &[
        ("wabbajack.b-cdn.net", "authored-files.wabbajack.org"),
        ("wabbajack-mirror.b-cdn.net", "mirror.wabbajack.org"),
        ("wabbajack-patches.b-cdn.net", "patches.wabbajack.org"),
        ("wabbajacktest.b-cdn.net", "test-files.wabbajack.org"),
    ];

    /// Apply domain remapping to a URL
    fn remap_domain(&self, url: &str) -> Result<String> {
        let parsed_url = url::Url::parse(url)?;
        let host = parsed_url.host_str().unwrap_or("");

        for (original, remapped) in Self::DOMAIN_REMAPS {
            if host == *original || host == *remapped {
                let mut new_url = parsed_url.clone();
                new_url.set_host(Some(remapped))
                    .map_err(|_| DownloadError::Legacy("Failed to remap domain".to_string()))?;
                return Ok(new_url.to_string());
            }
        }

        Ok(url.to_string())
    }

    /// Create HTTP client for WabbajackCDN requests
    fn create_client(&self) -> Client {
        Client::builder()
            .user_agent("Unifier/1.0")
            .build()
            .expect("Failed to create HTTP client")
    }

    /// Create HTTP request with proper headers
    fn create_request(&self, url: &str) -> Result<reqwest::RequestBuilder> {
        let client = self.create_client();
        let remapped_url = self.remap_domain(url)?;
        let parsed_url = url::Url::parse(&remapped_url)?;

        let mut request = client.get(&remapped_url);

        // Add Host header if domain was remapped
        if let Some(host) = parsed_url.host_str() {
            request = request.header("Host", host);
        }

        Ok(request)
    }

    /// Download and parse the file definition
    async fn get_file_definition(&self) -> Result<FileDefinition> {
        let definition_url = format!("{}/definition.json.gz", self.url);
        let request = self.create_request(&definition_url)?;

        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(DownloadError::Legacy(
                format!("HTTP error {}: Failed to fetch file definition from {}",
                       response.status(), definition_url)
            ));
        }

        let compressed_data = response.bytes().await?;

        // Decompress the gzipped definition
        let mut decoder = GzDecoder::new(&compressed_data[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)
            .map_err(|e| DownloadError::Legacy(format!("Failed to decompress definition: {}", e)))?;

        let definition: FileDefinition = serde_json::from_slice(&decompressed)
            .map_err(|e| DownloadError::Legacy(format!("Failed to parse definition JSON: {}", e)))?;

        Ok(definition)
    }

    /// Download a single part
    async fn download_part(&self, part: &PartDefinition) -> Result<Vec<u8>> {
        let part_url = format!("{}/parts/{}", self.url, part.index);
        debug!("Downloading part {} from URL: {}", part.index, part_url);
        let request = self.create_request(&part_url)?;

        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(DownloadError::Legacy(
                format!("HTTP error {}: Failed to download part {} from {}",
                       response.status(), part.index, part_url)
            ));
        }

        let data = response.bytes().await?;
        Ok(data.to_vec())
    }

    /// Download all parts and assemble the final file
    async fn download_chunked_file(
        &self,
        dest_path: &Path,
        progress_callback: Option<ProgressCallback>,
        expected_size: Option<u64>
    ) -> Result<u64> {
        // Get file definition
        let definition = self.get_file_definition().await?;

        // Use expected size if provided, otherwise use definition size
        let total_size = expected_size.unwrap_or(definition.size);

        // Create output file
        let mut output_file = fs::File::create(dest_path).await?;

        // Download parts in sequence
        let mut downloaded_bytes = 0u64;

        for part in &definition.parts {
            let part_data = self.download_part(part).await?;

            // Write part to output file at correct offset
            output_file.seek(tokio::io::SeekFrom::Start(part.offset)).await?;
            output_file.write_all(&part_data).await?;

            downloaded_bytes += part.size;

            // Report progress
            if let Some(ref callback) = progress_callback {
                callback(ProgressEvent::DownloadProgress {
                    url: self.url.clone(),
                    downloaded: downloaded_bytes,
                    total: Some(total_size),
                    speed_bps: 0.0, // TODO: calculate actual speed
                });
            }
        }

        output_file.flush().await?;
        Ok(total_size)
    }

    /// Validate WabbajackCDN file with base64-encoded MD5 hash
    async fn validate_wabbajack_file(
        &self,
        file_path: &Path,
        validation: &crate::downloader::core::FileValidation,
        progress_callback: Option<ProgressCallback>
    ) -> Result<()> {
        // Only validate if we have an MD5 hash (WabbajackCDN uses MD5 hashes in base64 format)
        if let Some(ref expected_md5_base64) = validation.md5 {
            // Read and hash the file
            let file_data = fs::read(file_path).await?;

            // Compute MD5 hash
            let computed_md5 = md5::compute(&file_data);

            // Convert computed hash to base64 (WabbajackCDN format)
            let computed_md5_base64 = general_purpose::STANDARD.encode(computed_md5.as_ref());

            // Compare base64 hashes
            if &computed_md5_base64 != expected_md5_base64 {
                // Delete the invalid file
                let _ = fs::remove_file(file_path).await;

                return Err(DownloadError::ValidationFailed {
                    file: file_path.to_path_buf(),
                    validation_type: ValidationType::Md5,
                    expected: expected_md5_base64.clone(),
                    actual: computed_md5_base64,
                    suggestion: "File may be corrupted, try downloading again".to_string()
                });
            }

            // Report validation success
            if let Some(ref callback) = progress_callback {
                callback(ProgressEvent::ValidationComplete {
                    file: file_path.display().to_string(),
                    valid: true,
                });
            }
        }

        Ok(())
    }

    /// Check if file exists and is valid
    async fn check_existing_file(
        &self,
        dest_path: &Path,
        _validation: &crate::downloader::core::FileValidation,
        _progress_callback: Option<ProgressCallback>,
    ) -> Result<Option<DownloadResult>> {
        if !dest_path.exists() {
            return Ok(None);
        }

        // TODO: Implement proper validation for existing files
        let metadata = fs::metadata(dest_path).await?;

        Ok(Some(DownloadResult::AlreadyExists {
            size: metadata.len()
        }))
    }
}

#[async_trait]
impl Downloadable for GameFileSource {
    async fn download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<ProgressCallback>,
        _config: &crate::downloader::config::DownloadConfig,
    ) -> Result<DownloadResult> {
        let filename = request.get_filename()?;
        let dest_path = request.destination.join(&filename);

        debug!("GameFile downloading {} from {} ({})",
               self.file_path, self.game, self.game_version);

        // Check if file already exists and is valid
        if let Some(result) = self.check_existing_file(&dest_path, &request.validation, progress_callback.clone()).await? {
            return Ok(result);
        }

        // Discover game location
        let game_dir = self.discover_game_location().await?;

        // Construct source file path
        let source_path = game_dir.join(&self.file_path);

        // Check if source file exists
        if !source_path.exists() {
            return Err(DownloadError::Legacy(
                format!("Game file not found: {}", source_path.display())
            ));
        }

        // Copy the file
        let size = self.copy_file_with_progress(&source_path, &dest_path, progress_callback.clone()).await?;

        // Validate the copied file (only if validation is specified)
        if !request.validation.is_empty() {
            debug!("Validating copied game file");
            if !request.validation.validate_file(&dest_path, progress_callback).await? {
                fs::remove_file(&dest_path).await?;
                return Err(DownloadError::ValidationFailed {
                    file: dest_path.clone(),
                    validation_type: ValidationType::Size,
                    expected: "valid file".to_string(),
                    actual: "invalid file".to_string(),
                    suggestion: "Check game file integrity or reinstall the game".to_string(),
                });
            }
            debug!("Game file validation passed");
        }

        Ok(DownloadResult::Downloaded { size })
    }

    fn description(&self) -> String {
        format!("Game file: {} from {}", self.file_path, self.game)
    }

    fn requires_external_dependencies(&self) -> bool {
        true
    }
}

impl GameFileSource {
    /// Discover the installation directory for this game
    async fn discover_game_location(&self) -> Result<std::path::PathBuf> {
        debug!("Discovering game location for: {}", self.game);

        // Try common Steam installation paths
        let steam_paths = self.get_steam_paths();
        for steam_path in steam_paths {
            let game_path = steam_path.join("steamapps").join("common").join(self.get_steam_folder_name());
            if game_path.exists() {
                debug!("Found game at Steam path: {}", game_path.display());
                return Ok(game_path);
            }
        }

        // Try Windows registry (if on Windows)
        #[cfg(windows)]
        if let Ok(registry_path) = self.get_game_from_registry().await {
            if registry_path.exists() {
                debug!("Found game via Windows registry: {}", registry_path.display());
                return Ok(registry_path);
            }
        }

        // Try environment variables
        if let Ok(env_path) = std::env::var(format!("{}_PATH", self.game.to_uppercase())) {
            let path = std::path::PathBuf::from(env_path);
            if path.exists() {
                debug!("Found game via environment variable: {}", path.display());
                return Ok(path);
            }
        }

        Err(DownloadError::Legacy(
            format!("Could not locate game installation for: {}. Please ensure {} is installed, or set the {}_PATH environment variable to point to the game directory",
                   self.game, self.game, self.game.to_uppercase())
        ))
    }

    /// Get common Steam installation paths
    fn get_steam_paths(&self) -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();

        // Default Steam path
        #[cfg(windows)]
        {
            if let Ok(program_files) = std::env::var("PROGRAMFILES(X86)") {
                paths.push(std::path::PathBuf::from(program_files).join("Steam"));
            }
            if let Ok(program_files) = std::env::var("PROGRAMFILES") {
                paths.push(std::path::PathBuf::from(program_files).join("Steam"));
            }
        }

        #[cfg(unix)]
        {
            if let Ok(home) = std::env::var("HOME") {
                paths.push(std::path::PathBuf::from(home).join(".steam").join("steam"));
                paths.push(std::path::PathBuf::from(home).join(".local").join("share").join("Steam"));
            }
        }

        paths
    }

    /// Get the Steam folder name for this game
    fn get_steam_folder_name(&self) -> String {
        match self.game.as_str() {
            "SkyrimSpecialEdition" => "The Elder Scrolls V Skyrim Special Edition".to_string(),
            "Skyrim" => "Skyrim".to_string(),
            "Fallout4" => "Fallout 4".to_string(),
            "FalloutNewVegas" => "Fallout New Vegas".to_string(),
            "Fallout3" => "Fallout 3".to_string(),
            "Oblivion" => "Oblivion".to_string(),
            "Morrowind" => "Morrowind".to_string(),
            _ => self.game.clone(), // Default to the game identifier
        }
    }

    /// Get game location from Windows registry
    #[cfg(windows)]
    async fn get_game_from_registry(&self) -> Result<std::path::PathBuf> {
        use winreg::enums::*;
        use winreg::RegKey;

        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

        // Try different registry paths based on the game
        let registry_paths = match self.game.as_str() {
            "SkyrimSpecialEdition" => vec![
                r"SOFTWARE\Bethesda Softworks\Skyrim Special Edition",
                r"SOFTWARE\WOW6432Node\Bethesda Softworks\Skyrim Special Edition",
            ],
            "Skyrim" => vec![
                r"SOFTWARE\Bethesda Softworks\Skyrim",
                r"SOFTWARE\WOW6432Node\Bethesda Softworks\Skyrim",
            ],
            "Fallout4" => vec![
                r"SOFTWARE\Bethesda Softworks\Fallout4",
                r"SOFTWARE\WOW6432Node\Bethesda Softworks\Fallout4",
            ],
            _ => return Err(DownloadError::Legacy(
                format!("No registry path configured for game: {}", self.game)
            )),
        };

        for registry_path in registry_paths {
            if let Ok(key) = hklm.open_subkey(registry_path) {
                if let Ok(install_path) = key.get_value::<String, _>("installed path") {
                    return Ok(std::path::PathBuf::from(install_path));
                }
            }
        }

        Err(DownloadError::Legacy(
            format!("Game not found in Windows registry: {}", self.game)
        ))
    }

    /// Copy a file from source to destination with progress reporting
    async fn copy_file_with_progress(
        &self,
        source_path: &Path,
        dest_path: &Path,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<u64> {
        // Get source file size
        let source_metadata = fs::metadata(source_path).await?;
        let total_size = source_metadata.len();
        debug!("Copying file: {} -> {} ({} bytes)",
               source_path.display(), dest_path.display(), total_size);

        // Create destination directory if needed
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Report start
        if let Some(ref callback) = progress_callback {
            callback(ProgressEvent::DownloadStarted {
                url: format!("gamefile://{}", source_path.display()),
                total_size: Some(total_size),
            });
        }

        // Open source and destination files
        let mut source_file = fs::File::open(source_path).await?;
        let mut dest_file = fs::File::create(dest_path).await?;

        // Copy with progress reporting
        let mut copied = 0u64;
        let mut buffer = vec![0u8; 64 * 1024]; // 64KB buffer
        let start_time = std::time::Instant::now();
        let mut last_progress_time = start_time;

        loop {
            use tokio::io::AsyncReadExt;

            let bytes_read = source_file.read(&mut buffer).await?;
            if bytes_read == 0 {
                break; // EOF
            }

            dest_file.write_all(&buffer[..bytes_read]).await?;
            copied += bytes_read as u64;

            // Report progress at most every 100ms
            let now = std::time::Instant::now();
            if now.duration_since(last_progress_time).as_millis() >= 100 {
                if let Some(ref callback) = progress_callback {
                    let elapsed = start_time.elapsed().as_secs_f64();
                    let speed = if elapsed > 0.0 {
                        copied as f64 / elapsed
                    } else {
                        0.0
                    };

                    callback(ProgressEvent::DownloadProgress {
                        url: format!("gamefile://{}", source_path.display()),
                        downloaded: copied,
                        total: Some(total_size),
                        speed_bps: speed,
                    });
                }
                last_progress_time = now;
            }
        }

        dest_file.flush().await?;

        // Report completion
        if let Some(ref callback) = progress_callback {
            callback(ProgressEvent::DownloadComplete {
                url: format!("gamefile://{}", source_path.display()),
                final_size: copied,
            });
        }

        debug!("Successfully copied game file: {} ({} bytes)", dest_path.display(), copied);
        Ok(copied)
    }

    /// Check if file exists and is valid
    async fn check_existing_file(
        &self,
        dest_path: &Path,
        validation: &crate::downloader::core::FileValidation,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<Option<DownloadResult>> {
        if !dest_path.exists() {
            return Ok(None);
        }

        debug!("File already exists: {}", dest_path.display());

        // If no validation required, assume file is good
        if validation.is_empty() {
            let metadata = fs::metadata(dest_path).await?;
            debug!("Using existing file: {} ({} bytes)", dest_path.display(), metadata.len());
            return Ok(Some(DownloadResult::AlreadyExists {
                size: metadata.len()
            }));
        }

        // Validate existing file
        if validation.validate_file(dest_path, progress_callback).await? {
            let metadata = fs::metadata(dest_path).await?;
            debug!("Using existing validated file: {} ({} bytes)", dest_path.display(), metadata.len());
            Ok(Some(DownloadResult::AlreadyExists {
                size: metadata.len()
            }))
        } else {
            warn!("Existing file failed validation, will re-copy: {}", dest_path.display());
            // Remove invalid file
            fs::remove_file(dest_path).await?;
            Ok(None)
        }
    }
}

// Placeholder implementations for remaining source types
#[async_trait]
impl Downloadable for NexusSource {
    async fn download(
        &self,
        _request: &DownloadRequest,
        _progress_callback: Option<ProgressCallback>,
        _config: &crate::downloader::config::DownloadConfig,
    ) -> Result<DownloadResult> {
        Err(DownloadError::Legacy(
            "Nexus downloads not yet implemented in new architecture".to_string()
        ))
    }

    fn description(&self) -> String {
        format!("Nexus download: {} by {} (mod {}, file {})",
                self.mod_name, self.author, self.mod_id, self.file_id)
    }

    fn requires_external_dependencies(&self) -> bool {
        true // Requires Nexus API key
    }
}

#[async_trait]
impl Downloadable for ManualSource {
    async fn download(
        &self,
        _request: &DownloadRequest,
        _progress_callback: Option<ProgressCallback>,
        _config: &crate::downloader::config::DownloadConfig,
    ) -> Result<DownloadResult> {
        Err(DownloadError::Legacy(
            format!("Manual download required: {}", self.instructions)
        ))
    }

    fn description(&self) -> String {
        format!("Manual download: {}", self.instructions)
    }

    fn requires_user_interaction(&self) -> bool {
        true
    }
}

#[async_trait]
impl Downloadable for ArchiveSource {
    async fn download(
        &self,
        _request: &DownloadRequest,
        _progress_callback: Option<ProgressCallback>,
        _config: &crate::downloader::config::DownloadConfig,
    ) -> Result<DownloadResult> {
        Err(DownloadError::Legacy(
            "Archive extraction not yet implemented in new architecture".to_string()
        ))
    }

    fn description(&self) -> String {
        format!("Extract {} from archive {}", self.inner_path, self.archive_hash)
    }
}
