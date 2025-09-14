//! Built-in progress reporters with good defaults
//!
//! Provides easy-to-use progress reporters that handle the complexity of progress tracking
//! while offering customization options for different use cases.

use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::io::{self, Write};
use tokio::sync::RwLock;

use crate::ProgressReporter;

/// Style configuration for the dashboard progress reporter
#[derive(Debug, Clone, Copy)]
pub enum DashboardStyle {
    /// Full dashboard with detailed statistics and active operations
    Full,
    /// Compact display with essential information only
    Compact,
    /// Quiet mode with minimal output (errors and final summary only)
    Quiet,
}

/// File operation status for tracking concurrent operations
#[derive(Debug, Clone)]
pub enum OperationStatus {
    Downloading {
        progress: f64,
        speed_mbps: f64,
        downloaded_mb: f64,
        total_mb: Option<f64>,
    },
    Validating {
        algorithms: Vec<String>,
        progress: f64,
    },
    Completed {
        size_mb: f64,
        validation_passed: bool,
    },
    Failed {
        error: String,
    },
}

/// Warning entry for tracking warnings in the dashboard
#[derive(Debug, Clone)]
pub struct WarningEntry {
    pub message: String,
    pub timestamp: Instant,
    pub filename: String,
}

/// Built-in dashboard-style progress reporter
pub struct DashboardProgressReporter {
    operations: Arc<RwLock<HashMap<String, OperationStatus>>>,
    warnings: Arc<RwLock<Vec<WarningEntry>>>,
    start_time: Instant,
    last_refresh: Arc<RwLock<Instant>>,
    update_mutex: Arc<tokio::sync::Mutex<()>>,
    style: DashboardStyle,
    refresh_rate: Duration,
}

impl DashboardProgressReporter {
    /// Create a new dashboard progress reporter with default settings
    pub fn new() -> Self {
        Self::with_style(DashboardStyle::Full)
    }

    /// Create a new dashboard progress reporter with custom style
    pub fn with_style(style: DashboardStyle) -> Self {
        Self {
            operations: Arc::new(RwLock::new(HashMap::new())),
            warnings: Arc::new(RwLock::new(Vec::new())),
            start_time: Instant::now(),
            last_refresh: Arc::new(RwLock::new(Instant::now())),
            update_mutex: Arc::new(tokio::sync::Mutex::new(())),
            style,
            refresh_rate: Duration::from_millis(500),
        }
    }

    /// Set the refresh rate for the display
    pub fn with_refresh_rate(mut self, rate: Duration) -> Self {
        self.refresh_rate = rate;
        self
    }

    /// Extract filename from URL or file path, removing query parameters
    fn extract_filename(url: &str) -> String {
        // First, get the last part of the path (after / or \)
        let path_part = url.split('/').last()
            .unwrap_or_else(|| url.split('\\').last().unwrap_or(url));

        // Then remove query parameters (everything after ?)
        let clean_filename = path_part.split('?').next().unwrap_or(path_part);

        // Also remove fragment identifiers (everything after #)
        let clean_filename = clean_filename.split('#').next().unwrap_or(clean_filename);

        clean_filename.to_string()
    }

    /// Check if display should be refreshed
    async fn should_refresh(&self) -> bool {
        let now = Instant::now();
        let mut last_refresh = self.last_refresh.write().await;
        if now.duration_since(*last_refresh) >= self.refresh_rate {
            *last_refresh = now;
            true
        } else {
            false
        }
    }

    /// Update the display based on current style
    async fn update_display(&self) {
        if matches!(self.style, DashboardStyle::Quiet) {
            return; // Quiet mode doesn't show progress
        }

        if !self.should_refresh().await {
            return;
        }

        // Lock to prevent simultaneous updates from different async tasks
        let _update_lock = self.update_mutex.lock().await;

        match self.style {
            DashboardStyle::Full => self.update_full_display().await,
            DashboardStyle::Compact => self.update_compact_display().await,
            DashboardStyle::Quiet => {} // Already handled above
        }
    }

    /// Update full dashboard display
    async fn update_full_display(&self) {
        // Clear screen
        io::stdout().flush().unwrap();
        print!("\x1b[2J\x1b[H");
        io::stdout().flush().unwrap();

        let elapsed = self.start_time.elapsed();
        println!("🚀 Download Progress Dashboard");
        println!("⏱️  Elapsed: {:.1}s", elapsed.as_secs_f64());
        println!();

        let operations = self.operations.read().await;
        let warnings = self.warnings.read().await;

        let mut downloading = 0;
        let mut validating = 0;
        let mut completed = 0;
        let mut failed = 0;
        let mut total_speed = 0.0;

        // Count status types and calculate total speed
        for status in operations.values() {
            match status {
                OperationStatus::Downloading { speed_mbps, .. } => {
                    downloading += 1;
                    total_speed += speed_mbps;
                },
                OperationStatus::Validating { .. } => validating += 1,
                OperationStatus::Completed { .. } => completed += 1,
                OperationStatus::Failed { .. } => failed += 1,
            }
        }

        // Summary line with warnings count
        let recent_warnings = warnings.iter()
            .filter(|w| w.timestamp.elapsed() < Duration::from_secs(300)) // Last 5 minutes
            .count();
        println!("📊 Status: {} downloading, {} validating, {} completed, {} failed, {} warnings (Total: {:.1} MB/s)",
                 downloading, validating, completed, failed, recent_warnings, total_speed);
        println!();

        // Show active operations
        println!("🔄 Active Operations:");
        let mut active_count = 0;
        for (filename, status) in operations.iter() {
            match status {
                OperationStatus::Downloading { progress, speed_mbps, downloaded_mb, total_mb } => {
                    let total_str = match total_mb {
                        Some(total) => format!("/{:.1}", total),
                        None => String::new(),
                    };
                    println!("  📥 {} - {:.1}% ({:.1}{} MB, {:.1} MB/s)",
                             filename, progress, downloaded_mb, total_str, speed_mbps);
                    active_count += 1;
                },
                OperationStatus::Validating { algorithms, progress } => {
                    let algo_str = algorithms.join("+");
                    println!("  🔍 {} - Validating {} ({:.1}%)",
                             filename, algo_str, progress);
                    active_count += 1;
                },
                _ => {}
            }
        }

        if active_count == 0 {
            println!("  (No active operations)");
        }

        println!();

        // Show recent completions/failures
        println!("📋 Recent Results:");
        let mut recent: Vec<_> = operations.iter().collect();
        recent.sort_by_key(|(filename, _)| *filename);

        let mut shown = 0;
        for (filename, status) in recent.iter().rev() {
            if shown >= 8 { break; }
            match status {
                OperationStatus::Completed { size_mb, validation_passed } => {
                    let validation_icon = if *validation_passed { "✅" } else { "⚠️" };
                    let status_text = if *validation_passed { "OK" } else { "VALIDATION FAILED" };
                    println!("  {} {} - {:.1} MB ({})", validation_icon, filename, size_mb, status_text);
                    shown += 1;
                },
                OperationStatus::Failed { error } => {
                    let display_error = if error.len() > 50 {
                        format!("{}...", &error[..50])
                    } else {
                        error.clone()
                    };
                    println!("  ❌ {} - ERROR: {}", filename, display_error);
                    shown += 1;
                },
                _ => {}
            }
        }

        if shown == 0 {
            println!("  (No completed operations yet)");
        }

        // Show recent warnings (last 10)
        if !warnings.is_empty() {
            println!();
            println!("⚠️ Recent Warnings:");
            let mut warning_count = 0;
            for warning in warnings.iter().rev() {
                if warning_count >= 10 { break; }
                let elapsed = warning.timestamp.elapsed().as_secs();
                let time_str = if elapsed < 60 {
                    format!("{}s ago", elapsed)
                } else {
                    format!("{}m ago", elapsed / 60)
                };

                let display_message = if warning.message.len() > 80 {
                    format!("{}...", &warning.message[..80])
                } else {
                    warning.message.clone()
                };

                println!("  ⚠️  {} - {} ({})", warning.filename, display_message, time_str);
                warning_count += 1;
            }
            if warning_count == 0 {
                println!("  (No recent warnings)");
            }
        }

        io::stdout().flush().unwrap();
    }

    /// Update compact display
    async fn update_compact_display(&self) {
        let operations = self.operations.read().await;
        let mut downloading = 0;
        let mut completed = 0;
        let mut failed = 0;
        let mut total_speed = 0.0;

        for status in operations.values() {
            match status {
                OperationStatus::Downloading { speed_mbps, .. } => {
                    downloading += 1;
                    total_speed += speed_mbps;
                },
                OperationStatus::Validating { .. } => downloading += 1, // Count as active
                OperationStatus::Completed { .. } => completed += 1,
                OperationStatus::Failed { .. } => failed += 1,
            }
        }

        // Simple one-line status update with warnings count
        let warnings = self.warnings.read().await;
        let recent_warnings = warnings.iter()
            .filter(|w| w.timestamp.elapsed() < Duration::from_secs(300)) // Last 5 minutes
            .count();
        let elapsed = self.start_time.elapsed();
        print!("\r🚀 Progress: {} active, {} done, {} failed, {} warnings ({:.1} MB/s, {:.0}s)              ",
               downloading, completed, failed, recent_warnings, total_speed, elapsed.as_secs_f64());
        io::stdout().flush().unwrap();
    }
}

impl Default for DashboardProgressReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressReporter for DashboardProgressReporter {
    fn on_download_started(&self, url: &str, total_size: Option<u64>) {
        let filename = Self::extract_filename(url);
        let operations = self.operations.clone();
        let total_mb = total_size.map(|size| size as f64 / 1_048_576.0);

        tokio::spawn(async move {
            let mut ops = operations.write().await;
            ops.insert(filename, OperationStatus::Downloading {
                progress: 0.0,
                speed_mbps: 0.0,
                downloaded_mb: 0.0,
                total_mb,
            });
        });
    }

    fn on_download_progress(&self, url: &str, downloaded: u64, total: Option<u64>, speed_bps: f64) {
        let filename = Self::extract_filename(url);
        let operations = self.operations.clone();
        let reporter = Arc::new(self.clone());

        tokio::spawn(async move {
            let progress = match total {
                Some(total) => (downloaded as f64 / total as f64) * 100.0,
                None => 0.0,
            };

            let downloaded_mb = downloaded as f64 / 1_048_576.0;
            let total_mb = total.map(|t| t as f64 / 1_048_576.0);
            let speed_mbps = speed_bps / 1_048_576.0;

            let mut ops = operations.write().await;
            ops.insert(filename, OperationStatus::Downloading {
                progress,
                speed_mbps,
                downloaded_mb,
                total_mb,
            });

            drop(ops);
            reporter.update_display().await;
        });
    }

    fn on_download_complete(&self, url: &str, final_size: u64) {
        let filename = Self::extract_filename(url);
        let operations = self.operations.clone();

        tokio::spawn(async move {
            let size_mb = final_size as f64 / 1_048_576.0;
            let mut ops = operations.write().await;
            ops.insert(filename, OperationStatus::Completed {
                size_mb,
                validation_passed: true, // Will be updated by validation if it runs
            });
        });
    }

    fn on_validation_started(&self, file: &str, validation: &crate::FileValidation) {
        let filename = Self::extract_filename(file);
        let operations = self.operations.clone();
        let reporter = Arc::new(self.clone());

        // Extract actual algorithms from validation config
        let mut algorithms = Vec::new();
        if validation.xxhash64_base64.is_some() {
            algorithms.push("XXHASH64".to_string());
        }
        if validation.expected_size.is_some() {
            algorithms.push("SIZE".to_string());
        }

        tokio::spawn(async move {
            let mut ops = operations.write().await;
            ops.insert(filename, OperationStatus::Validating {
                algorithms,
                progress: 0.0,
            });

            drop(ops);
            reporter.update_display().await;
        });
    }

    fn on_validation_progress(&self, file: &str, progress: f64) {
        let filename = Self::extract_filename(file);
        let operations = self.operations.clone();
        let reporter = Arc::new(self.clone());

        tokio::spawn(async move {
            let mut ops = operations.write().await;
            if let Some(OperationStatus::Validating { algorithms, .. }) = ops.get(&filename) {
                let algorithms = algorithms.clone();
                ops.insert(filename, OperationStatus::Validating {
                    algorithms,
                    progress: progress * 100.0,
                });
            }

            drop(ops);
            reporter.update_display().await;
        });
    }

    fn on_validation_complete(&self, file: &str, valid: bool) {
        let filename = Self::extract_filename(file);
        let operations = self.operations.clone();
        let reporter = Arc::new(self.clone());
        let file_path = file.to_string();

        // Print validation failure immediately for all styles except quiet
        if !valid && !matches!(self.style, DashboardStyle::Quiet) {
            eprintln!("\n⚠️ VALIDATION FAILED: {} - Hash mismatch or size error", filename);
            io::stderr().flush().unwrap();
        }

        tokio::spawn(async move {
            let mut ops = operations.write().await;

            // Read actual file size from disk
            let size_mb = match tokio::fs::metadata(&file_path).await {
                Ok(metadata) => metadata.len() as f64 / 1_048_576.0,
                Err(_) => {
                    // If file doesn't exist, try to get from cached value
                    match ops.get(&filename) {
                        Some(OperationStatus::Completed { size_mb, .. }) => *size_mb,
                        _ => 0.0,
                    }
                },
            };

            ops.insert(filename, OperationStatus::Completed {
                size_mb,
                validation_passed: valid,
            });

            drop(ops);
            reporter.update_display().await;
        });
    }

    fn on_retry_attempt(&self, url: &str, attempt: usize, max_attempts: usize) {
        let filename = Self::extract_filename(url);
        let operations = self.operations.clone();

        // Print retry notification immediately for all styles except quiet
        if !matches!(self.style, DashboardStyle::Quiet) {
            eprintln!("\n🔄 RETRY {}/{}: {} - Previous attempt failed", attempt, max_attempts, filename);
            io::stderr().flush().unwrap();
        }

        tokio::spawn(async move {
            let mut ops = operations.write().await;
            ops.insert(filename, OperationStatus::Downloading {
                progress: 0.0,
                speed_mbps: 0.0,
                downloaded_mb: 0.0,
                total_mb: None,
            });
        });
    }

    fn on_warning(&self, url: &str, message: &str) {
        let filename = Self::extract_filename(url);
        let warnings = self.warnings.clone();
        let reporter = Arc::new(self.clone());
        let message_string = message.to_string();

        // Print warning immediately to stderr for visibility (all styles except quiet)
        if !matches!(self.style, DashboardStyle::Quiet) {
            eprintln!("\n⚠️ WARNING: {} - {}", filename, message);
            io::stderr().flush().unwrap();
        }

        tokio::spawn(async move {
            {
                let mut warnings_list = warnings.write().await;
                warnings_list.push(WarningEntry {
                    message: message_string,
                    timestamp: Instant::now(),
                    filename,
                });

                // Keep only the last 50 warnings to prevent memory bloat
                if warnings_list.len() > 50 {
                    let excess = warnings_list.len() - 50;
                    warnings_list.drain(0..excess);
                }
            }
            reporter.update_display().await;
        });
    }

    fn on_error(&self, url: &str, error: &str) {
        let filename = Self::extract_filename(url);
        let operations = self.operations.clone();
        let reporter = Arc::new(self.clone());
        let error_string = error.to_string();

        // Print error immediately to stderr for visibility (all styles)
        eprintln!("\n❌ ERROR: {} - {}", filename, error);
        io::stderr().flush().unwrap();

        tokio::spawn(async move {
            let mut ops = operations.write().await;
            ops.insert(filename, OperationStatus::Failed {
                error: error_string,
            });

            drop(ops);
            reporter.update_display().await;
        });
    }
}

// Manual Clone implementation since we can't derive it due to Instant
impl Clone for DashboardProgressReporter {
    fn clone(&self) -> Self {
        Self {
            operations: Arc::clone(&self.operations),
            warnings: Arc::clone(&self.warnings),
            start_time: self.start_time,
            last_refresh: Arc::clone(&self.last_refresh),
            update_mutex: Arc::clone(&self.update_mutex),
            style: self.style,
            refresh_rate: self.refresh_rate,
        }
    }
}

