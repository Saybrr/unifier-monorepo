//! Archive extraction source implementation

use crate::downloader::core::{
    DownloadRequest, DownloadResult, ProgressCallback, Result, DownloadError
};

/// Archive extraction source
#[derive(Debug, Clone, PartialEq)]
pub struct ArchiveSource {
    /// Hash of the source archive
    pub archive_hash: String,
    /// Path within the archive to extract
    pub inner_path: String,
}

// Placeholder implementation for archive extraction
impl ArchiveSource {
    pub async fn download(
        &self,
        _request: &DownloadRequest,
        _progress_callback: Option<ProgressCallback>,
        _config: &crate::downloader::core::config::DownloadConfig,
    ) -> Result<DownloadResult> {
        Err(DownloadError::Legacy(
            "Archive extraction not yet implemented in new architecture".to_string()
        ))
    }
}

impl ArchiveSource {
    pub fn new<S1: Into<String>, S2: Into<String>>(archive_hash: S1, inner_path: S2) -> Self {
        Self {
            archive_hash: archive_hash.into(),
            inner_path: inner_path.into(),
        }
    }
}
