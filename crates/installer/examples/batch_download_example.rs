//! Example demonstrating enhanced batch downloads with the installer downloader
//!
//! This example shows how to use the new improved API with:
//! - Builder pattern for configuration
//! - Enhanced progress reporting with traits
//! - Built-in performance metrics
//! - Better error handling with context
//!
//! Run this example with:
//! ```
//! cargo run --example batch_download_example
//! ```

use installer::{
    DownloadConfigBuilder, DownloadRequest, EnhancedDownloader,
    FileValidation, ProgressReporter, IntoProgressCallback, DownloadResult
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::tempdir;
use tokio::time::{Duration, Instant};

#[tokio::main]
async fn main() -> installer::Result<()> {
    // Add this line to enable tracing logs
    tracing_subscriber::fmt::init();

    println!("🚀 Starting batch download example");

    // Create a temporary directory for downloads
    let temp_dir = tempdir().unwrap();
    println!("📁 Download directory: {}", temp_dir.path().display());

    // Configure the downloader using the new builder pattern
    let config = DownloadConfigBuilder::new()
        .high_performance() // Optimizes for speed and concurrency
        .max_retries(3)
        .timeout(Duration::from_secs(60))
        .user_agent("enhanced-batch-downloader/2.0")
        .validation_retries(2)
        .build();

    // Save config values we'll need later before moving config
    let async_validation = config.async_validation;
    let validation_retries = config.validation_retries;

    // Create the downloader
    let downloader = EnhancedDownloader::new(config);

    // Get access to built-in metrics
    let metrics = downloader.metrics();

    // Create multiple download requests with different file sizes
    let requests = vec![
        // Small file (1KB)
        DownloadRequest::new(
            "http://localhost:80/bytes/1024",
            temp_dir.path()
        )
        .with_filename("small_file.bin")
        .with_validation(
            FileValidation::new()
                .with_expected_size(1024)
        ),

        // Medium file (10KB)
        DownloadRequest::new(
            "http://localhost:80/bytes/10240",
            temp_dir.path()
        )
        .with_filename("medium_file.bin")
        .with_validation(
            FileValidation::new()
                .with_expected_size(10240)
        ),

        // Large file (100KB)
        DownloadRequest::new(
            "http://localhost:80/bytes/102400",
            temp_dir.path()
        )
        .with_filename("large_file.bin")
        .with_validation(
            FileValidation::new()
                .with_expected_size(102400)
        ),

        // Another small file with mirror URL demonstration
        DownloadRequest::new(
            "http://localhost:80/status/500", // This will fail
            temp_dir.path()
        )
        .with_mirror_url("http://localhost:80/bytes/2048") // Fallback will work
        .with_filename("fallback_file.bin")
        .with_validation(
            FileValidation::new()
                .with_expected_size(2048)
        ),

        // File with intentional validation failure to demonstrate retry
        DownloadRequest::new(
            "http://localhost:80/bytes/5120",
            temp_dir.path()
        )
        .with_filename("validation_retry_test.bin")
        .with_validation(
            FileValidation::new()
                .with_expected_size(1024) // Wrong size to trigger validation failure
        ),

        // File from a different test endpoint
        DownloadRequest::new(
            "http://localhost:80/bytes/4096",
            temp_dir.path()
        )
        .with_filename("another_file.bin")
        .with_validation(
            FileValidation::new()
                .with_expected_size(4096)
        ),
    ];

    println!("📦 Preparing to download {} files", requests.len());

    // Set up enhanced progress tracking using the new trait-based system
    let completed_downloads = Arc::new(AtomicUsize::new(0));
    let total_downloads = requests.len();

    // Create a custom progress reporter using the new trait system
    #[derive(Debug)]
    struct BatchProgressReporter {
        completed: Arc<AtomicUsize>,
        total: usize,
    }

    impl ProgressReporter for BatchProgressReporter {
        fn on_download_started(&self, url: &str, total_size: Option<u64>) {
            println!("📥 Started: {} ({:?} bytes)",
                extract_filename_from_url(url), total_size);
        }

        fn on_download_progress(&self, url: &str, downloaded: u64, total: Option<u64>, speed_bps: f64) {
            let filename = extract_filename_from_url(url);
            if let Some(total) = total {
                let percent = (downloaded as f64 / total as f64) * 100.0;
                println!("   📊 {}: {:.1}% ({}/{} bytes) @ {:.1} KB/s",
                    filename, percent, downloaded, total, speed_bps / 1024.0);
            } else {
                println!("   📊 {}: {} bytes @ {:.1} KB/s",
                    filename, downloaded, speed_bps / 1024.0);
            }
        }

        fn on_download_complete(&self, url: &str, final_size: u64) {
            let completed_count = self.completed.fetch_add(1, Ordering::SeqCst) + 1;
            println!("✅ Completed: {} ({} bytes) [{}/{}]",
                extract_filename_from_url(url), final_size, completed_count, self.total);
        }

        fn on_validation_started(&self, file: &str) {
            println!("🔍 Validating: {}", extract_filename_from_path(file));
        }

        fn on_validation_progress(&self, file: &str, progress: f64) {
            let filename = extract_filename_from_path(file);
            println!("   🔍 Validating {}: {:.1}%", filename, progress * 100.0);
        }

        fn on_validation_complete(&self, file: &str, valid: bool) {
            let filename = extract_filename_from_path(file);
            if valid {
                println!("✅ Validation passed: {}", filename);
            } else {
                println!("❌ Validation failed: {}", filename);
            }
        }

        fn on_retry_attempt(&self, url: &str, attempt: usize, max_attempts: usize) {
            println!("🔄 Retry {}/{} for: {}",
                attempt, max_attempts, extract_filename_from_url(url));
        }

        fn on_error(&self, url: &str, error: &str) {
            println!("❌ Error downloading {}: {}",
                extract_filename_from_url(url), error);
        }
    }

    let progress_reporter = BatchProgressReporter {
        completed: completed_downloads.clone(),
        total: total_downloads,
    };

    // Convert the progress reporter to a callback using the new trait system
    let progress_callback = Some(progress_reporter.into_callback());

    // Perform batch download with async validation and concurrency limit
    let max_concurrent_downloads = 3;
    println!("🔄 Starting batch download with async validation (max {} concurrent)...", max_concurrent_downloads);
    println!("💡 Using async validation with {} validation retries", validation_retries);

    let batch_start = Instant::now();
    let results = downloader
        .download_batch_with_async_validation(requests, progress_callback, max_concurrent_downloads)
        .await;

    let batch_duration = batch_start.elapsed();
    println!("\n🎯 Batch download and validation completed in {:.2?}", batch_duration);

    // Analyze results
    let mut successful_downloads = 0;
    let mut failed_downloads = 0;
    let mut total_bytes = 0u64;
    let mut already_existed = 0;
    let mut pending_validations = 0;
    let mut validation_retry_failures = 0;

    println!("\n📊 Results Summary:");
    println!("{}", "─".repeat(60));

    for (i, result) in results.iter().enumerate() {
        match result {
            Ok(download_result) => {
                match download_result {
                    DownloadResult::Downloaded { size } => {
                        successful_downloads += 1;
                        total_bytes += size;
                        println!("✅ File {}: Downloaded {} bytes", i + 1, size);
                    }
                    DownloadResult::AlreadyExists { size } => {
                        already_existed += 1;
                        total_bytes += size;
                        println!("📋 File {}: Already existed ({} bytes)", i + 1, size);
                    }
                    DownloadResult::Resumed { size } => {
                        successful_downloads += 1;
                        total_bytes += size;
                        println!("⏯️  File {}: Resumed and completed ({} bytes)", i + 1, size);
                    }
                    DownloadResult::DownloadedPendingValidation { size, .. } => {
                        // This should not happen with the new implementation since we wait for validation
                        pending_validations += 1;
                        total_bytes += size;
                        println!("⏳ File {}: Downloaded, validation was pending ({} bytes)", i + 1, size);
                    }
                }
            }
            Err(e) => {
                failed_downloads += 1;
                println!("❌ File {}: Failed - {}", i + 1, e);
            }
        }
    }

    println!("{}", "─".repeat(60));
    println!("📈 Statistics:");
    println!("   • Total files: {}", results.len());
    println!("   • Successful: {}", successful_downloads);
    println!("   • Already existed: {}", already_existed);
    println!("   • Failed: {}", failed_downloads);
    if pending_validations > 0 {
        println!("   • Pending validations: {}", pending_validations);
    }
    println!("   • Total bytes processed: {} ({:.2} KB)", total_bytes, total_bytes as f64 / 1024.0);
    println!("   • Average speed: {:.2} KB/s",
        (total_bytes as f64 / 1024.0) / batch_duration.as_secs_f64());
    println!("   • Duration (including validation): {:.2?}", batch_duration);
    println!("   • Async validation enabled: {}", async_validation);
    println!("   • Max validation retries: {}", validation_retries);

    // Verify files exist
    println!("\n📁 Verifying downloaded files:");
    for entry in std::fs::read_dir(temp_dir.path()).unwrap() {
        let entry = entry.unwrap();
        let metadata = entry.metadata().unwrap();
        if metadata.is_file() {
            println!("   📄 {} ({} bytes)",
                entry.file_name().to_string_lossy(),
                metadata.len());
        }
    }

    // Demonstrate error handling patterns and validation retry behavior
    println!("\n🔍 Error handling and validation retry examples:");
    for (i, result) in results.iter().enumerate() {
        if let Err(e) = result {
            match e {
                installer::DownloadError::HttpRequest { url, source } => {
                    println!("   🌐 HTTP Error in file {}: {} ({})", i + 1, url, source);
                }
                installer::DownloadError::ValidationFailed { expected, actual, .. } => {
                    println!("   ❓ Validation Error in file {}: expected '{}', got '{}'",
                        i + 1, expected, actual);
                    if actual.contains("retry") || actual.contains("retries") {
                        println!("      ➡️ This error occurred after validation retries were attempted");
                        validation_retry_failures += 1;
                    }
                }
                installer::DownloadError::SizeMismatch { expected, actual, file: _, diff: _ } => {
                    println!("   📏 Size Mismatch in file {}: expected {} bytes, got {}",
                        i + 1, expected, actual);
                }
                installer::DownloadError::MaxRetriesExceeded { .. } => {
                    println!("   🔄 Max Retries Exceeded for file {}", i + 1);
                }
                _ => {
                    println!("   ❌ Other Error in file {}: {}", i + 1, e);
                }
            }
        }
    }

    if validation_retry_failures > 0 {
        println!("\n➡️ {} files failed validation even after {} retry attempts",
            validation_retry_failures, validation_retries);
        println!("   This demonstrates the automatic retry mechanism for validation failures.");
    }

    if failed_downloads == 0 {
        println!("\n🎉 All downloads and validations completed successfully!");
        println!("   • Async validation allowed downloads to complete without blocking");
        println!("   • All files passed validation on the first attempt or after retries");
    } else {
        println!("\n⚠️  Some downloads or validations failed, but batch operation completed.");
        println!("   • Failed items may include validation failures after all retry attempts");
        println!("   • The async validation system allowed other downloads to continue");
    }

    // Demonstrate the new built-in metrics system
    println!("\n📊 Performance Metrics (New Feature!):");
    println!("{}", "─".repeat(60));
    let metrics_snapshot = metrics.snapshot();
    println!("   Total Downloads: {}", metrics_snapshot.total_downloads);
    println!("   Successful Downloads: {}", metrics_snapshot.successful_downloads);
    println!("   Failed Downloads: {}", metrics_snapshot.failed_downloads);
    println!("   Total Bytes: {} ({:.2} MB)",
        metrics_snapshot.total_bytes,
        metrics_snapshot.total_bytes as f64 / 1_000_000.0);
    println!("   Success Rate: {:.1}%", metrics_snapshot.success_rate() * 100.0);
    println!("   Average File Size: {:.0} bytes", metrics_snapshot.average_size());
    println!("   Validation Failures: {}", metrics_snapshot.validation_failures);
    println!("   Retry Attempts: {}", metrics_snapshot.retries_attempted);
    println!("   Cache Hits: {}", metrics_snapshot.cache_hits);

    println!("\n🔍 Enhanced Features Demonstrated:");
    println!("   • Builder Pattern: Easy configuration with presets (high_performance)");
    println!("   • Progress Reporter Traits: Clean, composable progress tracking");
    println!("   • Built-in Metrics: Automatic performance and reliability monitoring");
    println!("   • Enhanced Error Context: Detailed error information with recovery suggestions");
    println!("   • Async Validation: Non-blocking validation with automatic retries");
    println!("   • Memory Optimization: Buffer pooling and conditional hash computation");

    println!("\n✨ Enhanced batch download example completed!");
    println!("   This example showcased the new modular, high-performance downloader API");
    println!("   with improved developer experience and built-in observability.");

    Ok(())
}

/// Helper function to extract filename from URL for display purposes
fn extract_filename_from_url(url: &str) -> String {
    if url.contains("bytes/1024") {
        "small_file.bin".to_string()
    } else if url.contains("bytes/10240") {
        "medium_file.bin".to_string()
    } else if url.contains("bytes/102400") {
        "large_file.bin".to_string()
    } else if url.contains("bytes/2048") {
        "fallback_file.bin".to_string()
    } else if url.contains("bytes/4096") {
        "another_file.bin".to_string()
    } else if url.contains("status/500") {
        "fallback_file.bin (primary)".to_string()
    } else {
        url.split('/').last().unwrap_or("unknown").to_string()
    }
}

/// Helper function to extract filename from file path
fn extract_filename_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_string()
}
