//! Configuration types for the downloader system

use std::time::Duration;

/// Configuration for download operations
#[derive(Debug, Clone)]
pub struct DownloadConfig {
    pub max_retries: usize,
    pub timeout: Duration,
    pub user_agent: String,
    pub allow_resume: bool,
    /// Maximum number of concurrent validation tasks
    pub max_concurrent_validations: usize,
    /// Whether to validate files asynchronously (non-blocking)
    pub async_validation: bool,
    /// Number of retry attempts for failed validations
    pub validation_retries: usize,
    /// Minimum file size (bytes) to use streaming validation instead of in-memory
    pub streaming_threshold: u64,
    /// Enable parallel hash computation for small files
    pub parallel_validation: bool,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            timeout: Duration::from_secs(30),
            user_agent: "installer/0.1.0".to_string(),
            allow_resume: true,
            max_concurrent_validations: 4,
            async_validation: true,
            validation_retries: 2,
            streaming_threshold: 50_000_000, // 50MB
            parallel_validation: true,
        }
    }
}
