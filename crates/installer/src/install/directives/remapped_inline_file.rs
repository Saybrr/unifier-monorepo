//! RemappedInlineFile directive implementation
//!
//! Handles writing embedded data with path placeholder replacement.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::install::error::InstallError;

/// Write embedded data with path placeholder replacement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemappedInlineFileDirective {
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

impl RemappedInlineFileDirective {
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
        install_dir: &PathBuf,
        extracted_modlist_dir: &PathBuf,
        _game_dir: &PathBuf,
        _downloads_dir: &PathBuf,
        _progress_callback: Option<Box<dyn Fn(u64, u64)>>,
    ) -> Result<(), InstallError> {
        // TODO: Implement remapped inline file writing logic
        // 1. Load data from extracted_modlist_dir + self.source_data_id as string
        // 2. Replace path placeholders:
        //    - {GAME_PATH_MAGIC_*} -> game_dir
        //    - {MO2_PATH_MAGIC_*} -> install_dir
        //    - {DOWNLOAD_PATH_MAGIC_*} -> downloads_dir
        // 3. Write remapped data to install_dir + self.to
        // 4. Note: Hash verification may be tricky since content changes
        // 5. Update progress via callback

        let _destination = install_dir.join(&self.to);
        let _source_data_path = extracted_modlist_dir.join(&self.source_data_id);

        todo!("Implement RemappedInlineFile directive execution")
    }

    /// Get the path magic replacements that will be applied
    pub fn get_path_replacements(&self, install_dir: &PathBuf, game_dir: &PathBuf, downloads_dir: &PathBuf) -> Vec<(String, String)> {
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
