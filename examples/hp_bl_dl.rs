
//! Features demonstrated:
//! 1. High-performance configuration with concurrent downloads
//! 2. Concurrent hash validation with progress reporting
//! 3. Multi-file progress dashboard (built-in!)
//! 4. Automatic filtering of unsupported downloads
//! 5. Built-in error handling and statistics
//! 6. Periodic Nexus API rate limit monitoring during downloads

use installer::*;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing with info level (same as before)
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();


    // Test debug logging
    tracing::debug!("DEBUG LOGGING IS ENABLED - you should see this message");

   let downloader = ModlistDownloader::new(
    "Baseline/modlist",
    "./downloads",
    ModlistOptions::default(),
    Some(NexusRateLimitProgressReporter::new()
        .with_rate_limit_interval(Duration::from_secs(10))
        .into_callback())
    );
    let result = downloader.download().await?;

    // ===========================================
    // Results and Statistics (built-in!)
    // ===========================================

    // Clear screen for final summary (same as original)
    std::io::Write::flush(&mut std::io::stdout()).unwrap();
    print!("\x1b[2J\x1b[H");
    std::io::Write::flush(&mut std::io::stdout()).unwrap();

    println!("🎉 High-Performance Download Complete!");
    println!("✅ Successful: {}", result.successful_downloads);
    println!("❌ Failed: {}", result.failed_downloads);
    println!("⏱️  Total time: {:.1}s", result.elapsed_time.as_secs_f64());

    // Calculate and display transfer speed
    let total_mb = result.total_bytes_downloaded as f64 / 1_048_576.0;
    let speed_mbps = total_mb / result.elapsed_time.as_secs_f64();
    println!("📊 Downloaded: {:.1} MB at {:.1} MB/s", total_mb, speed_mbps);

    // Show download statistics
    println!("\n📋 Modlist Statistics:");
    println!("   Total requests processed: {}", result.total_requests);
    println!("   Successful downloads: {}", result.successful_downloads);
    println!("   Failed downloads: {}", result.failed_downloads);
    println!("   Skipped downloads: {}", result.skipped_downloads);
    println!("   Total download size: {:.1} MB", result.total_bytes_downloaded as f64 / 1_048_576.0);

    let filtered_count = result.total_requests - result.successful_downloads - result.failed_downloads - result.skipped_downloads;
    if filtered_count > 0 {
        println!("   Other downloads: {}", filtered_count);
    }

    // Show error summary if there were any (same as original)
    if !result.error_messages.is_empty() {
        println!("\n📋 ERROR SUMMARY:");
        println!("  Download/Validation Failures ({}):", result.error_messages.len());
        for error in result.error_messages {
            println!("    ❌ {}", error);
        }
    }

    if filtered_count > 0 {
        println!("\nℹ️  Note: {} downloads were skipped (some downloads may require API keys or user action)", filtered_count);
    }



    Ok(())
}