//! Download source implementations
//!
//! This module contains individual download source types and their implementations.
//! Each source type is defined in its own file along with its implementation.

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
#[derive(Debug, Clone, PartialEq)]
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