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
    core::{DownloadRequest, DownloadResult, ProgressCallback, Result, ValidationPool, DownloadSource, DownloadConfig, DownloadMetrics},
};
use std::sync::Arc;

/// Dispatch function to handle different download source types
async fn dispatch_download(
    source: &DownloadSource,
    request: &DownloadRequest,
    progress_callback: Option<ProgressCallback>,
    config: &DownloadConfig,
) -> Result<DownloadResult> {
    match source {
        DownloadSource::Http(http_source) => {
            http_source.download(request, progress_callback, config).await
        },
        DownloadSource::WabbajackCDN(cdn_source) => {
            cdn_source.download(request, progress_callback, config).await
        },
        DownloadSource::GameFile(gamefile_source) => {
            gamefile_source.download(request, progress_callback, config).await
        },
        DownloadSource::Nexus(nexus_source) => {
            nexus_source.download(request, progress_callback, config).await
        },
        DownloadSource::Manual(manual_source) => {
            manual_source.download(request, progress_callback, config).await
        },
        DownloadSource::Archive(archive_source) => {
            archive_source.download(request, progress_callback, config).await
        },
        DownloadSource::Unknown(unknown_source) => {
            unknown_source.download(request, progress_callback, config).await
        },
    }
}

/// Enhanced downloader with retry capability and batch operations
///
/// This is the main entry point for users. It provides:
/// - Single file downloads with retry logic
/// - Batch downloads with concurrency control
/// - Async validation with automatic retries
/// - Built-in performance metrics
/// - Mirror URL fallback support
pub struct Downloader {
    config: DownloadConfig,
    _validation_pool: ValidationPool,
    metrics: Arc<DownloadMetrics>,
}

impl Downloader {
    /// Create a new downloader
    pub fn new(config: DownloadConfig) -> Self {
        let validation_pool = ValidationPool::new(config.max_concurrent_validations);
        let metrics = Arc::new(DownloadMetrics::default());

        Self { config, _validation_pool: validation_pool, metrics }
    }

    /// Get access to built-in performance metrics
    pub fn metrics(&self) -> &DownloadMetrics {
        &self.metrics
    }

    /// Download a single file with retry logic and mirror fallback
    ///
    /// With the new enum-based architecture, each source handles its own download logic
    pub async fn download(
        &self,
        request: DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult> {
        // Dispatch to the appropriate download implementation
        dispatch_download(&request.source, &request, progress_callback, &self.config).await
    }

    /// Download a file with async validation option
    ///
    /// For now, this just calls the regular download method since validation
    /// is handled within each source's download implementation
    pub async fn download_with_async_validation(
        &self,
        request: DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult> {
        // For simplicity, we'll just use the regular download method
        // Each source handles its own validation
        self.download(request, progress_callback).await
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
        use futures::stream::{FuturesUnordered, StreamExt};
        use tokio::sync::Semaphore;
        use std::sync::Arc;

        let semaphore = Arc::new(Semaphore::new(max_concurrent));
        let mut futures = FuturesUnordered::new();

        for request in requests {
            let semaphore = Arc::clone(&semaphore);
            let config = self.config.clone();
            let progress_callback = progress_callback.clone();

            futures.push(async move {
                let _permit = semaphore.acquire().await.unwrap();
                dispatch_download(&request.source, &request, progress_callback, &config).await
            });
        }

        let mut results = Vec::new();
        while let Some(result) = futures.next().await {
            results.push(result);
        }

        results
    }

    /// Download multiple files with async validation and validation retry
    ///
    /// For now, this is the same as download_batch since each source
    /// handles its own validation
    pub async fn download_batch_with_async_validation(
        &self,
        requests: Vec<DownloadRequest>,
        progress_callback: Option<ProgressCallback>,
        max_concurrent_downloads: usize,
    ) -> Vec<Result<DownloadResult>> {
        self.download_batch(requests, progress_callback, max_concurrent_downloads).await
    }
}
