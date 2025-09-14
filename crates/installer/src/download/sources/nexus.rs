//! Nexus Mods download source implementation

use crate::downloader::core::{
    DownloadRequest, DownloadResult, ProgressCallback, Result, DownloadError
};
use crate::parse_wabbajack::sources::NexusSource;

// Placeholder implementation for Nexus downloads
impl NexusSource {
    pub async fn download(
        &self,
        _request: &DownloadRequest,
        _progress_callback: Option<ProgressCallback>,
        _config: &crate::downloader::config::DownloadConfig,
    ) -> Result<DownloadResult> {
        Err(DownloadError::Legacy(
            "Nexus downloads not yet implemented in new architecture".to_string()
        ))
    }
}
