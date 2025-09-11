//! Example of parsing a Wabbajack Baseline modlist file
//!
//! This example demonstrates how to:
//! 1. Load and parse a real Wabbajack modlist JSON file from the Baseline directory
//! 2. Analyze the parsed modlist structure and statistics
//! 3. Explore different operation types (HTTP, Nexus, GameFile, Manual, Archive)
//! 4. Convert operations to download requests (preparation only, no downloading)
//! 5. Display detailed breakdowns and sample operations for better understanding
//!
//! This example focuses purely on parsing and analysis - no actual downloads are performed.

use installer::{
    // Wabbajack parsing components
    parse_modlist, manifest_to_download_requests_with_stats,
    DownloadOperation, ArchiveManifest,

    // Source types for analysis
    WabbajackDownloadSource
};
use std::{path::PathBuf, fs, collections::HashMap};
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for better debugging
    tracing_subscriber::fmt().init();

    println!("🚀 Baseline Modlist Parser Demo");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Main demo: Parse the real Baseline modlist
    demo_parse_baseline_modlist().await?;

    Ok(())
}

/// Parse the Baseline modlist and perform comprehensive analysis
async fn demo_parse_baseline_modlist() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📋 Loading Baseline Modlist");

    // Load the modlist JSON file from the Baseline directory
    let modlist_path = PathBuf::from("Baseline/modlist");

    let modlist_json = match fs::read_to_string(&modlist_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("❌ Failed to read modlist file at {:?}: {}", modlist_path, e);
            eprintln!("💡 Make sure you're running from the project root directory");
            return Err(e.into());
        }
    };

    println!("✅ Loaded modlist file ({} bytes)", modlist_json.len());

    // Parse the modlist
    println!("\n⚙️ Parsing modlist JSON...");
    let manifest = parse_modlist(&modlist_json)?;
    println!("✅ Parsing completed successfully");

    // Display comprehensive modlist information
    display_modlist_overview(&manifest)?;

    // Analyze operation types in detail
    analyze_operation_types(&manifest)?;

    // Show sample operations from each category
    show_sample_operations(&manifest)?;

    // Demonstrate conversion to download requests
    demonstrate_download_request_conversion(&manifest).await?;

    println!("\n🎉 Analysis completed successfully!");
    println!("   The modlist has been fully parsed and analyzed without performing any downloads.");

    Ok(())
}

/// Display comprehensive overview of the modlist
fn display_modlist_overview(manifest: &ArchiveManifest) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📊 Modlist Overview");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━");

    println!("📝 Basic Information:");
    println!("  • Name: {}", manifest.metadata.name);
    println!("  • Version: {}", manifest.metadata.version);
    println!("  • Author: {}", manifest.metadata.author);
    println!("  • Game: {}", manifest.metadata.game);
    if !manifest.metadata.description.is_empty() {
        println!("  • Description: {}", manifest.metadata.description);
    }

    println!("\n📈 Statistics:");
    println!("  • Total Archives: {}", manifest.stats.total_operations);
    println!("  • Total Size: {}", manifest.stats.total_download_size_human());
    println!("  • Automation Rate: {:.1}%", manifest.stats.automation_percentage());

    println!("\n🔄 Operations by Type:");
    println!("  • HTTP Downloads: {} ({:.1}%)",
             manifest.stats.http_operations,
             (manifest.stats.http_operations as f64 / manifest.stats.total_operations as f64) * 100.0);
    println!("  • Nexus Downloads: {} ({:.1}%)",
             manifest.stats.nexus_operations,
             (manifest.stats.nexus_operations as f64 / manifest.stats.total_operations as f64) * 100.0);
    println!("  • Game File Copies: {} ({:.1}%)",
             manifest.stats.gamefile_operations,
             (manifest.stats.gamefile_operations as f64 / manifest.stats.total_operations as f64) * 100.0);
    println!("  • Manual Downloads: {} ({:.1}%)",
             manifest.stats.manual_operations,
             (manifest.stats.manual_operations as f64 / manifest.stats.total_operations as f64) * 100.0);
    println!("  • Archive References: {} ({:.1}%)",
             manifest.stats.archive_operations,
             (manifest.stats.archive_operations as f64 / manifest.stats.total_operations as f64) * 100.0);

    if manifest.stats.user_interaction_required > 0 {
        println!("\n⚠️  User Interaction Required: {} operations", manifest.stats.user_interaction_required);
    }
    if manifest.stats.external_dependencies_required > 0 {
        println!("🔗 External Dependencies: {} operations", manifest.stats.external_dependencies_required);
    }

    Ok(())
}

/// Analyze and display detailed breakdown of operation types
fn analyze_operation_types(manifest: &ArchiveManifest) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Detailed Operation Analysis");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let mut count_by_type: HashMap<String, usize> = HashMap::new();
    let mut priority_distribution: HashMap<u32, usize> = HashMap::new();

    // Analyze each operation
    for operation in &manifest.operations {
        let type_name = match &operation.source {
            WabbajackDownloadSource::Http(_) => "HTTP",
            WabbajackDownloadSource::Nexus(_) => "Nexus",
            WabbajackDownloadSource::GameFile(_) => "GameFile",
            WabbajackDownloadSource::Manual(_) => "Manual",
            WabbajackDownloadSource::Archive(_) => "Archive",
            WabbajackDownloadSource::WabbajackCDN(_) => "WabbajackCDN",
        }.to_string();

        *count_by_type.entry(type_name.clone()).or_insert(0) += 1;
        // Note: We don't have direct access to file size in DownloadOperation
        // In a real implementation, you might want to add this information

        *priority_distribution.entry(operation.priority).or_insert(0) += 1;
    }


    println!("\n📂 File Extension Analysis:");
    let mut extensions: HashMap<String, usize> = HashMap::new();

    for operation in &manifest.operations {
        let extension = operation.filename
            .split('.')
            .last()
            .unwrap_or("unknown")
            .to_lowercase();
        *extensions.entry(extension).or_insert(0) += 1;
    }

    let mut ext_vec: Vec<_> = extensions.iter().collect();
    ext_vec.sort_by(|a, b| b.1.cmp(a.1)); // Sort by count descending

    for (ext, count) in ext_vec.iter().take(10) {
        println!("  • .{}: {} files", ext, count);
    }

    if ext_vec.len() > 10 {
        println!("  • ... and {} more extensions", ext_vec.len() - 10);
    }

    Ok(())
}

/// Show sample operations from each category for better understanding
fn show_sample_operations(manifest: &ArchiveManifest) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔬 Sample Operations by Type");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let mut samples_by_type: HashMap<String, Vec<&DownloadOperation>> = HashMap::new();

    // Collect samples of each type
    for operation in &manifest.operations {
        let type_name = match &operation.source {
            WabbajackDownloadSource::Http(_) => "HTTP",
            WabbajackDownloadSource::Nexus(_) => "Nexus",
            WabbajackDownloadSource::GameFile(_) => "GameFile",
            WabbajackDownloadSource::Manual(_) => "Manual",
            WabbajackDownloadSource::Archive(_) => "Archive",
            WabbajackDownloadSource::WabbajackCDN(_) => "WabbajackCDN",
        }.to_string();

        let samples = samples_by_type.entry(type_name).or_insert(Vec::new());
        if samples.len() < 3 { // Keep up to 3 samples per type
            samples.push(operation);
        }
    }

    // Display samples
    for (type_name, samples) in samples_by_type {
        if !samples.is_empty() {
            println!("\n🔹 {} Operations:", type_name);

            for (i, sample) in samples.iter().enumerate() {
                println!("  {}. {}", i + 1, sample.filename);

                match &sample.source {
                    WabbajackDownloadSource::Http(http) => {
                        println!("     📍 URL: {}", http.url);
                    },
                    WabbajackDownloadSource::Nexus(nexus) => {
                        println!("     📍 Mod ID: {}, File ID: {}", nexus.mod_id, nexus.file_id);
                        println!("     🎮 Game: {}", nexus.game_name);
                        if !nexus.mod_name.is_empty() {
                            println!("     📝 Name: {}", nexus.mod_name);
                        }
                    },
                    WabbajackDownloadSource::GameFile(game_file) => {
                        println!("     🎮 Game: {}", game_file.game);
                        println!("     📁 Game File: {}", game_file.file_path);
                        if !game_file.game_version.is_empty() {
                            println!("     🔢 Version: {}", game_file.game_version);
                        }
                    },
                    WabbajackDownloadSource::Manual(manual) => {
                        if let Some(url) = &manual.url {
                            println!("     📍 URL: {}", url);
                        }
                        if !manual.instructions.is_empty() {
                            println!("     💬 Instructions: {}", manual.instructions);
                        }
                    },
                    WabbajackDownloadSource::Archive(archive) => {
                        println!("     🗃️  Archive Hash: {}", archive.archive_hash);
                    },
                    WabbajackDownloadSource::WabbajackCDN(cdn) => {
                        println!("     🌐 CDN URL: {}", cdn.url);
                    },
                }

                if !sample.expected_hash.is_empty() {
                    println!("     🔐 Hash: {} ({})", sample.expected_hash, sample.hash_algorithm);
                }
                if sample.priority > 0 {
                    println!("     ⭐ Priority: {}", sample.priority);
                }

                println!();
            }
        }
    }

    Ok(())
}

/// Demonstrate conversion to download requests without actually downloading
async fn demonstrate_download_request_conversion(manifest: &ArchiveManifest) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔧 Download Request Conversion Demo");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let destination_dir = PathBuf::from("./downloads");

    println!("🎯 Converting operations to download requests...");
    println!("   Target directory: {:?}", destination_dir);

    // Convert with statistics - excluding manual downloads for automation
    let (download_requests, conversion_stats) = manifest_to_download_requests_with_stats(
        &manifest,
        &destination_dir,
        false // Don't include manual downloads
    );

    println!("\n📈 Conversion Results:");
    println!("  • Total Operations: {}", conversion_stats.total_operations);
    println!("  • Converted Requests: {}", conversion_stats.converted_requests);
    println!("  • Skipped Manual: {}", conversion_stats.skipped_manual);
    println!("  • Total Download Size: {} MB", conversion_stats.total_download_size / 1_048_576);
    println!("  • Conversion Rate: {:.1}%",
             (conversion_stats.converted_requests as f64 / conversion_stats.total_operations as f64) * 100.0);

    println!("\n🗂️  Operations by Source Type:");
    for (source_type, count) in &conversion_stats.operations_by_source {
        println!("  • {}: {} operations", source_type, count);
    }

    // Show what would be downloaded vs what requires manual intervention
    let automatable = conversion_stats.converted_requests;
    let manual = conversion_stats.skipped_manual;
    let total = conversion_stats.total_operations;

    println!("\n🤖 Automation Analysis:");
    println!("  • Fully Automatic: {} operations ({:.1}%)",
             automatable, (automatable as f64 / total as f64) * 100.0);
    println!("  • Requires Manual Action: {} operations ({:.1}%)",
             manual, (manual as f64 / total as f64) * 100.0);

    if automatable > 0 {
        println!("\n✅ Ready for automated downloading: {} requests prepared", download_requests.len());
        println!("   (Use the download examples to actually perform the downloads)");
    } else {
        println!("\n⚠️  No operations can be automated - all require manual intervention");
    }

    Ok(())
}