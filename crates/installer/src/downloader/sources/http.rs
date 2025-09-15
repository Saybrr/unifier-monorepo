//! HTTP download source implementation

use std::collections::HashMap;
use std::path::Path;
use tracing::debug;

use crate::downloader::core::{
    DownloadRequest, DownloadResult, ProgressCallback, Result,
    DownloadError, ValidationType, ProgressEvent
};
use crate::downloader::core::http::HttpClient;
use crate::downloader::core::files::check_existing_file;

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
        config: &crate::downloader::core::config::DownloadConfig,
    ) -> Result<DownloadResult> {
        let filename = request.get_filename()?;
        let dest_path = request.destination.join(&filename);

        debug!("HTTP downloading {} to {}", self.url, dest_path.display());

        // Check existing file first using common utility
        if let Some(result) = check_existing_file(&dest_path, &request.validation, progress_callback.clone()).await? {
            return Ok(result);
        }

        // Download the file using centralized logic
        let size = self.download_with_mirrors(&dest_path, progress_callback.clone(), Some(request.expected_size), config).await?;

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
                    tokio::fs::remove_file(&dest_path).await?;
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
                    tokio::fs::remove_file(&dest_path).await?;
                    return Err(e); // Propagate the specific error (e.g., SizeMismatch)
                }
            }
        }

        Ok(DownloadResult::Downloaded { size })
    }

}

impl HttpSource {
    /// Download with mirror fallback support
    async fn download_with_mirrors(
        &self,
        dest_path: &Path,
        progress_callback: Option<ProgressCallback>,
        expected_size: Option<u64>,
        config: &crate::downloader::core::config::DownloadConfig,
    ) -> Result<u64> {
        debug!("Download: {} to {}", self.url, dest_path.display());

        // Create HTTP client with appropriate timeout
        let timeout = expected_size
            .map(|size| config.get_timeout_for_size(size))
            .unwrap_or(config.timeout);
        let http_client = HttpClient::with_timeout(config, timeout)?;

        // Try primary URL with built-in retry logic
        let primary_result = http_client.download_with_retry(&self.url, dest_path, expected_size, progress_callback.clone(), config).await;

        match primary_result {
            Ok(size) => return Ok(size),
            Err(e) => {
                // Report warning through progress callback
                if let Some(ref callback) = progress_callback {
                    callback(ProgressEvent::Warning {
                        url: self.url.clone(),
                        message: format!("Primary URL failed after {} retries: {}", config.max_retries, e),
                    });
                }

                // Try mirror URLs if primary fails
                for mirror_url in &self.mirror_urls {
                    debug!("Trying mirror URL: {}", mirror_url);
                    let mirror_result = http_client.download_with_retry(mirror_url, dest_path, expected_size, progress_callback.clone(), config).await;

                    match mirror_result {
                        Ok(size) => return Ok(size),
                        Err(mirror_error) => {
                            // Report mirror failure warning
                            if let Some(ref callback) = progress_callback {
                                callback(ProgressEvent::Warning {
                                    url: mirror_url.clone(),
                                    message: format!("Mirror URL failed after {} retries: {}", config.max_retries, mirror_error),
                                });
                            }
                            continue;
                        }
                    }
                }
                // If all URLs failed, return the original error
                return Err(e);
            }
        }
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
