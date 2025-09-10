//! Configuration types for the downloader system

use std::time::Duration;

/// Configuration for download operations
#[derive(Debug, Clone)]
pub struct DownloadConfig {
    pub max_retries: usize,
    pub timeout: Duration,
    pub user_agent: String,
    pub allow_resume: bool,
    pub chunk_size: usize,
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
            chunk_size: 8192,
            max_concurrent_validations: 4,
            async_validation: true,
            validation_retries: 2,
            streaming_threshold: 50_000_000, // 50MB
            parallel_validation: true,
        }
    }
}

/// Builder for DownloadConfig with fluent API
#[derive(Debug, Clone)]
pub struct DownloadConfigBuilder {
    config: DownloadConfig,
}

impl DownloadConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: DownloadConfig::default(),
        }
    }

    pub fn max_retries(mut self, retries: usize) -> Self {
        self.config.max_retries = retries;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    pub fn user_agent<S: Into<String>>(mut self, agent: S) -> Self {
        self.config.user_agent = agent.into();
        self
    }

    pub fn allow_resume(mut self, allow: bool) -> Self {
        self.config.allow_resume = allow;
        self
    }

    pub fn chunk_size(mut self, size: usize) -> Self {
        self.config.chunk_size = size;
        self
    }

    pub fn max_concurrent_validations(mut self, max: usize) -> Self {
        self.config.max_concurrent_validations = max;
        self
    }

    pub fn async_validation(mut self, async_val: bool) -> Self {
        self.config.async_validation = async_val;
        self
    }

    pub fn validation_retries(mut self, retries: usize) -> Self {
        self.config.validation_retries = retries;
        self
    }

    pub fn streaming_threshold(mut self, threshold: u64) -> Self {
        self.config.streaming_threshold = threshold;
        self
    }

    pub fn parallel_validation(mut self, parallel: bool) -> Self {
        self.config.parallel_validation = parallel;
        self
    }

    /// Create DownloadConfig with high performance settings
    pub fn high_performance(mut self) -> Self {
        self.config.max_concurrent_validations = 8;
        self.config.async_validation = true;
        self.config.parallel_validation = true;
        self.config.chunk_size = 16384; // 16KB chunks
        self.config.streaming_threshold = 20_000_000; // 20MB
        self
    }

    /// Create DownloadConfig optimized for low memory usage
    pub fn low_memory(mut self) -> Self {
        self.config.max_concurrent_validations = 2;
        self.config.async_validation = false;
        self.config.parallel_validation = false;
        self.config.chunk_size = 4096; // 4KB chunks
        self.config.streaming_threshold = 1_000_000; // 1MB
        self
    }

    /// Create DownloadConfig for reliable downloads (more retries, conservative settings)
    pub fn reliable(mut self) -> Self {
        self.config.max_retries = 5;
        self.config.validation_retries = 3;
        self.config.timeout = Duration::from_secs(60);
        self.config.async_validation = false; // Synchronous for reliability
        self
    }

    pub fn build(self) -> DownloadConfig {
        self.config
    }
}

impl Default for DownloadConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}
