//! RemappedInlineFile directive implementation
//!
//! Handles writing embedded data with path placeholder replacement.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use crate::install::error::InstallError;
use super::common_directive_utils::{
    load_source_text, write_file_with_hash, delete_if_exists, ensure_parent_dir, apply_path_replacements
};

/// Write embedded data with path placeholder replacement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemappedInlineFile {
    /// Destination path relative to install directory
    #[serde(rename = "To")]
    pub to: String,
    /// Content hash of the target file (after path remapping)
    #[serde(rename = "Hash")]
    pub hash: String,
    /// Size in bytes of the target file (after path remapping)
    #[serde(rename = "Size")]
    pub size: u64,
    /// Reference to embedded data in the modlist
    #[serde(rename = "SourceDataID")]
    pub source_data_id: String,
}

impl RemappedInlineFile {
    /// Create a new RemappedInlineFile directive
    pub fn new(to: String, hash: String, size: u64, source_data_id: String) -> Self {
        Self {
            to,
            hash,
            size,
            source_data_id,
        }
    }

    /// Execute the directive - write embedded data with path remapping to destination
    pub async fn execute(
        &self,
        install_dir: &Arc<PathBuf>,
        extracted_modlist_dir: &Arc<PathBuf>,
        game_dir: &Arc<PathBuf>,
        downloads_dir: &Arc<PathBuf>,
        progress_callback: Option<Box<dyn Fn(u64, u64) + Send + Sync>>,
    ) -> Result<String, InstallError> {
        let destination = install_dir.join(&self.to);
        let source_data_path = extracted_modlist_dir.join(&self.source_data_id);

        // Delete existing file if it exists (matches C#'s outPath.Delete())
        delete_if_exists(&destination).await?;

        // Ensure parent directory exists
        ensure_parent_dir(&destination).await?;

        // Load text data from extracted modlist (equivalent to LoadBytesFromPath + Encoding.UTF8.GetString in C#)
        let content = load_source_text(&source_data_path).await?;

        // Apply path magic replacements (equivalent to C#'s WriteRemappedFile logic)
        let replacements = self.get_path_replacements(install_dir, game_dir, downloads_dir);
        let remapped_content = apply_path_replacements(&content, &replacements);

        // Write remapped data to destination and compute hash
        let computed_hash = write_file_with_hash(&destination, remapped_content.as_bytes()).await?;

        // Note: Hash verification is skipped for remapped files since content changes
        // This matches C# behavior where FileHashCache.FileHashCachedAsync is called but no verification

        // Update progress via callback if provided
        if let Some(callback) = progress_callback {
            callback(self.size, self.size);
        }

        // Return the computed hash for caching
        Ok(computed_hash)
    }

    /// Get the path magic replacements that will be applied
    pub fn get_path_replacements(&self, install_dir: &Arc<PathBuf>, game_dir: &Arc<PathBuf>, downloads_dir: &Arc<PathBuf>) -> Vec<(String, String)> {
        vec![
            ("{GAME_PATH_MAGIC_BACK}".to_string(), game_dir.display().to_string()),
            ("{GAME_PATH_MAGIC_DOUBLE_BACK}".to_string(), game_dir.display().to_string().replace("\\", "\\\\")),
            ("{GAME_PATH_MAGIC_FORWARD}".to_string(), game_dir.display().to_string().replace("\\", "/")),
            ("{MO2_PATH_MAGIC_BACK}".to_string(), install_dir.display().to_string()),
            ("{MO2_PATH_MAGIC_DOUBLE_BACK}".to_string(), install_dir.display().to_string().replace("\\", "\\\\")),
            ("{MO2_PATH_MAGIC_FORWARD}".to_string(), install_dir.display().to_string().replace("\\", "/")),
            ("{DOWNLOAD_PATH_MAGIC_BACK}".to_string(), downloads_dir.display().to_string()),
            ("{DOWNLOAD_PATH_MAGIC_DOUBLE_BACK}".to_string(), downloads_dir.display().to_string().replace("\\", "\\\\")),
            ("{DOWNLOAD_PATH_MAGIC_FORWARD}".to_string(), downloads_dir.display().to_string().replace("\\", "/")),
        ]
    }
}
