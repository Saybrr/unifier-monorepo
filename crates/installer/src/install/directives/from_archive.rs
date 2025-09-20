//! FromArchive directive implementation
//!
//! Handles extracting files directly from downloaded archives.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use crate::install::error::InstallError;
use crate::install::vfs::{VfsContext};
use crate::install::vfs::vfs::{Hash, HashRelativePath, VirtualFileNode};
use crate::install::directives::common_directive_utils::{verify_file_hash, delete_if_exists, ensure_parent_dir, compute_file_hash};
use base64;
use std::fs::File;
use std::io;
use sevenz_rust2::{ArchiveReader, Password};
use std::rc::Rc;


use tracing::info;

// Extract a file directly from a downloaded archive
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

    /// Convert base64 hash string to Hash struct (matches Wabbajack's base64 xxhash64 format)
    fn parse_hash_from_string(hash_str: &str) -> Result<Hash, InstallError> {
        let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, hash_str)
            .map_err(|e| InstallError::InvalidDirective(format!("Invalid base64 hash: {}", e)))?;

        if decoded.len() != 8 {
            return Err(InstallError::InvalidDirective(
                format!("Hash must be 8 bytes, got {}", decoded.len())
            ));
        }

        let mut hash_bytes = [0u8; 8];
        hash_bytes.copy_from_slice(&decoded);
        Ok(Hash(hash_bytes))
    }

    /// Execute the directive - extract file from archive to destination
    /// Based on C#'s FromArchive case in AInstaller.cs
    pub async fn execute(
        &self,
        install_dir: &Arc<PathBuf>,
        _extracted_modlist_dir: &Arc<PathBuf>,
        vfs_context: Arc<VfsContext>,
        progress_callback: Option<Box<dyn Fn(u64, u64) + Send + Sync>>,
    ) -> Result<(), InstallError> {
        let destination = install_dir.join(&self.to);

        // Delete existing file if it exists (matches C#'s outPath.Delete())
        delete_if_exists(&destination).await?;

        // Ensure parent directory exists
        ensure_parent_dir(&destination).await?;

        // Parse archive hash from string format
        let archive_hash_str = self.archive_hash()
            .ok_or_else(|| InstallError::InvalidDirective("Missing archive hash".to_string()))?;
        let archive_hash = Self::parse_hash_from_string(archive_hash_str)?;

        // Build HashRelativePath for VFS lookup
        let archive_path_parts: Vec<PathBuf> = self.archive_path()
            .into_iter()
            .map(PathBuf::from)
            .collect();
        let hash_rel_path = HashRelativePath::new(archive_hash, archive_path_parts);

        // Find the file in the VFS (equivalent to C#'s vf.VirtualFile)
        let vf_node = vfs_context.find_file(&hash_rel_path)
            .ok_or_else(|| InstallError::ArchiveNotFound {
                hash: archive_hash_str.to_string()
            })?;

        // Extract file data from archive
        // This simulates the C# logic: if (grouped[vf].Count() == 1) { sf.MoveHashedAsync } else { sf.GetStream() }
        self.extract_file_from_archive(&vf_node, &vfs_context, &destination).await?;

        let computed_hash = compute_file_hash(&destination).await?;
        // Verify hash matches expected (equivalent to ThrowOnNonMatchingHash in C#)
        verify_file_hash(&self.to, &self.hash, &computed_hash)?;

        // Update progress via callback if provided
        if let Some(callback) = progress_callback {
            callback(self.size, self.size);
        }

        Ok(())
    }

    /// Extract file data from archive using VFS node
    /// This is a placeholder for actual archive extraction logic
    async fn extract_file_from_archive(
        &self,
        vf_node: &Arc<std::sync::RwLock<VirtualFileNode>>,
        vfs_context: &Arc<VfsContext>,
        destination: &PathBuf,
    ) -> Result<(), InstallError> {
        let node = vf_node.read().unwrap();

        // Get the source archive information
        let source_archive = node.source_archive.as_ref()
            .ok_or_else(|| InstallError::Vfs("File not from archive".to_string()))?;

        // Get the archive file path on disk
        let archive_path = vfs_context.archive_locations.get(&source_archive.archive_hash)
            .ok_or_else(|| InstallError::ArchiveNotFound {
                hash: format!("{:?}", source_archive.archive_hash)
            })?;

        // TODO: Implement actual archive extraction using appropriate library (zip, 7z, etc.)
        // For now, this is a placeholder that would need to be implemented with proper archive libraries
        // The logic would be similar to C#'s sf.GetStream() or sf.MoveHashedAsync()

        // Build the internal path within the archive
        let internal_path = node.archive_path.iter()
            .map(|p| p.to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");

        // This is where you'd use libraries like `zip` or `sevenz-rust` to extract the specific file
        // For now, return an error indicating this needs implementation
        let archive_type = archive_path.extension().unwrap().to_str().unwrap();
        match archive_type {
            "7z" => {
                    match sevenz_rust2::decompress_with_extract_fn_and_password(
                        File::open(archive_path).unwrap(),
                        destination,
                        Password::empty(),
                        |entry, reader, dest| {
                            let r = sevenz_rust2::default_entry_extract_fn(entry, reader, dest);
                            info!("complete extract {}", entry.name());
                            r
                        },
                    )
                    {
                        Ok(()) => Ok(()),
                        Err(e) => Err(InstallError::ArchiveExtraction(e.to_string())),
                    }
            }
            "zip" => {
                let filepath = File::open(archive_path).unwrap();
                let mut archive = zip::ZipArchive::new(filepath).unwrap();
                let mut file_in_archive = archive.by_name(&internal_path).unwrap();
               let mut dest = File::create(destination).unwrap();
               io::copy(&mut file_in_archive, &mut dest).map_err(|e| InstallError::ArchiveExtraction(e.to_string()))?;
                Ok(())
            }
            "rar" => {
                let mut archive =
                unrar::Archive::new(archive_path)
                    .open_for_processing()
                    .map_err(|e| InstallError::ArchiveExtraction(e.to_string()))?;

                loop {
                    match archive.read_header() {
                        Ok(Some(header)) => {
                            tracing::info!(
                                "{} bytes: {}",
                                header.entry().unpacked_size,
                                header.entry().filename.to_string_lossy(),
                            );
                            if header.entry().filename.to_string_lossy() == internal_path && header.entry().is_file() {
                                header.extract_to(destination)
                                    .map_err(|e| InstallError::ArchiveExtraction(e.to_string()))?;
                                break;
                            } else {
                                archive = header.skip()
                                    .map_err(|e| InstallError::ArchiveExtraction(e.to_string()))?;
                            }
                        }
                        Ok(None) => break,
                        Err(e) => return Err(InstallError::ArchiveExtraction(e.to_string())),
                    }
                }
                Ok(())
            }
            _ => Err(InstallError::Vfs(format!("Unsupported archive type: {}", archive_type))),
        }
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

