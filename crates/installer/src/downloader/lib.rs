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
    core::{DownloadRequest, DownloadResult, ProgressCallback, Result, ValidationPool, DownloadSource, DownloadConfig, DownloadMetrics, ValidationResult, VerifiedDownloadResult, DownloadError, ValidationType},
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

    /// Centralized validation function that validates a download result
    pub async fn validate_download_result(
        &self,
        result: DownloadResult,
        validation: &crate::downloader::core::FileValidation,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<VerifiedDownloadResult> {
        use tokio::fs;

        let validation_result = match &result {
            DownloadResult::Downloaded { file_path, .. } |
            DownloadResult::Resumed { file_path, .. } => {
                // Only validate if validation is configured
                if validation.xxhash64_base64.is_some() || validation.expected_size.is_some() {
                    match validation.validate_file(file_path, progress_callback).await {
                        Ok(true) => ValidationResult::Valid,
                        Ok(false) => {
                            // This shouldn't happen as validate_file returns Err for failures
                            let _ = fs::remove_file(file_path).await;
                            ValidationResult::Invalid(DownloadError::ValidationFailed {
                                file: file_path.clone(),
                                validation_type: ValidationType::Size,
                                expected: "valid file".to_string(),
                                actual: "invalid file".to_string(),
                                suggestion: "Check file integrity or download again".to_string(),
                            })
                        },
                        Err(e) => {
                            // Clean up invalid file and return the specific error
                            let _ = fs::remove_file(file_path).await;
                            ValidationResult::Invalid(e)
                        }
                    }
                } else {
                    ValidationResult::Skipped
                }
            },
            DownloadResult::AlreadyExists { validated: true, .. } => {
                ValidationResult::AlreadyValidated
            },
            DownloadResult::AlreadyExists { file_path, validated: false, .. } => {
                // Need to validate existing file
                if validation.xxhash64_base64.is_some() || validation.expected_size.is_some() {
                    match validation.validate_file(file_path, progress_callback).await {
                        Ok(true) => ValidationResult::Valid,
                        Ok(false) => {
                            ValidationResult::Invalid(DownloadError::ValidationFailed {
                                file: file_path.clone(),
                                validation_type: ValidationType::Size,
                                expected: "valid file".to_string(),
                                actual: "invalid file".to_string(),
                                suggestion: "Check file integrity or download again".to_string(),
                            })
                        },
                        Err(e) => ValidationResult::Invalid(e)
                    }
                } else {
                    ValidationResult::Skipped
                }
            },
            DownloadResult::DownloadedPendingValidation { .. } => {
                // This case is handled by the existing async validation system
                ValidationResult::Skipped
            },
            DownloadResult::Skipped { .. } => ValidationResult::Skipped,
        };

        Ok(VerifiedDownloadResult {
            download_result: result,
            validation_result,
        })
    }

    /// Download a single file with retry logic and centralized validation
    ///
    /// With the new enum-based architecture, each source handles its own download logic,
    /// and validation is centralized here.
    pub async fn download(
        &self,
        request: DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<VerifiedDownloadResult> {
        // 1. Download the file (no validation in sources)
        let download_result = dispatch_download(&request.source, &request, progress_callback.clone(), &self.config).await?;

        // 2. Centralized validation
        self.validate_download_result(download_result, &request.validation, progress_callback).await
    }

    /// Legacy download method that returns DownloadResult (for backward compatibility)
    #[deprecated(note = "Use download() which returns VerifiedDownloadResult")]
    pub async fn download_legacy(
        &self,
        request: DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult> {
        let verified = self.download(request, progress_callback).await?;
        match verified.validation_result {
            ValidationResult::Valid | ValidationResult::AlreadyValidated | ValidationResult::Skipped => {
                Ok(verified.download_result)
            },
            ValidationResult::Invalid(e) => Err(e),
        }
    }

    /// Download a file with async validation option
    ///
    /// This now uses the centralized validation system
    pub async fn download_with_async_validation(
        &self,
        request: DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<VerifiedDownloadResult> {
        // Use the centralized validation system
        self.download(request, progress_callback).await
    }

    /// Download multiple files concurrently with centralized validation
    ///
    /// This method provides basic batch downloading with concurrency control
    /// and centralized validation for all files
    pub async fn download_batch(
        &self,
        requests: &Vec<DownloadRequest>,
        progress_callback: Option<ProgressCallback>,
        max_concurrent: usize,
    ) -> Vec<Result<VerifiedDownloadResult>> {
        use futures::stream::{FuturesUnordered, StreamExt};
        use tokio::sync::Semaphore;
        use std::sync::Arc;

        let semaphore = Arc::new(Semaphore::new(max_concurrent));
        let mut futures = FuturesUnordered::new();

        for request in requests {
            let semaphore = Arc::clone(&semaphore);
            let config = self.config.clone();
            let progress_callback = progress_callback.clone();
            let validation = request.validation.clone();

            futures.push(async move {
                let _permit = semaphore.acquire().await.unwrap();

                // 1. Download without validation
                let download_result = dispatch_download(&request.source, &request, progress_callback.clone(), &config).await?;

                // 2. Centralized validation
                self.validate_download_result(download_result, &validation, progress_callback).await
            });
        }

        let mut results = Vec::new();
        while let Some(result) = futures.next().await {
            results.push(result);
        }

        results
    }
}
