//! Wabbajack modlist JSON parser
//!
//! This module handles parsing the Wabbajack modlist JSON format and converting
//! it into structured DownloadOperation objects.

use crate::downloader::core::{DownloadRequest,  DownloadSource};
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

impl WabbaModlist {

    /// Parse a modlist JSON string into an ArchiveManifest
    pub fn parse(json: &str ) -> Result<WabbaModlist, ParseError> {
        let modlist: WabbaModlist = serde_json::from_str(json)
            .map_err(ParseError::JsonParseError)?;
        Ok(modlist)
    }

    pub fn get_dl_requests(&self, base_destination: &PathBuf) -> Result<Vec<DownloadRequest>, ParseError> {
        let requests = self.archives.iter()
            .map(|archive| archive.to_dl_request(base_destination))
            .collect::<Result<Vec<DownloadRequest>, ParseError>>()?;
        Ok(requests)
    }

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

impl Archive {
    /// Convert a wabba modlist archive to a structured download request
    fn to_dl_request(
        &self,
        base_destination: &PathBuf,
    ) -> Result<DownloadRequest, ParseError> {

        let request = DownloadRequest::new(
            self.state.clone(),
            base_destination,
            self.name.clone(),
            self.size,
            self.hash.clone(),
        );

        Ok(request)
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

#[cfg(test)]
mod tests {
    use std::fs;

    use http::request;

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
        let manifest: WabbaModlist = serde_json::from_str(json).map_err(ParseError::JsonParseError)
            .expect("Failed to parse JSON");


        let requests = manifest.archives.iter()
            .map(|archive| archive.to_dl_request(&base_destination))
            .collect::<Result<Vec<DownloadRequest>, ParseError>>()
            .expect("Failed to convert archives to download requests");
        assert_eq!(requests.len(), 1);

        let request = &requests[0];
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
        let manifest: WabbaModlist = serde_json::from_str(json).map_err(ParseError::JsonParseError)
            .expect("Failed to parse JSON");

        let requests = manifest.archives.iter()
            .map(|archive| archive.to_dl_request(&base_destination))
            .collect::<Result<Vec<DownloadRequest>, ParseError>>()
            .expect("Failed to convert archives to download requests");
        assert_eq!(requests.len(), 1);

        let operation = &requests[0];
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
        let manifest: WabbaModlist = serde_json::from_str(json).map_err(ParseError::JsonParseError)
        .expect("Failed to parse JSON");

    let requests = manifest.archives.iter()
        .map(|archive| archive.to_dl_request(&base_destination))
        .collect::<Result<Vec<DownloadRequest>, ParseError>>()
        .expect("Failed to convert archives to download requests");
    assert_eq!(requests.len(), 1);

        let operation = &requests[0];
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
