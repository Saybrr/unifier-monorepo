//! High-level modlist download API
//!
//! Provides a simple, fluent API for downloading entire modlists with sensible defaults.

use std::path::{Path, PathBuf};
use crate::{
    EnhancedDownloader, DownloadConfigBuilder, ProgressCallback,
    ModlistParser, Result, DownloadError
};
use crate::integrations::progress::DashboardProgressReporter;
use crate::IntoProgressCallback;

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
            max_concurrent_downloads: 4,
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
pub struct ModlistDownloadBuilder {
    modlist_path: PathBuf,
    destination: Option<PathBuf>,
    options: ModlistOptions,
    progress_callback: Option<ProgressCallback>,
}

impl ModlistDownloadBuilder {
    /// Create a new builder for the given modlist file
    pub fn new<P: AsRef<Path>>(modlist_path: P) -> Self {
        Self {
            modlist_path: modlist_path.as_ref().to_path_buf(),
            destination: None,
            options: ModlistOptions::default(),
            progress_callback: None,
        }
    }

    /// Set the destination directory for downloads
    pub fn destination<P: AsRef<Path>>(mut self, destination: P) -> Self {
        self.destination = Some(destination.as_ref().to_path_buf());
        self
    }


    /// Set maximum concurrent downloads
    pub fn max_concurrent_downloads(mut self, max: usize) -> Self {
        self.options.max_concurrent_downloads = max;
        self
    }

    /// Use high-performance configuration (8 concurrent validations, async validation)
    pub fn high_performance(mut self) -> Self {
        self.options.high_performance = true;
        self
    }

    /// Use standard performance configuration (less resource intensive)
    pub fn standard_performance(mut self) -> Self {
        self.options.high_performance = false;
        self
    }

    /// Set timeout for individual downloads
    pub fn timeout_seconds(mut self, seconds: u64) -> Self {
        self.options.timeout_seconds = seconds;
        self
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

        // Use current directory as default destination
        let destination = self.destination.unwrap_or_else(|| PathBuf::from("./downloads"));

        // Read and parse the modlist
        let modlist_json = std::fs::read_to_string(&self.modlist_path)
            .map_err(|e| DownloadError::Legacy(format!("Failed to read modlist file: {}", e)))?;

        let manifest = ModlistParser::new().parse(&modlist_json, &destination)
            .map_err(|e| DownloadError::Legacy(format!("Failed to parse modlist: {}", e)))?;

        // Get requests directly from manifest - no conversion needed!
        let download_requests = manifest.requests;


        // Create downloader with appropriate configuration
        let config = if self.options.high_performance {
            DownloadConfigBuilder::new()
                .high_performance()
                .timeout(std::time::Duration::from_secs(self.options.timeout_seconds))
                .build()
        } else {
            DownloadConfigBuilder::new()
                .timeout(std::time::Duration::from_secs(self.options.timeout_seconds))
                .build()
        };

        let downloader = EnhancedDownloader::new(config);

        // Execute batch download
        let results = downloader.download_batch_with_async_validation(
            download_requests,
            self.progress_callback,
            self.options.max_concurrent_downloads,
        ).await;

        // Process results
        let mut successful_downloads = 0;
        let mut failed_downloads = 0;
        let mut total_bytes_downloaded = 0;
        let mut error_messages = Vec::new();

        for result in results {
            match result {
                Ok(download_result) => {
                    successful_downloads += 1;
                    match download_result {
                        crate::DownloadResult::Downloaded { size } |
                        crate::DownloadResult::AlreadyExists { size } |
                        crate::DownloadResult::Resumed { size } => {
                            total_bytes_downloaded += size;
                        }
                        crate::DownloadResult::DownloadedPendingValidation { size, .. } => {
                            total_bytes_downloaded += size;
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
            skipped_downloads: 0, // TODO: Track skipped downloads
            total_bytes_downloaded,
            elapsed_time,
            total_requests: manifest.stats.total_operations,
            error_messages,
        })
    }
}

/// Extension trait for EnhancedDownloader to add convenience methods
pub trait EnhancedDownloaderExt {
    /// Download a modlist with a fluent API
    fn modlist<P: AsRef<Path>>(modlist_path: P) -> ModlistDownloadBuilder;
}

impl EnhancedDownloaderExt for EnhancedDownloader {
    fn modlist<P: AsRef<Path>>(modlist_path: P) -> ModlistDownloadBuilder {
        ModlistDownloadBuilder::new(modlist_path)
    }
}
