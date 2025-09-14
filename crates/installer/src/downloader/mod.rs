//! Downloader module
//!
//! This module contains all the download functionality including
//! core types, configuration, sources, and batch operations.

pub mod core;
pub mod config;
pub mod batch;
pub mod sources;
pub mod r#lib;

// Re-export main types for convenience
pub use r#lib::Downloader;
pub use core::{
    DownloadRequest, DownloadResult, DownloadMetadata,
    ProgressCallback, ProgressEvent, ProgressReporter, IntoProgressCallback,
    ConsoleProgressReporter, NullProgressReporter, CompositeProgressReporter,
    FileValidation, ValidationHandle, ValidationPool,
    DownloadError, Result, ErrorSeverity, FileOperation, ValidationType, ErrorContext,
};
pub use config::{DownloadConfig};
pub use batch::{DownloadMetrics, DownloadMetricsSnapshot, BatchDownloadResult};

// Re-export source types
pub use sources::{
    DownloadSource, HttpSource, NexusSource, GameFileSource, ManualSource,
    ArchiveSource, WabbajackCDNSource, UnknownSource
};

#[cfg(test)]
mod tests;
