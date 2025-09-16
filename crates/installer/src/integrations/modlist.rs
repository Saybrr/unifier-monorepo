//! High-level modlist download API
//!
//! Provides a simple, fluent API for downloading entire modlists with sensible defaults.

use std::path::PathBuf;
use crate::parse_wabbajack::parser::WabbaModlist;
use crate::{
    Result, DownloadError
};
use crate::downloader::{Downloader, DownloadConfig, ProgressCallback};
use crate::integrations::progress::DashboardProgressReporter;
use crate::IntoProgressCallback;
use crate::downloader::core::DownloadResult;

/// Options for modlist downloading
#[derive(Debug, Clone)]
pub struct ModlistOptions {
    /// Maximum concurrent downloads (default: 4)
    pub max_concurrent_downloads: usize,
    /// Include manual downloads (default: false)

    pub high_performance: bool,
    /// Custom timeout in seconds (default: 120)
    pub timeout_seconds: u64,
}

impl Default for ModlistOptions {
    fn default() -> Self {
        Self {
            max_concurrent_downloads: std::thread::available_parallelism().unwrap().get(),
            high_performance: true,
            timeout_seconds: 120,
        }
    }
}

/// Result of a modlist download operation
#[derive(Debug)]
pub struct ModlistDownloadResult {
    /// Number of files successfully downloaded
    pub successful_downloads: usize,
    /// Number of files that failed to download
    pub failed_downloads: usize,
    /// Number of files that were skipped (already existed, etc.)
    pub skipped_downloads: usize,
    /// Total bytes downloaded
    pub total_bytes_downloaded: u64,
    /// Time taken for the entire operation
    pub elapsed_time: std::time::Duration,
    /// Total number of requests parsed
    pub total_requests: usize,
    /// List of error messages for failed downloads
    pub error_messages: Vec<String>,
}

/// Fluent API builder for modlist downloads
pub struct ModlistDownloader {
    modlist_path: PathBuf,
    destination: PathBuf,
    options: ModlistOptions,
    progress_callback: Option<ProgressCallback>,
}

impl ModlistDownloader {
    /// Create a new builder for the given modlist file
    pub fn new(modlist_path: &str, destination: &str, options: ModlistOptions, progress_callback: Option<ProgressCallback>) -> Self {
        Self {
            modlist_path: PathBuf::from(modlist_path),
            destination: PathBuf::from(destination),
            options,
            progress_callback,
        }
    }

    /// Use a built-in dashboard-style progress reporter
    pub fn with_dashboard_progress(mut self) -> Self {
        let reporter = DashboardProgressReporter::new();
        self.progress_callback = Some(reporter.into_callback());
        self
    }

    /// Use a custom progress callback
    pub fn with_progress<F>(mut self, callback: F) -> Self
    where
        F: Fn(crate::ProgressEvent) + Send + Sync + 'static,
    {
        self.progress_callback = Some(std::sync::Arc::new(callback));
        self
    }

    /// Execute the modlist download
    pub async fn download(self) -> Result<ModlistDownloadResult> {
        let start_time = std::time::Instant::now();

        // Read and parse the modlist
        let modlist_json = std::fs::read_to_string(&self.modlist_path)
            .map_err(|e| DownloadError::FileSystem {
                path: self.modlist_path.clone(),
                operation: crate::downloader::core::FileOperation::Read,
                source: e,
            })?;

        let manifest = WabbaModlist::parse(&modlist_json).unwrap();

        let download_requests = manifest.get_dl_requests(&self.destination).unwrap();
        // Check if any download request is a NexusSource, and initialize Nexus API if needed
        let needs_nexus = download_requests.iter().any(|req| {
            matches!(&req.source, crate::parse_wabbajack::DownloadSource::Nexus(_))
        });
        if needs_nexus {
            crate::initialize_nexus_api().await?;
        }
        // Create downloader with appropriate configuration
        let downloader = Downloader::new(DownloadConfig::default());

        // Execute batch download
        let results = downloader.download_batch(
            &download_requests,
            self.progress_callback,
            self.options.max_concurrent_downloads,
        ).await;

        // Process results and collect statistics
        let mut successful_downloads = 0;
        let mut failed_downloads = 0;
        let mut total_bytes_downloaded = 0;
        let mut error_messages = Vec::new();
        let mut skipped_downloads = 0;

        for result in results {
            match result {
                Ok(verified_dl_result) => {
                    // Check validation result first
                    match verified_dl_result.validation_result {
                        crate::downloader::core::ValidationResult::Valid |
                        crate::downloader::core::ValidationResult::AlreadyValidated |
                        crate::downloader::core::ValidationResult::Skipped => {
                            // Validation passed or was skipped, count based on download result
                            match verified_dl_result.download_result {
                                DownloadResult::Downloaded { size, .. } |
                                DownloadResult::Resumed { size, .. } => {
                                    total_bytes_downloaded += size;
                                    successful_downloads += 1;
                                }
                                DownloadResult::AlreadyExists { size, .. } => {
                                    total_bytes_downloaded += size;
                                    successful_downloads += 1;
                                }
                                DownloadResult::DownloadedPendingValidation { size, .. } => {
                                    total_bytes_downloaded += size;
                                    successful_downloads += 1;
                                }
                                DownloadResult::Skipped { .. } => {
                                    skipped_downloads += 1;
                                }
                            }
                        }
                        crate::downloader::core::ValidationResult::Invalid(e) => {
                            // Validation failed, count as failure
                            failed_downloads += 1;
                            error_messages.push(format!("Validation failed: {}", e));
                        }
                    }
                }
                Err(e) => {
                    failed_downloads += 1;
                    error_messages.push(e.to_string());
                }
            }
        }

        let elapsed_time = start_time.elapsed();

        Ok(ModlistDownloadResult {
            successful_downloads,
            failed_downloads,
            skipped_downloads: skipped_downloads,
            total_bytes_downloaded,
            elapsed_time,
            total_requests: download_requests.len(),
            error_messages,
        })
    }
}

