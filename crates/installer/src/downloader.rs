

// Module declarations following the hierarchy
pub mod config;
pub mod core;
pub mod batch;
pub mod r#lib;

// Re-export main types for backward compatibility and ease of use
// Core types that users need
pub use core::{
    DownloadRequest, DownloadResult, FileValidation,
    DownloadError, Result, ErrorSeverity, FileOperation, ValidationType, ErrorContext,
    ProgressEvent, ProgressCallback, ProgressReporter, IntoProgressCallback,
    ConsoleProgressReporter, NullProgressReporter, CompositeProgressReporter,
    ValidationHandle, ValidationPool
};

// Configuration
pub use config::{DownloadConfig, DownloadConfigBuilder};

// Main entry point
pub use r#lib::EnhancedDownloader;

// Batch operations and metrics
pub use batch::{DownloadMetrics, DownloadMetricsSnapshot, BatchDownloadResult};

#[cfg(test)]
mod tests;