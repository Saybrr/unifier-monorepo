//! FromArchive directive implementation
//!
//! Handles extracting files directly from downloaded archives.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use crate::install::error::InstallError;
use crate::install::vfs::VfsContext;
/// Extract a file directly from a downloaded archive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FromArchive {
    /// Destination path relative to install directory
    #[serde(rename = "To")]
    pub to: String,
    /// Content hash of the target file
    #[serde(rename = "Hash")]
    pub hash: String,
    /// Size in bytes of the target file
    #[serde(rename = "Size")]
    pub size: u64,
    /// Reference to file within an archive: [archive_hash, path, components...]
    #[serde(rename = "ArchiveHashPath")]
    pub archive_hash_path: Vec<String>,
}

impl FromArchive {
    /// Create a new FromArchive directive
    pub fn new(to: String, hash: String, size: u64, archive_hash_path: Vec<String>) -> Self {
        Self {
            to,
            hash,
            size,
            archive_hash_path,
        }
    }

    /// Execute the directive - extract file from archive to destination
    pub async fn execute(
        &self,
        install_dir: &Arc<PathBuf>,
        _extracted_modlist_dir: &Arc<PathBuf>,
        _vfs_context: Arc<VfsContext>,
        _progress_callback: Option<Box<dyn Fn(u64, u64) + Send + Sync>>,
    ) -> Result<(), InstallError> {
        // TODO: Implement archive extraction logic
        // 1. Use VFS to locate the file in the archive
        // 2. Extract to install_dir + self.to
        // 3. Verify hash matches self.hash
        // 4. Update progress via callback

        let _destination = install_dir.join(&self.to);
        dbg!("from archive");
        //todo!("Implement FromArchive directive execution")
        Ok(())
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

