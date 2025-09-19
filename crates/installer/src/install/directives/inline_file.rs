//! InlineFile directive implementation
//!
//! Handles writing embedded data directly to the destination.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use crate::install::error::InstallError;
use super::common_directive_utils::{
    load_source_data, write_file_with_hash, verify_file_hash, delete_if_exists, ensure_parent_dir
};

/// Write embedded data directly to the destination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineFileDirective {
    /// Destination path relative to install directory
    #[serde(rename = "To")]
    pub to: String,
    /// Content hash of the target file
    #[serde(rename = "Hash")]
    pub hash: String,
    /// Size in bytes of the target file
    #[serde(rename = "Size")]
    pub size: u64,
    /// Reference to embedded data in the modlist
    #[serde(rename = "SourceDataID")]
    pub source_data_id: String,
}

impl InlineFileDirective {
    /// Create a new InlineFile directive
    pub fn new(to: String, hash: String, size: u64, source_data_id: String) -> Self {
        Self {
            to,
            hash,
            size,
            source_data_id,
        }
    }

    /// Execute the directive - write embedded data to destination
    pub async fn execute(
        &self,
        install_dir: &Arc<PathBuf>,
        extracted_modlist_dir: &Arc<PathBuf>,
        progress_callback: Option<Box<dyn Fn(u64, u64) + Send + Sync>>,
    ) -> Result<String, InstallError> {
        let destination = install_dir.join(&self.to);
        let source_data_path = extracted_modlist_dir.join(&self.source_data_id);

        // Delete existing file if it exists (matches C#'s outPath.Delete())
        delete_if_exists(&destination).await?;

        // Ensure parent directory exists
        ensure_parent_dir(&destination).await?;

        // Load data from extracted modlist (equivalent to LoadBytesFromPath in C#)
        let data = load_source_data(&source_data_path).await?;

        // Write data to destination and compute hash (equivalent to WriteAllHashedAsync in C#)
        let computed_hash = write_file_with_hash(&destination, &data).await?;

        // Verify hash matches expected (equivalent to ThrowOnNonMatchingHash in C#)
        // Note: Skip verification for known modified files (like C#'s Consts.KnownModifiedFiles check)
        verify_file_hash(&self.to, &self.hash, &computed_hash)?;

        // Update progress via callback if provided
        if let Some(callback) = progress_callback {
            callback(self.size, self.size);
        }

        // Return the computed hash for caching
        Ok(computed_hash)
    }
}
