//! File operation utilities
//!
//! Centralized file handling utilities to eliminate duplication and ensure
//! consistent behavior across download sources.

use std::path::Path;
use tokio::fs;
use tracing::debug;
use crate::downloader::core::{
    DownloadResult, ProgressCallback, ProgressEvent, FileValidation, Result
};

/// Check if a file exists and validate it if needed
///
/// This function encapsulates the common pattern of checking for existing files
/// and validating them before deciding whether to re-download.
pub async fn check_existing_file(
    dest_path: &Path,
    validation: &FileValidation,
    progress_callback: Option<ProgressCallback>,
) -> Result<Option<DownloadResult>> {
    if !dest_path.exists() {
        return Ok(None);
    }

    let size = fs::metadata(dest_path).await?.len();
    debug!("File already exists: {} ({} bytes)", dest_path.display(), size);

    if validation.is_empty() {
        // No validation needed, file exists
        debug!("File exists and no validation required");
        return Ok(Some(DownloadResult::AlreadyExists { size }));
    }

    // Validate existing file
    match validation.validate_file(dest_path, progress_callback.clone()).await {
        Ok(true) => {
            debug!("File exists and is valid");
            Ok(Some(DownloadResult::AlreadyExists { size }))
        }
        Ok(false) => {
            // This shouldn't happen as validate_file returns Err for failures
            debug!("File exists but validation returned false (unexpected)");
            report_invalid_file_warning(dest_path, progress_callback).await;
            fs::remove_file(dest_path).await?;
            Ok(None)
        }
        Err(e) => {
            debug!("Existing file failed validation: {}", e);
            report_invalid_file_warning(dest_path, progress_callback).await;
            fs::remove_file(dest_path).await?;
            Ok(None)
        }
    }
}



/// Report a warning about an invalid existing file
async fn report_invalid_file_warning(
    dest_path: &Path,
    progress_callback: Option<ProgressCallback>,
) {
    if let Some(ref callback) = progress_callback {
        callback(ProgressEvent::Warning {
            url: format!("file://{}", dest_path.display()),
            message: format!("Existing file is invalid, removing: {}", dest_path.display()),
        });
    }
}

/// Create a temporary file path for partial downloads
///
/// Returns a path with .part extension for resume functionality.
pub fn create_temp_path(dest_path: &Path) -> std::path::PathBuf {
    dest_path.with_extension("part")
}

/// Atomically rename a temporary file to its final destination
///
/// This is used to ensure downloads are atomic - the file either exists
/// completely or not at all.
pub async fn atomic_rename(temp_path: &Path, dest_path: &Path) -> Result<()> {
    fs::rename(temp_path, dest_path).await?;
    debug!("Atomically renamed {} to {}", temp_path.display(), dest_path.display());
    Ok(())
}
