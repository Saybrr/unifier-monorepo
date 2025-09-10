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

/// A download request containing all necessary information
///
/// This is the main data structure that flows through the entire download system.
/// It contains the source URL, destination, validation requirements, and optional
/// configuration like mirror URLs and custom filenames.
#[derive(Debug, Clone)]
pub struct DownloadRequest {
    /// Primary URL to download from
    pub url: String,
    /// Optional fallback URL if primary fails
    pub mirror_url: Option<String>,
    /// Directory where the file should be saved
    pub destination: PathBuf,
    /// Validation requirements for the downloaded file
    pub validation: FileValidation,
    /// Optional override for the filename (defaults to extracting from URL)
    pub filename: Option<String>,
}

impl DownloadRequest {
    /// Create a new download request with basic URL and destination
    pub fn new<S: Into<String>, P: Into<PathBuf>>(url: S, destination: P) -> Self {
        Self {
            url: url.into(),
            mirror_url: None,
            destination: destination.into(),
            validation: FileValidation::default(),
            filename: None,
        }
    }

    /// Add a mirror URL for fallback support
    pub fn with_mirror_url<S: Into<String>>(mut self, mirror_url: S) -> Self {
        self.mirror_url = Some(mirror_url.into());
        self
    }

    /// Set validation requirements
    pub fn with_validation(mut self, validation: FileValidation) -> Self {
        self.validation = validation;
        self
    }

    /// Override the filename (otherwise extracted from URL)
    pub fn with_filename<S: Into<String>>(mut self, filename: S) -> Self {
        self.filename = Some(filename.into());
        self
    }

    /// Get the filename for this download
    ///
    /// Returns the explicit filename if set, otherwise extracts it from the URL.
    /// Falls back to a generic name if extraction fails.
    pub(crate) fn get_filename(&self) -> Result<String> {
        if let Some(ref filename) = self.filename {
            return Ok(filename.clone());
        }

        let url = url::Url::parse(&self.url)?;
        if let Some(segments) = url.path_segments() {
            if let Some(last_segment) = segments.last() {
                if !last_segment.is_empty() {
                    return Ok(last_segment.to_string());
                }
            }
        }

        Ok("downloaded_file".to_string())
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
