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
use async_trait::async_trait;

// Re-export the structured DownloadSource for convenience
pub use crate::parse_wabbajack::sources::DownloadSource;

/// Trait for types that can download files
///
/// This trait allows each download source type to implement its own download logic,
/// eliminating the need for a central registry and making the code more modular.
#[async_trait]
pub trait Downloadable: Send + Sync {
    /// Download the file to the specified destination
    ///
    /// # Arguments
    /// * `request` - The download request containing destination, validation, etc.
    /// * `progress_callback` - Optional progress reporting callback
    /// * `config` - Download configuration (timeouts, user agent, etc.)
    ///
    /// # Returns
    /// Result containing the download result with size information
    async fn download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<ProgressCallback>,
        config: &crate::downloader::config::DownloadConfig,
    ) -> Result<DownloadResult>;

    /// Check if this source supports resume functionality
    fn supports_resume(&self) -> bool {
        false
    }

    /// Check if this source can validate using the provided validation method
    fn can_validate(&self, _validation: &FileValidation) -> bool {
        // By default, assume we can validate any method (only xxHash64 is supported)
        true
    }

    /// Get a description of this download source for logging/UI
    fn description(&self) -> String;

    /// Check if this source requires user interaction (e.g., manual downloads)
    fn requires_user_interaction(&self) -> bool {
        false
    }

    /// Check if this source requires external dependencies (API keys, game installations, etc.)
    fn requires_external_dependencies(&self) -> bool {
        false
    }
}

/// A download request containing all necessary information
///
/// This is the main data structure that flows through the entire download system.
/// It uses downloadable trait objects for polymorphic downloading.
pub struct DownloadRequest {
    /// The download source (trait object for polymorphic downloading)
    pub source: Box<dyn Downloadable>,
    /// Directory where the file should be saved
    pub destination: PathBuf,
    /// Validation requirements for the downloaded file
    pub validation: FileValidation,
    /// Optional override for the filename (defaults to extracting from source)
    pub filename: Option<String>,
    /// Expected file size in bytes (for progress reporting)
    pub expected_size: Option<u64>,
}

impl std::fmt::Debug for DownloadRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownloadRequest")
            .field("source", &self.source.description())
            .field("destination", &self.destination)
            .field("validation", &self.validation)
            .field("filename", &self.filename)
            .field("expected_size", &self.expected_size)
            .finish()
    }
}

impl DownloadRequest {
    /// Create a new download request with HTTP URL and destination
    pub fn new_http<S: Into<String>, P: Into<PathBuf>>(url: S, destination: P) -> Self {
        use crate::parse_wabbajack::sources::HttpSource;
        Self {
            source: Box::new(HttpSource::new(url)),
            destination: destination.into(),
            validation: FileValidation::default(),
            filename: None,
            expected_size: None,
        }
    }

    /// Create a new download request with any downloadable source
    pub fn new<P: Into<PathBuf>>(source: Box<dyn Downloadable>, destination: P) -> Self {
        Self {
            source,
            destination: destination.into(),
            validation: FileValidation::default(),
            filename: None,
            expected_size: None,
        }
    }

    /// Create a download request from a concrete source type
    pub fn from_source<T: Downloadable + 'static, P: Into<PathBuf>>(source: T, destination: P) -> Self {
        Self::new(Box::new(source), destination)
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
    /// Returns the explicit filename if set, otherwise falls back to "downloaded_file".
    /// With trait objects, filename extraction is more complex so we encourage
    /// setting explicit filenames.
    pub fn get_filename(&self) -> Result<String> {
        if let Some(ref filename) = self.filename {
            return Ok(filename.clone());
        }

        // With trait objects, we can't easily extract filenames from URLs
        // so we fall back to a generic name. Users should set explicit filenames.
        Ok("downloaded_file".to_string())
    }

    /// Get a description of the download source
    pub fn get_description(&self) -> String {
        self.source.description()
    }

    /// Check if this download requires user interaction
    pub fn requires_user_interaction(&self) -> bool {
        self.source.requires_user_interaction()
    }

    /// Check if this download requires external dependencies
    pub fn requires_external_dependencies(&self) -> bool {
        self.source.requires_external_dependencies()
    }

    /// Check if this source supports resume functionality
    pub fn supports_resume(&self) -> bool {
        self.source.supports_resume()
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

