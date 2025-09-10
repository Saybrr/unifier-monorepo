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
//!     DownloadConfig, DownloadRequest, EnhancedDownloader,
//!     FileValidation, ProgressEvent
//! };
//! use std::sync::Arc;
//!
//! # async fn example() -> installer::Result<()> {
//! // Create a download configuration
//! let config = DownloadConfig::default();
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
//! // Set up progress callback (optional)
//! let progress_callback = Arc::new(|event: ProgressEvent| {
//!     match event {
//!         ProgressEvent::DownloadStarted { url, total_size } => {
//!             println!("Started downloading: {} ({:?} bytes)", url, total_size);
//!         }
//!         ProgressEvent::DownloadProgress { downloaded, total, speed_bps, .. } => {
//!             if let Some(total) = total {
//!                 let percent = (downloaded as f64 / total as f64) * 100.0;
//!                 println!("Progress: {:.1}% ({:.1} KB/s)", percent, speed_bps / 1024.0);
//!             }
//!         }
//!         ProgressEvent::DownloadComplete { final_size, .. } => {
//!             println!("Download complete: {} bytes", final_size);
//!         }
//!         ProgressEvent::ValidationComplete { file, valid } => {
//!             println!("Validation {}: {}", file, if valid { "PASS" } else { "FAIL" });
//!         }
//!         _ => {}
//!     }
//! });
//!
//! // Download the file
//! let result = downloader.download(request, Some(progress_callback)).await?;
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
    DownloadConfig, DownloadError, DownloadRequest, DownloadResult,
    EnhancedDownloader, FileValidation, ProgressCallback, ProgressEvent,
    Result,
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