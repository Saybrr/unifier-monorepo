//! Core types used throughout the downloader system
//!
//! This module contains the fundamental types that all other modules depend on.
//! By organizing these in a core module, we make the dependency relationships clear.

pub mod error;
pub mod validation;
pub mod progress;

// Re-export main types for convenience
pub use error::{DownloadError, Result, ErrorSeverity, FileOperation, ValidationType, ErrorContext};
pub use validation::{FileValidation, ValidationHandle, ValidationPool};
pub use progress::{ProgressEvent, ProgressCallback, ProgressReporter, IntoProgressCallback, ConsoleProgressReporter, NullProgressReporter, CompositeProgressReporter};

use std::path::PathBuf;

// Re-export the structured DownloadSource for convenience
pub use crate::parse_wabbajack::sources::DownloadSource;

/// A download request containing all necessary information
///
/// This is the main data structure that flows through the entire download system.
/// It uses structured download sources for type safety and performance.
#[derive(Debug, Clone)]
pub struct DownloadRequest {
    /// The download source
    pub source: DownloadSource,
    /// Directory where the file should be saved
    pub destination: PathBuf,
    /// Validation requirements for the downloaded file
    pub validation: FileValidation,
    /// Optional override for the filename (defaults to extracting from source)
    pub filename: Option<String>,
    /// Expected file size in bytes (for progress reporting)
    pub expected_size: Option<u64>,
}

impl DownloadRequest {
    /// Create a new download request with HTTP URL and destination
    pub fn new_http<S: Into<String>, P: Into<PathBuf>>(url: S, destination: P) -> Self {
        use crate::parse_wabbajack::sources::HttpSource;
        Self {
            source: DownloadSource::Http(HttpSource::new(url)),
            destination: destination.into(),
            validation: FileValidation::default(),
            filename: None,
            expected_size: None,
        }
    }

    /// Create a new download request with any structured source
    pub fn new<P: Into<PathBuf>>(source: DownloadSource, destination: P) -> Self {
        Self {
            source,
            destination: destination.into(),
            validation: FileValidation::default(),
            filename: None,
            expected_size: None,
        }
    }

    /// Add a mirror URL for HTTP sources
    pub fn with_mirror_url<S: Into<String>>(mut self, mirror_url: S) -> Self {
        if let DownloadSource::Http(ref mut http_source) = self.source {
            http_source.mirror_urls.push(mirror_url.into());
        }
        self
    }

    /// Set validation requirements
    pub fn with_validation(mut self, validation: FileValidation) -> Self {
        self.validation = validation;
        self
    }

    /// Override the filename (otherwise extracted from source)
    pub fn with_filename<S: Into<String>>(mut self, filename: S) -> Self {
        self.filename = Some(filename.into());
        self
    }

    /// Set the expected file size for progress reporting
    pub fn with_expected_size(mut self, size: u64) -> Self {
        self.expected_size = Some(size);
        self
    }

    /// Get the filename for this download
    ///
    /// Returns the explicit filename if set, otherwise extracts it from the source.
    /// Falls back to a generic name if extraction fails.
    pub fn get_filename(&self) -> Result<String> {
        if let Some(ref filename) = self.filename {
            return Ok(filename.clone());
        }

        match &self.source {
            DownloadSource::Http(http_source) => {
                let parsed_url = url::Url::parse(&http_source.url)?;
                if let Some(segments) = parsed_url.path_segments() {
                    if let Some(last_segment) = segments.last() {
                        if !last_segment.is_empty() {
                            return Ok(last_segment.to_string());
                        }
                    }
                }
                Ok("downloaded_file".to_string())
            },
            DownloadSource::WabbajackCDN(cdn_source) => {
                let parsed_url = url::Url::parse(&cdn_source.url)?;
                if let Some(segments) = parsed_url.path_segments() {
                    if let Some(last_segment) = segments.last() {
                        if !last_segment.is_empty() {
                            return Ok(last_segment.to_string());
                        }
                    }
                }
                Ok("downloaded_file".to_string())
            },
            DownloadSource::GameFile(gamefile_source) => {
                // Extract filename from the file path
                if let Some(filename) = std::path::Path::new(&gamefile_source.file_path).file_name() {
                    if let Some(filename_str) = filename.to_str() {
                        return Ok(filename_str.to_string());
                    }
                }
                Ok("game_file".to_string())
            },
            _ => {
                // For other sources, we don't have a reliable way to extract filename
                // so we return a generic name - the caller should set explicit filename
                Ok("downloaded_file".to_string())
            }
        }
    }

    /// Get the primary URL for this download (if it has one)
    pub fn get_primary_url(&self) -> Option<&str> {
        match &self.source {
            DownloadSource::Http(http_source) => Some(&http_source.url),
            DownloadSource::WabbajackCDN(cdn_source) => Some(&cdn_source.url),
            _ => None,
        }
    }

    /// Get the mirror URLs for this download (if it has any)
    pub fn get_mirror_urls(&self) -> Vec<&str> {
        match &self.source {
            DownloadSource::Http(http_source) => {
                http_source.mirror_urls.iter().map(|s| s.as_str()).collect()
            },
            _ => Vec::new(),
        }
    }

    /// Get the download source
    pub fn get_source(&self) -> &DownloadSource {
        &self.source
    }
}

/// Result of a download operation
///
/// This enum represents the different outcomes of a download attempt.
/// It provides detailed information about what happened and any ongoing
/// asynchronous operations (like background validation).
#[derive(Debug)]
pub enum DownloadResult {
    /// File was successfully downloaded
    Downloaded { size: u64 },
    /// File already existed and was validated
    AlreadyExists { size: u64 },
    /// File was partially downloaded and resumed to completion
    Resumed { size: u64 },
    /// File downloaded but validation is still in progress
    ///
    /// This variant is used when async validation is enabled.
    /// The caller can await the validation_handle to get the final result.
    DownloadedPendingValidation {
        size: u64,
        validation_handle: ValidationHandle,
    },
}

