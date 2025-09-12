//! Batch download operations and metrics
//!
//! In the new trait-based architecture, most batch operations are handled
//! directly by EnhancedDownloader. This module primarily contains metrics
//! and some helper types.

pub mod metrics;

// Re-export for convenience
pub use metrics::{DownloadMetrics, DownloadMetricsSnapshot};

/// Result of a batch download operation
#[derive(Debug)]
pub struct BatchDownloadResult {
    /// Number of successful downloads
    pub successful: usize,
    /// Number of failed downloads
    pub failed: usize,
    /// Total time taken for the batch
    pub duration: std::time::Duration,
}