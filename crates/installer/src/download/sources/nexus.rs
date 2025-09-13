//! Nexus Mods download source implementation

use async_trait::async_trait;

use crate::downloader::core::{
    Downloadable, DownloadRequest, DownloadResult, ProgressCallback, Result, DownloadError
};
use crate::parse_wabbajack::sources::NexusSource;

// Placeholder implementation for Nexus downloads
#[async_trait]
impl Downloadable for NexusSource {
    async fn download(
        &self,
        _request: &DownloadRequest,
        _progress_callback: Option<ProgressCallback>,
        _config: &crate::downloader::config::DownloadConfig,
    ) -> Result<DownloadResult> {
        Err(DownloadError::Legacy(
            "Nexus downloads not yet implemented in new architecture".to_string()
        ))
    }

    fn description(&self) -> String {
        format!("Nexus download: {} by {} (mod {}, file {})",
                self.mod_name, self.author, self.mod_id, self.file_id)
    }

    fn requires_external_dependencies(&self) -> bool {
        true // Requires Nexus API key
    }
}
