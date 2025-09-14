//! Structured download source types
//!
//! This module defines type-safe representations of different download sources
//! that can be parsed from Wabbajack modlist JSON. Each source type contains
//! the specific data needed for that download method.
//!
//! The actual download implementations have been moved to crate::download::sources::*

use std::collections::HashMap;

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

/// Unknown download source
#[derive(Debug, Clone, PartialEq)]
pub struct UnknownSource {
    /// Unknown download source
    pub source: String,
}

/// HTTP download source
#[derive(Debug, Clone, PartialEq)]
pub struct HttpSource {
    /// Primary download URL
    pub url: String,
    /// Optional HTTP headers to send with request
    pub headers: HashMap<String, String>,
    /// Optional fallback URLs if primary fails
    pub mirror_urls: Vec<String>,
}

/// Nexus Mods download source
#[derive(Debug, Clone, PartialEq)]
pub struct NexusSource {
    /// Nexus mod ID
    pub mod_id: u32,
    /// Nexus file ID
    pub file_id: u32,
    /// Game name (e.g., "SkyrimSpecialEdition")
    pub game_name: String,
    /// Mod name for display
    pub mod_name: String,
    /// Mod author
    pub author: String,
    /// Mod version
    pub version: String,
    /// Mod description
    pub description: String,
    /// Whether the mod is marked NSFW
    pub is_nsfw: bool,
}

/// Game file copy source
#[derive(Debug, Clone, PartialEq)]
pub struct GameFileSource {
    /// Game identifier (e.g., "SkyrimSpecialEdition")
    pub game: String,
    /// Relative path to file within game installation
    pub file_path: String,
    /// Expected game version
    pub game_version: String,
}

/// Manual download source (user must provide)
#[derive(Debug, Clone, PartialEq)]
pub struct ManualSource {
    /// Instructions for the user
    pub instructions: String,
    /// Optional URL where user can find the file
    pub url: Option<String>,
}

/// Archive extraction source
#[derive(Debug, Clone, PartialEq)]
pub struct ArchiveSource {
    /// Hash of the source archive
    pub archive_hash: String,
    /// Path within the archive to extract
    pub inner_path: String,
}

/// Wabbajack CDN download source
#[derive(Debug, Clone, PartialEq)]
pub struct WabbajackCDNSource {
    /// CDN URL for the file
    pub url: String,
}

// Builder methods for source types
impl HttpSource {
    pub fn new<S: Into<String>>(url: S) -> Self {
        Self {
            url: url.into(),
            headers: HashMap::new(),
            mirror_urls: Vec::new(),
        }
    }

    pub fn with_header<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    pub fn with_mirror<S: Into<String>>(mut self, mirror_url: S) -> Self {
        self.mirror_urls.push(mirror_url.into());
        self
    }
}

impl NexusSource {
    pub fn new(mod_id: u32, file_id: u32, game_name: String) -> Self {
        Self {
            mod_id,
            file_id,
            game_name,
            mod_name: String::new(),
            author: String::new(),
            version: String::new(),
            description: String::new(),
            is_nsfw: false,
        }
    }

    pub fn with_metadata<S: Into<String>>(
        mut self,
        name: S,
        author: S,
        version: S,
        description: S,
        is_nsfw: bool,
    ) -> Self {
        self.mod_name = name.into();
        self.author = author.into();
        self.version = version.into();
        self.description = description.into();
        self.is_nsfw = is_nsfw;
        self
    }
}

impl GameFileSource {
    pub fn new<S1: Into<String>, S2: Into<String>, S3: Into<String>>(
        game: S1,
        file_path: S2,
        game_version: S3,
    ) -> Self {
        Self {
            game: game.into(),
            file_path: file_path.into(),
            game_version: game_version.into(),
        }
    }
}

impl ManualSource {
    pub fn new<S: Into<String>>(instructions: S) -> Self {
        Self {
            instructions: instructions.into(),
            url: None,
        }
    }

    pub fn with_url<S: Into<String>>(mut self, url: S) -> Self {
        self.url = Some(url.into());
        self
    }
}

impl ArchiveSource {
    pub fn new<S1: Into<String>, S2: Into<String>>(archive_hash: S1, inner_path: S2) -> Self {
        Self {
            archive_hash: archive_hash.into(),
            inner_path: inner_path.into(),
        }
    }
}

impl WabbajackCDNSource {
    pub fn new<S: Into<String>>(url: S) -> Self {
        Self {
            url: url.into(),
        }
    }
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
                format!("Unknown download source: {}", unknown.source)
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