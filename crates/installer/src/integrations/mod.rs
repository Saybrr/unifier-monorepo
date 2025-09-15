//! High-level convenience APIs for common workflows
//!
//! This module provides simplified APIs that handle the most common use cases,
//! reducing the amount of boilerplate code users need to write.

pub mod modlist;
pub mod progress;
pub mod request_ext;
pub mod nexus_rate_limit_reporter;

// Re-export main convenience APIs
pub use modlist::{ModlistDownloader, ModlistOptions, ModlistDownloadResult};
pub use progress::{DashboardProgressReporter, DashboardStyle};
pub use request_ext::{DownloadRequestExt, DownloadRequestIteratorExt, DownloadRequestVecExt, RequestSummaryStats};
pub use nexus_rate_limit_reporter::NexusRateLimitProgressReporter;
