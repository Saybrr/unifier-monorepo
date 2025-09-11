//! Example of using the structured download system with Wabbajack modlists
//!
//! This example demonstrates how to:
//! 1. Parse a Wabbajack modlist JSON file
//! 2. Convert archives to structured download operations
//! 3. Transform operations to download requests
//! 4. Process downloads using the enhanced downloader
//!
//! The structured approach provides better type safety, performance, and
//! allows for richer data representation compared to URL strings.

use installer::{
    // Downloader components
    DownloadConfigBuilder, EnhancedDownloader, DownloaderRegistry,
    ConsoleProgressReporter, IntoProgressCallback,

    // Wabbajack parsing components
    parse_modlist, manifest_to_download_requests_with_stats,
    DownloadOperation, ArchiveManifest,

    // Source types for creating custom operations
    WabbajackDownloadSource, HttpSource, NexusSource, GameFileSource,
};
use std::path::PathBuf;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for better debugging
    tracing_subscriber::fmt().init();

    println!("🚀 Structured Download System Demo");


    // Example 2: Create structured operations manually
    demo_manual_operations().await?;


    println!("✅ All examples completed successfully!");
    Ok(())
}


/// Example 2: Create structured download operations manually
async fn demo_manual_operations() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🛠️  Example 2: Manual Structured Operations");

    // Create different types of download sources manually
    let http_source = HttpSource::new("https://example.com/mod.zip")
        .with_header("User-Agent", "ModInstaller/1.0")
        .with_mirror("https://mirror.example.com/mod.zip");

    let nexus_source = NexusSource::new(12345, 67890, "SkyrimSpecialEdition".to_string())
        .with_metadata(
            "Awesome Mod",
            "CoolModder",
            "2.1.0",
            "This is an awesome mod that does cool things",
            false
        );

    let gamefile_source = GameFileSource::new(
        "SkyrimSpecialEdition",
        "Data/Skyrim.esm",
        "1.6.1170.0"
    );

    // Create download operations
    let operations = vec![
        DownloadOperation::new(
            WabbajackDownloadSource::Http(http_source),
            "awesome-mod.zip",
            1048576, // 1MB
            "abcd1234567890ef",
        )
        .with_hash_algorithm("SHA256")
        .with_priority(1),

        DownloadOperation::new(
            WabbajackDownloadSource::Nexus(nexus_source),
            "nexus-mod.zip",
            2097152, // 2MB
            "ef9876543210dcba",
        )
        .with_hash_algorithm("SHA256")
        .with_priority(2),

        DownloadOperation::new(
            WabbajackDownloadSource::GameFile(gamefile_source),
            "Skyrim.esm",
            268435456, // 256MB
            "1234567890abcdef",
        )
        .with_hash_algorithm("CRC32")
        .with_priority(0), // Highest priority
    ];

    println!("📦 Created {} manual download operations:", operations.len());

    for (i, operation) in operations.iter().enumerate() {
        println!("  {}. {} ({} bytes, priority {}) - {}",
            i + 1,
            operation.filename,
            operation.expected_size,
            operation.priority,
            operation.source.description()
        );
    }

    // Create a manifest from the operations
    let mut manifest = ArchiveManifest::new();
    manifest.add_operations(operations);

    println!("📊 Manual Operations Statistics:");
    println!("  • Total Download Size: {}", manifest.stats.total_download_size_human());
    println!("  • External Dependencies Required: {}", manifest.stats.external_dependencies_required);
    println!("  • Estimated Time (10 Mbps): {:.1}s",
        manifest.estimated_total_download_time_seconds(10.0));

    Ok(())
}