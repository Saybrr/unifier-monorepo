//! Downloader module
//!
//! This module contains all the download functionality including
//! core types, configuration, sources, and batch operations.

pub mod core;
pub mod sources;
pub mod api;
pub mod r#lib;

// Re-export main types for convenience
pub use r#lib::DownloadPipeline;
pub use core::{
    DownloadRequest, DownloadResult, DownloadMetadata,
    ProgressCallback, ProgressEvent, ProgressReporter, IntoProgressCallback,
    ConsoleProgressReporter, NullProgressReporter, CompositeProgressReporter,
    FileValidation, ValidationHandle, ValidationPool,
    DownloadError, Result, ErrorSeverity, FileOperation, ValidationType, ErrorContext,
    DownloadConfig, DownloadMetrics, DownloadMetricsSnapshot, BatchDownloadResult,
};

// Re-export source types
pub use sources::{
    DownloadSource, HttpSource, NexusSource, GameFileSource, ManualSource,
    ArchiveSource, WabbajackCDNSource, UnknownSource
};

// Re-export auth types and functions
pub use api::{NexusAPI, UserValidation, NexusMod, NexusFile, NexusDownloadLink};
pub use sources::nexus::initialize_nexus_api;

#[cfg(test)]
mod tests;
