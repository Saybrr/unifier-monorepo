//! Manual download source implementation

use async_trait::async_trait;

use crate::downloader::core::{
    Downloadable, DownloadRequest, DownloadResult, ProgressCallback, Result, DownloadError
};
use crate::parse_wabbajack::sources::ManualSource;

// Placeholder implementation for manual downloads
#[async_trait]
impl Downloadable for ManualSource {
    async fn download(
        &self,
        _request: &DownloadRequest,
        _progress_callback: Option<ProgressCallback>,
        _config: &crate::downloader::config::DownloadConfig,
    ) -> Result<DownloadResult> {
        Err(DownloadError::Legacy(
            format!("Manual download required: {}", self.instructions)
        ))
    }

    fn description(&self) -> String {
        format!("Manual download: {}", self.instructions)
    }

    fn requires_user_interaction(&self) -> bool {
        true
    }
}
