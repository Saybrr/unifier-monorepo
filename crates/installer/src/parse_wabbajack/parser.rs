//! Wabbajack modlist JSON parser
//!
//! This module handles parsing the Wabbajack modlist JSON format and converting
//! it into structured DownloadOperation objects.

use crate::parse_wabbajack::{
    sources::{DownloadSource, HttpSource, NexusSource, GameFileSource, WabbajackCDNSource},
    operations::{DownloadOperation, ArchiveManifest, ManifestMetadata, OperationMetadata},
};
use serde::Deserialize;

/// Raw modlist JSON structure as it appears in the file
#[derive(Debug, Deserialize)]
pub struct RawModlist {
    #[serde(rename = "Archives")]
    pub archives: Vec<RawArchive>,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "Version", default)]
    pub version: String,
    #[serde(rename = "Author", default)]
    pub author: String,
    #[serde(rename = "GameName", default)]
    pub game: String,
    #[serde(rename = "Description", default)]
    pub description: String,
}

/// Raw archive entry from the JSON
#[derive(Debug, Deserialize, Clone)]
pub struct RawArchive {
    #[serde(rename = "Hash")]
    pub hash: String,

    #[serde(rename = "Meta")]
    pub meta: String,

    #[serde(rename = "Name")]
    pub name: String,

    #[serde(rename = "Size")]
    pub size: u64,

    #[serde(rename = "State")]
    pub state: RawDownloaderState,
}

/// Raw downloader state from JSON (tagged union)
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "$type")]
pub enum RawDownloaderState {
    #[serde(rename = "HttpDownloader, Wabbajack.Lib")]
    Http {
        #[serde(rename = "Url")]
        url: String,
        #[serde(rename = "Headers", default)]
        headers: Vec<String>,
    },

    #[serde(rename = "NexusDownloader, Wabbajack.Lib")]
    Nexus {
        #[serde(rename = "ModID")]
        mod_id: u32,
        #[serde(rename = "FileID")]
        file_id: u32,
        #[serde(rename = "GameName")]
        game_name: String,
        #[serde(rename = "Name")]
        name: String,
        #[serde(rename = "Author")]
        author: Option<String>,
        #[serde(rename = "Version")]
        version: String,
        #[serde(rename = "Description")]
        description: String,
        #[serde(rename = "IsNSFW")]
        is_nsfw: bool,
        #[serde(rename = "ImageURL")]
        image_url: Option<String>,
    },

    #[serde(rename = "GameFileSourceDownloader, Wabbajack.Lib")]
    GameFile {
        #[serde(rename = "Game")]
        game: String,
        #[serde(rename = "GameFile")]
        game_file: String,
        #[serde(rename = "GameVersion")]
        game_version: String,
        #[serde(rename = "Hash")]
        hash: String,
    },

    #[serde(rename = "WabbajackCDNDownloader+State, Wabbajack.Lib")]
    WabbajackCDN {
        #[serde(rename = "Url")]
        url: String,
    },

    // Handle unknown downloader types gracefully
    #[serde(other)]
    Unknown,
}

/// Parser for Wabbajack modlists
pub struct ModlistParser {
    /// Optional hash algorithm override (default: SHA256)
    pub default_hash_algorithm: String,
}

impl ModlistParser {
    /// Create a new parser with default settings
    pub fn new() -> Self {
        Self {
            default_hash_algorithm: "XXHASH64".to_string(),
        }
    }

    /// Set the default hash algorithm
    pub fn with_hash_algorithm<S: Into<String>>(mut self, algorithm: S) -> Self {
        self.default_hash_algorithm = algorithm.into();
        self
    }

    /// Parse a modlist JSON string into an ArchiveManifest
    pub fn parse(&self, json: &str) -> Result<ArchiveManifest, ParseError> {
        let raw_modlist: RawModlist = serde_json::from_str(json)
            .map_err(ParseError::JsonParseError)?;

        let mut manifest = ArchiveManifest::new();

        // Set manifest metadata
        manifest.metadata = ManifestMetadata {
            name: raw_modlist.name.clone(),
            version: raw_modlist.version.clone(),
            author: raw_modlist.author.clone(),
            game: raw_modlist.game.clone(),
            description: raw_modlist.description.clone(),
        };

        // Convert archives to operations
        for (index, archive) in raw_modlist.archives.iter().enumerate() {
            match self.convert_archive_to_operation(archive, index) {
                Ok(operation) => manifest.add_operation(operation),
                Err(e) => {
                    // Log the error but continue with other archives
                    eprintln!("Warning: Failed to convert archive '{}': {}", archive.name, e);
                }
            }
        }

        Ok(manifest)
    }

    /// Convert a raw archive to a structured download operation
    fn convert_archive_to_operation(
        &self,
        archive: &RawArchive,
        index: usize,
    ) -> Result<DownloadOperation, ParseError> {
        let source = self.convert_downloader_state(&archive.state)?;

        let metadata = OperationMetadata {
            description: format!("Archive: {}", archive.name),
            category: self.infer_category(&archive.name),
            required: true, // Most modlist archives are required
            tags: self.extract_tags_from_meta(&archive.meta),
        };

        let operation = DownloadOperation::new(
            source,
            archive.name.clone(),
            archive.hash.clone(),
            archive.size,
        )
        .with_priority(index as u32) // Use index as default priority
        .with_metadata(metadata);

        Ok(operation)
    }

    /// Convert raw downloader state to structured source
    fn convert_downloader_state(&self, state: &RawDownloaderState) -> Result<DownloadSource, ParseError> {
        match state {
            RawDownloaderState::Http { url, headers } => {
                let mut http_source = HttpSource::new(url);

                // Parse headers if any (they come as "Key: Value" strings)
                for header_str in headers {
                    if let Some((key, value)) = header_str.split_once(':') {
                        http_source = http_source.with_header(
                            key.trim().to_string(),
                            value.trim().to_string()
                        );
                    }
                }

                Ok(DownloadSource::Http(http_source))
            },

            RawDownloaderState::Nexus {
                mod_id, file_id, game_name, name, author, version, description, is_nsfw, ..
            } => {
                let author_str = author.as_deref().unwrap_or("Unknown");
                let nexus_source = NexusSource::new(*mod_id, *file_id, game_name.clone())
                    .with_metadata(
                        name.as_str(),
                        author_str,
                        version.as_str(),
                        description.as_str(),
                        *is_nsfw
                    );

                Ok(DownloadSource::Nexus(nexus_source))
            },

            RawDownloaderState::GameFile { game, game_file, game_version, .. } => {
                let gamefile_source = GameFileSource::new(game, game_file, game_version);
                Ok(DownloadSource::GameFile(gamefile_source))
            },

            RawDownloaderState::WabbajackCDN { url } => {
                let wabbajack_cdn_source = WabbajackCDNSource::new(url);
                Ok(DownloadSource::WabbajackCDN(wabbajack_cdn_source))
            },

            RawDownloaderState::Unknown => {
                Err(ParseError::UnsupportedDownloaderType("Unknown downloader type".to_string()))
            }
        }
    }

    /// Infer category from filename patterns
    fn infer_category(&self, filename: &str) -> String {
        let lower = filename.to_lowercase();

        if lower.contains("texture") || lower.contains("dds") {
            "Textures".to_string()
        } else if lower.contains("mesh") || lower.contains("nif") {
            "Meshes".to_string()
        } else if lower.contains("esp") || lower.contains("esm") || lower.contains("esl") {
            "Plugins".to_string()
        } else if lower.contains("script") || lower.contains("psc") {
            "Scripts".to_string()
        } else if lower.contains("sound") || lower.contains("wav") || lower.contains("xwm") {
            "Audio".to_string()
        } else if lower.contains("animation") || lower.contains("hkx") {
            "Animations".to_string()
        } else {
            "General".to_string()
        }
    }

    /// Extract tags from the Meta field
    fn extract_tags_from_meta(&self, meta: &str) -> Vec<String> {
        let mut tags = Vec::new();

        // Parse simple key-value pairs from Meta field
        for line in meta.lines() {
            if line.contains("gameName=") {
                tags.push("game-specific".to_string());
            }
            if line.contains("modID=") {
                tags.push("nexus-mod".to_string());
            }
            if line.contains("directURL=") {
                tags.push("direct-download".to_string());
            }
        }

        tags
    }
}

impl Default for ModlistParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that can occur during parsing
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("JSON parsing error: {0}")]
    JsonParseError(#[from] serde_json::Error),

    #[error("Unsupported downloader type: {0}")]
    UnsupportedDownloaderType(String),

    #[error("Invalid archive data: {0}")]
    InvalidArchiveData(String),
}

/// Convenience function to parse a modlist JSON string
pub fn parse_modlist(json: &str) -> Result<ArchiveManifest, ParseError> {
    ModlistParser::new().parse(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_http_archive() {
        let json = r#"{
            "Archives": [
                {
                    "Hash": "rXDEtl7gdOU=",
                    "Meta": "[General]\ndirectURL=https://example.com/file.zip",
                    "Name": "test-file.zip",
                    "Size": 1024,
                    "State": {
                        "$type": "HttpDownloader, Wabbajack.Lib",
                        "Headers": [],
                        "Url": "https://example.com/file.zip"
                    }
                }
            ],
            "Name": "Test Modlist",
            "Version": "1.0",
            "Author": "Test Author",
            "GameName": "TestGame",
            "Description": "A test modlist"
        }"#;

        let manifest = parse_modlist(json).expect("Failed to parse test modlist");

        assert_eq!(manifest.operations.len(), 1);
        assert_eq!(manifest.metadata.name, "Test Modlist");

        let operation = &manifest.operations[0];
        assert_eq!(operation.filename, "test-file.zip");
        // Note: expected_size field removed since it's unreliable from Wabbajack data

        if let DownloadSource::Http(http_source) = &operation.source {
            assert_eq!(http_source.url, "https://example.com/file.zip");
        } else {
            panic!("Expected HTTP source");
        }
    }

    #[test]
    fn test_parse_nexus_archive() {
        let json = r#"{
            "Archives": [
                {
                    "Hash": "testHash123",
                    "Meta": "[General]\ngameName=skyrimse\nmodID=12345",
                    "Name": "nexus-mod.zip",
                    "Size": 2048,
                    "State": {
                        "$type": "NexusDownloader, Wabbajack.Lib",
                        "ModID": 12345,
                        "FileID": 67890,
                        "GameName": "SkyrimSpecialEdition",
                        "Name": "Test Nexus Mod",
                        "Author": "Mod Author",
                        "Version": "1.2.3",
                        "Description": "A test mod from Nexus",
                        "IsNSFW": false,
                        "ImageURL": "https://example.com/image.jpg"
                    }
                }
            ]
        }"#;

        let manifest = parse_modlist(json).expect("Failed to parse nexus test");

        assert_eq!(manifest.operations.len(), 1);

        let operation = &manifest.operations[0];
        if let DownloadSource::Nexus(nexus_source) = &operation.source {
            assert_eq!(nexus_source.mod_id, 12345);
            assert_eq!(nexus_source.file_id, 67890);
            assert_eq!(nexus_source.game_name, "SkyrimSpecialEdition");
            assert_eq!(nexus_source.mod_name, "Test Nexus Mod");
            assert!(!nexus_source.is_nsfw);
        } else {
            panic!("Expected Nexus source");
        }
    }
}
