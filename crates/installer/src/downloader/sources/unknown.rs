use crate::downloader::core::{DownloadRequest, DownloadResult, ProgressCallback, Result};

/// Unknown download source
#[derive(Debug, Clone, PartialEq)]
pub struct UnknownSource {
    /// The original $type field from the JSON
    pub source_type: String,
    /// Archive name from the modlist
    pub archive_name: Option<String>,
    /// Meta information from the modlist
    pub meta: Option<String>,
}

impl UnknownSource {
    /// Create a new unknown source with all information
    pub fn new<S: Into<String>>(
        source_type: S,
        archive_name: Option<String>,
        meta: Option<String>,
    ) -> Self {
        Self {
            source_type: source_type.into(),
            archive_name,
            meta,
        }
    }

    pub async fn download(&self, _request: &DownloadRequest, _progress_callback: Option<ProgressCallback>, _config: &crate::downloader::core::config::DownloadConfig) -> Result<DownloadResult> {
        let mut reason = format!("Unknown download type: '{}'", self.source_type);

        if let Some(ref name) = self.archive_name {
            reason.push_str(&format!(" (Archive: '{}')", name));
        }

        if let Some(ref meta) = self.meta {
            // Extract key information from meta for the skip reason
            let meta_lines: Vec<&str> = meta.lines().take(3).collect();
            if !meta_lines.is_empty() {
                reason.push_str(&format!(" [Meta: {}]", meta_lines.join(", ")));
            }
        }

        Ok(DownloadResult::Skipped { reason })
    }
}