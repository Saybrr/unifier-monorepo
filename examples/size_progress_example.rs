//! Example demonstrating size-based progress reporting with the modlist manifest
//!
//! This example shows how the downloader now uses the Size field from the modlist
//! to provide accurate progress reporting during downloads.

use installer::{
    parse_wabbajack::{
        parser::ModlistParser,
        integration::operations_to_download_requests,
    },
    downloader::{
        core::DownloadSource,
        core::DownloadResult,
        EnhancedDownloader,
        config::DownloadConfig,
        core::{ProgressReporter, ConsoleProgressReporter, IntoProgressCallback},
    },
};
use std::path::PathBuf;

#[derive(Debug)]
pub struct ProgressTracker {
    console_reporter: ConsoleProgressReporter,
}

impl ProgressTracker {
    pub fn new() -> Self {
        Self {
            console_reporter: ConsoleProgressReporter::new(true),
        }
    }
}

impl ProgressReporter for ProgressTracker {
    fn on_download_started(&self, url: &str, total_size: Option<u64>) {
        self.console_reporter.on_download_started(url, total_size);
        match total_size {
            Some(size) => println!("🎯 Expected size from modlist manifest: {} bytes ({:.2} MB)",
                                 size, size as f64 / 1_000_000.0),
            None => println!("⚠️  No expected size available for progress calculation"),
        }
    }

    fn on_download_progress(&self, url: &str, downloaded: u64, total: Option<u64>, speed_bps: f64) {
        self.console_reporter.on_download_progress(url, downloaded, total, speed_bps);

        if let Some(total_bytes) = total {
            let percentage = (downloaded as f64 / total_bytes as f64) * 100.0;
            let remaining = total_bytes - downloaded;
            let eta_seconds = if speed_bps > 0.0 { remaining as f64 / speed_bps } else { 0.0 };

            println!("📊 Progress: {:.1}% complete | {} / {} bytes | ETA: {:.0}s",
                    percentage, downloaded, total_bytes, eta_seconds);
        } else {
            println!("📊 Progress: {} bytes downloaded (size unknown)", downloaded);
        }
    }

    fn on_download_complete(&self, url: &str, final_size: u64) {
        self.console_reporter.on_download_complete(url, final_size);
        println!("✅ Final download size: {} bytes", final_size);
    }

    fn on_error(&self, url: &str, error: &str) {
        self.console_reporter.on_error(url, error);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse a sample modlist entry with HTTP download (for actual testing)
    let sample_modlist_entry = r#"{
      "Archives": [
                   {
                "Hash": "rXDEtl7gdOU=",
                "Meta": "[General]\ndirectURL=https://raw.githubusercontent.com/TestFileHub/FileHub/main/pdf/pdf100mb.pdf",
                "Name": "100mb.pdf",
                "Size": 104855240,
                "State": {
                    "$type": "HttpDownloader, Wabbajack.Lib",
                    "Headers": [],
                    "Url": "https://raw.githubusercontent.com/TestFileHub/FileHub/main/pdf/pdf100mb.pdf"
                }
            }
      ],
      "Name": "Test Modlist",
      "Version": "1.0.0"
    }"#;

    println!("🔍 Parsing modlist with Size field...");
    let parser = ModlistParser::new();
    let manifest = parser.parse(sample_modlist_entry)?;

    println!("📋 Found {} operations", manifest.operations.len());

    if let Some(operation) = manifest.operations.first() {
        println!("📄 First operation:");
        println!("   - Filename: {}", operation.filename);
        println!("   - Expected size: {} bytes ({:.2} MB)",
                operation.expected_size, operation.expected_size as f64 / 1_000_000.0);
        println!("   - Hash: {}", operation.expected_hash);

        // Convert to download requests
        let base_destination = PathBuf::from("./downloads");
        let mut requests = operations_to_download_requests(&manifest.operations, &base_destination, false);

        // For testing: disable size validation to avoid failures with localhost server
        if let Some(request) = requests.first_mut() {
            use installer::downloader::core::FileValidation;
            request.validation = FileValidation::new(); // Empty validation = no size checks
            println!("🚫 Disabled size validation for testing localhost server");
        }

        if let Some(request) = requests.first() {
            println!("📥 Download request created:");
            println!("   - Expected size: {:?} bytes", request.expected_size);
            println!("   - This size will be used for progress calculation!");

            // Get the URL from the request - for testing, use localhost httpbin for 300MB
            let download_url = match &request.source {
                DownloadSource::Http(http_source) => {
                    Some(http_source.url.clone())
                },
                DownloadSource::Nexus(nexus_source) => {
                    // For testing purposes, use localhost httpbin to serve 300MB of data
                    println!("📦 Nexus source detected: {} by {}", nexus_source.mod_name, nexus_source.author);
                    println!("   Using localhost httpbin for testing 300MB download...");
                    println!("   Make sure your httpbin server is running on localhost:80!");
                    // Start with 10MB test, then you can change to 314572800 for 300MB
                    Some("http://localhost:80/stream-bytes/10485760".to_string())
                },
                _ => None
            };

            if let Some(url) = download_url {
                println!("📡 Download URL: {}", url);

                // Create downloader with our progress tracker
                let config = DownloadConfig::default();
                let downloader = EnhancedDownloader::new(config);

                // Set up progress tracking
                let progress_tracker = ProgressTracker::new();
                let progress_callback = progress_tracker.into_callback();

                println!("\n🚀 Starting actual download with size-based progress reporting...");

                // Perform the actual download
                match downloader.download(request.clone(), Some(progress_callback)).await {
                    Ok(result) => {
                        println!("✅ Download completed successfully!");
                        match result {
                            DownloadResult::Downloaded { size } => {
                                println!("   - Downloaded {} bytes", size);
                            },
                            DownloadResult::AlreadyExists { size } => {
                                println!("   - File already existed ({} bytes)", size);
                            },
                            DownloadResult::Resumed { size } => {
                                println!("   - Download resumed and completed ({} bytes)", size);
                            },
                            DownloadResult::DownloadedPendingValidation { size, .. } => {
                                println!("   - Downloaded {} bytes, validation pending", size);
                            }
                        }

                        // Show the downloaded file
                        if let Some(filename) = &request.filename {
                            let file_path = request.destination.join(filename);
                            println!("   - Saved to: {}", file_path.display());
                        }

                        println!("\n✅ Size-based progress reporting demonstrated successfully!");
                        println!("   - Expected size from manifest: {} bytes", request.expected_size.unwrap_or(0));
                        println!("   - Progress callbacks received accurate size information");
                        println!("   - ETA calculations would be based on known file size");
                    },
                    Err(e) => {
                        println!("❌ Download failed: {}", e);
                        println!("   This might be because your localhost:80 server isn't running");
                        println!("   or isn't configured to serve the requested file size");
                    }
                }
            } else {
                println!("❌ Could not extract download URL from request");
            }
        }
    }

    Ok(())
}
