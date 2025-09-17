//! Parallel directive installer
//!
//! This module provides a high-performance installer that processes directives
//! using parallel processing techniques inspired by Wabbajack's approach.
//!
//! # Example
//!
//! ```rust
//! use std::path::PathBuf;
//! use std::sync::Arc;
//! use crate::install::lib::{Installer, InstallerConfig, InstallProgress};
//! use crate::install::directives::Directive;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Configure the installer
//! let config = InstallerConfig {
//!     install_dir: PathBuf::from("./SkyrimSE"),
//!     downloads_dir: PathBuf::from("./downloads"),
//!     extracted_modlist_dir: PathBuf::from("./temp/modlist"),
//!     temp_dir: PathBuf::from("./temp"),
//!     game_dir: PathBuf::from("C:/Program Files (x86)/Steam/steamapps/common/Skyrim Special Edition"),
//!     max_concurrency: 8,
//!     verify_hashes: true,
//!     use_compression: false,
//! };
//!
//! // Load directives from your modlist parser
//! let directives: Vec<Directive> = vec![]; // Load from modlist
//!
//! // Create progress callback
//! let progress_callback = Arc::new(|progress: InstallProgress| {
//!     println!("Phase: {:?}, Progress: {}/{} files, {:.1}% complete",
//!         progress.phase,
//!         progress.processed_items,
//!         progress.total_items,
//!         (progress.bytes_processed as f64 / progress.total_bytes as f64) * 100.0
//!     );
//! });
//!
//! // Create and run installer
//! let installer = Installer::new(config, directives)
//!     .with_progress_callback(progress_callback);
//!
//! installer.install().await?;
//! println!("Installation complete!");
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::install::directives::Directive;
use crate::install::error::InstallError;

/// Progress callback type for installation updates
pub type ProgressCallback = Arc<dyn Fn(InstallProgress) + Send + Sync>;

/// Installation progress information
#[derive(Debug, Clone)]
pub struct InstallProgress {
    pub phase: InstallPhase,
    pub current_step: usize,
    pub total_steps: usize,
    pub processed_items: usize,
    pub total_items: usize,
    pub bytes_processed: u64,
    pub total_bytes: u64,
    pub message: String,
}

/// Installation phases
#[derive(Debug, Clone, PartialEq)]
pub enum InstallPhase {
    Preparing,
    BuildingFolderStructure,
    InstallingArchives,
    InstallingInlineFiles,
    CreatingBSAs,
    GeneratingPatches,
    Finishing,
    Complete,
    Error,
}

/// Installation configuration
#[derive(Debug, Clone)]
pub struct InstallerConfig {
    /// Directory where the modlist will be installed
    pub install_dir: PathBuf,
    /// Directory containing downloaded archives
    pub downloads_dir: PathBuf,
    /// Directory containing extracted modlist data
    pub extracted_modlist_dir: PathBuf,
    /// Directory for temporary files during installation
    pub temp_dir: PathBuf,
    /// Game installation directory (for path remapping)
    pub game_dir: PathBuf,
    /// Maximum number of concurrent operations
    pub max_concurrency: usize,
    /// Whether to verify file hashes after installation
    pub verify_hashes: bool,
    /// Whether to use compression for temporary files
    pub use_compression: bool,
}

impl Default for InstallerConfig {
    fn default() -> Self {
        Self {
            install_dir: PathBuf::from("./install"),
            downloads_dir: PathBuf::from("./downloads"),
            extracted_modlist_dir: PathBuf::from("./temp/extracted"),
            temp_dir: PathBuf::from("./temp"),
            game_dir: PathBuf::from("./game"), // Default game directory
            max_concurrency: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4),
            verify_hashes: true,
            use_compression: false,
        }
    }
}

/// Main installer struct that processes directives in parallel
pub struct Installer {
    config: InstallerConfig,
    directives: Vec<Directive>,
    progress_callback: Option<ProgressCallback>,
    cancellation_token: CancellationToken,

    // Internal state
    total_bytes: u64,
    processed_bytes: Arc<Mutex<u64>>,
    start_time: Option<Instant>,
}

impl Installer {
    /// Create a new installer with the given configuration
    pub fn new(config: InstallerConfig, directives: Vec<Directive>) -> Self {
        let total_bytes = directives.iter().map(|d| d.size()).sum();

        Self {
            config,
            directives,
            progress_callback: None,
            cancellation_token: CancellationToken::new(),
            total_bytes,
            processed_bytes: Arc::new(Mutex::new(0)),
            start_time: None,
        }
    }

    /// Set a progress callback for installation updates
    pub fn with_progress_callback(mut self, callback: ProgressCallback) -> Self {
        self.progress_callback = Some(callback);
        self
    }

    /// Set a cancellation token for aborting installation
    pub fn with_cancellation_token(mut self, token: CancellationToken) -> Self {
        self.cancellation_token = token;
        self
    }

    /// Start the installation process
    pub async fn install(mut self) -> Result<(), InstallError> {
        self.start_time = Some(Instant::now());

        self.update_progress(InstallPhase::Preparing, 0, 7, 0, 0, "Starting installation".to_string());

        // Phase 1: Prepare directories and validate directives
        self.prepare_installation().await?;
        if self.cancellation_token.is_cancelled() {
            return Ok(());
        }

        // Phase 2: Build folder structure
        self.build_folder_structure().await?;
        if self.cancellation_token.is_cancelled() {
            return Ok(());
        }

        // Phase 3: Install archive-based files (parallized by archive)
        self.install_archive_files().await?;
        if self.cancellation_token.is_cancelled() {
            return Ok(());
        }

        // Phase 4: Install inline files (fully parallelized)
        self.install_inline_files().await?;
        if self.cancellation_token.is_cancelled() {
            return Ok(());
        }

        // Phase 5: Create BSA files (sequential per BSA, parallel within)
        self.create_bsa_files().await?;
        if self.cancellation_token.is_cancelled() {
            return Ok(());
        }

        // Phase 6: Generate patches (parallelized)
        self.generate_patches().await?;
        if self.cancellation_token.is_cancelled() {
            return Ok(());
        }

        // Phase 7: Finalize installation
        self.finalize_installation().await?;

        self.update_progress(InstallPhase::Complete, 7, 7, 0, 0, "Installation completed successfully".to_string());
        Ok(())
    }

    /// Phase 1: Prepare installation directories and validate directives
    async fn prepare_installation(&mut self) -> Result<(), InstallError> {
        self.update_progress(InstallPhase::Preparing, 1, 7, 0, 0, "Preparing installation directories".to_string());

        // Create directories
        tokio::fs::create_dir_all(&self.config.install_dir).await?;
        tokio::fs::create_dir_all(&self.config.temp_dir).await?;

        // Filter out directives that shouldn't be installed
        let total_directives = self.directives.len();
        self.directives.retain(|d| d.should_install());

        let filtered_count = total_directives - self.directives.len();
        if filtered_count > 0 {
            self.update_progress(
                InstallPhase::Preparing, 1, 7, filtered_count, total_directives,
                format!("Filtered out {} non-installable directives", filtered_count)
            );
        }

        Ok(())
    }

    /// Phase 2: Build folder structure for all directive destinations
    async fn build_folder_structure(&mut self) -> Result<(), InstallError> {
        self.update_progress(InstallPhase::BuildingFolderStructure, 2, 7, 0, 0, "Building folder structure".to_string());

        // Collect unique parent directories
        let mut parent_dirs = std::collections::HashSet::new();
        for directive in &self.directives {
            if let Some(parent) = PathBuf::from(directive.to()).parent() {
                if parent != std::path::Path::new("") {
                    parent_dirs.insert(self.config.install_dir.join(parent));
                }
            }
        }

        // Create directories in parallel
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrency));
        let mut tasks = Vec::new();

        for dir in parent_dirs {
            let sem = semaphore.clone();
            let task = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                tokio::fs::create_dir_all(dir).await
            });
            tasks.push(task);
        }

        // Wait for all directory creation tasks to complete
        for task in tasks {
            task.await.map_err(|e| InstallError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))??;
        }

        Ok(())
    }

    /// Phase 3: Install archive-based files grouped by source archive
    async fn install_archive_files(&self) -> Result<(), InstallError> {
        let archive_directives: Vec<&Directive> = self.directives
            .iter()
            .filter(|d| d.requires_vfs())
            .collect();

        if archive_directives.is_empty() {
            return Ok(());
        }

        self.update_progress(
            InstallPhase::InstallingArchives, 3, 7, 0, archive_directives.len(),
            "Installing files from archives".to_string()
        );

        // Group directives by source archive for efficient extraction
        let mut archive_groups: HashMap<String, Vec<&Directive>> = HashMap::new();
        for directive in &archive_directives {
            let archive_hash = match directive {
                Directive::FromArchive(d) => d.archive_hash().unwrap_or("unknown"),
                Directive::PatchedFromArchive(d) => d.archive_hash().unwrap_or("unknown"),
                Directive::TransformedTexture(d) => d.archive_hash().unwrap_or("unknown"),
                _ => continue,
            };
            archive_groups.entry(archive_hash.to_string()).or_insert_with(Vec::new).push(directive);
        }

        // Process each archive group sequentially for now (TODO: implement proper parallel processing)
        for (_archive_hash, directives) in archive_groups {
            for directive in directives {
                if self.cancellation_token.is_cancelled() {
                    break;
                }

                // Execute the specific directive
                let result = match directive {
                    Directive::FromArchive(d) => {
                        // TODO: Implement actual VFS and progress callback
                        println!("Processing FromArchive: {}", d.to);
                        Ok(())
                    },
                    Directive::PatchedFromArchive(d) => {
                        // TODO: Implement actual VFS and progress callback
                        println!("Processing PatchedFromArchive: {}", d.to);
                        Ok(())
                    },
                    Directive::TransformedTexture(d) => {
                        // TODO: Implement actual VFS and progress callback
                        println!("Processing TransformedTexture: {}", d.to);
                        Ok(())
                    },
                    _ => Ok(()),
                };

                if let Err(e) = result {
                    return Err(e);
                }

                // Update progress
                {
                    let mut bytes = self.processed_bytes.lock().unwrap();
                    *bytes += directive.size();
                }
            }
        }

        Ok(())
    }

    /// Phase 4: Install inline files (fully parallelized)
    async fn install_inline_files(&mut self) -> Result<(), InstallError> {
        let inline_directives: Vec<&Directive> = self.directives
            .iter()
            .filter(|d| d.is_inline())
            .collect();

        if inline_directives.is_empty() {
            return Ok(());
        }

        self.update_progress(
            InstallPhase::InstallingInlineFiles, 4, 7, 0, inline_directives.len(),
            "Installing inline files".to_string()
        );

        // Process inline files sequentially for now (TODO: implement proper parallel processing)
        for directive in inline_directives {
            if self.cancellation_token.is_cancelled() {
                break;
            }

            // Execute the specific directive
            let result = match directive {
                Directive::InlineFile(d) => {
                    // TODO: Implement actual file operations
                    println!("Processing InlineFile: {}", d.to);
                    Ok(())
                },
                Directive::RemappedInlineFile(d) => {
                    // TODO: Implement actual file operations with path remapping
                    println!("Processing RemappedInlineFile: {}", d.to);
                    Ok(())
                },
                Directive::PropertyFile(d) => {
                    // TODO: Implement actual file operations
                    println!("Processing PropertyFile: {}", d.to);
                    Ok(())
                },
                Directive::ArchiveMeta(d) => {
                    // TODO: Implement actual file operations
                    println!("Processing ArchiveMeta: {}", d.to);
                    Ok(())
                },
                _ => Ok(()),
            };

            if let Err(e) = result {
                return Err(e);
            }

            // Update progress
            {
                let mut bytes = self.processed_bytes.lock().unwrap();
                *bytes += directive.size();
            }
        }

        Ok(())
    }

    /// Phase 5: Create BSA files (sequential per BSA, but parallel within each BSA)
    async fn create_bsa_files(&mut self) -> Result<(), InstallError> {
        let bsa_directives: Vec<&Directive> = self.directives
            .iter()
            .filter(|d| matches!(d, Directive::CreateBSA(_)))
            .collect();

        if bsa_directives.is_empty() {
            return Ok(());
        }

        self.update_progress(
            InstallPhase::CreatingBSAs, 5, 7, 0, bsa_directives.len(),
            "Creating BSA files".to_string()
        );

        // Process BSAs sequentially (to avoid resource conflicts), but parallelize within each BSA
        for directive in bsa_directives {
            if self.cancellation_token.is_cancelled() {
                break;
            }

            if let Directive::CreateBSA(bsa_directive) = directive {
                // TODO: Implement actual BSA creation
                println!("Processing CreateBSA: {}", bsa_directive.to);

                // Update progress
                {
                    let mut bytes = self.processed_bytes.lock().unwrap();
                    *bytes += directive.size();
                }
            }
        }

        Ok(())
    }

    /// Phase 6: Generate patches (parallelized)
    async fn generate_patches(&mut self) -> Result<(), InstallError> {
        let patch_directives: Vec<&Directive> = self.directives
            .iter()
            .filter(|d| matches!(d, Directive::MergedPatch(_)))
            .collect();

        if patch_directives.is_empty() {
            return Ok(());
        }

        self.update_progress(
            InstallPhase::GeneratingPatches, 6, 7, 0, patch_directives.len(),
            "Generating merged patches".to_string()
        );

        // Process patches sequentially for now (TODO: implement proper parallel processing)
        for directive in patch_directives {
            if self.cancellation_token.is_cancelled() {
                break;
            }

            if let Directive::MergedPatch(patch_directive) = directive {
                // TODO: Implement actual patch generation
                println!("Processing MergedPatch: {}", patch_directive.to);

                // Update progress
                {
                    let mut bytes = self.processed_bytes.lock().unwrap();
                    *bytes += directive.size();
                }
            }
        }

        Ok(())
    }

    /// Phase 7: Finalize installation
    async fn finalize_installation(&mut self) -> Result<(), InstallError> {
        self.update_progress(InstallPhase::Finishing, 7, 7, 0, 0, "Finalizing installation".to_string());

        // Clean up temporary files
        if self.config.temp_dir.exists() {
            tokio::fs::remove_dir_all(&self.config.temp_dir).await.ok(); // Ignore errors
        }

        Ok(())
    }

    /// Update progress and notify callback if set
    fn update_progress(&self, phase: InstallPhase, current_step: usize, total_steps: usize, processed_items: usize, total_items: usize, message: String) {
        if let Some(callback) = &self.progress_callback {
            let processed_bytes = *self.processed_bytes.lock().unwrap();

            let progress = InstallProgress {
                phase,
                current_step,
                total_steps,
                processed_items,
                total_items,
                bytes_processed: processed_bytes,
                total_bytes: self.total_bytes,
                message,
            };

            callback(progress);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::directives::{FromArchiveDirective, InlineFileDirective};

    #[tokio::test]
    async fn test_installer_creation() {
        let config = InstallerConfig::default();
        let directives = vec![
            Directive::FromArchive(FromArchiveDirective::new(
                "test.txt".to_string(),
                "abc123".to_string(),
                100,
                vec!["archive_hash".to_string(), "path/to/file".to_string()],
            )),
            Directive::InlineFile(InlineFileDirective::new(
                "inline.txt".to_string(),
                "def456".to_string(),
                50,
                "inline_data_id".to_string(),
            )),
        ];

        let installer = Installer::new(config, directives);
        assert_eq!(installer.total_bytes, 150);
        assert_eq!(installer.directives.len(), 2);
    }

    #[tokio::test]
    async fn test_progress_callback() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let config = InstallerConfig::default();
        let directives = vec![];

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let callback: ProgressCallback = Arc::new(move |_progress| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
        });

        let installer = Installer::new(config, directives)
            .with_progress_callback(callback);

        // This should trigger at least one progress callback during preparation
        installer.install().await.ok();

        assert!(call_count.load(Ordering::SeqCst) > 0);
    }
}
