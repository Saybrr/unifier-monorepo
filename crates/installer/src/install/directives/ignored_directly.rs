//! IgnoredDirectly directive implementation
//!
//! Handles files explicitly ignored during compilation.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::install::error::InstallError;

/// Files explicitly ignored during compilation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IgnoredDirectlyDirective {
    /// Destination path relative to install directory
    #[serde(rename = "To")]
    pub to: String,
    /// Content hash of the target file
    #[serde(rename = "Hash")]
    pub hash: String,
    /// Size in bytes of the target file
    #[serde(rename = "Size")]
    pub size: u64,
    /// Reason why the file was ignored
    #[serde(rename = "Reason")]
    pub reason: String,
}

impl IgnoredDirectlyDirective {
    /// Create a new IgnoredDirectly directive
    pub fn new(to: String, hash: String, size: u64, reason: String) -> Self {
        Self {
            to,
            hash,
            size,
            reason,
        }
    }

    /// Execute the directive - this should normally be a no-op
    pub async fn execute(
        &self,
        _install_dir: &PathBuf,
        _progress_callback: Option<Box<dyn Fn(u64, u64)>>,
    ) -> Result<(), InstallError> {
        // TODO: Handle ignored files
        // This directive type should normally not appear in final modlists
        // since ignored files shouldn't be installed.
        //
        // However, if it does appear:
        // 1. Log the reason for ignoring: self.reason
        // 2. Optionally create a placeholder or skip file
        // 3. Update progress via callback

        println!("Skipping ignored file '{}': {}", self.to, self.reason);

        // This is intentionally a no-op - ignored files should not be installed
        Ok(())
    }
}
