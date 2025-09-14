//! HTTP download source implementation

use futures::StreamExt;
use reqwest::Client;
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, warn};

use crate::downloader::core::{
    DownloadRequest, DownloadResult, ProgressCallback, Result,
    DownloadError, ValidationType, ProgressEvent
};

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

impl HttpSource {
    pub async fn download(
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

        let size = self.download_helper(&self.url, &dest_path, progress_callback.clone(), Some(request.expected_size), config).await?;

        // Validate the downloaded file (only if validation is specified)
        if !request.validation.is_empty() {
            debug!("Validating HTTP downloaded file: {} (expected_size: {:?})",
                   dest_path.display(), request.validation.expected_size);

            match request.validation.validate_file(&dest_path, progress_callback).await {
                Ok(true) => {
                    debug!("HTTP file validation passed");
                },
                Ok(false) => {
                    // This shouldn't happen as validate_file returns Err for failures
                    fs::remove_file(&dest_path).await?;
                    return Err(DownloadError::ValidationFailed {
                        file: dest_path.clone(),
                        validation_type: ValidationType::Size,
                        expected: "valid file".to_string(),
                        actual: "invalid file".to_string(),
                        suggestion: "Check file integrity or download again".to_string(),
                    });
                },
                Err(e) => {
                    // Log the specific validation error (like SizeMismatch)
                    debug!("HTTP file validation failed with error: {}", e);
                    fs::remove_file(&dest_path).await?;
                    return Err(e); // Propagate the specific error (e.g., SizeMismatch)
                }
            }
        }

        Ok(DownloadResult::Downloaded { size })
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
        file.sync_all().await?; // Ensure file is fully written to disk before rename

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

// Builder methods for HttpSource
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
