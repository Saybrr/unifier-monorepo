//! Example showing how to use the WabbajackCDN downloader
//!
//! This example demonstrates:
//! - Parsing a modlist with WabbajackCDN downloads
//! - Setting up the downloader registry with WabbajackCDN support
//! - Converting parsed operations to download requests

use installer::{
    downloader::{DownloaderRegistry, DownloadConfigBuilder},
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
                "Hash": "test-hash-123",
                "Meta": "[General]\ndirectURL=https://authored-files.wabbajack.org/test-file.zip",
                "Name": "test-file.zip",
                "Size": 1024000,
                "State": {
                    "$type": "WabbajackCDNDownloader+State, Wabbajack.Lib",
                    "Url": "https://authored-files.wabbajack.org/test-file"
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

    // Set up the downloader registry with WabbajackCDN support
    let config = DownloadConfigBuilder::new()
        .max_retries(3)
        .timeout(std::time::Duration::from_secs(30))
        .build();

    let registry = DownloaderRegistry::new()
        .with_http_downloader(config)
        .with_wabbajack_cdn_downloader();

    // Process each download request
    for (index, request) in download_requests.iter().enumerate() {
        println!("Request {}: {:?}", index + 1, request.source);

        // Find appropriate downloader
        match registry.find_downloader_for_request(request).await {
            Ok(_downloader) => {
                println!("  ✓ Found compatible downloader");

                // In a real scenario, you would call:
                // let result = downloader.download(request, None).await?;
                // println!("  ✓ Downloaded: {:?}", result);
            }
            Err(e) => {
                println!("  ✗ No compatible downloader: {}", e);
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
    async fn test_downloader_registry_supports_wabbajack_cdn() {
        let registry = DownloaderRegistry::new()
            .with_wabbajack_cdn_downloader();

        // Test with a WabbajackCDN URL
        let downloader = registry.find_downloader("https://authored-files.wabbajack.org/test").await;
        assert!(downloader.is_ok());

        // Test with a regular HTTP URL (should fail without HTTP downloader)
        let downloader = registry.find_downloader("https://example.com/test").await;
        assert!(downloader.is_err());
    }
}
