//! Progress tracking and reporting for download operations

use std::sync::Arc;

/// Progress callback for download operations
pub type ProgressCallback = Arc<dyn Fn(ProgressEvent) + Send + Sync>;

/// Events emitted during download operations
#[derive(Debug, Clone)]
pub enum ProgressEvent {
    DownloadStarted {
        url: String,
        total_size: Option<u64>,
    },
    DownloadProgress {
        url: String,
        downloaded: u64,
        total: Option<u64>,
        speed_bps: f64,
    },
    DownloadComplete {
        url: String,
        final_size: u64,
    },
    ValidationStarted {
        file: String,
        validation: crate::downloader::core::validation::FileValidation,
    },
    ValidationProgress {
        file: String,
        progress: f64,
    },
    ValidationComplete {
        file: String,
        valid: bool,
    },
    RetryAttempt {
        url: String,
        attempt: usize,
        max_attempts: usize,
    },
    Error {
        url: String,
        error: String,
    },
}

/// Trait for progress reporting with more granular control
pub trait ProgressReporter: Send + Sync {
    fn on_download_started(&self, _url: &str, _total_size: Option<u64>) {}
    fn on_download_progress(&self, _url: &str, _downloaded: u64, _total: Option<u64>, _speed_bps: f64) {}
    fn on_download_complete(&self, _url: &str, _final_size: u64) {}
    fn on_validation_started(&self, _file: &str, _validation: &crate::downloader::core::validation::FileValidation) {}
    fn on_validation_progress(&self, _file: &str, _progress: f64) {}
    fn on_validation_complete(&self, _file: &str, _valid: bool) {}
    fn on_retry_attempt(&self, _url: &str, _attempt: usize, _max_attempts: usize) {}
    fn on_error(&self, _url: &str, _error: &str) {}
}

/// Extension trait to convert ProgressReporter to ProgressCallback
pub trait IntoProgressCallback {
    fn into_callback(self) -> ProgressCallback;
}

impl<T: ProgressReporter + 'static> IntoProgressCallback for T {
    fn into_callback(self) -> ProgressCallback {
        Arc::new(move |event| match event {
            ProgressEvent::DownloadStarted { url, total_size } => {
                self.on_download_started(&url, total_size);
            }
            ProgressEvent::DownloadProgress { url, downloaded, total, speed_bps } => {
                self.on_download_progress(&url, downloaded, total, speed_bps);
            }
            ProgressEvent::DownloadComplete { url, final_size } => {
                self.on_download_complete(&url, final_size);
            }
            ProgressEvent::ValidationStarted { file, validation } => {
                self.on_validation_started(&file, &validation);
            }
            ProgressEvent::ValidationProgress { file, progress } => {
                self.on_validation_progress(&file, progress);
            }
            ProgressEvent::ValidationComplete { file, valid } => {
                self.on_validation_complete(&file, valid);
            }
            ProgressEvent::RetryAttempt { url, attempt, max_attempts } => {
                self.on_retry_attempt(&url, attempt, max_attempts);
            }
            ProgressEvent::Error { url, error } => {
                self.on_error(&url, &error);
            }
        })
    }
}

/// Simple console progress reporter implementation
#[derive(Debug, Default)]
pub struct ConsoleProgressReporter {
    pub verbose: bool,
}

impl ConsoleProgressReporter {
    pub fn new(verbose: bool) -> Self {
        Self { verbose }
    }
}

impl ProgressReporter for ConsoleProgressReporter {
    fn on_download_started(&self, url: &str, total_size: Option<u64>) {
        if self.verbose {
            match total_size {
                Some(size) => println!("📥 Starting download: {} ({} bytes)", url, size),
                None => println!("📥 Starting download: {}", url),
            }
        }
    }

    fn on_download_progress(&self, url: &str, downloaded: u64, total: Option<u64>, speed_bps: f64) {
        if self.verbose {
            let speed_mb = speed_bps / 1_000_000.0;
            match total {
                Some(total) => {
                    let percent = (downloaded as f64 / total as f64) * 100.0;
                    println!("⏬ {}: {:.1}% ({}/{} bytes, {:.1} MB/s)",
                        url, percent, downloaded, total, speed_mb);
                }
                None => {
                    println!("⏬ {}: {} bytes downloaded ({:.1} MB/s)",
                        url, downloaded, speed_mb);
                }
            }
        }
    }

    fn on_download_complete(&self, url: &str, final_size: u64) {
        println!("✅ Download complete: {} ({} bytes)", url, final_size);
    }

    fn on_validation_started(&self, file: &str, validation: &crate::downloader::core::validation::FileValidation) {
        if self.verbose {
            let mut algos = Vec::new();
            if validation.xxhash64_base64.is_some() { algos.push("XXHASH64"); }
            if validation.expected_size.is_some() { algos.push("SIZE"); }

            let algo_str = if algos.is_empty() { "NONE".to_string() } else { algos.join("+") };
            println!("🔍 Validating {}: {}", algo_str, file);
        }
    }

    fn on_validation_complete(&self, file: &str, valid: bool) {
        let icon = if valid { "✅" } else { "❌" };
        println!("{} Validation {}: {}", icon, if valid { "passed" } else { "failed" }, file);
    }

    fn on_retry_attempt(&self, url: &str, attempt: usize, max_attempts: usize) {
        println!("🔄 Retry {}/{} for: {}", attempt, max_attempts, url);
    }

    fn on_error(&self, url: &str, error: &str) {
        eprintln!("❌ Error downloading {}: {}", url, error);
    }
}

/// Null progress reporter that does nothing
#[derive(Debug, Default)]
pub struct NullProgressReporter;

impl ProgressReporter for NullProgressReporter {}

/// Composite progress reporter that forwards events to multiple reporters
pub struct CompositeProgressReporter {
    reporters: Vec<Box<dyn ProgressReporter>>,
}

impl std::fmt::Debug for CompositeProgressReporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeProgressReporter")
            .field("reporters_count", &self.reporters.len())
            .finish()
    }
}

impl CompositeProgressReporter {
    pub fn new() -> Self {
        Self {
            reporters: Vec::new(),
        }
    }

    pub fn add_reporter<R: ProgressReporter + 'static>(mut self, reporter: R) -> Self {
        self.reporters.push(Box::new(reporter));
        self
    }
}

impl Default for CompositeProgressReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressReporter for CompositeProgressReporter {
    fn on_download_started(&self, url: &str, total_size: Option<u64>) {
        for reporter in &self.reporters {
            reporter.on_download_started(url, total_size);
        }
    }

    fn on_download_progress(&self, url: &str, downloaded: u64, total: Option<u64>, speed_bps: f64) {
        for reporter in &self.reporters {
            reporter.on_download_progress(url, downloaded, total, speed_bps);
        }
    }

    fn on_download_complete(&self, url: &str, final_size: u64) {
        for reporter in &self.reporters {
            reporter.on_download_complete(url, final_size);
        }
    }

    fn on_validation_started(&self, file: &str, validation: &crate::downloader::core::validation::FileValidation) {
        for reporter in &self.reporters {
            reporter.on_validation_started(file, validation);
        }
    }

    fn on_validation_progress(&self, file: &str, progress: f64) {
        for reporter in &self.reporters {
            reporter.on_validation_progress(file, progress);
        }
    }

    fn on_validation_complete(&self, file: &str, valid: bool) {
        for reporter in &self.reporters {
            reporter.on_validation_complete(file, valid);
        }
    }

    fn on_retry_attempt(&self, url: &str, attempt: usize, max_attempts: usize) {
        for reporter in &self.reporters {
            reporter.on_retry_attempt(url, attempt, max_attempts);
        }
    }

    fn on_error(&self, url: &str, error: &str) {
        for reporter in &self.reporters {
            reporter.on_error(url, error);
        }
    }
}
