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
        }
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

        loop {
            check_interval.tick().await;

            let results_count = self.results.lock().await.len();
            let queue_count = self.download_queue.lock().await.len();

            if results_count == expected_count && queue_count == 0 {
                debug!("Pipeline completion detected: {} results, 0 queued", results_count);
                break;
            }

            // Optional: Add timeout logic here if needed
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
            self.results.lock().await.insert(
                task.original_index,
                Ok(VerifiedDownloadResult {
                    download_result: DownloadResult::Skipped {
                        reason: format!("Max retries exceeded: {}", validation_error)
                    },
                    validation_result: ValidationResult::Invalid(validation_error),
                })
            );
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
        }
    }
}

/// Enhanced downloader with retry capability and batch operations
///
/// This is the main entry point for users. It provides:
/// - Single file downloads with retry logic
/// - Pipeline-based batch downloads with concurrent validation
/// - Automatic retry on validation failures
/// - Built-in performance metrics
/// - Mirror URL fallback support
pub struct Downloader {
    config: DownloadConfig,
    metrics: Arc<DownloadMetrics>,
    /// Maximum retry attempts for pipeline downloads
    max_retries: u32,
}

impl Downloader {
    /// Create a new downloader
    pub fn new(config: DownloadConfig) -> Self {
        let metrics = Arc::new(DownloadMetrics::default());

        Self {
            config,
            metrics,
            max_retries: 3, // Default to 3 retries
        }
    }

    /// Create a new downloader with custom retry settings
    pub fn with_retries(config: DownloadConfig, max_retries: u32) -> Self {
        let metrics = Arc::new(DownloadMetrics::default());

        Self {
            config,
            metrics,
            max_retries,
        }
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

    /// Download multiple files using pipeline architecture with automatic retry
    ///
    /// This method uses a sophisticated pipeline approach:
    /// 1. Concurrent download and validation pools work in parallel
    /// 2. Validation failures automatically trigger download retry
    /// 3. Self-healing for temporary validation issues
    /// 4. Better resource utilization (both pools active simultaneously)
    ///
    /// Performance benefits over traditional batch downloading:
    /// - No download blocking during validation
    /// - Automatic recovery from validation failures
    /// - Near-constant resource utilization
    pub async fn download_batch(
        &self,
        requests: &Vec<DownloadRequest>,
        progress_callback: Option<ProgressCallback>,
        max_concurrent: usize,
    ) -> Vec<Result<VerifiedDownloadResult>> {
        info!("Starting pipeline batch download for {} files (max_concurrent: {}, max_retries: {})",
              requests.len(), max_concurrent, self.max_retries);

        // Create pipeline with current configuration
        let pipeline = DownloadPipeline::new(
            self.config.clone(),
            max_concurrent,
            self.max_retries,
        );

        // Process the batch using pipeline architecture
        let results = pipeline.process_batch(requests.clone(), progress_callback).await;

        info!("Pipeline batch download completed for {} files", requests.len());
        results
    }

}
