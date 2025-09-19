//! ArchiveMeta directive implementation
//!
//! Handles creating .meta files for Mod Organizer 2.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use crate::install::error::InstallError;
use super::common_directive_utils::{
    load_source_text, write_file_with_hash, verify_file_hash, delete_if_exists, ensure_parent_dir
};

/// Create .meta files for Mod Organizer 2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveMetaDirective {
    /// Destination path relative to install directory
    #[serde(rename = "To")]
    pub to: String,
    /// Content hash of the target file
    #[serde(rename = "Hash")]
    pub hash: String,
    /// Size in bytes of the target file
    #[serde(rename = "Size")]
    pub size: u64,
    /// Reference to metadata content in the modlist
    #[serde(rename = "SourceDataID")]
    pub source_data_id: String,
}

impl ArchiveMetaDirective {
    /// Create a new ArchiveMeta directive
    pub fn new(to: String, hash: String, size: u64, source_data_id: String) -> Self {
        Self {
            to,
            hash,
            size,
            source_data_id,
        }
    }

    /// Execute the directive - create .meta file for MO2
    pub async fn execute(
        &self,
        install_dir: &Arc<PathBuf>,
        extracted_modlist_dir: &Arc<PathBuf>,
        progress_callback: Option<Box<dyn Fn(u64, u64) + Send + Sync>>,
    ) -> Result<String, InstallError> {
        let destination = install_dir.join(&self.to);
        let source_data_path = extracted_modlist_dir.join(&self.source_data_id);

        // Delete existing file if it exists (matches C#'s outPath.Delete())
        delete_if_exists(&destination).await?;

        // Ensure parent directory exists
        ensure_parent_dir(&destination).await?;

        // Load metadata content from extracted modlist (equivalent to MetaIni content in C#)
        let meta_content = load_source_text(&source_data_path).await?;

        // Format as MO2 .meta file (INI format with [General] section)
        // This matches the C# AddInstalled function which adds "installed=true" and "[General]" section
        let formatted_content = format_meta_ini(&meta_content);

        // Write meta file and compute hash (equivalent to WriteAllLinesAsync in C#)
        let computed_hash = write_file_with_hash(&destination, formatted_content.as_bytes()).await?;

        // Verify hash matches expected (equivalent to hash verification in C#)
        verify_file_hash(&self.to, &self.hash, &computed_hash)?;

        // Update progress via callback if provided
        if let Some(callback) = progress_callback {
            callback(self.size, self.size);
        }

        // Return the computed hash for caching (matches C#'s FileHashWriteCache pattern)
        Ok(computed_hash)
    }
}

/// Format meta content as MO2 INI file (equivalent to C#'s AddInstalled function)
///
/// This matches the C# implementation that adds "[General]" and "installed=true"
/// to the beginning of the meta content from MetaIni.
fn format_meta_ini(meta_content: &str) -> String {
    let mut result = String::new();

    // Add [General] section header (matches C# AddInstalled implementation)
    result.push_str("[General]\n");
    result.push_str("installed=true\n");

    // Add the rest of the meta content (from MetaIni)
    // Skip any existing [General] header in the source content to avoid duplication
    for line in meta_content.lines() {
        let trimmed = line.trim();
        if trimmed != "[General]" && !trimmed.is_empty() {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_format_meta_ini() {
        // Test the INI formatting function
        let meta_content = "gameName=Skyrim Special Edition\nmodID=12345\nfileID=67890";
        let formatted = format_meta_ini(meta_content);

        assert!(formatted.contains("[General]"));
        assert!(formatted.contains("installed=true"));
        assert!(formatted.contains("gameName=Skyrim Special Edition"));
        assert!(formatted.contains("modID=12345"));
        assert!(formatted.contains("fileID=67890"));
    }

    #[tokio::test]
    async fn test_format_meta_ini_with_existing_general_section() {
        // Test handling of existing [General] section
        let meta_content = "[General]\ngameName=Skyrim\nmodID=123";
        let formatted = format_meta_ini(meta_content);

        // Should only have one [General] section
        let general_count = formatted.matches("[General]").count();
        assert_eq!(general_count, 1);
        assert!(formatted.contains("installed=true"));
        assert!(formatted.contains("gameName=Skyrim"));
    }

    #[tokio::test]
    async fn test_archive_meta_execute() -> Result<(), Box<dyn std::error::Error>> {
        // Create temporary directories
        let temp_dir = tempdir()?;
        let install_dir = Arc::new(temp_dir.path().join("install"));
        let extracted_dir = Arc::new(temp_dir.path().join("extracted"));

        fs::create_dir_all(&**install_dir).await?;
        fs::create_dir_all(&**extracted_dir).await?;

        // Create test meta content
        let source_data_id = "test-meta-id";
        let meta_content = "gameName=TestGame\nmodID=999\nfileID=111";
        let source_path = extracted_dir.join(source_data_id);
        fs::write(&source_path, meta_content).await?;

        // Create ArchiveMeta directive
        let directive = ArchiveMetaDirective::new(
            "mods/TestMod/meta.ini".to_string(),
            "test_hash".to_string(), // We'll skip hash verification for this test
            42,
            source_data_id.to_string(),
        );

        // Execute the directive (this will fail on hash verification, but that's expected)
        let result = directive.execute(&install_dir, &extracted_dir, None).await;

        // The execution should fail due to hash mismatch, but the file should be created
        assert!(result.is_err());

        // Check that the meta file was created with correct content
        let meta_file_path = install_dir.join("mods/TestMod/meta.ini");
        assert!(meta_file_path.exists());

        let written_content = fs::read_to_string(&meta_file_path).await?;
        assert!(written_content.contains("[General]"));
        assert!(written_content.contains("installed=true"));
        assert!(written_content.contains("gameName=TestGame"));

        Ok(())
    }
}
