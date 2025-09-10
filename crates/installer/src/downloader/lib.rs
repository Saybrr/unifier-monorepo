//! Main entry point for the modular file downloader
//!
//! This module provides the primary `EnhancedDownloader` interface that users interact with.
//! The call chain flows as follows:
//!
//! User Code
//! ↓
//! EnhancedDownloader (this file)
//! ↓
//! batch:: functions (batch/mod.rs)
//! ↓
//! DownloaderRegistry (registry.rs)
//! ↓
//! HttpDownloader (backends/http.rs)
//! ↓
//! Core types (core/*)

use crate::downloader::{
    config::DownloadConfig,
    core::{DownloadRequest, DownloadResult, ProgressCallback, Result, ValidationPool},
    batch::{DownloadMetrics},
    registry::DownloaderRegistry,
};
use std::sync::Arc;

/// Enhanced downloader with retry capability and batch operations
///
/// This is the main entry point for users. It provides:
/// - Single file downloads with retry logic
/// - Batch downloads with concurrency control
/// - Async validation with automatic retries
/// - Built-in performance metrics
/// - Mirror URL fallback support
pub struct EnhancedDownloader {
    registry: DownloaderRegistry,
    config: DownloadConfig,
    validation_pool: ValidationPool,
    metrics: Arc<DownloadMetrics>,
}

impl EnhancedDownloader {
    /// Create a new downloader with default HTTP backend
    pub fn new(config: DownloadConfig) -> Self {
        let validation_pool = ValidationPool::new(config.max_concurrent_validations);
        let registry = DownloaderRegistry::new()
            .with_http_downloader(config.clone());
        let metrics = Arc::new(DownloadMetrics::default());

        Self { registry, config, validation_pool, metrics }
    }

    /// Create a downloader with a custom registry
    pub fn with_registry(registry: DownloaderRegistry, config: DownloadConfig) -> Self {
        let validation_pool = ValidationPool::new(config.max_concurrent_validations);
        let metrics = Arc::new(DownloadMetrics::default());
        Self { registry, config, validation_pool, metrics }
    }

    /// Get access to built-in performance metrics
    pub fn metrics(&self) -> &DownloadMetrics {
        &self.metrics
    }

    /// Download a single file with retry logic and mirror fallback
    ///
    /// This method delegates to batch::download_with_retry for the actual implementation
    pub async fn download(
        &self,
        request: DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult> {
        crate::downloader::batch::download_with_retry(
            &self.registry,
            &self.config,
            &self.metrics,
            request,
            progress_callback,
        ).await
    }

    /// Download a file with async validation option
    ///
    /// This allows validation to run in the background while other downloads continue
    pub async fn download_with_async_validation(
        &self,
        request: DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult> {
        crate::downloader::batch::download_with_async_validation(
            &self.registry,
            &self.config,
            &self.validation_pool,
            &self.metrics,
            request,
            progress_callback,
        ).await
    }

    /// Download multiple files concurrently
    ///
    /// This method provides basic batch downloading with concurrency control
    pub async fn download_batch(
        &self,
        requests: Vec<DownloadRequest>,
        progress_callback: Option<ProgressCallback>,
        max_concurrent: usize,
    ) -> Vec<Result<DownloadResult>> {
        crate::downloader::batch::download_batch(
            &self.registry,
            &self.config,
            &self.metrics,
            requests,
            progress_callback,
            max_concurrent,
        ).await
    }

    /// Download multiple files with async validation and validation retry
    ///
    /// This is the most advanced batch download method, providing:
    /// - Concurrent downloads
    /// - Background async validation
    /// - Automatic retry of failed validations
    /// - Built-in performance metrics
    pub async fn download_batch_with_async_validation(
        &self,
        requests: Vec<DownloadRequest>,
        progress_callback: Option<ProgressCallback>,
        max_concurrent_downloads: usize,
    ) -> Vec<Result<DownloadResult>> {
        crate::downloader::batch::download_batch_with_async_validation(
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
