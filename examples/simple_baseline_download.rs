//! Simple example that parses the Baseline modlist and downloads the files
//!
//! This is a minimal, no-frills approach to:
//! 1. Parse the Baseline modlist JSON file
//! 2. Convert to download requests
//! 3. Download all automatable files

use installer::{
    // Downloader components
    DownloadConfigBuilder, EnhancedDownloader,
    IntoProgressCallback,

    // Wabbajack parsing components
    parse_modlist, manifest_to_download_requests_with_stats,
};
use installer::{FileValidation, ProgressReporter};
use std::{path::PathBuf, fs, sync::Arc, io::{self, Write}};
use tokio;

/// In-place progress reporter that updates the current line instead of printing new ones
#[derive(Debug, Default)]
pub struct InPlaceProgressReporter {
    current_file: Arc<std::sync::Mutex<String>>,
}

impl InPlaceProgressReporter {
    pub fn new() -> Self {
        Self {
            current_file: Arc::new(std::sync::Mutex::new(String::new())),
        }
    }

    fn clear_line() {
        print!("\r\x1b[K"); // Clear current line
        io::stdout().flush().unwrap();
    }

    fn extract_filename(url: &str) -> String {
        url.split('/').last().unwrap_or(url).to_string()
    }
}

impl ProgressReporter for InPlaceProgressReporter {
    fn on_download_started(&self, url: &str, total_size: Option<u64>) {
        let filename = Self::extract_filename(url);
        if let Ok(mut current) = self.current_file.lock() {
            *current = filename.clone();
        }

        Self::clear_line();
        match total_size {
            Some(size) => print!("📥 Starting: {} ({:.1} MB)", filename, size as f64 / 1_048_576.0),
            None => print!("📥 Starting: {}", filename),
        }
        io::stdout().flush().unwrap();
    }

    fn on_download_progress(&self, _url: &str, downloaded: u64, total: Option<u64>, speed_bps: f64) {
        let filename = if let Ok(current) = self.current_file.lock() {
            current.clone()
        } else {
            "Unknown".to_string()
        };

        Self::clear_line();
        let speed_mb = speed_bps / 1_048_576.0;
        match total {
            Some(total) => {
                let percent = (downloaded as f64 / total as f64) * 100.0;
                print!("⏬ {}: {:.1}% ({:.1}/{:.1} MB, {:.1} MB/s)",
                    filename, percent,
                    downloaded as f64 / 1_048_576.0,
                    total as f64 / 1_048_576.0,
                    speed_mb);
            }
            None => {
                print!("⏬ {}: {:.1} MB ({:.1} MB/s)",
                    filename,
                    downloaded as f64 / 1_048_576.0,
                    speed_mb);
            }
        }
        io::stdout().flush().unwrap();
    }

    fn on_download_complete(&self, _url: &str, final_size: u64) {
        let filename = if let Ok(current) = self.current_file.lock() {
            current.clone()
        } else {
            "Unknown".to_string()
        };

        Self::clear_line();
        println!("✅ Completed: {} ({:.1} MB)", filename, final_size as f64 / 1_048_576.0);
    }

    fn on_validation_started(&self, file: &str) {
        let filename = Self::extract_filename(file);
        Self::clear_line();
        print!("🔍 Validating: {}", filename);
        io::stdout().flush().unwrap();
    }

    fn on_validation_complete(&self, file: &str, valid: bool) {
        let filename = Self::extract_filename(file);
        let icon = if valid { "✅" } else { "❌" };
        Self::clear_line();
        println!("{} {}: {}", icon, if valid { "Valid" } else { "Invalid" }, filename);
    }

    fn on_retry_attempt(&self, _url: &str, attempt: usize, max_attempts: usize) {
        let filename = if let Ok(current) = self.current_file.lock() {
            current.clone()
        } else {
            "Unknown".to_string()
        };

        Self::clear_line();
        print!("🔄 Retry {}/{}: {}", attempt, max_attempts, filename);
        io::stdout().flush().unwrap();
    }

    fn on_error(&self, _url: &str, error: &str) {
        let filename = if let Ok(current) = self.current_file.lock() {
            current.clone()
        } else {
            "Unknown".to_string()
        };

        Self::clear_line();
        println!("❌ Error: {} - {}", filename, error);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt().init();

    println!("🚀 Simple Baseline Download");

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

    // Set up downloader with reduced retries and disabled validation for simplicity
    let config = DownloadConfigBuilder::new()
        .max_retries(1)
        .timeout(std::time::Duration::from_secs(60))
        .build();

    // Create enhanced downloader (no registry needed in new architecture)
    let downloader = EnhancedDownloader::new(config);

    // Set up progress reporting
    let progress_reporter = InPlaceProgressReporter::new();
    let progress_callback = progress_reporter.into_callback();

    // Clear validation from download requests to avoid hash validation failures
    let mut download_requests = download_requests;
    for request in &mut download_requests {
        request.validation = FileValidation::new(); // No validation
    }

    // Download all files
    println!("⬇️  Starting downloads...");

    let results = downloader.download_batch_with_async_validation(
        download_requests,
        Some(progress_callback), // With progress reporting
        1, // max concurrent downloads to avoid conflicts
    ).await;

    // Count results
    let mut successful = 0;
    let mut failed = 0;

    for result in results {
        match result {
            Ok(_) => successful += 1,
            Err(_) => failed += 1,
        }
    }

    println!("\n✅ Downloads completed: {} successful, {} failed", successful, failed);

    if filtered_count > 0 {
        println!("ℹ️  Note: {} downloads were skipped (Nexus mods require API keys, Manual downloads need user action)", filtered_count);
    }

    Ok(())
}
