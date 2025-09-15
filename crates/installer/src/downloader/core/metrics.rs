//! Performance metrics and monitoring for batch downloads
//!
//! This module provides built-in performance monitoring capabilities
//! that track download statistics, success rates, and performance metrics.

use std::sync::atomic::{AtomicU64, Ordering};

/// Performance metrics for downloads
///
/// This struct tracks various statistics about download operations
/// using atomic counters for thread-safe updates across concurrent downloads.
#[derive(Debug, Default)]
pub struct DownloadMetrics {
    pub total_bytes: AtomicU64,
    pub total_downloads: AtomicU64,
    pub successful_downloads: AtomicU64,
    pub failed_downloads: AtomicU64,
    pub validation_failures: AtomicU64,
    pub retries_attempted: AtomicU64,
    pub cache_hits: AtomicU64,
}

impl DownloadMetrics {
    /// Record that a download has started
    pub fn record_download_started(&self) {
        self.total_downloads.fetch_add(1, Ordering::Relaxed);
    }

    /// Record that a download completed successfully
    pub fn record_download_completed(&self, size: u64) {
        self.successful_downloads.fetch_add(1, Ordering::Relaxed);
        self.total_bytes.fetch_add(size, Ordering::Relaxed);
    }

    /// Record that a download failed
    pub fn record_download_failed(&self) {
        self.failed_downloads.fetch_add(1, Ordering::Relaxed);
    }

    /// Record that a file validation failed
    pub fn record_validation_failed(&self) {
        self.validation_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Record that a retry was attempted
    pub fn record_retry(&self) {
        self.retries_attempted.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a cache hit (file already existed and was valid)
    pub fn record_cache_hit(&self, size: u64) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
        self.total_bytes.fetch_add(size, Ordering::Relaxed);
    }

    /// Get a snapshot of current metrics
    pub fn snapshot(&self) -> DownloadMetricsSnapshot {
        DownloadMetricsSnapshot {
            total_downloads: self.total_downloads.load(Ordering::Relaxed),
            successful_downloads: self.successful_downloads.load(Ordering::Relaxed),
            failed_downloads: self.failed_downloads.load(Ordering::Relaxed),
            total_bytes: self.total_bytes.load(Ordering::Relaxed),
            validation_failures: self.validation_failures.load(Ordering::Relaxed),
            retries_attempted: self.retries_attempted.load(Ordering::Relaxed),
            cache_hits: self.cache_hits.load(Ordering::Relaxed),
        }
    }
}

/// Immutable snapshot of download metrics
///
/// This struct provides a point-in-time view of the metrics
/// with convenient methods for calculating derived statistics.
#[derive(Debug, Clone)]
pub struct DownloadMetricsSnapshot {
    pub total_downloads: u64,
    pub successful_downloads: u64,
    pub failed_downloads: u64,
    pub total_bytes: u64,
    pub validation_failures: u64,
    pub retries_attempted: u64,
    pub cache_hits: u64,
}

impl DownloadMetricsSnapshot {
    /// Calculate success rate as a percentage (0.0 to 1.0)
    pub fn success_rate(&self) -> f64 {
        if self.total_downloads == 0 {
            0.0
        } else {
            self.successful_downloads as f64 / self.total_downloads as f64
        }
    }

    /// Calculate average file size
    pub fn average_size(&self) -> f64 {
        let completed = self.successful_downloads + self.cache_hits;
        if completed == 0 {
            0.0
        } else {
            self.total_bytes as f64 / completed as f64
        }
    }
}

/// Result of a batch download operation
#[derive(Debug)]
pub struct BatchDownloadResult {
    /// Individual results for each download
    pub results: Vec<crate::downloader::core::Result<crate::downloader::core::DownloadResult>>,
    /// Performance metrics for the batch operation
    pub metrics: DownloadMetricsSnapshot,
    /// Total time taken for the batch operation
    pub duration: std::time::Duration,
}
