//! High-level convenience APIs for common workflows
//!
//! This module provides simplified APIs that handle the most common use cases,
//! reducing the amount of boilerplate code users need to write.

pub mod modlist;
pub mod progress;
pub mod request_ext;

// Re-export main convenience APIs
pub use modlist::{ModlistDownloadBuilder, ModlistOptions, ModlistDownloadResult, EnhancedDownloaderExt};
pub use progress::{DashboardProgressReporter, DashboardStyle};
pub use request_ext::{DownloadRequestExt, DownloadRequestIteratorExt, DownloadRequestVecExt, RequestSummaryStats};
