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
use tracing::{error, info, warn};
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

    /// Check if validation is needed
    pub fn is_empty(&self) -> bool {
        self.crc32.is_none()
            && self.md5.is_none()
            && self.sha256.is_none()
            && self.expected_size.is_none()
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

    /// Validate file in a blocking context (optimized for CPU-intensive work)
    pub async fn validate_file_blocking<P: AsRef<Path>>(
        &self,
        path: P,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<bool> {
        let path = path.as_ref().to_path_buf();
        let validation = self.clone();
        let callback = progress_callback.clone();

        // Move to blocking thread to avoid blocking the async runtime
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(validation.validate_file(&path, callback))
        })
        .await
        .map_err(|e| DownloadError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Validation task failed: {}", e),
        )))?
    }
}

/// Validation thread pool for async validation
pub struct ValidationPool {
    semaphore: Arc<tokio::sync::Semaphore>,
}

impl ValidationPool {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(tokio::sync::Semaphore::new(max_concurrent)),
        }
    }

    /// Spawn async validation task
    pub fn validate_async(
        &self,
        validation: FileValidation,
        file_path: PathBuf,
        url: String,
        request: DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> ValidationHandle {
        let semaphore = self.semaphore.clone();
        let path_clone = file_path.clone();

        let task_handle = tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();
            validation.validate_file_blocking(&path_clone, progress_callback).await
        });

        ValidationHandle {
            file_path,
            task_handle,
            url,
            request,
        }
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
    /// Maximum number of concurrent validation tasks
    pub max_concurrent_validations: usize,
    /// Whether to validate files asynchronously (non-blocking)
    pub async_validation: bool,
    /// Number of retry attempts for failed validations
    pub validation_retries: usize,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            timeout: Duration::from_secs(30),
            user_agent: "installer/0.1.0".to_string(),
            allow_resume: true,
            chunk_size: 8192,
            max_concurrent_validations: 4,
            async_validation: true,
            validation_retries: 2,
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

/// Handle to track async validation
#[derive(Debug)]
pub struct ValidationHandle {
    pub file_path: PathBuf,
    pub task_handle: tokio::task::JoinHandle<Result<bool>>,
    pub url: String,
    pub request: DownloadRequest,
}

/// Result of a download operation
#[derive(Debug)]
pub enum DownloadResult {
    Downloaded { size: u64 },
    AlreadyExists { size: u64 },
    Resumed { size: u64 },
    /// File downloaded but validation is still in progress
    DownloadedPendingValidation {
        size: u64,
        validation_handle: ValidationHandle,
    },
}

/// Intermediate result for batch operations with validation
#[derive(Debug)]
pub enum BatchDownloadResult {
    Completed(Result<DownloadResult>),
    PendingValidation {
        size: u64,
        validation_handle: ValidationHandle,
        original_index: usize,
    },
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
            if request.validation.is_empty() {
                // No validation needed, file exists
                let size = fs::metadata(&dest_path).await?.len();
                return Ok(DownloadResult::AlreadyExists { size });
            } else if request.validation.validate_file(&dest_path, progress_callback.clone()).await? {
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

        // Validate the downloaded file (only if validation is specified)
        if !request.validation.is_empty() {
            if !request.validation.validate_file(&dest_path, progress_callback).await? {
                fs::remove_file(&dest_path).await?;
                return Err(DownloadError::ValidationError {
                    expected: "valid file".to_string(),
                    actual: "invalid file".to_string(),
                });
            }
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
    validation_pool: ValidationPool,
}

impl EnhancedDownloader {
    pub fn new(config: DownloadConfig) -> Self {
        let validation_pool = ValidationPool::new(config.max_concurrent_validations);
        let registry = DownloaderRegistry::new()
            .with_http_downloader(config.clone());

        Self { registry, config, validation_pool }
    }

    pub fn with_registry(registry: DownloaderRegistry, config: DownloadConfig) -> Self {
        let validation_pool = ValidationPool::new(config.max_concurrent_validations);
        Self { registry, config, validation_pool }
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

    /// Download with async validation option
    pub async fn download_with_async_validation(
        &self,
        request: DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult> {
        let filename = request.get_filename()?;
        let dest_path = request.destination.join(&filename);

        // Check if file already exists and is valid (still synchronous)
        if dest_path.exists() {
            if request.validation.is_empty() {
                // No validation needed, file exists
                let size = fs::metadata(&dest_path).await?.len();
                return Ok(DownloadResult::AlreadyExists { size });
            } else if request.validation.validate_file(&dest_path, progress_callback.clone()).await? {
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

        // Download the file without validation
        let size = self.download_file_only(&request, progress_callback.clone()).await?;

        // Start async validation if configured and needed
        if self.config.async_validation && !request.validation.is_empty() {
            let validation_handle = self.validation_pool.validate_async(
                request.validation.clone(),
                dest_path,
                request.url.clone(),
                request.clone(),
                progress_callback,
            );

            Ok(DownloadResult::DownloadedPendingValidation {
                size,
                validation_handle,
            })
        } else if !request.validation.is_empty() {
            // Synchronous validation (existing behavior)
            if !request.validation.validate_file(&dest_path, progress_callback).await? {
                fs::remove_file(&dest_path).await?;
                return Err(DownloadError::ValidationError {
                    expected: "valid file".to_string(),
                    actual: "invalid file".to_string(),
                });
            }
            Ok(DownloadResult::Downloaded { size })
        } else {
            // No validation needed
            Ok(DownloadResult::Downloaded { size })
        }
    }

    /// Download file without validation (helper method)
    async fn download_file_only(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<u64> {
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

            // Create a minimal request for just downloading
            let download_request = DownloadRequest {
                validation: FileValidation::new(), // No validation
                ..request.clone()
            };

            match self.registry.attempt_download(&download_request, progress_callback.clone()).await {
                Ok(DownloadResult::Downloaded { size }) => return Ok(size),
                Ok(DownloadResult::Resumed { size }) => return Ok(size),
                Ok(DownloadResult::AlreadyExists { size }) => return Ok(size),
                Ok(DownloadResult::DownloadedPendingValidation { .. }) => {
                    // This shouldn't happen since we disabled validation
                    return Err(DownloadError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Unexpected pending validation state",
                    )));
                }
                Err(e) => {
                    last_error = Some(e);
                    if attempt < max_retries {
                        continue;
                    }
                }
            }
        }

        // All retries failed, try mirror if available
        if let Some(ref mirror_url) = request.mirror_url {
            info!("Primary download failed, trying mirror URL");

            if let Some(ref callback) = progress_callback {
                callback(ProgressEvent::RetryAttempt {
                    url: url.clone(),
                    attempt: 1,
                    max_attempts: 1,
                });
            }

            let mirror_request = DownloadRequest {
                url: mirror_url.clone(),
                mirror_url: None,
                validation: FileValidation::new(), // No validation
                ..request.clone()
            };

            match self.registry.attempt_download(&mirror_request, progress_callback.clone()).await {
                Ok(DownloadResult::Downloaded { size }) => return Ok(size),
                Ok(DownloadResult::Resumed { size }) => return Ok(size),
                Ok(DownloadResult::AlreadyExists { size }) => return Ok(size),
                Ok(DownloadResult::DownloadedPendingValidation { .. }) => {
                    return Err(DownloadError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Unexpected pending validation state",
                    )));
                }
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }

        // No mirror available or mirror failed, return error
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

    /// Download multiple files with async validation and validation retry
    pub async fn download_batch_with_async_validation(
        &self,
        requests: Vec<DownloadRequest>,
        progress_callback: Option<ProgressCallback>,
        max_concurrent_downloads: usize,
    ) -> Vec<Result<DownloadResult>> {
        use futures::stream::{self, StreamExt};
        use std::collections::VecDeque;

        // Initial downloads with intermediate results
        let intermediate_results = stream::iter(requests.into_iter().enumerate())
            .map(|(index, request)| {
                let progress_cb = progress_callback.clone();
                async move {
                    if self.config.async_validation {
                        match self.download_with_async_validation(request, progress_cb).await {
                            Ok(DownloadResult::DownloadedPendingValidation { size, validation_handle }) => {
                                BatchDownloadResult::PendingValidation {
                                    size,
                                    validation_handle,
                                    original_index: index,
                                }
                            }
                            other => BatchDownloadResult::Completed(other),
                        }
                    } else {
                        BatchDownloadResult::Completed(self.download(request, progress_cb).await)
                    }
                }
            })
            .buffer_unordered(max_concurrent_downloads)
            .collect::<Vec<_>>()
            .await;

        // Separate completed results from pending validations
        let total_requests = intermediate_results.len();
        let mut final_results: Vec<Option<Result<DownloadResult>>> = (0..total_requests).map(|_| None).collect();
        let mut pending_validations = Vec::new();
        let mut next_index = 0;

        for intermediate_result in intermediate_results {
            match intermediate_result {
                BatchDownloadResult::Completed(result) => {
                    // For completed results, we'll assign them sequentially
                    // since we've lost the original index mapping
                    while next_index < final_results.len() && final_results[next_index].is_some() {
                        next_index += 1;
                    }
                    if next_index < final_results.len() {
                        final_results[next_index] = Some(result);
                    } else {
                        final_results.push(Some(result));
                    }
                }
                BatchDownloadResult::PendingValidation { size, validation_handle, original_index } => {
                    pending_validations.push((original_index, size, validation_handle));
                }
            }
        }

        // Handle validation results and retries
        if self.config.async_validation && !pending_validations.is_empty() {
            let mut retry_queue: VecDeque<(usize, DownloadRequest)> = VecDeque::new();

            // Wait for all initial validations to complete
            for (original_index, size, validation_handle) in pending_validations {
                match validation_handle.task_handle.await {
                    Ok(Ok(true)) => {
                        // Validation passed
                        final_results[original_index] = Some(Ok(DownloadResult::Downloaded { size }));
                    }
                    Ok(Ok(false)) | Ok(Err(_)) => {
                        // Validation failed, queue for retry if retries are enabled
                        if self.config.validation_retries > 0 {
                            warn!("Validation failed for {}, queuing for retry", validation_handle.url);

                            // Remove the invalid file
                            if let Err(e) = fs::remove_file(&validation_handle.file_path).await {
                                warn!("Failed to remove invalid file {}: {}", validation_handle.file_path.display(), e);
                            }

                            retry_queue.push_back((original_index, validation_handle.request));
                        } else {
                            // No retries, mark as validation error
                            final_results[original_index] = Some(Err(DownloadError::ValidationError {
                                expected: "valid file".to_string(),
                                actual: "invalid file".to_string(),
                            }));
                        }
                    }
                    Err(e) => {
                        // Task panicked or was cancelled
                        warn!("Validation task failed for {}: {}", validation_handle.url, e);
                        if self.config.validation_retries > 0 {
                            retry_queue.push_back((original_index, validation_handle.request));
                        } else {
                            final_results[original_index] = Some(Err(DownloadError::IoError(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                format!("Validation task failed: {}", e),
                            ))));
                        }
                    }
                }
            }

            // Process retry queue
            let mut retry_attempts = 0;
            while !retry_queue.is_empty() && retry_attempts < self.config.validation_retries {
                retry_attempts += 1;
                info!("Starting validation retry attempt {} of {}", retry_attempts, self.config.validation_retries);

                let current_retries: Vec<_> = retry_queue.drain(..).collect();
                let retry_results = stream::iter(current_retries.iter().cloned())
                    .map(|(original_index, request)| {
                        let progress_cb = progress_callback.clone();
                        async move {
                            let result = self.download_with_async_validation(request, progress_cb).await;
                            (original_index, result)
                        }
                    })
                    .buffer_unordered(max_concurrent_downloads)
                    .collect::<Vec<_>>()
                    .await;

                // Process retry results
                let mut new_pending_validations = Vec::new();

                for (original_index, retry_result) in retry_results {
                    match retry_result {
                        Ok(DownloadResult::DownloadedPendingValidation { size, validation_handle }) => {
                            new_pending_validations.push((original_index, size, validation_handle));
                        }
                        Ok(success_result) => {
                            // Direct success
                            final_results[original_index] = Some(Ok(success_result));
                        }
                        Err(e) => {
                            final_results[original_index] = Some(Err(e));
                        }
                    }
                }

                // Wait for retry validations
                for (original_index, size, validation_handle) in new_pending_validations {
                    match validation_handle.task_handle.await {
                        Ok(Ok(true)) => {
                            // Retry validation passed
                            final_results[original_index] = Some(Ok(DownloadResult::Downloaded { size }));
                        }
                        Ok(Ok(false)) | Ok(Err(_)) => {
                            // Retry validation failed, queue for another retry if possible
                            if retry_attempts < self.config.validation_retries {
                                if let Err(e) = fs::remove_file(&validation_handle.file_path).await {
                                    warn!("Failed to remove invalid file after retry {}: {}", validation_handle.file_path.display(), e);
                                }
                                retry_queue.push_back((original_index, validation_handle.request));
                            } else {
                                final_results[original_index] = Some(Err(DownloadError::ValidationError {
                                    expected: "valid file".to_string(),
                                    actual: "invalid file after retries".to_string(),
                                }));
                            }
                        }
                        Err(e) => {
                            warn!("Retry validation task failed: {}", e);
                            final_results[original_index] = Some(Err(DownloadError::IoError(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                format!("Retry validation task failed: {}", e),
                            ))));
                        }
                    }
                }
            }

            // Mark any remaining failed retries
            for (original_index, failed_request) in retry_queue {
                warn!("Max validation retries exceeded for {}", failed_request.url);
                final_results[original_index] = Some(Err(DownloadError::ValidationError {
                    expected: "valid file".to_string(),
                    actual: "max validation retries exceeded".to_string(),
                }));
            }
        }

        // Convert Option<Result<DownloadResult>> to Vec<Result<DownloadResult>>
        // Fill any remaining None values with errors
        final_results.into_iter()
            .map(|opt_result| {
                opt_result.unwrap_or_else(|| Err(DownloadError::IoError(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Download result was not properly set".to_string(),
                ))))
            })
            .collect()
    }
}

impl Default for DownloaderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
