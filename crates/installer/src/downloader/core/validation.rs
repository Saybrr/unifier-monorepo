//! File validation with optimized memory usage and parallel computation

use crate::downloader::core::{DownloadRequest, error::{DownloadError, Result}};
use crate::downloader::core::progress::ProgressCallback;
use base64;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::fs;
use tokio::io::AsyncReadExt;
use tracing::debug;
use xxhash_rust::xxh64::Xxh64;

/// Convert xxHash64 u64 to base64 format (matching Wabbajack format)
fn xxhash64_to_base64(hash: u64) -> String {
    // Convert u64 to bytes in little-endian format (matching Wabbajack)
    let bytes = hash.to_le_bytes();
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes)
}

// Buffer pool for efficient memory reuse (fixed to prevent dirty buffer issues)
static BUFFER_POOL: Lazy<Mutex<Vec<Vec<u8>>>> = Lazy::new(|| Mutex::new(Vec::new()));

fn get_buffer(size: usize) -> Vec<u8> {
    let mut pool = BUFFER_POOL.lock().unwrap();

    // Try to get a buffer from the pool
    if let Some(mut buf) = pool.pop().filter(|buf| buf.capacity() >= size) {
        // Ensure the buffer is properly sized and zeroed
        buf.clear();
        buf.resize(size, 0); // This actually zeros the memory
        buf
    } else {
        // Create a new buffer if none suitable in pool
        vec![0u8; size]
    }
}

fn return_buffer(mut buf: Vec<u8>) {
    // Only keep reasonably sized buffers in the pool
    if buf.capacity() <= 65536 && buf.capacity() >= 4096 {
        let mut pool = BUFFER_POOL.lock().unwrap();
        if pool.len() < 10 { // Limit pool size
            buf.clear(); // Reset length to 0
            buf.shrink_to_fit(); // Free excess capacity if any
            pool.push(buf);
        }
    }
}

/// File validation configuration - xxHash64 only
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileValidation {
    /// Expected xxHash64 hash in base64 format (matching Wabbajack format)
    pub xxhash64_base64: Option<String>,
    /// Expected file size in bytes
    pub expected_size: Option<u64>,
}

impl Default for FileValidation {
    fn default() -> Self {
        Self {
            xxhash64_base64: None,
            expected_size: None,
        }
    }
}

impl FileValidation {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the expected xxHash64 in base64 format (matching Wabbajack format)
    pub fn with_xxhash64_base64<S: Into<String>>(mut self, hash: S) -> Self {
        self.xxhash64_base64 = Some(hash.into());
        self
    }

    pub fn with_expected_size(mut self, size: u64) -> Self {
        self.expected_size = Some(size);
        self
    }

    /// Check if validation is needed
    pub fn is_empty(&self) -> bool {
        self.xxhash64_base64.is_none() && self.expected_size.is_none()
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
            callback(crate::downloader::core::progress::ProgressEvent::ValidationStarted {
                file: path.display().to_string(),
                validation: self.clone(),
            });
        }

        // Check file size first (fastest check)
        if let Some(expected_size) = self.expected_size {
            if file_size != expected_size {
                if let Some(ref callback) = progress_callback {
                    callback(crate::downloader::core::progress::ProgressEvent::ValidationComplete {
                        file: path.display().to_string(),
                        valid: false,
                    });
                }
                return Err(DownloadError::SizeMismatch {
                    file: path.to_path_buf(),
                    expected: expected_size,
                    actual: file_size,
                    diff: (file_size as i64) - (expected_size as i64),
                });
            }
        }

        // For small files (< streaming threshold), use parallel in-memory validation
        const DEFAULT_STREAMING_THRESHOLD: u64 = 50_000_000; // 50MB
        if file_size < DEFAULT_STREAMING_THRESHOLD {
            self.validate_file_in_memory(path, progress_callback).await
        } else {
            self.validate_file_streaming(path, progress_callback).await
        }
    }

    /// Validate small files in memory using xxHash64
    async fn validate_file_in_memory<P: AsRef<Path>>(
        &self,
        path: P,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<bool> {
        let path = path.as_ref();
        let file_data = fs::read(path).await?;

        debug!("Using in-memory validation for {} bytes", file_data.len());

        // Compute xxHash64 if we have an expected hash
        if let Some(ref expected_hash_base64) = self.xxhash64_base64 {
            let actual_hash = tokio::task::spawn_blocking(move || {
                // Use the same incremental API as streaming for consistency
                let mut hasher = Xxh64::new(0);
                hasher.update(&file_data);
                let hash = hasher.digest();
                xxhash64_to_base64(hash)
            }).await.map_err(|e| {
                DownloadError::ValidationTaskFailed {
                    file: path.to_path_buf(),
                    reason: format!("Hash computation failed: {}", e),
                    source: Some(Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
                }
            })?;

            let validation_passed = &actual_hash == expected_hash_base64;
            debug!("XXHash64 in-memory validation: expected={}, actual={}, passed={}",
                   expected_hash_base64, actual_hash, validation_passed);

            if !validation_passed {
                debug!("Hash validation failed for {}: xxhash64 mismatch", path.display());
                self.report_validation_complete(path, false, progress_callback);
                return Ok(false);
            }
        }

        self.report_validation_complete(path, true, progress_callback);
        Ok(true)
    }

    /// Validate large files using streaming xxHash64
    async fn validate_file_streaming<P: AsRef<Path>>(
        &self,
        path: P,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<bool> {
        let path = path.as_ref();
        let file_size = fs::metadata(path).await?.len();

        debug!("Using streaming validation for {} bytes", file_size);

        // Only create xxhash64 hasher if we need it
        let mut xxhash64_hasher = self.xxhash64_base64.as_ref().map(|_| {
            debug!("Creating new xxHash64 hasher for streaming validation");
            Xxh64::new(0)
        });

        let mut file = fs::File::open(path).await?;
        let buffer_size = (64 * 1024).min(file_size as usize); // Use 64KB buffer, more reasonable than 8KB
        let mut buffer = get_buffer(buffer_size); // Use fixed buffer pool
        let mut bytes_read_total = 0u64;

        loop {
            let bytes_read = file.read(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }

            let chunk = &buffer[..bytes_read];

            // Update the xxhash64 hasher if we have one
            if let Some(ref mut hasher) = xxhash64_hasher {
                // Safety check: ensure we're only processing the bytes we actually read
                debug_assert_eq!(chunk.len(), bytes_read);
                hasher.update(chunk);
            }

            bytes_read_total += bytes_read as u64;

            if let Some(ref callback) = progress_callback {
                let progress = bytes_read_total as f64 / file_size as f64;
                callback(crate::downloader::core::progress::ProgressEvent::ValidationProgress {
                    file: path.display().to_string(),
                    progress,
                });
            }
        }

        return_buffer(buffer);

        // Validate xxHash64 result
        if let (Some(expected_hash_base64), Some(hasher)) = (self.xxhash64_base64.as_ref(), xxhash64_hasher) {
            let actual_hash_u64 = hasher.digest();
            debug!("Streaming: Raw hash u64={}, bytes processed={}", actual_hash_u64, bytes_read_total);
            let actual_hash_base64 = xxhash64_to_base64(actual_hash_u64);
            let passed = &actual_hash_base64 == expected_hash_base64;
            debug!("XXHash64 streaming validation: expected={}, actual={}, passed={}",
                   expected_hash_base64, actual_hash_base64, passed);
            if !passed {
                debug!("XXHash64 streaming validation failed for {}", path.display());
                self.report_validation_complete(path, false, progress_callback);
                return Ok(false);
            }
        }

        self.report_validation_complete(path, true, progress_callback);
        Ok(true)
    }

    /// Helper to report validation completion
    fn report_validation_complete<P: AsRef<Path>>(
        &self,
        path: P,
        valid: bool,
        progress_callback: Option<ProgressCallback>,
    ) {
        if let Some(ref callback) = progress_callback {
            callback(crate::downloader::core::progress::ProgressEvent::ValidationComplete {
                file: path.as_ref().display().to_string(),
                valid,
            });
        }
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
        let path_clone = path.clone();
        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(validation.validate_file(&path, callback))
        })
        .await
        .map_err(|e| DownloadError::ValidationTaskFailed {
            file: path_clone,
            reason: format!("Validation task failed: {}", e),
            source: Some(Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
        })?
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
