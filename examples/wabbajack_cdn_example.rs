//! Example showing how to use the WabbajackCDN downloader
//!
//! This example demonstrates:
//! - Parsing a modlist with WabbajackCDN downloads
//! - Setting up the downloader registry with WabbajackCDN support
//! - Converting parsed operations to download requests

use installer::{
    EnhancedDownloader, DownloadConfigBuilder,
    parse_wabbajack::{
        parser::parse_modlist,
        integration::manifest_to_download_requests,
    },
};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Example modlist JSON with a WabbajackCDN download
    let modlist_json = r#"{
        "Archives": [
            {
                "Hash": "nTKgBjLzrFQ=",
                "Meta": "[General]\ndirectURL=https://authored-files.wabbajack.org/xLODGen.129.7z_a9440abe-ca6c-48aa-ab31-4fe7e8f2484c",
                "Name": "xLODGen.129.7z",
                "Size": 16797898,
                "State": {
                    "$type": "WabbajackCDNDownloader+State, Wabbajack.Lib",
                    "Url": "https://authored-files.wabbajack.org/xLODGen.129.7z_a9440abe-ca6c-48aa-ab31-4fe7e8f2484c"
                }
            }
        ],
        "Name": "Test Modlist with WabbajackCDN",
        "Version": "1.0",
        "Author": "Test Author",
        "GameName": "SkyrimSpecialEdition",
        "Description": "A test modlist with WabbajackCDN downloads"
    }"#;

    // Parse the modlist
    let manifest = parse_modlist(modlist_json)?;
    println!("Parsed modlist: {} by {}", manifest.metadata.name, manifest.metadata.author);
    println!("Found {} archives", manifest.operations.len());

    // Convert to download requests
    let download_destination = PathBuf::from("./downloads");
    let download_requests = manifest_to_download_requests(
        &manifest,
        &download_destination,
        false // Don't include manual downloads
    );

    println!("Generated {} download requests", download_requests.len());

    // Set up the enhanced downloader (no registry needed)
    let config = DownloadConfigBuilder::new()
        .max_retries(3)
        .timeout(std::time::Duration::from_secs(30))
        .build();

    let downloader = EnhancedDownloader::new(config);

    // Process each download request
    for (index, request) in download_requests.into_iter().enumerate() {
        println!("Request {}: {}", index + 1, request.get_description());
        println!("  Destination: {}", request.destination.display());

        // Show what filename will be used
        match request.get_filename() {
            Ok(filename) => {
                let full_path = request.destination.join(&filename);
                println!("  Target file: {}", full_path.display());
                println!("  File exists: {}", full_path.exists());
            }
            Err(e) => {
                println!("  Error getting filename: {}", e);
            }
        }

        // Download using the enhanced downloader
        // Each source handles its own download logic
        match downloader.download(request, None).await {
            Ok(result) => {
                println!("  ✓ Downloaded: {:?}", result);
            }
            Err(e) => {
                println!("  ✗ Download failed: {}", e);
            }
        }
    }

    println!("Example completed successfully!");
    Ok(())
}

/// Test function to demonstrate WabbajackCDN source handling
fn demonstrate_wabbajack_cdn_parsing() {
    use installer::parse_wabbajack::sources::{DownloadSource, WabbajackCDNSource};

    // Create a WabbajackCDN source manually
    let cdn_source = WabbajackCDNSource::new("https://authored-files.wabbajack.org/example");
    let download_source = DownloadSource::WabbajackCDN(cdn_source);

    println!("WabbajackCDN source description: {}", download_source.description());
    println!("Requires user interaction: {}", download_source.requires_user_interaction());
    println!("Requires external dependencies: {}", download_source.requires_external_dependencies());
}

#[cfg(test)]
mod tests {
    use super::*;
    use installer::parse_wabbajack::sources::DownloadSource;

    #[tokio::test]
    async fn test_wabbajack_cdn_parsing() {
        let modlist_json = r#"{
            "Archives": [
                {
                    "Hash": "test-hash",
                    "Meta": "",
                    "Name": "test.zip",
                    "Size": 1024,
                    "State": {
                        "$type": "WabbajackCDNDownloader+State, Wabbajack.Lib",
                        "Url": "https://authored-files.wabbajack.org/test"
                    }
                }
            ]
        }"#;

        let manifest = parse_modlist(modlist_json).unwrap();
        assert_eq!(manifest.operations.len(), 1);

        let operation = &manifest.operations[0];
        match &operation.source {
            DownloadSource::WabbajackCDN(cdn_source) => {
                assert_eq!(cdn_source.url, "https://authored-files.wabbajack.org/test");
            }
            _ => panic!("Expected WabbajackCDN source"),
        }
    }

    #[tokio::test]
    async fn test_enhanced_downloader_creation() {
        use installer::DownloadConfigBuilder;

        let config = DownloadConfigBuilder::new().build();
        let downloader = EnhancedDownloader::new(config);

        // Test that the downloader can be created
        assert!(downloader.metrics().successful_downloads.load(std::sync::atomic::Ordering::Relaxed) == 0);
    }
}
