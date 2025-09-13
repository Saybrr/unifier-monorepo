//! Installer Library
//!
//! This library provides file downloading and validation capabilities
//! for installer applications. It supports multiple download sources,
//! file integrity validation, retry logic, and progress tracking.
//!
//! # Quick Start - Simple File Download
//!
//! ```rust,no_run
//! use installer::{DownloadRequest, EnhancedDownloader, DownloadConfigBuilder};
//!
//! # async fn simple_example() -> installer::Result<()> {
//! let downloader = EnhancedDownloader::new(DownloadConfigBuilder::new().build());
//! let request = DownloadRequest::new_http("https://example.com/file.zip", "./downloads");
//! let result = downloader.download(request, None).await?;
//! println!("Downloaded: {:?}", result);
//! # Ok(())
//! # }
//! ```
//!
//! # Quick Start - Modlist Download (High-level API)
//!
//! ```rust,no_run
//! use installer::{ModlistDownloadBuilder, DashboardStyle};
//!
//! # async fn modlist_example() -> installer::Result<()> {
//! let result = ModlistDownloadBuilder::new("Baseline/modlist")
//!     .destination("./downloads")
//!     .automated_only() // Only download what doesn't require user interaction
//!     .high_performance() // Use all available CPU cores for validation
//!     .with_dashboard_progress() // Built-in progress reporting
//!     .max_concurrent_downloads(8)
//!     .download()
//!     .await?;
//!
//! println!("Downloaded {} files, {} failed", result.successful_downloads, result.failed_downloads);
//! # Ok(())
//! # }
//! ```
//!
//! # Low-level API for Custom Use Cases
//!
//! ```rust,no_run
//! use installer::{
//!     DownloadConfigBuilder, DownloadRequest, EnhancedDownloader,
//!     FileValidation, DashboardProgressReporter
//! };
//!
//! # async fn advanced_example() -> installer::Result<()> {
//! // High-performance configuration
//! let config = DownloadConfigBuilder::new()
//!     .high_performance()
//!     .timeout(std::time::Duration::from_secs(60))
//!     .build();
//!
//! let downloader = EnhancedDownloader::new(config);
//!
//! // File with validation
//! let validation = FileValidation::new()
//!     .with_xxhash64_base64("ejsZTQOI370=")
//!     .with_expected_size(1024);
//!
//! let request = DownloadRequest::new_http("https://example.com/file.zip", "./downloads")
//!     .with_filename("file.zip")
//!     .with_validation(validation);
//!
//! // Built-in dashboard progress reporter
//! let progress_reporter = DashboardProgressReporter::new();
//! let result = downloader.download(request, Some(progress_reporter.into_callback())).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Features
//!
//! - **Multiple download sources**: HTTP/HTTPS with extensible support for other protocols
//! - **File validation**: CRC32, MD5, and SHA256 hash verification with size checking
//! - **Retry logic**: Configurable retry attempts with exponential backoff
//! - **Mirror fallback**: Automatic fallback to mirror URLs on primary failure
//! - **Resume capability**: Resume interrupted downloads
//! - **Progress tracking**: Real-time progress events with speed calculation
//! - **Batch downloads**: Download multiple files concurrently with configurable limits
//! - **Async/await**: Full async support with Tokio runtime

pub mod downloader;
pub mod parse_wabbajack;
pub mod download;
pub mod integrations;

// Re-export commonly used types for convenience
pub use downloader::{
    // Core types
    DownloadRequest, DownloadResult, EnhancedDownloader, ValidationHandle,

    // Configuration
    DownloadConfig, DownloadConfigBuilder,

    // Validation
    FileValidation,

    // Progress tracking
    ProgressCallback, ProgressEvent, ProgressReporter, IntoProgressCallback,
    ConsoleProgressReporter, NullProgressReporter, CompositeProgressReporter,

    // Batch operations and metrics
    BatchDownloadResult, DownloadMetrics, DownloadMetricsSnapshot,

    // Error handling
    DownloadError, Result, ErrorSeverity, FileOperation, ValidationType, ErrorContext,
};

// Re-export parse_wabbajack types
pub use parse_wabbajack::{
    // Core parsing types
    DownloadOperation, ArchiveManifest, OperationMetadata, ManifestMetadata,
    parse_modlist, ModlistParser,

    // Source types
    DownloadSource as WabbajackDownloadSource, HttpSource, NexusSource,
    GameFileSource, ManualSource, ArchiveSource,

    // Integration functions
    operation_to_download_request, operations_to_download_requests,
    manifest_to_download_requests, manifest_to_prioritized_download_requests,
    manifest_to_download_requests_with_stats, ConversionStats,
};

// Re-export high-level convenience APIs (the main improvement!)
pub use integrations::{
    // Fluent modlist API
    ModlistDownloadBuilder, ModlistOptions, ModlistDownloadResult, EnhancedDownloaderExt,

    // Built-in progress reporters
    DashboardProgressReporter, DashboardStyle,

    // Extension traits for better ergonomics
    DownloadRequestExt, DownloadRequestIteratorExt, DownloadRequestVecExt, RequestSummaryStats,
};

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}