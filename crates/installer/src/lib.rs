//! Installer Library
//!
//! This library provides file downloading and validation capabilities
//! for installer applications. It supports multiple download sources,
//! file integrity validation, retry logic, and progress tracking.
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use installer::{
//!     DownloadConfigBuilder, DownloadRequest, EnhancedDownloader,
//!     FileValidation, ProgressEvent, ConsoleProgressReporter, IntoProgressCallback
//! };
//! use std::sync::Arc;
//!
//! # async fn example() -> installer::Result<()> {
//! // Create a download configuration using the builder
//! let config = DownloadConfigBuilder::new()
//!     .high_performance()
//!     .timeout(std::time::Duration::from_secs(60))
//!     .build();
//!
//! // Create the downloader
//! let downloader = EnhancedDownloader::new(config);
//!
//! // Set up file validation (optional)
//! let validation = FileValidation::new()
//!     .with_crc32(0x12345678)
//!     .with_expected_size(1024);
//!
//! // Create a download request
//! let request = DownloadRequest::new(
//!     "https://example.com/file.zip",
//!     "/path/to/download/directory"
//! )
//! .with_filename("file.zip")
//! .with_mirror_url("https://mirror.example.com/file.zip")
//! .with_validation(validation);
//!
//! // Set up progress reporting (optional)
//! let progress_reporter = ConsoleProgressReporter::new(true); // verbose output
//! let progress_callback = Some(progress_reporter.into_callback());
//!
//! // Download the file
//! let result = downloader.download(request, progress_callback).await?;
//! println!("Download result: {:?}", result);
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

// Re-export commonly used types for convenience
pub use downloader::{
    // Core types
    DownloadRequest, DownloadResult, EnhancedDownloader, FileDownloader,
    DownloaderRegistry, ValidationHandle,

    // Configuration
    DownloadConfig, DownloadConfigBuilder,

    // Validation
    FileValidation,

    // Progress tracking
    ProgressCallback, ProgressEvent, ProgressReporter, IntoProgressCallback,
    ConsoleProgressReporter, NullProgressReporter, CompositeProgressReporter,

    // Downloaders
    HttpDownloader,

    // Batch operations and metrics
    BatchDownloadResult, DownloadMetrics, DownloadMetricsSnapshot,

    // Error handling
    DownloadError, Result,
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