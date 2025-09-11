//! Batch download operations and orchestration
//!
//! This module contains the implementation of batch download operations that
//! EnhancedDownloader delegates to. This is where the core download logic lives,
//! including retry handling, mirror fallback, and async validation coordination.

pub mod metrics;

// Re-export for convenience
pub use metrics::{DownloadMetrics, DownloadMetricsSnapshot};

use crate::downloader::{
    core::{DownloadRequest, DownloadResult, ValidationHandle, ProgressCallback, ProgressEvent, DownloadError, Result},
    config::DownloadConfig,
    registry::DownloaderRegistry,
    core::ValidationPool,
};
use futures::stream::{self, StreamExt};
use std::time::Duration;
use tokio::fs;
use tracing::{info, warn, debug};

/// Extract URL from a DownloadRequest, handling both URL and structured sources
fn get_url_from_request(request: &DownloadRequest) -> Result<String> {
    match &request.source {
        crate::downloader::core::DownloadSource::Url { url, .. } => Ok(url.clone()),
        crate::downloader::core::DownloadSource::Structured(structured) => {
            match structured {
                crate::parse_wabbajack::sources::DownloadSource::Http(http_source) => {
                    Ok(http_source.url.clone())
                },
                _ => {
                    Ok("structured_source".to_string())
                }
            }
        }
    }
}

/// Intermediate result type for batch operations
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

    let url = request.get_primary_url().unwrap_or("unknown").to_string();
    let max_retries = config.max_retries;
    let progress_callback_clone = progress_callback.clone();

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
    if let Some(mirror_url) = request.get_mirror_url() {
        warn!("All retries failed for {}, attempting mirror URL: {}", url, mirror_url);

        let mirror_request = DownloadRequest::new(mirror_url, &request.destination)
            .with_validation(request.validation.clone())
            .with_filename(request.filename.clone().unwrap_or_else(|| "file".to_string()));

        match registry.attempt_download(&mirror_request, progress_callback).await {
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
            Err(mirror_error) => {
                warn!("Mirror URL also failed: {}", mirror_error);
            }
        }
    }

    // All attempts failed
    metrics.record_download_failed();
    let final_error = last_error.unwrap_or_else(|| {
        DownloadError::MaxRetriesExceeded {
            url: url.clone(),
            max_retries,
            total_duration_secs: 0,
            last_error: "All attempts failed".to_string(),
        }
    });

    if let Some(ref callback) = progress_callback_clone {
        callback(ProgressEvent::Error {
            url: url.clone(),
            error: final_error.to_string(),
        });
    }

    Err(final_error)
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
    let downloader = registry.find_downloader_for_request(&request).await?;
    if let Some(existing_result) = downloader.check_existing_file(
        &dest_path,
        &request.validation,
        progress_callback.clone()
    ).await? {
        let size = match &existing_result {
            DownloadResult::AlreadyExists { size } => *size,
            _ => 0,
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
            get_url_from_request(&request)?,
            request.clone(),
            progress_callback,
        );

        Ok(DownloadResult::DownloadedPendingValidation {
            size,
            validation_handle,
        })
    } else if !request.validation.is_empty() {
        // Synchronous validation
        if !request.validation.validate_file(&dest_path, progress_callback).await? {
            metrics.record_validation_failed();
            fs::remove_file(&dest_path).await?;
            return Err(DownloadError::ValidationFailed {
                file: dest_path.clone(),
                    validation_type: crate::downloader::core::ValidationType::Size,
                expected: "valid file".to_string(),
                actual: "invalid file".to_string(),
                suggestion: "Check file integrity or download again".to_string(),
            });
        }
        Ok(DownloadResult::Downloaded { size })
    } else {
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
    let url = get_url_from_request(request)?;
    let max_retries = config.max_retries;
    let filename = request.get_filename()?;
    let dest_path = request.destination.join(&filename);

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

            let delay = Duration::from_millis(1000 * (1 << (attempt - 1).min(5)));
            tokio::time::sleep(delay).await;
        }

        let downloader = match registry.find_downloader(&url).await {
            Ok(d) => d,
            Err(e) => {
                last_error = Some(e);
                if attempt < max_retries { continue; }
                break;
            }
        };

        match downloader.download_helper(&url, &dest_path, progress_callback.clone(), None).await {
            Ok(size) => {
                metrics.record_download_completed(size);
                return Ok(size);
            }
            Err(e) => {
                last_error = Some(e);
                if attempt < max_retries { continue; }
            }
        }
    }

    // Try mirror if available
    if let Some(mirror_url) = request.get_mirror_url() {
        info!("Primary download failed, trying mirror URL");

        match registry.find_downloader(mirror_url).await {
            Ok(mirror_downloader) => {
                match mirror_downloader.download_helper(mirror_url, &dest_path, progress_callback.clone(), None).await {
                    Ok(size) => {
                        metrics.record_download_completed(size);
                        return Ok(size);
                    }
                    Err(e) => last_error = Some(e),
                }
            }
            Err(e) => last_error = Some(e),
        }
    }

    metrics.record_download_failed();

    if let Some(ref callback) = progress_callback {
        if let Some(ref error) = last_error {
            callback(ProgressEvent::Error {
                url: url.clone(),
                error: error.to_string(),
            });
        }
    }

    Err(DownloadError::MaxRetriesExceeded {
        url: url,
        max_retries,
        total_duration_secs: 0,
        last_error: last_error.map_or("No error recorded".to_string(), |e| e.to_string()),
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
    debug!("Starting batch download of {} files", requests.len());

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

/// Download multiple files with async validation
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

    // Initial downloads
    let intermediate_results = stream::iter(requests.into_iter().enumerate())
        .map(|(index, request)| {
            let progress_cb = progress_callback.clone();
            async move {
                if config.async_validation {
                    match download_with_async_validation(
                        registry, config, validation_pool, metrics, request, progress_cb
                    ).await {
                        Ok(DownloadResult::DownloadedPendingValidation { size, validation_handle }) => {
                            BatchDownloadResult::PendingValidation { size, validation_handle, original_index: index }
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

    // Process results
    let total_requests = intermediate_results.len();
    let mut final_results: Vec<Option<Result<DownloadResult>>> = (0..total_requests).map(|_| None).collect();
    let mut pending_validations = Vec::new();

    for result in intermediate_results {
        match result {
            BatchDownloadResult::Completed(res) => {
                // Find first empty slot
                if let Some(slot) = final_results.iter_mut().find(|slot| slot.is_none()) {
                    *slot = Some(res);
                }
            }
            BatchDownloadResult::PendingValidation { size, validation_handle, original_index } => {
                pending_validations.push((original_index, size, validation_handle));
            }
        }
    }

    // Wait for pending validations
    for (original_index, size, validation_handle) in pending_validations {
        match validation_handle.task_handle.await {
            Ok(Ok(true)) => {
                final_results[original_index] = Some(Ok(DownloadResult::Downloaded { size }));
            }
            Ok(Ok(false)) | Ok(Err(_)) | Err(_) => {
                metrics.record_validation_failed();
                final_results[original_index] = Some(Err(DownloadError::ValidationFailed {
            file: validation_handle.file_path.clone(),
            validation_type: crate::downloader::core::ValidationType::Size,
            expected: "valid file".to_string(),
                    actual: "invalid file".to_string(),
                    suggestion: "Check file integrity".to_string(),
                }));
            }
        }
    }

    // Convert to final result
    final_results.into_iter()
        .map(|opt| opt.unwrap_or_else(|| Err(DownloadError::ValidationTaskFailed {
            file: std::path::PathBuf::from("unknown"),
            reason: "Download result not set".to_string(),
            source: None,
        })))
        .collect()
}