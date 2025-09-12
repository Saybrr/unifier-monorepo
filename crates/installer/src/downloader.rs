//! Modular file downloader with validation and retry capabilities
//!
//! This module provides a comprehensive file downloading system with:
//! - Multiple download source support (HTTP, Google Drive, etc.)
//! - File integrity validation (CRC32, MD5, SHA256)
//! - Retry logic with exponential backoff
//! - Progress tracking
//! - Resume capability
//!
//! # Module Organization
//!
//! The module is organized to reflect the call chain clearly:
//!
//! ```text
//! User Code
//! ↓
//! EnhancedDownloader (lib.rs)
//! ↓
//! Batch operations (batch/)
//! ↓
//! DownloaderRegistry (registry.rs)
//! ↓
//! Backend implementations (backends/)
//! ↓
//! Core types (core/)
//! ```
//!
//! This organization makes it easy to understand how data flows through the system
//! and where to find specific functionality.

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