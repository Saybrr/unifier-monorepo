//! Installation error types

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during installation
#[derive(Debug, Error)]
pub enum InstallError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Hash mismatch for file {file_path}: expected {expected}, got {actual}")]
    HashMismatch {
        file_path: String,
        expected: String,
        actual: String,
    },

    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    #[error("Archive not found: {hash}")]
    ArchiveNotFound { hash: String },

    #[error("VFS error: {0}")]
    Vfs(String),

    #[error("Patch application failed: {0}")]
    PatchFailed(String),

    #[error("Texture transformation failed: {0}")]
    TextureTransform(String),

    #[error("BSA creation failed: {0}")]
    BsaCreation(String),

    #[error("No match found for file {file_path}: {reason}")]
    NoMatch { file_path: String, reason: String },

    #[error("Invalid directive data: {0}")]
    InvalidDirective(String),
}
