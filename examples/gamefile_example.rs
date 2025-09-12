//! Example demonstrating GameFile downloading (copying from game installation)
//!
//! This example shows how to use the GameFileDownloader to copy files
//! from a game installation directory to a destination.

use installer::{
    DownloadRequest, DownloadConfig, EnhancedDownloader,
    ProgressReporter, IntoProgressCallback,
};
use installer::parse_wabbajack::sources::GameFileSource;
use std::path::PathBuf;
use std::time::Duration;
use tokio;
use tracing::{info, Level};
use tracing_subscriber;

/// Simple progress reporter for the example
#[derive(Debug)]
struct ExampleProgressReporter;

impl ProgressReporter for ExampleProgressReporter {
    fn on_download_started(&self, url: &str, total_size: Option<u64>) {
        match total_size {
            Some(size) => println!("🎮 Starting game file copy: {} ({} bytes)", url, size),
            None => println!("🎮 Starting game file copy: {}", url),
        }
    }

    fn on_download_progress(&self, url: &str, downloaded: u64, total: Option<u64>, speed_bps: f64) {
        let speed_mb = speed_bps / 1_000_000.0;
        match total {
            Some(total) => {
                let percent = (downloaded as f64 / total as f64) * 100.0;
                println!("📁 {}: {:.1}% ({}/{} bytes, {:.1} MB/s)",
                    url, percent, downloaded, total, speed_mb);
            }
            None => {
                println!("📁 {}: {} bytes copied ({:.1} MB/s)",
                    url, downloaded, speed_mb);
            }
        }
    }

    fn on_download_complete(&self, url: &str, final_size: u64) {
        println!("✅ Game file copy complete: {} ({} bytes)", url, final_size);
    }

    fn on_error(&self, url: &str, error: &str) {
        eprintln!("❌ Error copying game file {}: {}", url, error);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("GameFile Download Example");

    // Create download configuration
    let config = DownloadConfig {
        max_retries: 3,
        timeout: Duration::from_secs(30),
        allow_resume: true,
        chunk_size: 64 * 1024, // 64KB
        user_agent: "GameFile-Example/1.0".to_string(),
        ..Default::default()
    };

    // Create enhanced downloader (no registry needed)
    let downloader = EnhancedDownloader::new(config);

    // Create a GameFile source
    // This example tries to copy a file from Skyrim Special Edition
    let gamefile_source = GameFileSource::new(
        "SkyrimSpecialEdition",
        "Data/Skyrim.esm",
        "1.6.659.0"
    );

    // Create download request with trait object
    let request = DownloadRequest::from_source(
        gamefile_source,
        PathBuf::from("./downloads")
    );

    info!("Attempting to copy game file: {}", request.get_description());

    // Set up progress reporting
    let progress_reporter = ExampleProgressReporter;
    let progress_callback = progress_reporter.into_callback();

    // Attempt the download
    match downloader.download(request, Some(progress_callback)).await {
        Ok(result) => {
            println!("✅ Success! Result: {:?}", result);
        }
        Err(e) => {
            eprintln!("❌ Failed to copy game file: {}", e);
            eprintln!("💡 Make sure:");
            eprintln!("   - Skyrim Special Edition is installed");
            eprintln!("   - The game is installed via Steam in the default location");
            eprintln!("   - Or set SKYRIMSPECIALEDITION_PATH environment variable");
            return Err(e.into());
        }
    }

    Ok(())
}
