
//! Features demonstrated:
//! 1. High-performance configuration with concurrent downloads
//! 2. Concurrent hash validation with progress reporting
//! 3. Multi-file progress dashboard (built-in!)
//! 4. Automatic filtering of unsupported downloads
//! 5. Built-in error handling and statistics

use installer::*;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing with info level (same as before)
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🚀 High-Performance Baseline Download with Hash Validation");
    println!("✨ NEW SIMPLIFIED API - 551 lines → 70 lines!");

    // Test debug logging
    tracing::debug!("DEBUG LOGGING IS ENABLED - you should see this message");

   let result = ModlistDownloadBuilder::new("Baseline/modlist")
        .destination("./downloads")
        .automated_only()                    // Filter out manual/Nexus downloads automatically
        .high_performance()                  // 8 concurrent validations, async validation, parallel hashing
        .max_concurrent_downloads(8)         // Allow 8 simultaneous downloads
        .with_dashboard_progress()           // Built-in beautiful progress dashboard
        .download()
        .await?;

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

    // Show conversion statistics (built-in!)
    let stats = &result.conversion_stats;
    println!("\n📋 Modlist Statistics:");
    println!("   Total operations in modlist: {}", stats.total_operations);
    println!("   Converted to download requests: {}", stats.converted_requests);
    println!("   Total download size: {:.1} MB", stats.total_download_size as f64 / 1_048_576.0);

    let filtered_count = stats.total_operations - stats.converted_requests;
    if filtered_count > 0 {
        println!("   Filtered out {} unsupported downloads", filtered_count);
    }

    // Show error summary if there were any (same as original)
    if !result.error_messages.is_empty() {
        println!("\n📋 ERROR SUMMARY:");
        println!("  Download/Validation Failures ({}):", result.error_messages.len());

        // Show first 5 errors to avoid spam
        for (i, error) in result.error_messages.iter().enumerate() {
            if i >= 5 {
                println!("    ... and {} more errors", result.error_messages.len() - 5);
                break;
            }
            // Truncate long errors
            let display_error = if error.len() > 80 {
                format!("{}...", &error[..77])
            } else {
                error.clone()
            };
            println!("    ❌ {}", display_error);
        }
    }

    if filtered_count > 0 {
        println!("\nℹ️  Note: {} downloads were skipped (Nexus mods require API keys, Manual downloads need user action)", filtered_count);
    }



    Ok(())
}