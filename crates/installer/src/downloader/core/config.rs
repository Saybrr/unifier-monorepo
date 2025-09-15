//! Configuration types for the downloader system

use std::time::Duration;

/// Configuration for download operations
#[derive(Debug, Clone)]
pub struct DownloadConfig {
    pub max_retries: usize,
    pub timeout: Duration,
    /// Timeout specifically for large files (>100MB)
    pub large_file_timeout: Duration,
    /// File size threshold (bytes) to use large file timeout
    pub large_file_threshold: u64,
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
    /// Initial delay between retries (doubles each retry)
    pub retry_delay: Duration,
    /// Maximum retry delay cap (prevents exponential backoff from getting too long)
    pub max_retry_delay: Duration,
}

impl DownloadConfig {
    /// Get appropriate timeout based on expected file size
    pub fn get_timeout_for_size(&self, expected_size: u64) -> Duration {
        if expected_size >= self.large_file_threshold {
            self.large_file_timeout
        } else {
            self.timeout
        }
    }

    /// Check if a file size qualifies as a large file
    pub fn is_large_file(&self, size: u64) -> bool {
        size >= self.large_file_threshold
    }

    /// Calculate retry delay for the given attempt using exponential backoff
    pub fn get_retry_delay(&self, attempt: usize) -> Duration {
        let delay = self.retry_delay.as_millis() as u64 * 2_u64.pow(attempt as u32);
        Duration::from_millis(delay.min(self.max_retry_delay.as_millis() as u64))
    }
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            timeout: Duration::from_secs(30),
            large_file_timeout: Duration::from_secs(600), // 10 minutes for large files
            large_file_threshold: 100_000_000, // 100MB
            user_agent: "installer/0.1.0".to_string(),
            allow_resume: true,
            max_concurrent_validations: 4,
            async_validation: true,
            validation_retries: 2,
            streaming_threshold: 50_000_000, // 50MB
            parallel_validation: true,
            retry_delay: Duration::from_millis(1000), // Start with 1 second
            max_retry_delay: Duration::from_secs(60), // Cap at 1 minute
        }
    }
}
