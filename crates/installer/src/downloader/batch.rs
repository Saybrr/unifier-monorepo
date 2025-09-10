//! Batch download operations with validation and metrics

use crate::downloader::{
    DownloadRequest, DownloadResult, DownloaderRegistry,
    config::DownloadConfig,
    error::{DownloadError, Result},
    progress::{ProgressCallback, ProgressEvent},
    validation::{ValidationHandle, ValidationPool},
};
use futures::stream::{self, StreamExt};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::fs;
use tracing::{info, warn, debug};

/// Performance metrics for downloads
#[derive(Debug, Default)]
pub struct DownloadMetrics {
    pub total_bytes: AtomicU64,
    pub total_downloads: AtomicU64,
    pub successful_downloads: AtomicU64,
    pub failed_downloads: AtomicU64,
    pub validation_failures: AtomicU64,
    pub retries_attempted: AtomicU64,
    pub cache_hits: AtomicU64,
}

impl DownloadMetrics {
    pub fn record_download_started(&self) {
        self.total_downloads.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_download_completed(&self, size: u64) {
        self.successful_downloads.fetch_add(1, Ordering::Relaxed);
        self.total_bytes.fetch_add(size, Ordering::Relaxed);
    }

    pub fn record_download_failed(&self) {
        self.failed_downloads.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_validation_failed(&self) {
        self.validation_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_retry(&self) {
        self.retries_attempted.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_cache_hit(&self, size: u64) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
        self.total_bytes.fetch_add(size, Ordering::Relaxed);
    }

    /// Get metrics snapshot
    pub fn snapshot(&self) -> DownloadMetricsSnapshot {
        DownloadMetricsSnapshot {
            total_bytes: self.total_bytes.load(Ordering::Relaxed),
            total_downloads: self.total_downloads.load(Ordering::Relaxed),
            successful_downloads: self.successful_downloads.load(Ordering::Relaxed),
            failed_downloads: self.failed_downloads.load(Ordering::Relaxed),
            validation_failures: self.validation_failures.load(Ordering::Relaxed),
            retries_attempted: self.retries_attempted.load(Ordering::Relaxed),
            cache_hits: self.cache_hits.load(Ordering::Relaxed),
        }
    }
}

/// Immutable snapshot of metrics
#[derive(Debug, Clone)]
pub struct DownloadMetricsSnapshot {
    pub total_bytes: u64,
    pub total_downloads: u64,
    pub successful_downloads: u64,
    pub failed_downloads: u64,
    pub validation_failures: u64,
    pub retries_attempted: u64,
    pub cache_hits: u64,
}

impl DownloadMetricsSnapshot {
    pub fn success_rate(&self) -> f64 {
        if self.total_downloads == 0 {
            0.0
        } else {
            self.successful_downloads as f64 / self.total_downloads as f64
        }
    }

    pub fn average_size(&self) -> f64 {
        if self.successful_downloads == 0 {
            0.0
        } else {
            self.total_bytes as f64 / self.successful_downloads as f64
        }
    }
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

/// Download a file with retry logic and mirror fallback
pub async fn download_with_retry(
    registry: &DownloaderRegistry,
    config: &DownloadConfig,
    metrics: &DownloadMetrics,
    request: DownloadRequest,
    progress_callback: Option<ProgressCallback>,
) -> Result<DownloadResult> {
    metrics.record_download_started();

    let url = request.url.clone();
    let max_retries = config.max_retries;

    // Custom retry loop with progress feedback
    let mut last_error = None;
    for attempt in 0..=max_retries {
        if attempt > 0 {
            metrics.record_retry();

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

        match registry.attempt_download(&request, progress_callback.clone()).await {
            Ok(result) => {
                match &result {
                    DownloadResult::Downloaded { size } |
                    DownloadResult::Resumed { size } => {
                        metrics.record_download_completed(*size);
                    }
                    DownloadResult::AlreadyExists { size } => {
                        metrics.record_cache_hit(*size);
                    }
                    DownloadResult::DownloadedPendingValidation { size, .. } => {
                        metrics.record_download_completed(*size);
                    }
                }
                return Ok(result);
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
            ..request
        };

        match registry.attempt_download(&mirror_request, progress_callback.clone()).await {
            Ok(result) => {
                match &result {
                    DownloadResult::Downloaded { size } |
                    DownloadResult::Resumed { size } => {
                        metrics.record_download_completed(*size);
                    }
                    DownloadResult::AlreadyExists { size } => {
                        metrics.record_cache_hit(*size);
                    }
                    DownloadResult::DownloadedPendingValidation { size, .. } => {
                        metrics.record_download_completed(*size);
                    }
                }
                return Ok(result);
            }
            Err(e) => {
                last_error = Some(e);
            }
        }
    }

    // No mirror available or mirror failed, return error
    metrics.record_download_failed();

    if let Some(ref callback) = progress_callback {
        if let Some(ref error) = last_error {
            callback(ProgressEvent::Error {
                url,
                error: error.to_string(),
            });
        }
    }

    Err(DownloadError::MaxRetriesExceeded {
        url: request.url.clone(),
        max_retries,
        total_duration_secs: 0, // Duration not tracked in this function
        last_error: last_error.map_or("No specific error recorded".to_string(), |e| e.to_string()),
    })
}

/// Download with async validation option
pub async fn download_with_async_validation(
    registry: &DownloaderRegistry,
    config: &DownloadConfig,
    validation_pool: &ValidationPool,
    metrics: &DownloadMetrics,
    request: DownloadRequest,
    progress_callback: Option<ProgressCallback>,
) -> Result<DownloadResult> {
    let filename = request.get_filename()?;
    let dest_path = request.destination.join(&filename);

    // Check if file already exists and is valid
    let downloader = registry.find_downloader(&request.url).await?;
    if let Some(existing_result) = downloader.check_existing_file(
        &dest_path,
        &request.validation,
        progress_callback.clone()
    ).await? {
        let size = match &existing_result {
            DownloadResult::AlreadyExists { size } => *size,
            _ => 0, // This shouldn't happen with check_existing_file
        };
        metrics.record_cache_hit(size);
        return Ok(existing_result);
    }

    // Download the file without validation using pure download
    let size = download_file_with_retry(registry, config, metrics, &request, progress_callback.clone()).await?;

    // Start async validation if configured and needed
    if config.async_validation && !request.validation.is_empty() {
        let validation_handle = validation_pool.validate_async(
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
            metrics.record_validation_failed();
            fs::remove_file(&dest_path).await?;
            return Err(DownloadError::ValidationFailed {
                file: dest_path.clone(),
                validation_type: crate::downloader::error::ValidationType::Size, // Default validation type
                expected: "valid file".to_string(),
                actual: "invalid file".to_string(),
                suggestion: "Check file integrity or download again".to_string(),
            });
        }
        Ok(DownloadResult::Downloaded { size })
    } else {
        // No validation needed
        Ok(DownloadResult::Downloaded { size })
    }
}

/// Download file with retry logic - pure download without validation
async fn download_file_with_retry(
    registry: &DownloaderRegistry,
    config: &DownloadConfig,
    metrics: &DownloadMetrics,
    request: &DownloadRequest,
    progress_callback: Option<ProgressCallback>,
) -> Result<u64> {
    let url = request.url.clone();
    let max_retries = config.max_retries;
    let filename = request.get_filename()?;
    let dest_path = request.destination.join(&filename);

    // Custom retry loop with progress feedback
    let mut last_error = None;
    for attempt in 0..=max_retries {
        if attempt > 0 {
            metrics.record_retry();

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

        // Use pure download method - no validation, no request cloning
        let downloader = match registry.find_downloader(&url).await {
            Ok(d) => d,
            Err(e) => {
                last_error = Some(e);
                if attempt < max_retries {
                    continue;
                }
                break;
            }
        };
        match downloader.download_helper(&url, &dest_path, progress_callback.clone()).await {
            Ok(size) => {
                metrics.record_download_completed(size);
                return Ok(size);
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

        match registry.find_downloader(mirror_url).await {
            Ok(mirror_downloader) => {
                match mirror_downloader.download_helper(mirror_url, &dest_path, progress_callback.clone()).await {
                    Ok(size) => {
                        metrics.record_download_completed(size);
                        return Ok(size);
                    }
                    Err(e) => {
                        last_error = Some(e);
                    }
                }
            }
            Err(e) => {
                last_error = Some(e);
            }
        }
    }

    // No mirror available or mirror failed, return error
    metrics.record_download_failed();

    if let Some(ref callback) = progress_callback {
        if let Some(ref error) = last_error {
            callback(ProgressEvent::Error {
                url,
                error: error.to_string(),
            });
        }
    }

    Err(DownloadError::MaxRetriesExceeded {
        url: request.url.clone(),
        max_retries,
        total_duration_secs: 0, // Duration not tracked in this function
        last_error: last_error.map_or("No specific error recorded".to_string(), |e| e.to_string()),
    })
}

/// Download multiple files concurrently
pub async fn download_batch(
    registry: &DownloaderRegistry,
    config: &DownloadConfig,
    metrics: &DownloadMetrics,
    requests: Vec<DownloadRequest>,
    progress_callback: Option<ProgressCallback>,
    max_concurrent: usize,
) -> Vec<Result<DownloadResult>> {
    debug!("Starting batch download of {} files with max_concurrent={}", requests.len(), max_concurrent);

    stream::iter(requests)
        .map(|request| {
            let progress_cb = progress_callback.clone();
            async move {
                download_with_retry(registry, config, metrics, request, progress_cb).await
            }
        })
        .buffer_unordered(max_concurrent)
        .collect()
        .await
}

/// Download multiple files with async validation and validation retry
pub async fn download_batch_with_async_validation(
    registry: &DownloaderRegistry,
    config: &DownloadConfig,
    validation_pool: &ValidationPool,
    metrics: &DownloadMetrics,
    requests: Vec<DownloadRequest>,
    progress_callback: Option<ProgressCallback>,
    max_concurrent_downloads: usize,
) -> Vec<Result<DownloadResult>> {
    debug!("Starting batch download with async validation: {} files", requests.len());

    // Initial downloads with intermediate results
    let intermediate_results = stream::iter(requests.into_iter().enumerate())
        .map(|(index, request)| {
            let progress_cb = progress_callback.clone();
            async move {
                if config.async_validation {
                    match download_with_async_validation(
                        registry,
                        config,
                        validation_pool,
                        metrics,
                        request,
                        progress_cb
                    ).await {
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
                    BatchDownloadResult::Completed(
                        download_with_retry(registry, config, metrics, request, progress_cb).await
                    )
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
                // For completed results, assign them sequentially since we've lost original index mapping
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
    if config.async_validation && !pending_validations.is_empty() {
        debug!("Waiting for {} async validations", pending_validations.len());
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
                    metrics.record_validation_failed();
                    if config.validation_retries > 0 {
                        warn!("Validation failed for {}, queuing for retry", validation_handle.url);

                        // Remove the invalid file
                        if let Err(e) = fs::remove_file(&validation_handle.file_path).await {
                            warn!("Failed to remove invalid file {}: {}", validation_handle.file_path.display(), e);
                        }

                        retry_queue.push_back((original_index, validation_handle.request));
                    } else {
                        // No retries, mark as validation error
                        final_results[original_index] = Some(Err(DownloadError::ValidationFailed {
                            file: validation_handle.file_path.clone(),
                            validation_type: crate::downloader::error::ValidationType::Size, // Default validation type
                            expected: "valid file".to_string(),
                            actual: "invalid file".to_string(),
                            suggestion: "Check file integrity or download again".to_string(),
                        }));
                    }
                }
                Err(e) => {
                    // Task panicked or was cancelled
                    warn!("Validation task failed for {}: {}", validation_handle.url, e);
                    if config.validation_retries > 0 {
                        retry_queue.push_back((original_index, validation_handle.request));
                    } else {
                        final_results[original_index] = Some(Err(DownloadError::ValidationTaskFailed {
                            file: validation_handle.file_path.clone(),
                            reason: format!("Validation task failed: {}", e),
                            source: Some(Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
                        }));
                    }
                }
            }
        }

        // Process retry queue
        let mut retry_attempts = 0;
        while !retry_queue.is_empty() && retry_attempts < config.validation_retries {
            retry_attempts += 1;
            info!("Starting validation retry attempt {} of {}", retry_attempts, config.validation_retries);

            let current_retries: Vec<_> = retry_queue.drain(..).collect();
            let progress_callback_ref = progress_callback.as_ref();
            let retry_results = stream::iter(current_retries.iter().cloned())
                .map(|(original_index, request)| {
                    let progress_cb = progress_callback_ref.cloned();
                    async move {
                        let result = download_with_async_validation(
                            registry,
                            config,
                            validation_pool,
                            metrics,
                            request,
                            progress_cb
                        ).await;
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
                        metrics.record_validation_failed();
                        if retry_attempts < config.validation_retries {
                            if let Err(e) = fs::remove_file(&validation_handle.file_path).await {
                                warn!("Failed to remove invalid file after retry {}: {}", validation_handle.file_path.display(), e);
                            }
                            retry_queue.push_back((original_index, validation_handle.request));
                        } else {
                            final_results[original_index] = Some(Err(DownloadError::ValidationFailed {
                                file: validation_handle.file_path.clone(),
                                validation_type: crate::downloader::error::ValidationType::Size, // Default validation type
                                expected: "valid file".to_string(),
                                actual: "invalid file after retries".to_string(),
                                suggestion: "File failed validation even after retries - may be corrupted".to_string(),
                            }));
                        }
                    }
                    Err(e) => {
                        warn!("Retry validation task failed: {}", e);
                        final_results[original_index] = Some(Err(DownloadError::ValidationTaskFailed {
                            file: validation_handle.file_path.clone(),
                            reason: format!("Retry validation task failed: {}", e),
                            source: Some(Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
                        }));
                    }
                }
            }
        }

        // Mark any remaining failed retries
        for (original_index, failed_request) in retry_queue {
            warn!("Max validation retries exceeded for {}", failed_request.url);
            let filename = failed_request.get_filename().unwrap_or_else(|_| "unknown".to_string());
            let file_path = failed_request.destination.join(filename);
            final_results[original_index] = Some(Err(DownloadError::ValidationFailed {
                file: file_path,
                validation_type: crate::downloader::error::ValidationType::Size, // Default validation type
                expected: "valid file".to_string(),
                actual: "max validation retries exceeded".to_string(),
                suggestion: "File failed validation after maximum retry attempts".to_string(),
            }));
        }
    }

    // Convert Option<Result<DownloadResult>> to Vec<Result<DownloadResult>>
    // Fill any remaining None values with errors
    let result: Vec<Result<DownloadResult>> = final_results.into_iter()
        .map(|opt_result| {
            opt_result.unwrap_or_else(|| Err(DownloadError::ValidationTaskFailed {
                file: std::path::PathBuf::from("unknown"),
                reason: "Download result was not properly set".to_string(),
                source: None,
            }))
        })
        .collect();

    debug!("Batch download completed: {} results", result.len());
    result
}
