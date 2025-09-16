

pub mod downloader;
pub mod parse_wabbajack;
pub mod integrations;
pub mod install;

// Re-export commonly used types for convenience
pub use downloader::{
    // Core types
    DownloadRequest, DownloadResult, ValidationHandle,



    // Validation
    FileValidation,

    // Progress tracking
    ProgressCallback, ProgressEvent, ProgressReporter, IntoProgressCallback,
    ConsoleProgressReporter, NullProgressReporter, CompositeProgressReporter,


    // Error handling
    DownloadError, Result, ErrorSeverity, FileOperation, ValidationType, ErrorContext,

    // Nexus authentication
    NexusAPI, UserValidation, initialize_nexus_api,
};

// Re-export parse_wabbajack types
pub use parse_wabbajack::{

    // Source types
    DownloadSource as WabbajackDownloadSource, HttpSource, NexusSource,
    GameFileSource, ManualSource, ArchiveSource,
};

// Re-export high-level convenience APIs (the main improvement!)
pub use integrations::{
    // Fluent modlist API
    ModlistDownloader, ModlistOptions, ModlistDownloadResult,

    // Built-in progress reporters
    DashboardProgressReporter, DashboardStyle, NexusRateLimitProgressReporter,

    // Extension traits for better ergonomics
    DownloadRequestExt, DownloadRequestIteratorExt, DownloadRequestVecExt, RequestSummaryStats,
};

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}