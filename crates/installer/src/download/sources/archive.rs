//! Archive extraction source implementation

use async_trait::async_trait;

use crate::downloader::core::{
    Downloadable, DownloadRequest, DownloadResult, ProgressCallback, Result, DownloadError
};
use crate::parse_wabbajack::sources::ArchiveSource;

// Placeholder implementation for archive extraction
#[async_trait]
impl Downloadable for ArchiveSource {
    async fn download(
        &self,
        _request: &DownloadRequest,
        _progress_callback: Option<ProgressCallback>,
        _config: &crate::downloader::config::DownloadConfig,
    ) -> Result<DownloadResult> {
        Err(DownloadError::Legacy(
            "Archive extraction not yet implemented in new architecture".to_string()
        ))
    }

    fn description(&self) -> String {
        format!("Extract {} from archive {}", self.inner_path, self.archive_hash)
    }
}
