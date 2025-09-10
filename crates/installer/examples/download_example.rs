//! Example demonstrating how to use the installer downloader
//!
//! This example shows a complete workflow of downloading a file with validation.
//!
//! Run this example with:
//! ```
//! cargo run --example download_example
//! ```

use installer::{
    DownloadConfig, DownloadRequest, EnhancedDownloader,
    FileValidation, ProgressEvent
};
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::main]
async fn main() -> installer::Result<()> {
    // Initialize tracing for logging
   // tracing_subscriber::fmt::init();

    println!("🚀 Starting download example");

    // Create a temporary directory for downloads
    let temp_dir = tempdir().unwrap();
    println!("📁 Download directory: {}", temp_dir.path().display());

    // Configure the downloader
    let config = DownloadConfig {
        max_retries: 3,
        timeout: std::time::Duration::from_secs(30),
        user_agent: "installer-example/1.0".to_string(),
        allow_resume: true,
        chunk_size: 8192,
    };

    // Create the downloader
    let downloader = EnhancedDownloader::new(config);

    // Set up file validation (for a known test file)
    // In practice, you would get these values from your package manifest
    let validation = FileValidation::new()
        .with_expected_size(1024) // Example: expect 1KB file
        // .with_crc32(0x12345678) // Uncomment if you know the CRC32
        // .with_md5("d41d8cd98f00b204e9800998ecf8427e".to_string()) // Uncomment if you know the MD5
        ;

    // Create a download request
    let request = DownloadRequest::new(
        "https://httpbin.org/bytes/1024", // Test endpoint that returns exactly 1024 bytes
        temp_dir.path()
    )
    .with_filename("test_file.bin")
    .with_validation(validation);

    // Set up progress tracking
    let progress_callback = Arc::new(|event: ProgressEvent| {
        match event {
            ProgressEvent::DownloadStarted { url, total_size } => {
                println!("📥 Started downloading: {}", url);
                if let Some(size) = total_size {
                    println!("   Expected size: {} bytes", size);
                }
            }
            ProgressEvent::DownloadProgress { downloaded, total, speed_bps, .. } => {
                if let Some(total) = total {
                    let percent = (downloaded as f64 / total as f64) * 100.0;
                    println!(
                        "   Progress: {:.1}% ({} / {} bytes) at {:.1} KB/s",
                        percent,
                        downloaded,
                        total,
                        speed_bps / 1024.0
                    );
                } else {
                    println!(
                        "   Downloaded: {} bytes at {:.1} KB/s",
                        downloaded,
                        speed_bps / 1024.0
                    );
                }
            }
            ProgressEvent::DownloadComplete { final_size, .. } => {
                println!("✅ Download complete: {} bytes", final_size);
            }
            ProgressEvent::ValidationStarted { file } => {
                println!("🔍 Starting validation of: {}", file);
            }
            ProgressEvent::ValidationProgress { file, progress } => {
                println!("   Validating {}: {:.1}%", file, progress * 100.0);
            }
            ProgressEvent::ValidationComplete { file, valid } => {
                if valid {
                    println!("✅ Validation passed: {}", file);
                } else {
                    println!("❌ Validation failed: {}", file);
                }
            }
            ProgressEvent::RetryAttempt { url, attempt, max_attempts } => {
                println!("🔄 Retry attempt {} of {} for: {}", attempt, max_attempts, url);
            }
            ProgressEvent::Error { url, error } => {
                println!("❌ Error downloading {}: {}", url, error);
            }
        }
    });

    // Download the file
    println!("🔄 Starting download...");
    let result = downloader.download(request, Some(progress_callback)).await?;

    match result {
        installer::DownloadResult::Downloaded { size } => {
            println!("🎉 Successfully downloaded {} bytes!", size);
        }
        installer::DownloadResult::AlreadyExists { size } => {
            println!("📋 File already exists ({} bytes)", size);
        }
        installer::DownloadResult::Resumed { size } => {
            println!("⏯️  Resumed and completed download ({} bytes)", size);
        }
    }

    // Verify the file exists
    let downloaded_file = temp_dir.path().join("test_file.bin");
    if downloaded_file.exists() {
        let file_size = std::fs::metadata(&downloaded_file).unwrap().len();
        println!("📄 File saved: {} ({} bytes)", downloaded_file.display(), file_size);
    }

    println!("✨ Example completed successfully!");

    Ok(())
}
