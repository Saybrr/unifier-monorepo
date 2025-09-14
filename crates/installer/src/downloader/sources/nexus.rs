//! Nexus Mods download source implementation

use crate::downloader::core::{
    DownloadRequest, DownloadResult, ProgressCallback, Result, DownloadError
};

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

// Placeholder implementation for Nexus downloads
impl NexusSource {
    pub async fn download(
        &self,
        _request: &DownloadRequest,
        _progress_callback: Option<ProgressCallback>,
        _config: &crate::downloader::config::DownloadConfig,
    ) -> Result<DownloadResult> {
        Ok(DownloadResult::Skipped { reason: format!("Nexus download unimplemented, file id: {}", self.file_id) })
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
