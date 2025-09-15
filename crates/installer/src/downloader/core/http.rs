//! HTTP utilities
//!
//! Centralized HTTP client with integrated streaming download functionality.
//! This combines client configuration and downloading into a single cohesive API.

use futures::StreamExt;
use reqwest::Client;
use std::path::Path;
use std::time::Duration;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::debug;

use crate::downloader::core::{ProgressCallback, ProgressEvent, Result, DownloadError, FileOperation};
use super::files::{create_temp_path, atomic_rename};

/// HTTP client with integrated download functionality
///
/// This combines HTTP client configuration and streaming download capabilities
/// into a single, cohesive API. It handles:
/// - HTTP client configuration (timeout, user agent, etc.)
/// - Streaming downloads with progress tracking
/// - Resume support via .part files
/// - Atomic file operations
pub struct HttpClient {
    client: Client,
    allow_resume: bool,
}

impl HttpClient {
    /// Create a new HTTP client from download configuration
    pub fn from_config(config: &crate::downloader::core::config::DownloadConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(config.timeout)
            .user_agent(&config.user_agent)
            .build()
            .map_err(|e| DownloadError::Legacy(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            allow_resume: config.allow_resume,
        })
    }

    /// Create an HTTP client with custom timeout
    pub fn with_timeout(config: &crate::downloader::core::config::DownloadConfig, timeout: Duration) -> Result<Self> {
        let client = Client::builder()
            .timeout(timeout)
            .user_agent(&config.user_agent)
            .build()
            .map_err(|e| DownloadError::Legacy(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            allow_resume: config.allow_resume,
        })
    }

    /// Create an HTTP client with custom configuration
    pub fn with_config(timeout: Duration, user_agent: String, allow_resume: bool) -> Result<Self> {
        let client = Client::builder()
            .timeout(timeout)
            .user_agent(&user_agent)
            .build()
            .map_err(|e| DownloadError::Legacy(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            allow_resume,
        })
    }

    /// Download from URL to file with full streaming support
    ///
    /// This is the centralized implementation that replaces the duplicate
    /// download logic in HttpSource and NexusSource.
    pub async fn download_to_file(
        &self,
        url: &str,
        dest_path: &Path,
        expected_size: Option<u64>,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<u64> {
        debug!("Stream downloading: {} to {}", url, dest_path.display());

        // Ensure destination directory exists
        fs::create_dir_all(dest_path.parent().unwrap()).await?;

        // Check for existing partial file and resume support
        let temp_path = create_temp_path(dest_path);
        let start_byte = if self.allow_resume && temp_path.exists() {
            let size = fs::metadata(&temp_path).await
                .map_err(|e| DownloadError::FileSystem {
                    path: temp_path.clone(),
                    operation: FileOperation::Metadata,
                    source: e,
                })?.len();
            debug!("Found partial file, resuming from byte {}", size);
            size
        } else {
            0
        };

        // Get total size for progress tracking
        let total_size = if let Some(expected) = expected_size {
            debug!("Using expected size: {} bytes", expected);
            Some(expected)
        } else {
            debug!("No expected size provided, will get from response");
            None
        };

        // Build request with range header for resume
        let mut request = self.client.get(url);
        if start_byte > 0 {
            request = request.header("Range", format!("bytes={}-", start_byte));
            debug!("Requesting range: bytes={}-", start_byte);
        }

        // Send request
        let response = request.send().await
            .map_err(|e| DownloadError::HttpRequest {
                url: url.to_string(),
                source: e,
            })?;

        // Check for success status
        if !response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(DownloadError::HttpRequest {
                url: url.to_string(),
                source: reqwest::Error::from(response.error_for_status().unwrap_err()),
            });
        }

        // Get content length and calculate total size
        let content_length = response.content_length().unwrap_or(0);
        let total_size = total_size.or(Some(if start_byte > 0 {
            start_byte + content_length
        } else {
            content_length
        }));

        debug!("Content length: {} bytes, total size: {:?}", content_length, total_size);

        // Report download started
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
                .await
                .map_err(|e| DownloadError::FileSystem {
                    path: temp_path.clone(),
                    operation: FileOperation::Write,
                    source: e,
                })?
        } else {
            fs::File::create(&temp_path).await
                .map_err(|e| DownloadError::FileSystem {
                    path: temp_path.clone(),
                    operation: FileOperation::Create,
                    source: e,
                })?
        };

        // Stream download with progress tracking
        let mut stream = response.bytes_stream();
        let mut downloaded = start_byte;
        let start_time = std::time::Instant::now();
        let mut last_progress_time = start_time;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| DownloadError::HttpRequest {
                url: url.to_string(),
                source: e,
            })?;

            file.write_all(&chunk).await
                .map_err(|e| DownloadError::FileSystem {
                    path: temp_path.clone(),
                    operation: FileOperation::Write,
                    source: e,
                })?;

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

        // Flush and sync file
        file.flush().await
            .map_err(|e| DownloadError::FileSystem {
                path: temp_path.clone(),
                operation: FileOperation::Write,
                source: e,
            })?;

        file.sync_all().await
            .map_err(|e| DownloadError::FileSystem {
                path: temp_path.clone(),
                operation: FileOperation::Write,
                source: e,
            })?;

        // Atomically rename temp file to final destination
        atomic_rename(&temp_path, dest_path).await?;

        // Report completion
        if let Some(ref callback) = progress_callback {
            callback(ProgressEvent::DownloadComplete {
                url: url.to_string(),
                final_size: downloaded,
            });
        }

        debug!("Stream download completed: {} bytes", downloaded);
        Ok(downloaded)
    }


    pub async fn download_with_retry(
        &self,
        url: &str,
        dest_path: &Path,
        expected_size: Option<u64>,
        progress_callback: Option<ProgressCallback>,
        config: &crate::downloader::core::config::DownloadConfig,
    ) -> Result<u64> {

        retry_with_backoff(
            || async {
                self.download_to_file(url, dest_path, expected_size, progress_callback.clone()).await
            },
            config,
            progress_callback.clone(),
            url,
        ).await
    }

    /// Get file size from server using HEAD request
    ///
    /// This is useful for progress tracking when no expected size is provided.
    pub async fn get_remote_file_size(&self, url: &str) -> Result<Option<u64>> {
        debug!("Getting file size for: {}", url);
        let response = self.client.head(url).send().await
            .map_err(|e| DownloadError::HttpRequest {
                url: url.to_string(),
                source: e,
            })?;

        response.error_for_status_ref()
            .map_err(|e| DownloadError::HttpRequest {
                url: url.to_string(),
                source: e,
            })?;

        Ok(response.content_length())
    }
}

// Legacy compatibility - keep old builder and functions for backward compatibility
/// Builder for creating configured HTTP clients (legacy compatibility)
pub struct HttpClientBuilder {
    timeout: Duration,
    user_agent: String,
}

impl HttpClientBuilder {
    /// Create a new HTTP client builder from download configuration
    pub fn from_config(config: &crate::downloader::core::config::DownloadConfig) -> Self {
        Self {
            timeout: config.timeout,
            user_agent: config.user_agent.clone(),
        }
    }

    /// Override the timeout for this client
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Override the user agent for this client
    pub fn with_user_agent<S: Into<String>>(mut self, user_agent: S) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// Build the HTTP client with the configured settings
    pub fn build(self) -> Result<Client> {
        Client::builder()
            .timeout(self.timeout)
            .user_agent(&self.user_agent)
            .build()
            .map_err(|e| DownloadError::Legacy(format!("Failed to create HTTP client: {}", e)))
    }
}

/// Create a basic HTTP client from configuration (legacy compatibility)
pub fn create_client(config: &crate::downloader::core::config::DownloadConfig) -> Result<Client> {
    HttpClientBuilder::from_config(config).build()
}

/// Create an HTTP client with custom timeout (legacy compatibility)
pub fn create_client_with_timeout(
    config: &crate::downloader::core::config::DownloadConfig,
    timeout: Duration
) -> Result<Client> {
    HttpClientBuilder::from_config(config)
        .with_timeout(timeout)
        .build()
}


pub async fn retry_with_backoff<F, T, Fut>(
    mut operation: F,
    config: &crate::downloader::core::config::DownloadConfig,
    progress_callback: Option<ProgressCallback>,
    url: &str,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut last_error = None;

    for attempt in 0..=config.max_retries {
        if attempt > 0 {
            // Calculate exponential backoff delay
            let delay = config.get_retry_delay(attempt - 1);
            debug!("Retry attempt {} for {} after {:?} delay", attempt, url, delay);

            // Report retry attempt
            if let Some(ref callback) = progress_callback {
                callback(ProgressEvent::RetryAttempt {
                    url: url.to_string(),
                    attempt,
                    max_attempts: config.max_retries,
                });
            }

            tokio::time::sleep(delay).await;
        }

        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                // Check if this error is worth retrying
                if !e.is_recoverable() {
                    debug!("Error is not recoverable, failing immediately: {}", e);
                    return Err(e);
                }

                last_error = Some(e);
                debug!("Error is recoverable, will retry if attempts remain");
                continue;
            }
        }
    }

    // All retries exhausted
    Err(last_error.unwrap())
}
