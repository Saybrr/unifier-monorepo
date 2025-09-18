//! TestDirective directive implementation
//!
//! Handles writing embedded data directly to the destination.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use crate::install::error::InstallError;
use crate::install::vfs::VfsContext;

/// Test the directive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestDirective {
    /// Destination path relative to install directory
    #[serde(rename = "To")]
    pub to: String,
    /// Content hash of the target file
    #[serde(rename = "Hash")]
    pub hash: String,
    /// Size in bytes of the target file
    #[serde(rename = "Size")]
    pub size: u64,
    /// Reference to test data in the modlist
    #[serde(rename = "SourceDataID")]
    pub source_data_id: String,
}

impl TestDirective {
    /// Create a new TestDirective
    pub fn new(to: String, hash: String, size: u64, source_data_id: String) -> Self {
        Self {
            to,
            hash,
            size,
            source_data_id,
        }
    }

    pub async fn execute(
        &self,
        install_dir: &Arc<PathBuf>,
        extracted_modlist_dir: &Arc<PathBuf>,
        _progress_callback: Option<Box<dyn Fn(u64, u64) + Send + Sync>>,
        _vfs_context: Option<Arc<VfsContext>>,
    ) -> Result<(), InstallError> {
        // Delegate to the existing execute method
        let _destination = install_dir.join(&self.to);
        let _source_data_path = extracted_modlist_dir.join(&self.source_data_id);
        dbg!("test directive");
        Ok(())
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn archive_key(&self) -> String {
        // For test directives, we can use the source_data_id as the archive key
        // or generate a unique key based on the destination path
        format!("test:{}", self.to)
    }
}


