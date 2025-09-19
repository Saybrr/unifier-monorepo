//! Common utilities for directive execution
//!
//! Shared functionality used across different directive types including
//! hash computation, file operations, and verification.

use crate::install::error::InstallError;
use std::path::Path;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use xxhash_rust::xxh64::Xxh64;
use base64;

/// Convert xxHash64 u64 to base64 format (matching Wabbajack format)
pub fn xxhash64_to_base64(hash: u64) -> String {
    // Convert u64 to bytes in little-endian format (matching Wabbajack)
    let bytes = hash.to_le_bytes();
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes)
}

/// Write data to file and return its hash (equivalent to C#'s WriteAllHashedAsync)
pub async fn write_file_with_hash<P: AsRef<Path>>(
    file_path: P,
    data: &[u8],
) -> Result<String, InstallError> {
    let path = file_path.as_ref();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }

    // Write file and compute hash simultaneously
    let mut file = fs::File::create(path).await?;
    let mut hasher = Xxh64::new(0);

    file.write_all(data).await?;
    file.flush().await?;

    hasher.update(data);
    let hash = hasher.digest();

    Ok(xxhash64_to_base64(hash))
}

/// Compute hash of existing file
pub async fn compute_file_hash<P: AsRef<Path>>(file_path: P) -> Result<String, InstallError> {
    let path = file_path.as_ref();
    let mut file = fs::File::open(path).await?;
    let mut hasher = Xxh64::new(0);

    const BUFFER_SIZE: usize = 64 * 1024; // 64KB buffer
    let mut buffer = vec![0u8; BUFFER_SIZE];

    loop {
        let bytes_read = file.read(&mut buffer).await?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let hash = hasher.digest();
    Ok(xxhash64_to_base64(hash))
}

/// Verify file hash matches expected hash (equivalent to C#'s ThrowOnNonMatchingHash)
pub fn verify_file_hash(file_path: &str, expected_hash: &str, actual_hash: &str) -> Result<(), InstallError> {
    if expected_hash != actual_hash {
        return Err(InstallError::HashMismatch {
            file_path: file_path.to_string(),
            expected: expected_hash.to_string(),
            actual: actual_hash.to_string(),
        });
    }
    Ok(())
}

/// Load raw bytes from a source data file
pub async fn load_source_data<P: AsRef<Path>>(source_path: P) -> Result<Vec<u8>, InstallError> {
    let path = source_path.as_ref();
    if !path.exists() {
        return Err(InstallError::FileNotFound(path.to_path_buf()));
    }

    let data = fs::read(path).await?;
    Ok(data)
}

/// Load text data from a source data file (for remapped files)
pub async fn load_source_text<P: AsRef<Path>>(source_path: P) -> Result<String, InstallError> {
    let data = load_source_data(source_path).await?;
    String::from_utf8(data).map_err(|e| InstallError::InvalidDirective(format!("Invalid UTF-8 data: {}", e)))
}

/// Apply path magic replacements to text content (for RemappedInlineFile)
pub fn apply_path_replacements(content: &str, replacements: &[(String, String)]) -> String {
    let mut result = content.to_string();

    for (pattern, replacement) in replacements {
        result = result.replace(pattern, replacement);
    }

    result
}

/// Delete file if it exists (equivalent to C#'s outPath.Delete())
pub async fn delete_if_exists<P: AsRef<Path>>(file_path: P) -> Result<(), InstallError> {
    let path = file_path.as_ref();
    if path.exists() {
        fs::remove_file(path).await?;
    }
    Ok(())
}

/// Ensure parent directory exists for a file path
pub async fn ensure_parent_dir<P: AsRef<Path>>(file_path: P) -> Result<(), InstallError> {
    let path = file_path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    Ok(())
}
