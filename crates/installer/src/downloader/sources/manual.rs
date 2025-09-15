//! Manual download source implementation

use crate::downloader::core::{
    DownloadRequest, DownloadResult, ProgressCallback, Result, DownloadError
};

/// Manual download source (user must provide)
#[derive(Debug, Clone, PartialEq)]
pub struct ManualSource {
    /// Instructions for the user
    pub instructions: String,
    /// Optional URL where user can find the file
    pub url: Option<String>,
}

// Placeholder implementation for manual downloads
impl ManualSource {
    pub async fn download(
        &self,
        _request: &DownloadRequest,
        _progress_callback: Option<ProgressCallback>,
        _config: &crate::downloader::core::config::DownloadConfig,
    ) -> Result<DownloadResult> {
        Err(DownloadError::Legacy(
            format!("Manual download required: {}", self.instructions)
        ))
    }
}

impl ManualSource {
    pub fn new<S: Into<String>>(instructions: S) -> Self {
        Self {
            instructions: instructions.into(),
            url: None,
        }
    }

    pub fn with_url<S: Into<String>>(mut self, url: S) -> Self {
        self.url = Some(url.into());
        self
    }
}
