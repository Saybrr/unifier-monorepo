//! File downloader with validation and retry capabilities
//!
//! This module provides a comprehensive file downloading system with:
//! - Multiple download source support (HTTP, Google Drive, etc.)
//! - File integrity validation (CRC32, MD5, SHA256)
//! - Retry logic with exponential backoff
//! - Progress tracking
//! - Resume capability

use async_trait::async_trait;
use crc32fast::Hasher as Crc32Hasher;
use futures::StreamExt;
use digest::Digest;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{error, info};
use url::Url;

/// Custom error types for the downloader
#[derive(Error, Debug)]
pub enum DownloadError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("URL parse error: {0}")]
    UrlError(#[from] url::ParseError),

    #[error("File validation failed: expected {expected}, got {actual}")]
    ValidationError { expected: String, actual: String },

    #[error("Unsupported URL: {0}")]
    UnsupportedUrl(String),

    #[error("Download timeout")]
    Timeout,

    #[error("Maximum retry attempts exceeded")]
    MaxRetriesExceeded,

    #[error("File size mismatch: expected {expected}, got {actual}")]
    SizeMismatch { expected: u64, actual: u64 },
}

pub type Result<T> = std::result::Result<T, DownloadError>;

/// Progress callback for download operations
pub type ProgressCallback = Arc<dyn Fn(ProgressEvent) + Send + Sync>;

/// Events emitted during download operations
#[derive(Debug, Clone)]
pub enum ProgressEvent {
    DownloadStarted {
        url: String,
        total_size: Option<u64>,
    },
    DownloadProgress {
        url: String,
        downloaded: u64,
        total: Option<u64>,
        speed_bps: f64,
    },
    DownloadComplete {
        url: String,
        final_size: u64,
    },
    ValidationStarted {
        file: String,
    },
    ValidationProgress {
        file: String,
        progress: f64,
    },
    ValidationComplete {
        file: String,
        valid: bool,
    },
    RetryAttempt {
        url: String,
        attempt: usize,
        max_attempts: usize,
    },
    Error {
        url: String,
        error: String,
    },
}

/// File validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileValidation {
    pub crc32: Option<u32>,
    pub md5: Option<String>,
    pub sha256: Option<String>,
    pub expected_size: Option<u64>,
}

impl Default for FileValidation {
    fn default() -> Self {
        Self {
            crc32: None,
            md5: None,
            sha256: None,
            expected_size: None,
        }
    }
}

impl FileValidation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_crc32(mut self, crc32: u32) -> Self {
        self.crc32 = Some(crc32);
        self
    }

    pub fn with_md5(mut self, md5: String) -> Self {
        self.md5 = Some(md5.to_lowercase());
        self
    }

    pub fn with_sha256(mut self, sha256: String) -> Self {
        self.sha256 = Some(sha256.to_lowercase());
        self
    }

    pub fn with_expected_size(mut self, size: u64) -> Self {
        self.expected_size = Some(size);
        self
    }

    /// Validate a file against the configured validation parameters
    pub async fn validate_file<P: AsRef<Path>>(
        &self,
        path: P,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<bool> {
        let path = path.as_ref();
        let file_size = fs::metadata(path).await?.len();

        if let Some(ref callback) = progress_callback {
            callback(ProgressEvent::ValidationStarted {
                file: path.display().to_string(),
            });
        }

        // Check file size first (fastest check)
        if let Some(expected_size) = self.expected_size {
            if file_size != expected_size {
                if let Some(ref callback) = progress_callback {
                    callback(ProgressEvent::ValidationComplete {
                        file: path.display().to_string(),
                        valid: false,
                    });
                }
                return Err(DownloadError::SizeMismatch {
                    expected: expected_size,
                    actual: file_size,
                });
            }
        }

        // Only calculate hashes if we need to
        if self.crc32.is_some() || self.md5.is_some() || self.sha256.is_some() {
            let mut file = fs::File::open(path).await?;
            let mut buffer = vec![0u8; 8192];
            let mut bytes_read_total = 0u64;

            let mut crc32_hasher = Crc32Hasher::new();
            let mut md5_hasher = md5::Context::new();
            let mut sha256_hasher = Sha256::new();

            loop {
                let bytes_read = file.read(&mut buffer).await?;
                if bytes_read == 0 {
                    break;
                }

                let chunk = &buffer[..bytes_read];
                crc32_hasher.update(chunk);
                md5_hasher.consume(chunk);
                sha256_hasher.update(chunk);

                bytes_read_total += bytes_read as u64;

                if let Some(ref callback) = progress_callback {
                    let progress = bytes_read_total as f64 / file_size as f64;
                    callback(ProgressEvent::ValidationProgress {
                        file: path.display().to_string(),
                        progress,
                    });
                }
            }

            // Validate CRC32
            if let Some(expected_crc32) = self.crc32 {
                let actual_crc32 = crc32_hasher.finalize();
                if actual_crc32 != expected_crc32 {
                    if let Some(ref callback) = progress_callback {
                        callback(ProgressEvent::ValidationComplete {
                            file: path.display().to_string(),
                            valid: false,
                        });
                    }
                    return Ok(false);
                }
            }

            // Validate MD5
            if let Some(ref expected_md5) = self.md5 {
                let actual_md5 = format!("{:x}", md5_hasher.compute());
                if &actual_md5 != expected_md5 {
                    if let Some(ref callback) = progress_callback {
                        callback(ProgressEvent::ValidationComplete {
                            file: path.display().to_string(),
                            valid: false,
                        });
                    }
                    return Ok(false);
                }
            }

            // Validate SHA256
            if let Some(ref expected_sha256) = self.sha256 {
                let actual_sha256 = format!("{:x}", sha256_hasher.finalize());
                if &actual_sha256 != expected_sha256 {
                    if let Some(ref callback) = progress_callback {
                        callback(ProgressEvent::ValidationComplete {
                            file: path.display().to_string(),
                            valid: false,
                        });
                    }
                    return Ok(false);
                }
            }
        }

        if let Some(ref callback) = progress_callback {
            callback(ProgressEvent::ValidationComplete {
                file: path.display().to_string(),
                valid: true,
            });
        }

        Ok(true)
    }
}

/// Configuration for download operations
#[derive(Debug, Clone)]
pub struct DownloadConfig {
    pub max_retries: usize,
    pub timeout: Duration,
    pub user_agent: String,
    pub allow_resume: bool,
    pub chunk_size: usize,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            timeout: Duration::from_secs(30),
            user_agent: "installer/0.1.0".to_string(),
            allow_resume: true,
            chunk_size: 8192,
        }
    }
}

/// A download request containing all necessary information
#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub url: String,
    pub mirror_url: Option<String>,
    pub destination: PathBuf,
    pub validation: FileValidation,
    pub filename: Option<String>,
}

impl DownloadRequest {
    pub fn new<S: Into<String>, P: Into<PathBuf>>(url: S, destination: P) -> Self {
        Self {
            url: url.into(),
            mirror_url: None,
            destination: destination.into(),
            validation: FileValidation::default(),
            filename: None,
        }
    }

    pub fn with_mirror_url<S: Into<String>>(mut self, mirror_url: S) -> Self {
        self.mirror_url = Some(mirror_url.into());
        self
    }

    pub fn with_validation(mut self, validation: FileValidation) -> Self {
        self.validation = validation;
        self
    }

    pub fn with_filename<S: Into<String>>(mut self, filename: S) -> Self {
        self.filename = Some(filename.into());
        self
    }

    fn get_filename(&self) -> Result<String> {
        if let Some(ref filename) = self.filename {
            return Ok(filename.clone());
        }

        let url = Url::parse(&self.url)?;
        if let Some(segments) = url.path_segments() {
            if let Some(last_segment) = segments.last() {
                if !last_segment.is_empty() {
                    return Ok(last_segment.to_string());
                }
            }
        }

        Ok("downloaded_file".to_string())
    }
}

/// Result of a download operation
#[derive(Debug, Clone)]
pub enum DownloadResult {
    Downloaded { size: u64 },
    AlreadyExists { size: u64 },
    Resumed { size: u64 },
}

/// Trait for different download implementations
#[async_trait]
pub trait FileDownloader: Send + Sync {
    async fn download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult>;

    fn supports_url(&self, url: &str) -> bool;
}

/// HTTP-based file downloader
pub struct HttpDownloader {
    client: Client,
    config: DownloadConfig,
}

impl HttpDownloader {
    pub fn new(config: DownloadConfig) -> Self {
        let client = Client::builder()
            .timeout(config.timeout)
            .user_agent(&config.user_agent)
            .build()
            .expect("Failed to create HTTP client");

        Self { client, config }
    }

    async fn get_file_size(&self, url: &str) -> Result<Option<u64>> {
        let response = self.client.head(url).send().await?;
        response.error_for_status_ref()?;

        Ok(response.content_length())
    }

    async fn download_file(
        &self,
        url: &str,
        dest_path: &Path,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<u64> {
        // Check for existing partial file
        let temp_path = dest_path.with_extension("part");
        let start_byte = if self.config.allow_resume && temp_path.exists() {
            fs::metadata(&temp_path).await?.len()
        } else {
            0
        };

        // Get file size for progress tracking
        let mut total_size = self.get_file_size(url).await?;

        // Build request with range header for resume
        let mut request = self.client.get(url);
        if start_byte > 0 {
            request = request.header("Range", format!("bytes={}-", start_byte));
        }

        let response = request.send().await?;
        response.error_for_status_ref()?;

        // If we didn't get size from HEAD request, try to get it from GET response
        if total_size.is_none() {
            total_size = response.content_length();
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

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;

            if let Some(ref callback) = progress_callback {
                let elapsed = start_time.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 { downloaded as f64 / elapsed } else { 0.0 };

                callback(ProgressEvent::DownloadProgress {
                    url: url.to_string(),
                    downloaded,
                    total: total_size,
                    speed_bps: speed,
                });
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

        Ok(downloaded)
    }
}

#[async_trait]
impl FileDownloader for HttpDownloader {
    async fn download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult> {
        let filename = request.get_filename()?;
        let dest_path = request.destination.join(&filename);

        // Check if file already exists and is valid
        if dest_path.exists() {
            if request.validation.validate_file(&dest_path, progress_callback.clone()).await? {
                let size = fs::metadata(&dest_path).await?.len();
                return Ok(DownloadResult::AlreadyExists { size });
            } else {
                // Remove invalid file
                fs::remove_file(&dest_path).await?;
            }
        }

        // Create destination directory if it doesn't exist
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let size = self.download_file(&request.url, &dest_path, progress_callback.clone()).await?;

        // Validate the downloaded file
        if !request.validation.validate_file(&dest_path, progress_callback).await? {
            fs::remove_file(&dest_path).await?;
            return Err(DownloadError::ValidationError {
                expected: "valid file".to_string(),
                actual: "invalid file".to_string(),
            });
        }

        Ok(DownloadResult::Downloaded { size })
    }

    fn supports_url(&self, url: &str) -> bool {
        url.starts_with("http://") || url.starts_with("https://")
    }
}

/// Registry for managing multiple downloader implementations
pub struct DownloaderRegistry {
    downloaders: Vec<Box<dyn FileDownloader>>,
}

impl DownloaderRegistry {
    pub fn new() -> Self {
        Self {
            downloaders: Vec::new(),
        }
    }

    pub fn register<D: FileDownloader + 'static>(mut self, downloader: D) -> Self {
        self.downloaders.push(Box::new(downloader));
        self
    }

    pub fn with_http_downloader(self, config: DownloadConfig) -> Self {
        self.register(HttpDownloader::new(config))
    }

    async fn find_downloader(&self, url: &str) -> Result<&dyn FileDownloader> {
        self.downloaders
            .iter()
            .find(|d| d.supports_url(url))
            .map(|d| d.as_ref())
            .ok_or_else(|| DownloadError::UnsupportedUrl(url.to_string()))
    }

    async fn attempt_download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult> {
        let downloader = self.find_downloader(&request.url).await?;
        downloader.download(request, progress_callback).await
    }
}

/// Enhanced downloader with retry capability
pub struct EnhancedDownloader {
    registry: DownloaderRegistry,
    config: DownloadConfig,
}

impl EnhancedDownloader {
    pub fn new(config: DownloadConfig) -> Self {
        let registry = DownloaderRegistry::new()
            .with_http_downloader(config.clone());

        Self { registry, config }
    }

    pub fn with_registry(registry: DownloaderRegistry, config: DownloadConfig) -> Self {
        Self { registry, config }
    }

    /// Download a file with retry logic and mirror fallback
    pub async fn download(
        &self,
        request: DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult> {
        let url = request.url.clone();
        let max_retries = self.config.max_retries;

        // Custom retry loop with progress feedback
        let mut last_error = None;
        for attempt in 0..=max_retries {
            if attempt > 0 {
                if let Some(ref callback) = progress_callback {
                    callback(ProgressEvent::RetryAttempt {
                        url: url.clone(),
                        attempt,
                        max_attempts: max_retries,
                    });
                }

                // Exponential backoff delay
                let delay = Duration::from_millis(1000 * (1 << (attempt - 1).min(5)));
                tokio::time::sleep(delay).await;
            }

            match self.registry.attempt_download(&request, progress_callback.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < max_retries {
                        continue;
                    }
                }
            }
        }

        // All retries failed, try mirror if available
        if request.mirror_url.is_some() {
            info!("Primary download failed, trying mirror URL");

            if let Some(ref callback) = progress_callback {
                callback(ProgressEvent::RetryAttempt {
                    url: url.clone(),
                    attempt: 1,
                    max_attempts: 1,
                });
            }

            let mirror_request = DownloadRequest {
                url: request.mirror_url.unwrap(),
                mirror_url: None,
                ..request
            };

            return self.registry.attempt_download(&mirror_request, progress_callback).await;
        }

        // No mirror available, return error
        if let Some(ref callback) = progress_callback {
            if let Some(ref error) = last_error {
                callback(ProgressEvent::Error {
                    url,
                    error: error.to_string(),
                });
            }
        }

        Err(DownloadError::MaxRetriesExceeded)
    }

    /// Download multiple files concurrently
    pub async fn download_batch(
        &self,
        requests: Vec<DownloadRequest>,
        progress_callback: Option<ProgressCallback>,
        max_concurrent: usize,
    ) -> Vec<Result<DownloadResult>> {
        use futures::stream::{self, StreamExt};

        stream::iter(requests)
            .map(|request| {
                let progress_cb = progress_callback.clone();
                async move {
                    self.download(request, progress_cb).await
                }
            })
            .buffer_unordered(max_concurrent)
            .collect()
            .await
    }
}

impl Default for DownloaderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
