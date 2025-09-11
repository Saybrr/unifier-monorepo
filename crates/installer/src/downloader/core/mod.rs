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

/// A download source specification
///
/// This enum allows DownloadRequest to accept either traditional URLs
/// or structured download sources for better type safety and performance.
#[derive(Debug, Clone)]
pub enum DownloadSource {
    /// Traditional URL-based download (for backward compatibility)
    Url {
        url: String,
        mirror_url: Option<String>,
    },
    /// Structured download source from parse_wabbajack module
    Structured(crate::parse_wabbajack::sources::DownloadSource),
}

/// A download request containing all necessary information
///
/// This is the main data structure that flows through the entire download system.
/// It now supports both URL-based downloads (for backward compatibility) and
/// structured download sources (for better type safety and performance).
#[derive(Debug, Clone)]
pub struct DownloadRequest {
    /// The download source (URL or structured)
    pub source: DownloadSource,
    /// Directory where the file should be saved
    pub destination: PathBuf,
    /// Validation requirements for the downloaded file
    pub validation: FileValidation,
    /// Optional override for the filename (defaults to extracting from source)
    pub filename: Option<String>,
}

impl DownloadRequest {
    /// Create a new download request with basic URL and destination
    pub fn new<S: Into<String>, P: Into<PathBuf>>(url: S, destination: P) -> Self {
        Self {
            source: DownloadSource::Url {
                url: url.into(),
                mirror_url: None,
            },
            destination: destination.into(),
            validation: FileValidation::default(),
            filename: None,
        }
    }

    /// Create a new download request with structured source
    pub fn new_structured<P: Into<PathBuf>>(
        source: crate::parse_wabbajack::sources::DownloadSource,
        destination: P,
    ) -> Self {
        Self {
            source: DownloadSource::Structured(source),
            destination: destination.into(),
            validation: FileValidation::default(),
            filename: None,
        }
    }

    /// Add a mirror URL for fallback support (only works with URL sources)
    pub fn with_mirror_url<S: Into<String>>(mut self, new_mirror_url: S) -> Self {
        if let DownloadSource::Url { ref mut mirror_url, .. } = self.source {
            *mirror_url = Some(new_mirror_url.into());
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

    /// Get the filename for this download
    ///
    /// Returns the explicit filename if set, otherwise extracts it from the source.
    /// Falls back to a generic name if extraction fails.
    pub(crate) fn get_filename(&self) -> Result<String> {
        if let Some(ref filename) = self.filename {
            return Ok(filename.clone());
        }

        match &self.source {
            DownloadSource::Url { url, .. } => {
                let parsed_url = url::Url::parse(url)?;
                if let Some(segments) = parsed_url.path_segments() {
                    if let Some(last_segment) = segments.last() {
                        if !last_segment.is_empty() {
                            return Ok(last_segment.to_string());
                        }
                    }
                }
                Ok("downloaded_file".to_string())
            },
            DownloadSource::Structured(_structured_source) => {
                // For structured sources, we don't have a reliable way to extract filename
                // so we return a generic name - the caller should set explicit filename
                Ok("downloaded_file".to_string())
            }
        }
    }

    /// Get the primary URL for this download (if it's a URL-based source)
    pub fn get_primary_url(&self) -> Option<&str> {
        match &self.source {
            DownloadSource::Url { url, .. } => Some(url),
            DownloadSource::Structured(_) => None,
        }
    }

    /// Get the mirror URL for this download (if it's a URL-based source with mirror)
    pub fn get_mirror_url(&self) -> Option<&str> {
        match &self.source {
            DownloadSource::Url { mirror_url: Some(mirror), .. } => Some(mirror),
            _ => None,
        }
    }

    /// Check if this is a structured download request
    pub fn is_structured(&self) -> bool {
        matches!(self.source, DownloadSource::Structured(_))
    }

    /// Get the structured source if this is a structured request
    pub fn get_structured_source(&self) -> Option<&crate::parse_wabbajack::sources::DownloadSource> {
        match &self.source {
            DownloadSource::Structured(source) => Some(source),
            _ => None,
        }
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
