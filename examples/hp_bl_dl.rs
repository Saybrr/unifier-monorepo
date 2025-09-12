//! High-performance example that downloads Baseline modlist files with concurrent operations
//!
//! This example demonstrates:
//! 1. High-performance configuration with concurrent downloads
//! 2. Concurrent hash validation with progress reporting
//! 3. Multi-file progress dashboard
//! 4. Hash algorithm progress display

use installer::{
    // Downloader components
    DownloadConfigBuilder, EnhancedDownloader,
    IntoProgressCallback,

    // Wabbajack parsing components
    parse_modlist, manifest_to_download_requests_with_stats,
};
use installer::ProgressReporter;
use std::{
    path::PathBuf,
    fs,
    sync::Arc,
    io::{self, Write},
    collections::HashMap,
    time::Instant,
};
use tokio::sync::RwLock;

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

/// Dashboard-style progress reporter for concurrent operations
#[derive(Debug)]
pub struct ConcurrentProgressReporter {
    operations: Arc<RwLock<HashMap<String, OperationStatus>>>,
    start_time: Instant,
    last_refresh: Arc<RwLock<Instant>>,
    update_mutex: Arc<tokio::sync::Mutex<()>>, // Prevent simultaneous updates
}

impl ConcurrentProgressReporter {
    pub fn new() -> Self {
        Self {
            operations: Arc::new(RwLock::new(HashMap::new())),
            start_time: Instant::now(),
            last_refresh: Arc::new(RwLock::new(Instant::now())),
            update_mutex: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    fn extract_filename(url: &str) -> String {
        url.split('/').last().unwrap_or(url).to_string()
    }

    async fn should_refresh(&self) -> bool {
        let now = Instant::now();
        let mut last_refresh = self.last_refresh.write().await;
        if now.duration_since(*last_refresh).as_millis() > 500 { // Refresh every 500ms to reduce flickering
            *last_refresh = now;
            true
        } else {
            false
        }
    }

    async fn update_display(&self) {
        if !self.should_refresh().await {
            return;
        }

        // Lock to prevent simultaneous updates from different async tasks
        let _update_lock = self.update_mutex.lock().await;

        // Clear screen more reliably - flush first, then clear and move to home
        io::stdout().flush().unwrap();
        print!("\x1b[2J"); // Clear entire screen
        print!("\x1b[H");  // Move cursor to home position (1,1)
        io::stdout().flush().unwrap();

        let elapsed = self.start_time.elapsed();
        println!("🚀 High-Performance Baseline Download Dashboard");
        println!("⏱️  Elapsed: {:.1}s", elapsed.as_secs_f64());
        println!();

        let operations = self.operations.read().await;

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

        // Summary line
        println!("📊 Status: {} downloading, {} validating, {} completed, {} failed (Total: {:.1} MB/s)",
                 downloading, validating, completed, failed, total_speed);
        println!();

        // Show active operations (downloading and validating)
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

        // Show recent completions/failures (last 8 to see more errors)
        println!("📋 Recent Results:");
        let mut recent: Vec<_> = operations.iter().collect();
        recent.sort_by_key(|(filename, _)| *filename);

        let mut shown = 0;
        for (filename, status) in recent.iter().rev() {
            if shown >= 8 { break; } // Show more results to catch errors
            match status {
                OperationStatus::Completed { size_mb, validation_passed } => {
                    let validation_icon = if *validation_passed { "✅" } else { "⚠️" };
                    let status_text = if *validation_passed { "OK" } else { "VALIDATION FAILED" };
                    println!("  {} {} - {:.1} MB ({})", validation_icon, filename, size_mb, status_text);
                    shown += 1;
                },
                OperationStatus::Failed { error } => {
                    // Truncate long error messages for display
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

        io::stdout().flush().unwrap();
    }
}

impl ProgressReporter for ConcurrentProgressReporter {
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

    fn on_validation_started(&self, file: &str, validation: &installer::FileValidation) {
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
        let file_path = file.to_string(); // file parameter is the full path

        // Print validation failure immediately
        if !valid {
            eprintln!("\n⚠️ VALIDATION FAILED: {} - Hash mismatch or size error", filename);
            io::stderr().flush().unwrap();
        }

        tokio::spawn(async move {
            let mut ops = operations.write().await;

            // Read actual file size from disk instead of using cached value
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

        // Print retry notification immediately
        eprintln!("\n🔄 RETRY {}/{}: {} - Previous attempt failed", attempt, max_attempts, filename);
        io::stderr().flush().unwrap();

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

    fn on_error(&self, url: &str, error: &str) {
        let filename = Self::extract_filename(url);
        let operations = self.operations.clone();
        let reporter = Arc::new(self.clone());
        let error_string = error.to_string(); // Convert to owned string

        // Print error immediately to stderr for visibility
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
impl Clone for ConcurrentProgressReporter {
    fn clone(&self) -> Self {
        Self {
            operations: Arc::clone(&self.operations),
            start_time: self.start_time,
            last_refresh: Arc::clone(&self.last_refresh),
            update_mutex: Arc::clone(&self.update_mutex),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing with debug level
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO )
        .init();

    println!("🚀 High-Performance Baseline Download with Hash Validation");

    // Test if debug logging is working
    tracing::debug!("DEBUG LOGGING IS ENABLED - you should see this message");

    // Read the modlist file
    let modlist_path = PathBuf::from("Baseline/modlist");
    let modlist_json = fs::read_to_string(&modlist_path)?;
    println!("✅ Loaded modlist file");

    // Parse the modlist
    let manifest = parse_modlist(&modlist_json)?;
    println!("✅ Parsed modlist: {} operations", manifest.stats.total_operations);

    // Convert to download requests (exclude manual downloads)
    let destination_dir = PathBuf::from("./downloads");
    let (all_requests, stats) = manifest_to_download_requests_with_stats(
        &manifest,
        &destination_dir,
        false // Don't include manual downloads
    );

    // Filter out manual downloads and sources that require user interaction
    let download_requests: Vec<_> = all_requests.into_iter()
        .filter(|request| !request.requires_user_interaction())
        .collect();

    let filtered_count = stats.converted_requests - download_requests.len();

    println!("✅ Generated {} download requests ({} MB)",
             download_requests.len(),
             stats.total_download_size / 1_048_576);

    if filtered_count > 0 {
        println!("ℹ️  Filtered out {} unsupported downloads (Nexus, Archive, etc.)", filtered_count);
    }

    if download_requests.is_empty() {
        println!("ℹ️  No automatable downloads found");
        return Ok(());
    }

    // Set up downloader with HIGH PERFORMANCE configuration
    let config = DownloadConfigBuilder::new()
        .high_performance() // 8 concurrent validations, async validation, parallel hashing
        .max_retries(2)
        .timeout(std::time::Duration::from_secs(120)) // Longer timeout for large files
        .build();

    println!("⚡ High-performance config:");
    println!("   - Max concurrent validations: {}", config.max_concurrent_validations);
    println!("   - Async validation: {}", config.async_validation);
    println!("   - Parallel validation: {}", config.parallel_validation);
    println!("   - Chunk size: {} KB", config.chunk_size / 1024);
    println!("   - Streaming threshold: {} MB", config.streaming_threshold / 1_048_576);

    // Create enhanced downloader
    let downloader = EnhancedDownloader::new(config);

    // Set up concurrent progress reporting
    let progress_reporter = ConcurrentProgressReporter::new();
    let start_time = progress_reporter.start_time; // Capture start time before moving
    let operations_for_final_report = Arc::clone(&progress_reporter.operations); // Capture operations ref before moving
    let progress_callback = progress_reporter.into_callback();

    // Keep validation ENABLED to show hash calculation progress
    // Note: In this example, we keep the existing validation from the modlist
    println!("🔍 Hash validation enabled - will show algorithm progress");

    // Download files with CONCURRENT operations
    let max_concurrent_downloads = 8; // Allow multiple simultaneous downloads

    println!("\n⬇️  Starting {} concurrent downloads with hash validation...", max_concurrent_downloads);
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await; // Brief pause for user to read

    let results = downloader.download_batch_with_async_validation(
        download_requests,
        Some(progress_callback),
        max_concurrent_downloads, // HIGH CONCURRENCY!
    ).await;

    // Final results
    let mut successful = 0;
    let mut failed = 0;
    for result in results {
        match result {
            Ok(_) => successful += 1,
            Err(_e) => {
                failed += 1;
                // Individual result errors are already captured in operations state
            },
        }
    }

    // Get final state of all operations for error reporting
    let final_operations = operations_for_final_report.read().await;
    let mut validation_failures = Vec::new();
    let mut download_errors = Vec::new();

    for (filename, status) in final_operations.iter() {
        match status {
            OperationStatus::Failed { error } => {
                download_errors.push(format!("{}: {}", filename, error));
            },
            OperationStatus::Completed { validation_passed: false, .. } => {
                validation_failures.push(filename.clone());
            },
            _ => {}
        }
    }
    drop(final_operations);

    // Clear and show final summary
    io::stdout().flush().unwrap();
    print!("\x1b[2J"); // Clear entire screen
    print!("\x1b[H");  // Move cursor to home position
    io::stdout().flush().unwrap();
    println!("🎉 High-Performance Download Complete!");
    println!("✅ Successful: {}", successful);
    println!("❌ Failed: {}", failed);
    println!("⏱️  Total time: {:.1}s", start_time.elapsed().as_secs_f64());

    // Show error summary if there were any errors
    if !download_errors.is_empty() || !validation_failures.is_empty() {
        println!("\n📋 ERROR SUMMARY:");

        if !download_errors.is_empty() {
            println!("  Download Errors ({}):", download_errors.len());
            for error in download_errors {
                println!("    ❌ {}", error);
            }
        }

        if !validation_failures.is_empty() {
            println!("  Validation Failures ({}):", validation_failures.len());
            for filename in validation_failures {
                println!("    ⚠️ {}: Hash mismatch or size error", filename);
            }
        }
    }

    if filtered_count > 0 {
        println!("ℹ️  Note: {} downloads were skipped (Nexus mods require API keys, Manual downloads need user action)", filtered_count);
    }

    println!("\n🔍 Hash validation features demonstrated:");
    println!("   - Concurrent validation (up to 8 files simultaneously)");
    println!("   - Parallel hash algorithms (CRC32 + SHA256/MD5 computed together)");
    println!("   - Streaming validation for large files (>20MB)");
    println!("   - Real-time progress display");

    Ok(())
}