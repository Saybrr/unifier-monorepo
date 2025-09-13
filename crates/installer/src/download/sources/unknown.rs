use async_trait::async_trait;
use crate::downloader::core::{Downloadable, DownloadRequest, DownloadResult, ProgressCallback, Result, DownloadError};
use crate::parse_wabbajack::sources::UnknownSource;

 impl Downloadable for UnknownSource {
    async fn download(&self, request: &DownloadRequest, progress_callback: Option<ProgressCallback>, config: &crate::downloader::config::DownloadConfig) -> Result<DownloadResult> {
        Err(DownloadError::Legacy(
            "Unknown downloads not yet implemented in new architecture".to_string()
        ))
    }
 }