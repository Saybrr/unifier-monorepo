//! WabbajackCDN download source implementation

use flate2::read::GzDecoder;
use reqwest::Client;
use serde::Deserialize;
use std::io::Read;
use std::path::Path;
use tokio::fs;
use tokio::io::{AsyncWriteExt, AsyncSeekExt};
use tracing::debug;

use crate::downloader::core::{
    DownloadRequest, DownloadResult, ProgressCallback, Result,
    DownloadError, ProgressEvent, files::check_existing_file
};

/// Raw WabbajackCDN archive state from JSON parsing
#[derive(Debug, Deserialize, Clone)]
pub struct WabbajackCDNArchiveState {
    #[serde(rename = "Url")]
    pub url: String,
}

/// Wabbajack CDN download source
#[derive(Debug, Clone, PartialEq)]
pub struct WabbajackCDNSource {
    /// CDN URL for the file
    pub url: String,
}

// WabbajackCDN specific types
#[derive(Debug, Deserialize)]
struct FileDefinition {
    #[serde(rename = "MungedName")]
    _munged_name: String,
    #[serde(rename = "Hash")]
    _hash: String,
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
    _hash: String,
    #[serde(rename = "Offset")]
    offset: u64,
}

impl WabbajackCDNSource {
    pub async fn download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<ProgressCallback>,
        _config: &crate::downloader::core::config::DownloadConfig,
    ) -> Result<DownloadResult> {
        let filename = request.get_filename()?;
        let dest_path = request.destination.join(&filename);

        debug!("WabbajackCDN downloading {} to {}", self.url, dest_path.display());

        // Check if file already exists and is valid
        if let Some(result) = check_existing_file(&dest_path, &request.validation, progress_callback.clone()).await? {
            return Ok(result);
        }

        // Create destination directory if it doesn't exist
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Download the chunked file
        let final_size = self.download_chunked_file(&dest_path, progress_callback.clone(), Some(request.expected_size)).await?;

        // Return result with file path for centralized validation
        Ok(DownloadResult::Downloaded {
            size: final_size,
            file_path: dest_path
        })
    }

    // Move helper methods to the same impl block
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
        output_file.sync_all().await?; // Ensure file is fully written to disk before validation
        Ok(total_size)
    }


}

impl WabbajackCDNSource {
    pub fn new<S: Into<String>>(url: S) -> Self {
        Self {
            url: url.into(),
        }
    }
}
