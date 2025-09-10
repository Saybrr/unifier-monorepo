//! File validation with optimized memory usage and parallel computation

use crate::downloader::{DownloadRequest, error::{DownloadError, Result}, progress::ProgressCallback};
use crc32fast::Hasher as Crc32Hasher;
use digest::Digest;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::fs;
use tokio::io::AsyncReadExt;
use tracing::debug;

// Buffer pool for efficient memory reuse
static BUFFER_POOL: Lazy<Mutex<Vec<Vec<u8>>>> = Lazy::new(|| Mutex::new(Vec::new()));

fn get_buffer(size: usize) -> Vec<u8> {
    let mut pool = BUFFER_POOL.lock().unwrap();
    pool.pop()
        .filter(|buf| buf.capacity() >= size)
        .unwrap_or_else(|| vec![0u8; size])
}

fn return_buffer(mut buf: Vec<u8>) {
    buf.clear();
    // Only keep reasonably sized buffers in the pool
    if buf.capacity() <= 65536 && buf.capacity() >= 4096 {
        let mut pool = BUFFER_POOL.lock().unwrap();
        if pool.len() < 10 { // Limit pool size
            pool.push(buf);
        }
    }
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

    pub fn with_md5<S: Into<String>>(mut self, md5: S) -> Self {
        self.md5 = Some(md5.into().to_lowercase());
        self
    }

    pub fn with_sha256<S: Into<String>>(mut self, sha256: S) -> Self {
        self.sha256 = Some(sha256.into().to_lowercase());
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
            callback(crate::downloader::progress::ProgressEvent::ValidationStarted {
                file: path.display().to_string(),
            });
        }

        // Check file size first (fastest check)
        if let Some(expected_size) = self.expected_size {
            if file_size != expected_size {
                if let Some(ref callback) = progress_callback {
                    callback(crate::downloader::progress::ProgressEvent::ValidationComplete {
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

    /// Validate small files in memory with parallel hash computation
    async fn validate_file_in_memory<P: AsRef<Path>>(
        &self,
        path: P,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<bool> {
        let path = path.as_ref();
        let file_data = fs::read(path).await?;

        debug!("Using in-memory validation for {} bytes", file_data.len());

        // Compute hashes in parallel if we have multiple algorithms to run
        let mut tasks = Vec::new();

        // Spawn parallel tasks for each hash algorithm
        if self.crc32.is_some() {
            let data = file_data.clone();
            tasks.push(tokio::task::spawn_blocking(move || {
                ("crc32", crc32fast::hash(&data).to_string())
            }));
        }

        if let Some(ref _expected_md5) = self.md5 {
            let data = file_data.clone();
            tasks.push(tokio::task::spawn_blocking(move || {
                let actual = format!("{:x}", md5::compute(&data));
                ("md5", actual)
            }));
        }

        if let Some(ref _expected_sha256) = self.sha256 {
            let data = file_data.clone();
            tasks.push(tokio::task::spawn_blocking(move || {
                let actual = format!("{:x}", sha2::Sha256::digest(&data));
                ("sha256", actual)
            }));
        }

        // Wait for all tasks and validate results
        for task in tasks {
            let (hash_type, actual_hash) = task.await.map_err(|e| {
                DownloadError::ValidationTaskFailed {
                    file: path.to_path_buf(),
                    reason: format!("Hash computation failed: {}", e),
                    source: Some(Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
                }
            })?;

            let validation_passed = match hash_type {
                "crc32" => {
                    if let Some(expected_crc32) = self.crc32 {
                        actual_hash == expected_crc32.to_string()
                    } else {
                        true
                    }
                }
                "md5" => {
                    if let Some(ref expected_md5) = self.md5 {
                        &actual_hash == expected_md5
                    } else {
                        true
                    }
                }
                "sha256" => {
                    if let Some(ref expected_sha256) = self.sha256 {
                        &actual_hash == expected_sha256
                    } else {
                        true
                    }
                }
                _ => true,
            };

            if !validation_passed {
                self.report_validation_complete(path, false, progress_callback);
                return Ok(false);
            }
        }

        self.report_validation_complete(path, true, progress_callback);
        Ok(true)
    }

    /// Validate large files using streaming with optimized hash creation
    async fn validate_file_streaming<P: AsRef<Path>>(
        &self,
        path: P,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<bool> {
        let path = path.as_ref();
        let file_size = fs::metadata(path).await?.len();

        debug!("Using streaming validation for {} bytes", file_size);

        // Only create hashers we actually need
        let mut crc32_hasher = self.crc32.map(|_| Crc32Hasher::new());
        let mut md5_hasher = self.md5.as_ref().map(|_| md5::Context::new());
        let mut sha256_hasher = self.sha256.as_ref().map(|_| Sha256::new());

        let mut file = fs::File::open(path).await?;
        let buffer_size = 8192.min(file_size as usize);
        let mut buffer = get_buffer(buffer_size);
        let mut bytes_read_total = 0u64;

        loop {
            let bytes_read = file.read(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }

            let chunk = &buffer[..bytes_read];

            // Update only the hashers we have
            if let Some(ref mut hasher) = crc32_hasher {
                hasher.update(chunk);
            }
            if let Some(ref mut hasher) = md5_hasher {
                hasher.consume(chunk);
            }
            if let Some(ref mut hasher) = sha256_hasher {
                hasher.update(chunk);
            }

            bytes_read_total += bytes_read as u64;

            if let Some(ref callback) = progress_callback {
                let progress = bytes_read_total as f64 / file_size as f64;
                callback(crate::downloader::progress::ProgressEvent::ValidationProgress {
                    file: path.display().to_string(),
                    progress,
                });
            }
        }

        return_buffer(buffer);

        // Validate results
        if let (Some(expected_crc32), Some(hasher)) = (self.crc32, crc32_hasher) {
            let actual_crc32 = hasher.finalize();
            if actual_crc32 != expected_crc32 {
                self.report_validation_complete(path, false, progress_callback);
                return Ok(false);
            }
        }

        if let (Some(expected_md5), Some(hasher)) = (self.md5.as_ref(), md5_hasher) {
            let actual_md5 = format!("{:x}", hasher.compute());
            if &actual_md5 != expected_md5 {
                self.report_validation_complete(path, false, progress_callback);
                return Ok(false);
            }
        }

        if let (Some(expected_sha256), Some(hasher)) = (self.sha256.as_ref(), sha256_hasher) {
            let actual_sha256 = format!("{:x}", hasher.finalize());
            if &actual_sha256 != expected_sha256 {
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
            callback(crate::downloader::progress::ProgressEvent::ValidationComplete {
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
