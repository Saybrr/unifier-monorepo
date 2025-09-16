//! Example of downloading mods from Nexus Mods
//!
//! This example shows how to:
//! 1. Initialize Nexus authentication with your API key
//! 2. Create a Nexus download request
//! 3. Download a mod file with progress tracking
//!
//! # Prerequisites
//!
//! 1. Create a .env file in the project root with your Nexus API key:
//!    ```
//!    NEXUS_API_KEY=your_api_key_here
//!    ```
//!
//! 2. Get your API key from: https://www.nexusmods.com/users/myaccount?tab=api
//!
//! # Usage
//!
//! ```bash
//! cargo run --example nexus_download_example
//! ```

use installer::{
    downloader::api::nexus_api::NexusAPI,
    downloader::sources::DownloadSource,
    Result
};
use std::path::PathBuf;
use std::str::FromStr;
use tracing_subscriber;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use installer::downloader::core::DownloadError;
use installer::parse_wabbajack::parser::WabbaModlist;


#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("Nexus Mods Download Example");
    println!("===========================");

    // Initialize Nexus authentication
    // This will load NEXUS_API_KEY from environment variables
    println!("Initializing Nexus authentication...");
    if let Ok(_) = dotenv::dotenv() {
        println!("Loaded environment variables from .env file");
    }

    let api = NexusAPI::new()?;

    // Validate the API key on initialization
    match api.validate_user().await {
        Ok(user) => {
            println!("Nexus authentication initialized for user: {} (Premium: {})", user.name, user.is_premium);
        }
        Err(e) => {
            println!("Failed to validate Nexus API key: {}", e);
            return Err(e);
        }
    }

    println!("✓ Nexus authentication initialized");

    // Display initial rate limit status
    if let Some(rate_limit) = api.get_rate_limit_status() {
        println!("📊 {}", rate_limit.format_status());
        println!("⏰ {}", rate_limit.time_until_reset());
    } else {
        println!("📊 Rate limit info not available yet (will update after first API call)");
    }

    let skse_json = r#"
    {
    "Archives": [
        {
        "Hash": "EvSXscYx5Zw=",
        "Meta": "[General]\ngameName=skyrimse\nmodID=16495\nfileID=463765",
        "Name": "JContainers SE-16495-4-2-9-1705929247.7z",
        "Size": 822742,
        "State": {
            "$type": "NexusDownloader, Wabbajack.Lib",
            "Author": "silvericed",
            "Description": "Extends Skyrim SE Papyrus scripts (or SKSE/C++ plugins) with JSON based serializable data structures like arrays and maps. Embedded Lua interpreter.",
            "FileID": 463765,
            "GameName": "SkyrimSpecialEdition",
            "ImageURL": "https://staticdelivery.nexusmods.com/mods/1704/images/16495/16495-1646226083-1749138482.jpeg",
            "IsNSFW": false,
            "ModID": 16495,
            "Name": "JContainers SE",
            "Version": "4.2.3"
           }
        }
        ]
    }
    "#;

    // parse the modlist
    let manifest = WabbaModlist::parse(skse_json).expect("Failed to prse skse manifest");
    let requests = manifest.get_dl_requests(&PathBuf::from_str("/tmp").unwrap()).expect("Failed to get download requests");
    // call nexus api
    for request in requests {
        println!("Starting download...");
        println!("Mod: {} ", request.source.description());
        println!("Destination: {}", request.destination.display());

        // Extract mod info from the request source
        match &request.source {
            DownloadSource::Nexus(nexus_source) => {
                // Use the actual mod info from the parsed manifest
                let mod_id = nexus_source.mod_id;
                let file_id = nexus_source.file_id;
                let game_name = &nexus_source.game_name;

                println!("Nexus mod info - Game: {}, Mod ID: {}, File ID: {}", game_name, mod_id, file_id);

                let links = api.get_download_links(game_name, mod_id, file_id).await?;
                //println!("Links: {:?}", links);

                // Display updated rate limit status after API call
                if let Some(rate_limit) = api.get_rate_limit_status() {
                    println!("📊 {}", rate_limit.format_status());
                }
                let best_link = api.select_best_download_link(&links).ok_or(DownloadError::Legacy("No best link found".to_string()))?;
                //println!("Best link: {:?}", best_link);

                let client = reqwest::Client::new();
                let response = client.get(best_link.uri.clone()).send().await?;
                let body_bytes = response.bytes().await?;

                // Create the destination directory if it doesn't exist
                if let Some(parent) = request.destination.parent() {
                    fs::create_dir_all(parent).await?;
                }

                // Save the file with proper filename (use the request's filename)
                let file_path = request.destination.join(&request.filename);
                println!("Saving file to: {}", file_path.display());
                let mut file = fs::File::create(&file_path).await?;
                file.write_all(&body_bytes).await?;
                file.flush().await?;

                println!("✓ File downloaded successfully: {}", file_path.display());
            }
            _ => {
                println!("⚠ Skipping non-Nexus download: {}", request.source.description());
            }
        }


        //NexusSource::new(request.source.mod_id, request.source.file_id, request.source.game_name.to_string()).download(request, Some(ConsoleProgressReporter::new(true).into_callback()), &DownloadConfig::default()).await?;
    }



    // Display final rate limit status
    println!("\n=== Final Rate Limit Status ===");
    if let Some(rate_limit) = api.get_rate_limit_status() {
        println!("📊 {}", rate_limit.format_status());
        println!("⏰ {}", rate_limit.time_until_reset());
        if rate_limit.is_blocked {
            println!("⚠️  You are currently rate limited!");
        }
    } else {
        println!("📊 No rate limit data available");
    }

    Ok(())
}
