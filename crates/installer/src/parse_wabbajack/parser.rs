//! Wabbajack modlist JSON parser
//!
//! This module handles parsing the Wabbajack modlist JSON format and converting
//! it into structured DownloadOperation objects.

use crate::parse_wabbajack::{
    operations::{ArchiveManifest, ManifestMetadata},
};
use crate::downloader::core::{DownloadRequest, DownloadMetadata, DownloadSource};
use crate::downloader::sources::{HttpArchiveState, NexusArchiveState, GameFileArchiveState, WabbajackCDNArchiveState};
use crate::install::directives::{
    FromArchiveDirective,
    PatchedFromArchiveDirective,
    InlineFileDirective,
    RemappedInlineFileDirective,
    TransformedTextureDirective,
    CreateBSADirective,
    MergedPatchDirective,
    PropertyFileDirective,
    ArchiveMetaDirective,
    IgnoredDirectlyDirective,
    NoMatchDirective,
};
use serde::Deserialize;
use std::path::PathBuf;

/// Raw modlist JSON structure as it appears in the file
#[derive(Debug, Deserialize)]
pub struct WabbaModlist {
    #[serde(rename = "Archives")]
    pub archives: Vec<Archive>,
    #[serde(rename = "Directives")]
    pub directives: Vec<Directive>,
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

/// Raw directive entry from the JSON
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "$type")]
pub enum Directive {
    /// Extract a file directly from a downloaded archive
    #[serde(rename = "FromArchive")]
    FromArchive(FromArchiveDirective),

    /// Extract a file from archive and apply a binary patch
    #[serde(rename = "PatchedFromArchive")]
    PatchedFromArchive(PatchedFromArchiveDirective),

    /// Write embedded data directly to the destination
    #[serde(rename = "InlineFile")]
    InlineFile(InlineFileDirective),

    /// Write embedded data with path placeholder replacement
    #[serde(rename = "RemappedInlineFile")]
    RemappedInlineFile(RemappedInlineFileDirective),

    /// Extract texture and apply format/compression changes
    #[serde(rename = "TransformedTexture")]
    TransformedTexture(TransformedTextureDirective),

    /// Build BSA/BA2 archive files from loose files
    #[serde(rename = "CreateBSA")]
    CreateBSA(CreateBSADirective),

    /// Create merged plugin files (like zEdit merges)
    #[serde(rename = "MergedPatch")]
    MergedPatch(MergedPatchDirective),

    /// Modlist metadata files (banner, readme)
    #[serde(rename = "PropertyFile")]
    PropertyFile(PropertyFileDirective),

    /// Create .meta files for Mod Organizer 2
    #[serde(rename = "ArchiveMeta")]
    ArchiveMeta(ArchiveMetaDirective),

    /// Files explicitly ignored during compilation (shouldn't appear in final modlist)
    #[serde(rename = "IgnoredDirectly")]
    IgnoredDirectly(IgnoredDirectlyDirective),

    /// Files that couldn't be matched during compilation (shouldn't appear in final modlist)
    #[serde(rename = "NoMatch")]
    NoMatch(NoMatchDirective),
}


impl Directive {
    /// Get the destination path for any directive type
    pub fn to(&self) -> &str {
        match self {
            Directive::FromArchive(d) => &d.to,
            Directive::PatchedFromArchive(d) => &d.to,
            Directive::InlineFile(d) => &d.to,
            Directive::RemappedInlineFile(d) => &d.to,
            Directive::TransformedTexture(d) => &d.to,
            Directive::CreateBSA(d) => &d.to,
            Directive::MergedPatch(d) => &d.to,
            Directive::PropertyFile(d) => &d.to,
            Directive::ArchiveMeta(d) => &d.to,
            Directive::IgnoredDirectly(d) => &d.to,
            Directive::NoMatch(d) => &d.to,
        }
    }

    /// Get the content hash for any directive type
    pub fn hash(&self) -> &str {
        match self {
            Directive::FromArchive(d) => &d.hash,
            Directive::PatchedFromArchive(d) => &d.hash,
            Directive::InlineFile(d) => &d.hash,
            Directive::RemappedInlineFile(d) => &d.hash,
            Directive::TransformedTexture(d) => &d.hash,
            Directive::CreateBSA(d) => &d.hash,
            Directive::MergedPatch(d) => &d.hash,
            Directive::PropertyFile(d) => &d.hash,
            Directive::ArchiveMeta(d) => &d.hash,
            Directive::IgnoredDirectly(d) => &d.hash,
            Directive::NoMatch(d) => &d.hash,
        }
    }

    /// Get the file size for any directive type
    pub fn size(&self) -> u64 {
        match self {
            Directive::FromArchive(d) => d.size,
            Directive::PatchedFromArchive(d) => d.size,
            Directive::InlineFile(d) => d.size,
            Directive::RemappedInlineFile(d) => d.size,
            Directive::TransformedTexture(d) => d.size,
            Directive::CreateBSA(d) => d.size,
            Directive::MergedPatch(d) => d.size,
            Directive::PropertyFile(d) => d.size,
            Directive::ArchiveMeta(d) => d.size,
            Directive::IgnoredDirectly(d) => d.size,
            Directive::NoMatch(d) => d.size,
        }
    }

    /// Check if this directive requires VFS (archive-based installation)
    pub fn requires_vfs(&self) -> bool {
        matches!(self,
            Directive::FromArchive(_) |
            Directive::PatchedFromArchive(_) |
            Directive::TransformedTexture(_)
        )
    }

    /// Check if this directive is an inline file (embedded data)
    pub fn is_inline(&self) -> bool {
        matches!(self,
            Directive::InlineFile(_) |
            Directive::RemappedInlineFile(_) |
            Directive::PropertyFile(_) |
            Directive::ArchiveMeta(_)
        )
    }

    /// Check if this directive should be processed during installation
    pub fn should_install(&self) -> bool {
        !matches!(self,
            Directive::IgnoredDirectly(_) |
            Directive::NoMatch(_)
        )
    }
}

/// Raw archive entry from the JSON
#[derive(Debug, Deserialize, Clone)]
pub struct Archive {
    #[serde(rename = "Hash")]
    pub hash: String,

    #[serde(rename = "Meta")]
    pub meta: String,

    #[serde(rename = "Name")]
    pub name: String,

    #[serde(rename = "Size")]
    pub size: u64,

    #[serde(rename = "State")]
    pub state: DownloadSource,
}

/// Raw downloader state from JSON (tag-based deserialization using $type field)
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "$type")]
pub enum ArchiveState {
    #[serde(rename = "HttpDownloader, Wabbajack.Lib")]
    Http(HttpArchiveState),

    #[serde(rename = "NexusDownloader, Wabbajack.Lib")]
    Nexus(NexusArchiveState),

    #[serde(rename = "GameFileSourceDownloader, Wabbajack.Lib")]
    GameFile(GameFileArchiveState),

    #[serde(rename = "WabbajackCDNDownloader+State, Wabbajack.Lib")]
    WabbajackCDN(WabbajackCDNArchiveState),

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
    pub fn parse(&self, json: &str, base_destination: &PathBuf) -> Result<ArchiveManifest, ParseError> {
        let raw_modlist: WabbaModlist = serde_json::from_str(json)
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

        // Convert archives to requests
        for (index, archive) in raw_modlist.archives.iter().enumerate() {
            match self.convert_archive_to_request(archive, index, base_destination) {
                Ok(request) => manifest.add_request(request),
                Err(e) => {
                    // Log the error but continue with other archives
                    eprintln!("Warning: Failed to convert archive '{}': {}", archive.name, e);
                }
            }
        }

        for directive in raw_modlist.directives.iter() {
                manifest.add_directive(directive.clone());
        }

        Ok(manifest)
    }

    /// Convert a wabba modlist archive to a structured download request
    fn convert_archive_to_request(
        &self,
        archive: &Archive,
        index: usize,
        base_destination: &PathBuf,
    ) -> Result<DownloadRequest, ParseError> {
        let metadata = DownloadMetadata {
            description: format!("Archive: {}", archive.name),
            category: self.infer_category(&archive.name),
            required: true, // Most modlist archives are required
            tags: self.extract_tags_from_meta(&archive.meta),
        };

        let request = DownloadRequest::new(
            archive.state.clone(),
            base_destination,
            archive.name.clone(),
            archive.size,
            archive.hash.clone(),
        )
        .with_hash_algorithm(&self.default_hash_algorithm)
        .with_priority(index as u32) // Use index as default priority
        .with_metadata(metadata);

        Ok(request)
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
pub fn parse_modlist(json: &str, base_destination: &PathBuf) -> Result<ArchiveManifest, ParseError> {
    ModlistParser::new().parse(json, base_destination)
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
            "Directives": [],
            "Name": "Test Modlist",
            "Version": "1.0",
            "Author": "Test Author",
            "GameName": "TestGame",
            "Description": "A test modlist"
        }"#;

        let base_destination = PathBuf::from("/downloads");
        let manifest = parse_modlist(json, &base_destination).expect("Failed to parse test modlist");

        assert_eq!(manifest.requests.len(), 1);
        assert_eq!(manifest.metadata.name, "Test Modlist");

        let request = &manifest.requests[0];
        assert_eq!(request.filename, "test-file.zip");

        if let DownloadSource::Http(http_source) = &request.source {
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
            ],
            "Directives": []
        }"#;

        let base_destination = std::path::PathBuf::from("/tmp");
        let manifest = parse_modlist(json, &base_destination).expect("Failed to parse nexus test");

        assert_eq!(manifest.requests.len(), 1);

        let operation = &manifest.requests[0];
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

    #[test]
    fn test_parse_unknown_archive() {
        let json = r#"{
            "Archives": [
                {
                    "Hash": "testHash456",
                    "Meta": "[General]\ngameName=skyrimse\nmodID=71371\nfileID=575985",
                    "Name": "unknown-downloader.zip",
                    "Size": 4096,
                    "State": {
                        "$type": "SomeNewDownloader, Custom.Lib",
                        "CustomField": "custom value",
                        "AnotherField": 42
                    }
                }
            ],
            "Directives": []
        }"#;

        let base_destination = std::path::PathBuf::from("/tmp");
        let manifest = parse_modlist(json, &base_destination).expect("Failed to parse unknown test");

        assert_eq!(manifest.requests.len(), 1);

        let operation = &manifest.requests[0];
        if let DownloadSource::Unknown(unknown_source) = &operation.source {
            assert_eq!(unknown_source.source_type, "Unknown Downloader Type");
            // With serde from conversion, archive_name and meta are not available in the ArchiveState::Unknown variant
            assert_eq!(unknown_source.archive_name, None);
            assert_eq!(unknown_source.meta, None);
        } else {
            panic!("Expected Unknown source, got: {:?}", operation.source);
        }
    }
}
