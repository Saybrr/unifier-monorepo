//! NoMatch directive implementation
//!
//! Handles files that couldn't be matched during compilation.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::install::error::InstallError;

/// Files that couldn't be matched during compilation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoMatchDirective {
    /// Destination path relative to install directory
    #[serde(rename = "To")]
    pub to: String,
    /// Content hash of the target file
    #[serde(rename = "Hash")]
    pub hash: String,
    /// Size in bytes of the target file
    #[serde(rename = "Size")]
    pub size: u64,
    /// Reason why the file couldn't be matched
    #[serde(rename = "Reason")]
    pub reason: String,
}

impl NoMatchDirective {
    /// Create a new NoMatch directive
    pub fn new(to: String, hash: String, size: u64, reason: String) -> Self {
        Self {
            to,
            hash,
            size,
            reason,
        }
    }

    /// Execute the directive - this should normally indicate an error
    pub async fn execute(
        &self,
        _install_dir: &PathBuf,
        _progress_callback: Option<Box<dyn Fn(u64, u64)>>,
    ) -> Result<(), InstallError> {
        // TODO: Handle no-match files
        // This directive type should normally not appear in final modlists
        // since no-match files represent compilation errors.
        //
        // If it does appear, it likely indicates:
        // 1. The modlist is corrupted or incomplete
        // 2. A compilation error that wasn't properly handled
        // 3. Missing source files or archives
        //
        // Appropriate response:
        // 1. Log the error with self.reason
        // 2. Return an InstallError to indicate the problem
        // 3. Do NOT create the file since we don't know how

        eprintln!("Error: No match found for file '{}': {}", self.to, self.reason);

        Err(InstallError::NoMatch {
            file_path: self.to.clone(),
            reason: self.reason.clone(),
        })
    }
}
