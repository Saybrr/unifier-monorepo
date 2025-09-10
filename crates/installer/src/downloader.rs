//! Modular file downloader with validation and retry capabilities
//!
//! This module provides a comprehensive file downloading system with:
//! - Multiple download source support (HTTP, Google Drive, etc.)
//! - File integrity validation (CRC32, MD5, SHA256)
//! - Retry logic with exponential backoff
//! - Progress tracking
//! - Resume capability

pub mod config;
pub mod error;
pub mod validation;
pub mod http;
pub mod progress;
pub mod batch;

// Re-export main types for backward compatibility
pub use config::{DownloadConfig, DownloadConfigBuilder};
pub use error::{DownloadError, Result, ErrorSeverity, FileOperation, ValidationType, ErrorContext};
pub use validation::{FileValidation, ValidationHandle, ValidationPool};
pub use http::HttpDownloader;
pub use progress::{ProgressEvent, ProgressCallback, ProgressReporter, IntoProgressCallback, ConsoleProgressReporter, NullProgressReporter, CompositeProgressReporter};
pub use batch::{BatchDownloadResult, DownloadMetrics, DownloadMetricsSnapshot};

use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

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

    pub(crate) fn get_filename(&self) -> Result<String> {
        if let Some(ref filename) = self.filename {
            return Ok(filename.clone());
        }

        let url = url::Url::parse(&self.url)?;
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

/// Trait for different download implementations
#[async_trait]
pub trait FileDownloader: Send + Sync {
    async fn download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult>;

    /// Download file without any validation - pure download logic
    async fn download_helper(
        &self,
        url: &str,
        dest_path: &std::path::Path,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<u64>;

    /// Check if file exists and handle validation if needed
    async fn check_existing_file(
        &self,
        dest_path: &std::path::Path,
        validation: &FileValidation,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<Option<DownloadResult>>;

    fn supports_url(&self, url: &str) -> bool;
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

    pub async fn find_downloader(&self, url: &str) -> Result<&dyn FileDownloader> {
        self.downloaders
            .iter()
            .find(|d| d.supports_url(url))
            .map(|d| d.as_ref())
            .ok_or_else(|| {
                let parsed_url = url::Url::parse(url);
                let scheme = parsed_url.map(|u| u.scheme().to_string()).unwrap_or_else(|_| "unknown".to_string());
                DownloadError::UnsupportedUrl {
                    url: url.to_string(),
                    scheme,
                    supported_schemes: "http, https".to_string(),
                }
            })
    }

    pub async fn attempt_download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult> {
        let downloader = self.find_downloader(&request.url).await?;
        downloader.download(request, progress_callback).await
    }

}

impl Default for DownloaderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Enhanced downloader with retry capability
pub struct EnhancedDownloader {
    registry: DownloaderRegistry,
    config: DownloadConfig,
    validation_pool: ValidationPool,
    metrics: Arc<DownloadMetrics>,
}

impl EnhancedDownloader {
    pub fn new(config: DownloadConfig) -> Self {
        let validation_pool = ValidationPool::new(config.max_concurrent_validations);
        let registry = DownloaderRegistry::new()
            .with_http_downloader(config.clone());
        let metrics = Arc::new(DownloadMetrics::default());

        Self { registry, config, validation_pool, metrics }
    }

    pub fn with_registry(registry: DownloaderRegistry, config: DownloadConfig) -> Self {
        let validation_pool = ValidationPool::new(config.max_concurrent_validations);
        let metrics = Arc::new(DownloadMetrics::default());
        Self { registry, config, validation_pool, metrics }
    }

    pub fn metrics(&self) -> &DownloadMetrics {
        &self.metrics
    }

    /// Download a file with retry logic and mirror fallback
    pub async fn download(
        &self,
        request: DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult> {
        batch::download_with_retry(
            &self.registry,
            &self.config,
            &self.metrics,
            request,
            progress_callback,
        ).await
    }

    /// Download with async validation option
    pub async fn download_with_async_validation(
        &self,
        request: DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult> {
        batch::download_with_async_validation(
            &self.registry,
            &self.config,
            &self.validation_pool,
            &self.metrics,
            request,
            progress_callback,
        ).await
    }

    /// Download multiple files concurrently
    pub async fn download_batch(
        &self,
        requests: Vec<DownloadRequest>,
        progress_callback: Option<ProgressCallback>,
        max_concurrent: usize,
    ) -> Vec<Result<DownloadResult>> {
        batch::download_batch(
            &self.registry,
            &self.config,
            &self.metrics,
            requests,
            progress_callback,
            max_concurrent,
        ).await
    }

    /// Download multiple files with async validation and validation retry
    pub async fn download_batch_with_async_validation(
        &self,
        requests: Vec<DownloadRequest>,
        progress_callback: Option<ProgressCallback>,
        max_concurrent_downloads: usize,
    ) -> Vec<Result<DownloadResult>> {
        batch::download_batch_with_async_validation(
            &self.registry,
            &self.config,
            &self.validation_pool,
            &self.metrics,
            requests,
            progress_callback,
            max_concurrent_downloads,
        ).await
    }
}

#[cfg(test)]
mod tests;

