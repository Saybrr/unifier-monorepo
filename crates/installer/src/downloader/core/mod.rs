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
pub use crate::downloader::sources::DownloadSource;

// Trait removed - using enum dispatch instead

/// Additional metadata for a download request
#[derive(Debug, Clone, Default)]
pub struct DownloadMetadata {
    /// Human-readable description
    pub description: String,
    /// Category/group this download belongs to
    pub category: String,
    /// Whether this is required or optional
    pub required: bool,
    /// Tags for filtering/grouping
    pub tags: Vec<String>,
}

/// A download request containing all necessary information
///
/// This is the unified structure that combines parsing and downloading.
/// It contains all data needed to download, validate, and organize files.
/// It uses DownloadSource enum for compile-time dispatch.
pub struct DownloadRequest {
    /// The download source (enum for compile-time dispatch)
    pub source: DownloadSource,
    /// Directory where the file should be saved
    pub destination: PathBuf,
    /// Final filename for the downloaded file
    pub filename: String,
    /// Expected file hash for validation
    pub expected_hash: String,
    /// Hash algorithm used (e.g., "XXHASH64", "SHA256")
    pub hash_algorithm: String,
    /// Expected file size in bytes
    pub expected_size: u64,
    /// Validation requirements for the downloaded file (derived from hash/size)
    pub validation: FileValidation,
    /// Priority for download ordering (lower = higher priority)
    pub priority: u32,
    /// Optional metadata for display/logging purposes
    pub metadata: DownloadMetadata,
}

impl std::fmt::Debug for DownloadRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownloadRequest")
            .field("source", &self.source.description())
            .field("destination", &self.destination)
            .field("filename", &self.filename)
            .field("expected_hash", &self.expected_hash)
            .field("expected_size", &self.expected_size)
            .field("priority", &self.priority)
            .finish()
    }
}

impl DownloadRequest {
    /// Create a new download request with HTTP URL
    pub fn new_http<S: Into<String>, P: Into<PathBuf>, F: Into<String>>(
        url: S,
        destination: P,
        filename: F,
        expected_size: u64,
        expected_hash: String
    ) -> Self {
        use crate::downloader::sources::HttpSource;
        let hash_copy = expected_hash.clone();
        Self {
            source: DownloadSource::Http(HttpSource::new(url)),
            destination: destination.into(),
            filename: filename.into(),
            expected_hash,
            hash_algorithm: "XXHASH64".to_string(),
            expected_size,
            validation: FileValidation::new(hash_copy, expected_size),
            priority: 0,
            metadata: DownloadMetadata::default(),
        }
    }

    /// Create a new download request with any download source
    pub fn new<P: Into<PathBuf>, F: Into<String>>(
        source: DownloadSource,
        destination: P,
        filename: F,
        expected_size: u64,
        expected_hash: String
    ) -> Self {
        let hash_copy = expected_hash.clone();
        Self {
            source,
            destination: destination.into(),
            filename: filename.into(),
            expected_hash,
            hash_algorithm: "XXHASH64".to_string(),
            expected_size,
            validation: FileValidation::new(hash_copy, expected_size),
            priority: 0,
            metadata: DownloadMetadata::default(),
        }
    }

    // Removed from_source method as it's no longer needed with enum dispatch

    /// Set the hash algorithm
    pub fn with_hash_algorithm<S: Into<String>>(mut self, algorithm: S) -> Self {
        self.hash_algorithm = algorithm.into();
        self
    }

    /// Set the priority (lower = higher priority)
    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    /// Set metadata
    pub fn with_metadata(mut self, metadata: DownloadMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Get the filename for this download
    pub fn get_filename(&self) -> Result<String> {
        Ok(self.filename.clone())
    }

    /// Get a description of the download source
    pub fn get_description(&self) -> String {
        self.source.description()
    }

    /// Check if this source supports resume functionality
    pub fn supports_resume(&self) -> bool {
        self.source.supports_resume()
    }

    /// Check if this download requires user interaction
    pub fn requires_user_interaction(&self) -> bool {
        self.source.requires_user_interaction()
    }

    /// Check if this download requires external dependencies
    pub fn requires_external_dependencies(&self) -> bool {
        self.source.requires_external_dependencies()
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

