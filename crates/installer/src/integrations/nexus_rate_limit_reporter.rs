//! Nexus Rate Limit aware progress reporter
//!
//! This module provides a progress reporter that periodically displays
//! Nexus API rate limit information alongside download progress.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{ProgressReporter, DashboardProgressReporter, DashboardStyle};
use crate::downloader::sources::nexus::get_nexus_api;

/// Progress reporter that combines dashboard display with Nexus rate limit monitoring
pub struct NexusRateLimitProgressReporter {
    dashboard: DashboardProgressReporter,
    last_rate_limit_check: AtomicU64, // Store as timestamp
    rate_limit_check_interval_secs: u64,
    show_rate_limits: bool,
}

impl std::fmt::Debug for NexusRateLimitProgressReporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NexusRateLimitProgressReporter")
            .field("show_rate_limits", &self.show_rate_limits)
            .field("rate_limit_check_interval_secs", &self.rate_limit_check_interval_secs)
            .finish()
    }
}

impl NexusRateLimitProgressReporter {
    /// Create a new Nexus rate limit aware progress reporter
    pub fn new() -> Self {
        Self::with_style(DashboardStyle::Full)
    }

    /// Create with custom dashboard style
    pub fn with_style(style: DashboardStyle) -> Self {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            dashboard: DashboardProgressReporter::with_style(style),
            last_rate_limit_check: AtomicU64::new(now_secs),
            rate_limit_check_interval_secs: 15, // Check every 15 seconds
            show_rate_limits: true,
        }
    }

    /// Set how often to check and display rate limit info
    pub fn with_rate_limit_interval(mut self, interval: Duration) -> Self {
        self.rate_limit_check_interval_secs = interval.as_secs();
        self
    }

    /// Enable or disable rate limit display
    pub fn with_rate_limits(mut self, show: bool) -> Self {
        self.show_rate_limits = show;
        self
    }

    /// Set refresh rate for the underlying dashboard
    pub fn with_refresh_rate(mut self, rate: Duration) -> Self {
        self.dashboard = self.dashboard.with_refresh_rate(rate);
        self
    }

    /// Check if we should display rate limit info and update time if so
    fn should_check_rate_limit(&self) -> bool {
        if !self.show_rate_limits {
            return false;
        }

        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let last_check = self.last_rate_limit_check.load(Ordering::Relaxed);

        if now_secs.saturating_sub(last_check) >= self.rate_limit_check_interval_secs {
            // Try to update the timestamp, but don't worry if another thread beats us
            self.last_rate_limit_check.store(now_secs, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Display current Nexus rate limit status
    fn display_rate_limit_info(&self) {
        // Try to get Nexus API - it may not be initialized if no Nexus downloads
        if let Ok(api) = get_nexus_api() {
            if let Some(rate_limit) = api.get_rate_limit_status() {
                // Simple inline display - just print the info without complex terminal manipulation
                println!("📊 {}", rate_limit.format_status());

                if rate_limit.is_blocked {
                    println!("⚠️  API Rate Limited! {}", rate_limit.time_until_reset());
                }
            }
        }
    }
}

impl Default for NexusRateLimitProgressReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressReporter for NexusRateLimitProgressReporter {
    fn on_download_started(&self, url: &str, total_size: Option<u64>) {
        // Check rate limits when downloads start
        if self.should_check_rate_limit() {
            self.display_rate_limit_info();
        }

        // Forward to dashboard
        self.dashboard.on_download_started(url, total_size);
    }

    fn on_download_progress(&self, url: &str, downloaded: u64, total: Option<u64>, speed_bps: f64) {
        // Forward to dashboard first
        self.dashboard.on_download_progress(url, downloaded, total, speed_bps);

        // Periodic rate limit check during progress updates
        if self.should_check_rate_limit() {
            self.display_rate_limit_info();
        }
    }

    fn on_download_complete(&self, url: &str, final_size: u64) {
        // Forward to dashboard
        self.dashboard.on_download_complete(url, final_size);

        // Show rate limit status after downloads complete
        if self.should_check_rate_limit() {
            self.display_rate_limit_info();
        }
    }

    fn on_validation_started(&self, file: &str, validation: &crate::downloader::core::validation::FileValidation) {
        self.dashboard.on_validation_started(file, validation);
    }

    fn on_validation_progress(&self, file: &str, progress: f64) {
        self.dashboard.on_validation_progress(file, progress);
    }

    fn on_validation_complete(&self, file: &str, valid: bool) {
        self.dashboard.on_validation_complete(file, valid);
    }

    fn on_retry_attempt(&self, url: &str, attempt: usize, max_attempts: usize) {
        // Forward to dashboard
        self.dashboard.on_retry_attempt(url, attempt, max_attempts);

        // Show rate limits during retries (often due to rate limiting)
        println!("🔄 Checking rate limits due to retry attempt...");
        if let Ok(api) = get_nexus_api() {
            if let Some(rate_limit) = api.get_rate_limit_status() {
                println!("📊 {}", rate_limit.format_status());
            }
        }
    }

    fn on_error(&self, url: &str, error: &str) {
        // Forward to dashboard
        self.dashboard.on_error(url, error);

        // Check if error might be rate limit related
        if error.contains("rate") || error.contains("429") || error.contains("Too Many Requests") {
            println!("🚫 Possible rate limit error detected!");
            if let Ok(api) = get_nexus_api() {
                if let Some(rate_limit) = api.get_rate_limit_status() {
                    println!("📊 {}", rate_limit.format_status());
                    if rate_limit.is_blocked {
                        println!("⚠️  You are currently rate limited! {}", rate_limit.time_until_reset());
                    }
                }
            }
        }
    }
}
