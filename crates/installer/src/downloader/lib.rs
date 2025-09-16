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
    core::{DownloadRequest, DownloadResult, ProgressCallback, Result, ValidationPool, DownloadSource, DownloadConfig, ValidationResult, VerifiedDownloadResult, DownloadError, ValidationType},
};
use std::sync::Arc;
use std::collections::{HashMap, VecDeque};
use tokio::sync::{Mutex, Semaphore};
use tracing::{debug, warn, info};

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

/// A download task with retry tracking
#[derive(Clone, Debug)]
struct DownloadTask {
    /// The original download request
    pub request: DownloadRequest,
    /// Number of retry attempts made
    pub retry_count: u32,
    /// Original index in the batch for result ordering
    pub original_index: usize,
}

/// Pipeline-based downloader with concurrent download and validation pools
///
/// This architecture provides:
/// - Concurrent download and validation processing
/// - Automatic retry on validation failures
/// - Better resource utilization
/// - Self-healing for temporary validation issues
pub struct DownloadPipeline {
    /// Semaphore controlling concurrent downloads
    download_pool: Arc<Semaphore>,
    /// Pool for validation operations
    validation_pool: ValidationPool,
    /// Queue of pending download tasks
    download_queue: Arc<Mutex<VecDeque<DownloadTask>>>,
    /// Results map indexed by original task index
    results: Arc<Mutex<HashMap<usize, Result<VerifiedDownloadResult>>>>,
    /// Configuration for downloads
    config: DownloadConfig,
    /// Maximum retry attempts per file
    max_retries: u32,
    /// Total number of tasks expected (for completion detection)
    total_tasks: Arc<Mutex<Option<usize>>>,
    /// Maximum concurrent downloads (stored for getter)
    max_concurrent_downloads: usize,
}

impl DownloadPipeline {
    /// Create a new download pipeline
    pub fn new(config: DownloadConfig, max_concurrent_downloads: usize, max_retries: u32) -> Self {
        Self {
            download_pool: Arc::new(Semaphore::new(max_concurrent_downloads)),
            validation_pool: ValidationPool::new(config.max_concurrent_validations),
            download_queue: Arc::new(Mutex::new(VecDeque::new())),
            results: Arc::new(Mutex::new(HashMap::new())),
            config,
            max_retries,
            total_tasks: Arc::new(Mutex::new(None)),
            max_concurrent_downloads,
        }
    }

    /// Download a single file with direct validation (bypasses complex pipeline retry logic)
    pub async fn download(
        &self,
        request: DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult> {
        use tokio::fs;

        // Perform download using dispatch
        let download_result = dispatch_download(&request.source, &request, progress_callback.clone(), &self.config).await?;

        // Handle validation directly without pipeline complexity
        match &download_result {
            DownloadResult::Downloaded { file_path, .. } |
            DownloadResult::Resumed { file_path, .. } => {
                // Only validate if validation is configured
                if request.validation.xxhash64_base64.is_some() || request.validation.expected_size.is_some() {
                    match request.validation.validate_file(file_path, progress_callback).await {
                        Ok(true) => Ok(download_result),
                        Ok(false) => {
                            // This shouldn't happen as validate_file returns Err for failures
                            let _ = fs::remove_file(file_path).await;
                            Err(DownloadError::ValidationFailed {
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
                            Err(e)
                        }
                    }
                } else {
                    Ok(download_result)
                }
            },
            DownloadResult::AlreadyExists { file_path, validated: false, .. } => {
                // Need to validate existing file
                if request.validation.xxhash64_base64.is_some() || request.validation.expected_size.is_some() {
                    match request.validation.validate_file(file_path, progress_callback).await {
                        Ok(true) => Ok(download_result),
                        Ok(false) => Err(DownloadError::ValidationFailed {
                            file: file_path.clone(),
                            validation_type: ValidationType::Size,
                            expected: "valid file".to_string(),
                            actual: "invalid file".to_string(),
                            suggestion: "Check file integrity or download again".to_string(),
                        }),
                        Err(e) => Err(e)
                    }
                } else {
                    Ok(download_result)
                }
            },
            _ => Ok(download_result),
        }
    }

    /// Download multiple files (alias for process_batch for backward compatibility)
    pub async fn download_batch(
        &self,
        requests: &Vec<DownloadRequest>,
        progress_callback: Option<ProgressCallback>,
        _max_concurrent: usize, // Ignored, uses the pipeline's configured concurrency
    ) -> Vec<Result<VerifiedDownloadResult>> {
        self.process_batch(requests.clone(), progress_callback).await
    }

    /// Get the maximum number of retries
    pub fn max_retries(&self) -> u32 {
        self.max_retries
    }

    /// Get the maximum number of concurrent downloads
    pub fn max_concurrent_downloads(&self) -> usize {
        self.max_concurrent_downloads
    }

    /// Create a mock metrics object for backward compatibility with tests
    pub fn metrics(&self) -> crate::downloader::core::DownloadMetrics {
        crate::downloader::core::DownloadMetrics::default()
    }

    /// Process a batch of download requests using the pipeline architecture
    pub async fn process_batch(
        &self,
        requests: Vec<DownloadRequest>,
        progress_callback: Option<ProgressCallback>,
    ) -> Vec<Result<VerifiedDownloadResult>> {
        let total_count = requests.len();
        debug!("Starting pipeline processing for {} files", total_count);

        // Set total task count for completion detection
        *self.total_tasks.lock().await = Some(total_count);

        // Initialize download queue with all requests
        {
            let mut queue = self.download_queue.lock().await;
            for (index, request) in requests.into_iter().enumerate() {
                queue.push_back(DownloadTask {
                    request,
                    retry_count: 0,
                    original_index: index,
                });
            }
            info!("Queued {} download tasks", total_count);
        }

        // Spawn download workers (as many as permits available)
        let max_download_workers = self.download_pool.available_permits().min(total_count);
        let mut download_handles = Vec::new();

        for worker_id in 0..max_download_workers {
            let pipeline = self.clone();
            let callback = progress_callback.clone();
            let handle = tokio::spawn(async move {
                pipeline.download_worker(worker_id, callback).await;
            });
            download_handles.push(handle);
        }

        debug!("Started {} download workers", max_download_workers);

        // Wait for all tasks to complete (either success or max retries exceeded)
        self.wait_for_completion(total_count).await;

        // Collect results and clean up
        for handle in download_handles {
            let _ = handle.await; // Workers should have finished naturally
        }

        // Extract final results in original order (consume the HashMap)
        let mut results_map = self.results.lock().await;
        let mut final_results = Vec::with_capacity(total_count);

        for index in 0..total_count {
            match results_map.remove(&index) {
                Some(result) => {
                    final_results.push(result);
                },
                None => {
                    // This shouldn't happen if our logic is correct
                    warn!("Missing result for index {}", index);
                    final_results.push(Err(DownloadError::Configuration {
                        message: "Internal pipeline error: missing result".to_string(),
                        field: None,
                        suggestion: Some("This indicates a bug in the pipeline logic".to_string()),
                    }));
                }
            }
        }

        info!("Pipeline processing completed for {} files", total_count);
        final_results
    }

    /// Wait for all tasks to complete
    async fn wait_for_completion(&self, expected_count: usize) {
        let mut check_interval = tokio::time::interval(std::time::Duration::from_millis(100));
        let timeout = std::time::Instant::now() + std::time::Duration::from_secs(30); // 30 second timeout

        loop {
            check_interval.tick().await;

            let results_count = self.results.lock().await.len();
            let queue_count = self.download_queue.lock().await.len();

            if results_count == expected_count {
                debug!("Pipeline completion detected: {} results, {} queued", results_count, queue_count);
                break;
            }

            // Timeout logic to prevent infinite waiting
            if std::time::Instant::now() > timeout {
                warn!("Pipeline completion timeout reached. Results: {}, Queue: {}, Expected: {}",
                      results_count, queue_count, expected_count);
                break;
            }
        }
    }

    /// Download worker that processes tasks from the download queue
    async fn download_worker(&self, worker_id: usize, progress_callback: Option<ProgressCallback>) {
        debug!("Download worker {} started", worker_id);

        loop {
            // Get next download task
            let task = {
                let mut queue = self.download_queue.lock().await;
                match queue.pop_front() {
                    Some(task) => task,
                    None => {
                        debug!("Download worker {}: no more tasks, exiting", worker_id);
                        break; // No more tasks
                    }
                }
            };

            debug!("Download worker {} processing task {} (retry {})",
                   worker_id, task.original_index, task.retry_count);

            // Acquire download permit
            let _permit = self.download_pool.acquire().await.unwrap();

            // Perform download
            match dispatch_download(&task.request.source, &task.request, progress_callback.clone(), &self.config).await {
                Ok(download_result) => {
                    debug!("Download worker {} completed task {} successfully", worker_id, task.original_index);
                    // Release download permit immediately
                    drop(_permit);

                    // Queue for validation (this spawns async task)
                    self.queue_for_validation(task, download_result, progress_callback.clone()).await;
                }
                Err(download_error) => {
                    debug!("Download worker {} failed task {}: {}", worker_id, task.original_index, download_error);
                    drop(_permit);

                    if task.retry_count < self.max_retries {
                        // Re-queue for download retry
                        let retry_task = DownloadTask {
                            retry_count: task.retry_count + 1,
                            ..task
                        };
                        warn!("Re-queueing task {} for retry {} due to download error",
                              retry_task.original_index, retry_task.retry_count);
                        self.download_queue.lock().await.push_back(retry_task);
                    } else {
                        // Permanent failure - max retries exceeded
                        warn!("Task {} failed permanently after {} retries", task.original_index, task.retry_count);
                        self.results.lock().await.insert(task.original_index, Err(download_error));
                    }
                }
            }
        }

        debug!("Download worker {} finished", worker_id);
    }

    /// Queue a completed download for validation
    async fn queue_for_validation(
        &self,
        task: DownloadTask,
        download_result: DownloadResult,
        progress_callback: Option<ProgressCallback>,
    ) {
        // Handle already validated files first
        if let DownloadResult::AlreadyExists { validated: true, .. } = &download_result {
            debug!("Task {} file already exists and was validated, skipping validation", task.original_index);
            self.results.lock().await.insert(
                task.original_index,
                Ok(VerifiedDownloadResult {
                    download_result,
                    validation_result: ValidationResult::AlreadyValidated,
                })
            );
            return;
        }

        // Extract file path for validation
        let file_path = match &download_result {
            DownloadResult::Downloaded { file_path, .. } |
            DownloadResult::Resumed { file_path, .. } |
            DownloadResult::AlreadyExists { file_path, .. } => file_path.clone(),
            DownloadResult::DownloadedPendingValidation { .. } => {
                // This variant already has async validation in progress, handle differently
                debug!("Task {} already has validation in progress", task.original_index);
                self.results.lock().await.insert(
                    task.original_index,
                    Ok(VerifiedDownloadResult {
                        download_result,
                        validation_result: ValidationResult::Skipped, // Will be handled by existing async validation
                    })
                );
                return;
            }
            DownloadResult::Skipped { .. } => {
                // No validation needed for skipped files
                debug!("Task {} was skipped, no validation needed", task.original_index);
                self.results.lock().await.insert(
                    task.original_index,
                    Ok(VerifiedDownloadResult {
                        download_result,
                        validation_result: ValidationResult::Skipped,
                    })
                );
                return;
            }
        };

        // Check if validation is needed
        if task.request.validation.xxhash64_base64.is_none() && task.request.validation.expected_size.is_none() {
            // No validation configured
            debug!("Task {} has no validation configured", task.original_index);
            self.results.lock().await.insert(
                task.original_index,
                Ok(VerifiedDownloadResult {
                    download_result,
                    validation_result: ValidationResult::Skipped,
                })
            );
            return;
        }

        debug!("Starting validation for task {}", task.original_index);

        // Start async validation
        let validation_handle = self.validation_pool.validate_async(
            task.request.validation.clone(),
            file_path,
            task.request.source.description(),
            task.request.clone(),
            progress_callback.clone(),
        );

        // Spawn task to handle validation completion
        let pipeline = self.clone();
        tokio::spawn(async move {
            match validation_handle.task_handle.await {
                Ok(validation_result) => {
                    match validation_result {
                        Ok(true) => {
                            // Validation succeeded
                            debug!("Validation succeeded for task {}", task.original_index);
                            pipeline.results.lock().await.insert(
                                task.original_index,
                                Ok(VerifiedDownloadResult {
                                    download_result,
                                    validation_result: ValidationResult::Valid,
                                })
                            );
                        }
                        Ok(false) => {
                            // Validation failed - this shouldn't happen as validate_file returns Err for failures
                            warn!("Validation returned false for task {} (unexpected)", task.original_index);
                            pipeline.handle_validation_failure(task, download_result, DownloadError::ValidationFailed {
                                file: validation_handle.file_path,
                                validation_type: ValidationType::Size,
                                expected: "valid file".to_string(),
                                actual: "invalid file".to_string(),
                                suggestion: "Check file integrity or download again".to_string(),
                            }).await;
                        }
                        Err(validation_error) => {
                            // Validation failed with specific error
                            debug!("Validation failed for task {}: {}", task.original_index, validation_error);
                            pipeline.handle_validation_failure(task, download_result, validation_error).await;
                        }
                    }
                }
                Err(join_error) => {
                    // Validation task panicked
                    warn!("Validation task panicked for task {}: {}", task.original_index, join_error);
                    let validation_error = DownloadError::ValidationTaskFailed {
                        file: validation_handle.file_path,
                        reason: format!("Validation task panicked: {}", join_error),
                        source: Some(Box::new(join_error) as Box<dyn std::error::Error + Send + Sync>),
                    };
                    pipeline.handle_validation_failure(task, download_result, validation_error).await;
                }
            }
        });
    }

    /// Handle validation failure by either retrying or marking as permanent failure
    async fn handle_validation_failure(
        &self,
        task: DownloadTask,
        _download_result: DownloadResult, // We'll discard this and re-download
        validation_error: DownloadError,
    ) {
        debug!("Handling validation failure for task {}: retry_count={}, max_retries={}",
               task.original_index, task.retry_count, self.max_retries);

        if task.retry_count < self.max_retries {
            // Re-queue for download retry (validation failure triggers full retry)
            let retry_task = DownloadTask {
                retry_count: task.retry_count + 1,
                ..task
            };
            warn!("Re-queueing task {} for retry {} due to validation failure",
                  retry_task.original_index, retry_task.retry_count);
            self.download_queue.lock().await.push_back(retry_task);
        } else {
            // Max retries exceeded - mark as permanent validation failure
            warn!("Task {} failed permanently after {} retries due to validation failure",
                  task.original_index, task.retry_count);

            let result = Ok(VerifiedDownloadResult {
                download_result: DownloadResult::Skipped {
                    reason: format!("Max retries exceeded: {}", validation_error)
                },
                validation_result: ValidationResult::Invalid(validation_error),
            });

            debug!("Inserting final result for task {}: {:?}", task.original_index, result);
            self.results.lock().await.insert(task.original_index, result);
        }
    }
}

// Implement Clone for DownloadPipeline (needed for spawning tasks)
impl Clone for DownloadPipeline {
    fn clone(&self) -> Self {
        Self {
            download_pool: Arc::clone(&self.download_pool),
            validation_pool: ValidationPool::new(self.config.max_concurrent_validations), // Create new validation pool
            download_queue: Arc::clone(&self.download_queue),
            results: Arc::clone(&self.results),
            config: self.config.clone(),
            max_retries: self.max_retries,
            total_tasks: Arc::clone(&self.total_tasks),
            max_concurrent_downloads: self.max_concurrent_downloads,
        }
    }
}

