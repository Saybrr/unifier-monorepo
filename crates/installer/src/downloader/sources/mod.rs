//! Download source implementations
//!
//! This module contains individual download source types and their implementations.
//! Each source type is defined in its own file along with its implementation.

use serde::Deserialize;

// Individual source type modules
pub mod unknown;
pub mod http;
pub mod nexus;
pub mod gamefile;
pub mod manual;
pub mod archive;
pub mod wabbajack_cdn;


// Re-export the main enum and all individual types for cleaner imports
pub use unknown::UnknownSource;
pub use http::HttpSource;
pub use nexus::NexusSource;
pub use gamefile::GameFileSource;
pub use manual::ManualSource;
pub use archive::ArchiveSource;
pub use wabbajack_cdn::WabbajackCDNSource;


/// Structured representation of a download source
///
/// This enum represents the different ways a file can be obtained,
/// providing type safety and avoiding the need to serialize/parse URLs.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(from = "crate::parse_wabbajack::parser::ArchiveState")]
pub enum DownloadSource {
    /// Direct HTTP/HTTPS download
    Http(HttpSource),
    /// Download from Nexus Mods via API
    Nexus(NexusSource),
    /// Copy from local game installation
    GameFile(GameFileSource),
    /// Manual download (user must provide file)
    Manual(ManualSource),
    /// Archive extraction from another archive
    Archive(ArchiveSource),
    /// Download from Wabbajack CD
    WabbajackCDN(WabbajackCDNSource),
    /// Unknown download source
    Unknown(UnknownSource),
}


impl DownloadSource {
    /// Get a human-readable description of this download source
    pub fn description(&self) -> String {
        match self {
            DownloadSource::Http(http) => format!("HTTP download from {}", http.url),
            DownloadSource::Nexus(nexus) => {
                format!("Nexus download: {} by {} (mod {}, file {})",
                    nexus.mod_name, nexus.author, nexus.mod_id, nexus.file_id)
            },
            DownloadSource::GameFile(game) => {
                format!("Game file: {} from {}", game.file_path, game.game)
            },
            DownloadSource::Manual(manual) => format!("Manual download: {}", manual.instructions),
            DownloadSource::Archive(archive) => {
                format!("Extract {} from archive {}", archive.inner_path, archive.archive_hash)
            },
            DownloadSource::WabbajackCDN(wabbajack_cdn) => {
                format!("WabbajackCDN download from {}", wabbajack_cdn.url)
            },
            DownloadSource::Unknown(unknown) => {
                format!("Unknown download source: {}", unknown.source_type)
            },
        }
    }

    /// Check if this source requires user interaction
    pub fn requires_user_interaction(&self) -> bool {
        matches!(self, DownloadSource::Manual(_))
    }

    /// Check if this source requires external dependencies (API keys, game installations, etc.)
    pub fn requires_external_dependencies(&self) -> bool {
        matches!(self, DownloadSource::Nexus(_) | DownloadSource::GameFile(_))
    }

    /// Check if this source supports resume functionality
    pub fn supports_resume(&self) -> bool {
        match self {
            DownloadSource::Http(_) => true,
            DownloadSource::WabbajackCDN(_) => true,
            _ => false,
        }
    }
}

/// Implement conversion from ArchiveState to DownloadSource for serde
impl From<crate::parse_wabbajack::parser::ArchiveState> for DownloadSource {
    fn from(state: crate::parse_wabbajack::parser::ArchiveState) -> Self {
        use crate::parse_wabbajack::parser::ArchiveState;

        match state {
            ArchiveState::Http { url, headers } => {
                let mut http_source = HttpSource::new(&url);

                // Parse headers if any (they come as "Key: Value" strings)
                for header_str in &headers {
                    if let Some((key, value)) = header_str.split_once(':') {
                        http_source = http_source.with_header(
                            key.trim().to_string(),
                            value.trim().to_string()
                        );
                    }
                }

                DownloadSource::Http(http_source)
            },

            ArchiveState::Nexus {
                mod_id, file_id, game_name, name, author, version, description, is_nsfw, ..
            } => {
                let author_str = author.as_deref().unwrap_or("Unknown").to_string();
                let nexus_source = NexusSource::new(mod_id, file_id, game_name)
                    .with_metadata(
                        name,
                        author_str,
                        version,
                        description,
                        is_nsfw
                    );

                DownloadSource::Nexus(nexus_source)
            },

            ArchiveState::GameFile { game, game_file, game_version, .. } => {
                let gamefile_source = GameFileSource::new(&game, &game_file, &game_version);
                DownloadSource::GameFile(gamefile_source)
            },

            ArchiveState::WabbajackCDN { url } => {
                let wabbajack_cdn_source = WabbajackCDNSource::new(&url);
                DownloadSource::WabbajackCDN(wabbajack_cdn_source)
            },

            ArchiveState::Unknown => {
                let unknown_source = UnknownSource::new(
                    "Unknown Downloader Type".to_string(),
                    None, // Archive name would need to be passed in context
                    None, // Meta would need to be passed in context
                );
                DownloadSource::Unknown(unknown_source)
            }
        }
    }
}