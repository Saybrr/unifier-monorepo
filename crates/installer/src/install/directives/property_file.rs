//! PropertyFile directive implementation
//!
//! Handles modlist metadata files (banner, readme).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::install::error::InstallError;

/// Property file types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PropertyType {
    Banner,
    Readme,
}

/// Modlist metadata files (banner, readme)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyFileDirective {
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
    /// Type of property file
    #[serde(rename = "Type")]
    pub property_type: PropertyType,
}

impl PropertyFileDirective {
    /// Create a new PropertyFile directive
    pub fn new(
        to: String,
        hash: String,
        size: u64,
        source_data_id: String,
        property_type: PropertyType,
    ) -> Self {
        Self {
            to,
            hash,
            size,
            source_data_id,
            property_type,
        }
    }

    /// Execute the directive - write property file to destination
    pub async fn execute(
        &self,
        install_dir: &PathBuf,
        extracted_modlist_dir: &PathBuf,
        _progress_callback: Option<Box<dyn Fn(u64, u64)>>,
    ) -> Result<(), InstallError> {
        // TODO: Implement property file writing logic
        // 1. Load data from extracted_modlist_dir + self.source_data_id
        // 2. Write data to install_dir + self.to
        // 3. Verify hash matches self.hash
        // 4. Update progress via callback
        // 5. Handle special cases based on self.property_type if needed

        let _destination = install_dir.join(&self.to);
        let _source_data_path = extracted_modlist_dir.join(&self.source_data_id);

        todo!("Implement PropertyFile directive execution")
    }
}
