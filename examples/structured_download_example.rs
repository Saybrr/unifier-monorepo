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
    WabbajackDownloadSource, HttpSource, NexusSource
};
use std::path::PathBuf;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for better debugging
    tracing_subscriber::fmt().init();

    println!("🚀 Structured Download System Demo");

    // Example 1: Parse a modlist JSON file (simulated data)
    demo_parse_modlist().await?;


    // Example 3: Advanced batch processing with statistics
    demo_advanced_batch_processing().await?;

    Ok(())
}

/// Example 1: Parse a Wabbajack modlist and convert to download requests
async fn demo_parse_modlist() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📋 Example 1: Parsing Wabbajack Modlist");

    // Simulated modlist JSON (in real usage, read from file)
    let modlist_json = r#"{
        "Name": "Example Modlist",
        "Version": "1.0.0",
        "Author": "Demo Author",
        "GameName": "SkyrimSpecialEdition",
        "Description": "A demonstration modlist",
        "Archives": [
            {
                "Hash": "rXDEtl7gdOU=",
                "Meta": "[General]\ndirectURL=https://github.com/ModOrganizer2/modorganizer/releases/download/v2.5.2/Mod.Organizer-2.5.2.7z",
                "Name": "Mod.Organizer-2.5.2.7z",
                "Size": 149660212,
                "State": {
                    "$type": "HttpDownloader, Wabbajack.Lib",
                    "Headers": [],
                    "Url": "https://github.com/ModOrganizer2/modorganizer/releases/download/v2.5.2/Mod.Organizer-2.5.2.7z"
                }
            },
            {
                "Hash": "DqfjP/t70iI=",
                "Meta": "[General]\ngameName=skyrimse\nmodID=71371\nfileID=575985",
                "Name": "CK Platform Extended-71371-0-4-b952-1735142537.zip",
                "Size": 4245464,
                "State": {
                    "$type": "NexusDownloader, Wabbajack.Lib",
                    "Author": "Nukem-perchik71",
                    "Description": "Updating the popular mod SSE Creation Kit Fixes",
                    "FileID": 575985,
                    "GameName": "SkyrimSpecialEdition",
                    "ImageURL": "https://staticdelivery.nexusmods.com/mods/1704/images/71371/71371-1658621415-603973569.jpeg",
                    "IsNSFW": false,
                    "ModID": 71371,
                    "Name": "SSE Creation Kit Fixes Update",
                    "Version": "3.5"
                }
            },
            {
                "Hash": "Y4roDBZIq+0=",
                "Meta": "[General]\ngameName=SkyrimSpecialEdition\ngameFile=SkyrimSE.exe",
                "Name": "SkyrimSE.exe",
                "Size": 37157144,
                "State": {
                    "$type": "GameFileSourceDownloader, Wabbajack.Lib",
                    "Game": "SkyrimSpecialEdition",
                    "GameFile": "SkyrimSE.exe",
                    "GameVersion": "1.6.1170.0",
                    "Hash": "Y4roDBZIq+0="
                }
            }
        ]
    }"#;

    // Parse the modlist
    let manifest = parse_modlist(modlist_json)?;

    println!("📊 Parsed Modlist Statistics:");
    println!("  • Name: {}", manifest.metadata.name);
    println!("  • Author: {}", manifest.metadata.author);
    println!("  • Game: {}", manifest.metadata.game);
    println!("  • Total Archives: {}", manifest.stats.total_operations);
    println!("  • HTTP Downloads: {}", manifest.stats.http_operations);
    println!("  • Nexus Downloads: {}", manifest.stats.nexus_operations);
    println!("  • Game File Copies: {}", manifest.stats.gamefile_operations);
    println!("  • Total Size: {}", manifest.stats.total_download_size_human());
    println!("  • Automation Rate: {:.1}%", manifest.stats.automation_percentage());

    // Convert to download requests with statistics
    let destination_dir = PathBuf::from("./downloads");
    let (download_requests, conversion_stats) = manifest_to_download_requests_with_stats(
        &manifest,
        &destination_dir,
        false // Don't include manual downloads for this demo
    );

    println!("🔄 Conversion Statistics:");
    println!("  • Total Operations: {}", conversion_stats.total_operations);
    println!("  • Converted Requests: {}", conversion_stats.converted_requests);
    println!("  • Skipped Manual: {}", conversion_stats.skipped_manual);
    println!("  • Download Size: {} MB", conversion_stats.total_download_size / 1_048_576);

    for (source_type, count) in &conversion_stats.operations_by_source {
        println!("  • {}: {} operations", source_type, count);
    }

    println!("✅ Modlist parsing completed - {} download requests ready", download_requests.len());

    Ok(())
}

/// Example 3: Advanced batch processing with full downloader setup
async fn demo_advanced_batch_processing() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔧 Example 3: Advanced Batch Processing");

    // Set up comprehensive download configuration
    let config = DownloadConfigBuilder::new()
        .max_retries(3)
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("StructuredDownloader/1.0")
        .build();

    // Create a basic registry (in real usage, you'd add Nexus/GameFile downloaders too)
    let registry = DownloaderRegistry::new()
        .with_http_downloader(config.clone());

    // Create the enhanced downloader
    let downloader = EnhancedDownloader::with_registry(registry, config);

    // Create some test HTTP operations (only HTTP works in this demo)
    let test_operations = vec![
        DownloadOperation::new(
            WabbajackDownloadSource::Http(HttpSource::new("http://localhost:80/json")),
            "test-data.json",
            "dummy_hash_1",
        ),
        DownloadOperation::new(
            WabbajackDownloadSource::Http(HttpSource::new("http://localhost:80/uuid")),
            "test-uuid.json",
            "dummy_hash_2",
        ),
    ];

    let mut manifest = ArchiveManifest::new();
    manifest.add_operations(test_operations);

    println!("🎯 Processing {} operations with structured sources", manifest.operations.len());

    // Convert operations to download requests
    let destination_dir = PathBuf::from("./downloads");
    let (download_requests, conversion_stats) = manifest_to_download_requests_with_stats(
        &manifest,
        &destination_dir,
        false
    );

    println!("🔄 Converted {} operations to {} download requests",
             conversion_stats.total_operations, conversion_stats.converted_requests);

    // Set up progress reporting
    let progress_reporter = ConsoleProgressReporter::new(true); // verbose = true
    let progress_callback = progress_reporter.into_callback();

    println!("⬇️  Starting batch download of {} requests...", download_requests.len());

    // Process the downloads (commented out to avoid network calls in example)

    let results = downloader.download_batch_with_async_validation(
        download_requests,
        Some(progress_callback),
        2, // max concurrent downloads
    ).await;

    // Analyze results
    let mut successful = 0;
    let mut failed = 0;

    for result in results {
        match result {
            Ok(_) => successful += 1,
            Err(e) => {
                failed += 1;
                println!("❌ Download failed: {}", e);
            }
        }
    }

    println!("📊 Final Results:");
    println!("  • Successful: {}", successful);
    println!("  • Failed: {}", failed);

    // Display performance metrics
    let metrics = downloader.metrics().snapshot();
    println!("  • Total Bytes: {}", metrics.total_bytes);
    println!("  • Average Speed: {:.2} MB/s", metrics.average_size());




    Ok(())
}

