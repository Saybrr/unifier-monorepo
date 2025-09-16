//! TransformedTexture directive implementation
//!
//! Handles extracting textures and applying format/compression changes.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::install::error::InstallError;

/// Extract texture and apply format/compression changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformedTextureDirective {
    /// Destination path relative to install directory
    #[serde(rename = "To")]
    pub to: String,
    /// Content hash of the target file (after transformation)
    #[serde(rename = "Hash")]
    pub hash: String,
    /// Size in bytes of the target file (after transformation)
    #[serde(rename = "Size")]
    pub size: u64,
    /// Reference to file within an archive: [archive_hash, path, components...]
    #[serde(rename = "ArchiveHashPath")]
    pub archive_hash_path: Vec<String>,
    /// Texture transformation parameters (complex object)
    #[serde(rename = "ImageState")]
    pub image_state: serde_json::Value,
}

impl TransformedTextureDirective {
    /// Create a new TransformedTexture directive
    pub fn new(
        to: String,
        hash: String,
        size: u64,
        archive_hash_path: Vec<String>,
        image_state: serde_json::Value,
    ) -> Self {
        Self {
            to,
            hash,
            size,
            archive_hash_path,
            image_state,
        }
    }

    /// Execute the directive - extract texture, apply transformations, write to destination
    pub async fn execute(
        &self,
        install_dir: &PathBuf,
        _vfs_context: &(), // TODO: Replace with actual VFS type
        _progress_callback: Option<Box<dyn Fn(u64, u64)>>,
    ) -> Result<(), InstallError> {
        // TODO: Implement texture transformation logic
        // 1. Use VFS to locate and extract source texture from archive
        // 2. Parse self.image_state to determine transformations needed
        // 3. Apply transformations (format conversion, compression, resizing, etc.)
        // 4. Write transformed texture to install_dir + self.to
        // 5. Verify hash matches self.hash
        // 6. Update progress via callback

        let _destination = install_dir.join(&self.to);

        todo!("Implement TransformedTexture directive execution")
    }

    /// Get the archive hash (first element of archive_hash_path)
    pub fn archive_hash(&self) -> Option<&str> {
        self.archive_hash_path.first().map(|s| s.as_str())
    }

    /// Get the path within the archive (remaining elements)
    pub fn archive_path(&self) -> Vec<&str> {
        if self.archive_hash_path.len() > 1 {
            self.archive_hash_path[1..].iter().map(|s| s.as_str()).collect()
        } else {
            vec![]
        }
    }
}
