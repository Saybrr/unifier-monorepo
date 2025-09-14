//! Manual download source implementation

use crate::downloader::core::{
    DownloadRequest, DownloadResult, ProgressCallback, Result, DownloadError
};
use crate::parse_wabbajack::sources::ManualSource;

// Placeholder implementation for manual downloads
impl ManualSource {
    pub async fn download(
        &self,
        _request: &DownloadRequest,
        _progress_callback: Option<ProgressCallback>,
        _config: &crate::downloader::config::DownloadConfig,
    ) -> Result<DownloadResult> {
        Err(DownloadError::Legacy(
            format!("Manual download required: {}", self.instructions)
        ))
    }
}
