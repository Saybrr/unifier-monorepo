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
//! // Configure the installer (paths are wrapped in Arc for memory efficiency)
//! let config = InstallerConfig {
//!     install_dir: Arc::new(PathBuf::from("./SkyrimSE")),
//!     downloads_dir: Arc::new(PathBuf::from("./downloads")),
//!     extracted_modlist_dir: Arc::new(PathBuf::from("./temp/modlist")),
//!     temp_dir: Arc::new(PathBuf::from("./temp")),
//!     game_dir: Arc::new(PathBuf::from("C:/Program Files (x86)/Steam/steamapps/common/Skyrim Special Edition")),
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

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use std::collections::HashMap;

use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::install::error::InstallError;
use crate::install::directives::Directive;
use crate::install::vfs::VfsContext;

use futures::stream::{self, StreamExt, TryStreamExt};

/// Simple in-memory hash cache (equivalent to C#'s FileHashCache)
#[derive(Clone)]
pub struct FileHashCache {
    cache: Arc<Mutex<HashMap<PathBuf, String>>>,
}

impl FileHashCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Write hash to cache (equivalent to C#'s FileHashWriteCache)
    pub fn write_cache(&self, file_path: &PathBuf, hash: String) {
        let path = file_path.clone();
        let mut cache = self.cache.lock().unwrap();
        cache.insert(path, hash);
    }

    /// Get hash from cache if available
    pub fn get_cached(&self, file_path: &PathBuf) -> Option<String> {
        let cache = self.cache.lock().unwrap();
        cache.get(file_path).cloned()
    }
}

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
    WritingMetaFiles,
    CreatingBSAs,
    GeneratingPatches,
    Finishing,
    Complete,
    Error,
}

/// Installation configuration
///
/// **Memory Optimization Note**: All paths are wrapped in `Arc<PathBuf>` to minimize
/// memory usage during parallel processing. This reduces path cloning overhead from
/// ~70-100 MB to ~5-10 MB for large modlists by sharing path data across threads.
#[derive(Debug, Clone)]
pub struct InstallerConfig {
    /// Directory where the modlist will be installed
    pub install_dir: Arc<PathBuf>,
    /// Directory containing downloaded archives
    pub downloads_dir: Arc<PathBuf>,
    /// Directory containing extracted modlist data
    pub extracted_modlist_dir: Arc<PathBuf>,
    /// Directory for temporary files during installation
    pub temp_dir: Arc<PathBuf>,
    /// Game installation directory (for path remapping)
    pub game_dir: Arc<PathBuf>,
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
            install_dir: Arc::new(PathBuf::from("./install")),  //FINAL INSTALL DIR
            downloads_dir: Arc::new(PathBuf::from("./downloads")), //DOWNLOADS DIR - default is install dir/downloads
            extracted_modlist_dir: Arc::new(PathBuf::from("./temp/extracted")),
            temp_dir: Arc::new(PathBuf::from("./temp")),
            game_dir: Arc::new(PathBuf::from("./game")), // directory of the game we are modding, likey from steamapps
            max_concurrency: std::cmp::min(
                std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4),
                8  // Cap at 8 to balance performance vs memory usage
            ),
            verify_hashes: true,
            use_compression: false,
        }
    }
}

/// Main installer struct that processes directives in parallel
///
/// **Memory Optimization**: Directives are stored as `Arc<Directive>` to minimize
/// memory usage when cloning for parallel processing.
pub struct Installer {
    config: InstallerConfig,
    directives: Vec<Arc<Directive>>,
    vfs: VfsContext,
    progress_callback: ProgressCallback,
    cancellation_token: CancellationToken,
    hash_cache: FileHashCache,

    // Internal state
    total_bytes: u64,
    processed_bytes: Arc<Mutex<u64>>,
    start_time: Option<Instant>,
}

impl Installer {
    /// Create a new installer with the given configuration
    pub fn new(config: InstallerConfig, directives: Vec<Arc<Directive>>, vfs: VfsContext) -> Self {
        let total_bytes = directives.iter().map(|d| d.size()).sum();

        Self {
            config,
            directives,
            vfs,
            progress_callback: Arc::new(|_| {}),
            cancellation_token: CancellationToken::new(),
            hash_cache: FileHashCache::new(),
            total_bytes,
            processed_bytes: Arc::new(Mutex::new(0)),
            start_time: None,
        }
    }

    /// Set a progress callback for installation updates
    pub fn with_progress_callback(mut self, callback: ProgressCallback) -> Self {
        self.progress_callback = callback;
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

        //filter for archive directives
        // let _archive_directives = self.directives
        // .iter()
        // .filter(|d| d.requires_vfs())
        // .collect::<Vec<_>>();

        // // Phase 3: Install archive-based files (parallized by archive)
        // self.install_archive_files(archive_directives).await?;
        // if self.cancellation_token.is_cancelled() {
        //     return Ok(());
        // }

        // //filter for inline directives

        // // Phase 4: Install inline files (fully parallelized)
        // self.install_inline_files().await?;
        // if self.cancellation_token.is_cancelled() {
        //     return Ok(());
        // }

        // //filter for bsa directives

        // // Phase 5: Create BSA files (sequential per BSA, parallel within)
        // self.create_bsa_files().await?;
        // if self.cancellation_token.is_cancelled() {
        //     return Ok(());
        // }

        // //filter for patch directives

        // // Phase 6: Generate patches (parallelized)
        // self.generate_patches().await?;
        // if self.cancellation_token.is_cancelled() {
        //     return Ok(());
        // }

        // // Phase 7: Finalize installation
        // self.finalize_installation().await?;

        self.update_progress(InstallPhase::Complete, 7, 7, 0, 0, "Installation completed successfully".to_string());
        Ok(())
    }

    /// Phase 1: Prepare installation directories and validate directives
    async fn prepare_installation(&mut self) -> Result<(), InstallError> {
        self.update_progress(InstallPhase::Preparing, 1, 7, 0, 0, "Preparing installation directories".to_string());
        tokio::fs::create_dir_all(&**self.config.install_dir).await?;
        tokio::fs::create_dir_all(&**self.config.temp_dir).await?;

        // Filter out directives that shouldn't be installed
        let total_directives = self.directives.len();
        self.directives.retain(|d| {
            !matches!(d.as_ref(),
                Directive::IgnoredDirectly(_) |
                Directive::NoMatch(_)
            )
        });

        let filtered_count = total_directives - self.directives.len();

        // for d in self.directives {
        //     //Prime VFS self

        // }


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
                    parent_dirs.insert((**self.config.install_dir).join(parent));
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

    /// Phase 3: Install archive-based files grouped by directive type
    #[allow(dead_code)]
    async fn install_archive_files(&self) -> Result<(), InstallError> {
        if self.directives.is_empty() {
            return Ok(());
        }

        self.update_progress(
            InstallPhase::InstallingArchives, 3, 7, 0, self.directives.len(),
            "Installing files from archives".to_string()
        );

               // Group directives by source archive (like C#'s GroupBy(a => a.VF))
               let mut archive_groups: std::collections::HashMap<String, Vec<Arc<Directive>>> = std::collections::HashMap::new();

               for directive in &self.directives {
                   let archive_key = match directive.as_ref() {
                       // Group archive-based directives by their source archive hash
                       Directive::FromArchive(d) => {
                           d.archive_hash().unwrap_or("unknown_archive").to_string()
                       },
                       Directive::PatchedFromArchive(d) => {
                           d.archive_hash().unwrap_or("unknown_archive").to_string()
                       },
                       Directive::TransformedTexture(d) => {
                           d.archive_hash().unwrap_or("unknown_archive").to_string()
                       },
                       _ => "other_directives".to_string(),
                   };

                   archive_groups.entry(archive_key).or_insert_with(Vec::new).push(Arc::clone(directive));
               }

               archive_groups.retain(|k, _| k != "other_directives");

               // Process archives concurrently (like C#'s _vfs.Extract with concurrent archives)
               stream::iter(archive_groups)
                   .map(Ok)
                   .try_for_each_concurrent(self.config.max_concurrency, |(_archive_key, directives)| {
                       let install_dir = self.config.install_dir.clone();
                       let downloads_dir = self.config.downloads_dir.clone();
                       let extracted_modlist_dir = self.config.extracted_modlist_dir.clone();
                       let processed_bytes = self.processed_bytes.clone();
                       let cancellation_token = self.cancellation_token.clone();
                       let vfs = Arc::new(self.vfs.clone());

                       async move {
                           // Process all directives from this archive sequentially
                           // (matches C#'s foreach loop within each archive - lines 248-301)
                           for directive in directives {
                               if cancellation_token.is_cancelled() {
                                   return Ok(());
                               }

                            let _computed_hash = directive.execute(&install_dir, &extracted_modlist_dir, &downloads_dir, &self.config.game_dir, vfs.clone(), None).await?;
                            *processed_bytes.lock().unwrap() += directive.size();
                           }
                           Ok::<(), InstallError>(())
                       }
                   }).await?;

        Ok(())
    }

    /// Phase 4: Install inline files (matches C# InstallIncludedFiles exactly)
    #[allow(dead_code)]
    async fn install_inline_files(&self) -> Result<(), InstallError> {
        // Filter to only inline file directives (matches C#'s .OfType<InlineFile>())
        let inline_directives: Vec<Arc<Directive>> = self.directives
            .iter()
            .filter(|d| matches!(d.as_ref(),
                Directive::InlineFile(_) |
                Directive::RemappedInlineFile(_)
            ))
            .cloned()
            .collect();

        if inline_directives.is_empty() {
            return Ok(());
        }

        self.update_progress(
            InstallPhase::InstallingInlineFiles, 4, 7, 0, inline_directives.len(),
            "Installing inline files".to_string()
        );

        // Process all inline files in parallel (matches C#'s .PDoAll())
        stream::iter(inline_directives)
            .map(Ok)
            .try_for_each_concurrent(self.config.max_concurrency, |directive| {
                let install_dir = self.config.install_dir.clone();
                let extracted_modlist_dir = self.config.extracted_modlist_dir.clone();
                let downloads_dir = self.config.downloads_dir.clone();
                let game_dir = self.config.game_dir.clone();
                let processed_bytes = self.processed_bytes.clone();
                let cancellation_token = self.cancellation_token.clone();
                let vfs = Arc::new(self.vfs.clone());
                let hash_cache = self.hash_cache.clone();

                async move {
                    if cancellation_token.is_cancelled() {
                        return Ok(());
                    }

                    // Execute directive - hash computation and verification now handled within the directive
                    let computed_hash = directive.execute(
                        &install_dir,
                        &extracted_modlist_dir,
                        &downloads_dir,
                        &game_dir,
                        vfs,
                        None
                    ).await?;

                    // Cache the computed hash if one was returned
                    if let Some(hash) = computed_hash {
                        let out_path = (**install_dir).join(directive.to());
                        hash_cache.write_cache(&out_path, hash);
                    }

                    // Update progress (matches C#'s UpdateProgress(1))
                    *processed_bytes.lock().unwrap() += directive.size();

                    Ok::<(), InstallError>(())
                }
            }).await?;

        Ok(())
    }


    #[allow(dead_code)]
    async fn write_meta_files(&self) -> Result<(), InstallError> {
        // Filter to only inline file directives (matches C#'s .OfType<InlineFile>())
        let meta_directives: Vec<Arc<Directive>> = self.directives
            .iter()
            .filter(|d| matches!(d.as_ref(),
                Directive::ArchiveMeta(_)
            ))
            .cloned()
            .collect();

        if meta_directives.is_empty() {
            return Ok(());
        }

        self.update_progress(
            InstallPhase::WritingMetaFiles, 5, 7, 0, meta_directives.len(),
            "Writing meta files".to_string()
        );

        // Process all meta files in parallel (matches C#'s .PDoAll())
        stream::iter(meta_directives)
            .map(Ok)
            .try_for_each_concurrent(self.config.max_concurrency, |directive| {
                let install_dir = self.config.install_dir.clone();
                let extracted_modlist_dir = self.config.extracted_modlist_dir.clone();
                let downloads_dir = self.config.downloads_dir.clone();
                let game_dir = self.config.game_dir.clone();
                let processed_bytes = self.processed_bytes.clone();
                let cancellation_token = self.cancellation_token.clone();
                let vfs = Arc::new(self.vfs.clone());
                let hash_cache = self.hash_cache.clone();

                async move {
                    if cancellation_token.is_cancelled() {
                        return Ok(());
                    }

                    // Execute directive - hash computation and verification now handled within the directive
                    let computed_hash = directive.execute(
                        &install_dir,
                        &extracted_modlist_dir,
                        &downloads_dir,
                        &game_dir,
                        vfs,
                        None
                    ).await?;

                    // Cache the computed hash if one was returned
                    if let Some(hash) = computed_hash {
                        let out_path = (**install_dir).join(directive.to());
                        hash_cache.write_cache(&out_path, hash);
                    }

                    // Update progress (matches C#'s UpdateProgress(1))
                    *processed_bytes.lock().unwrap() += directive.size();

                    Ok::<(), InstallError>(())
                }
            }).await?;

        Ok(())
    }

    /// Phase 5: Create BSA files (sequential per BSA, but parallel within each BSA)
    #[allow(dead_code)]
    async fn create_bsa_files(&mut self) -> Result<(), InstallError> {

        if self.directives.is_empty() {
            return Ok(());
        }

        self.update_progress(
            InstallPhase::CreatingBSAs, 5, 7, 0, self.directives.len(),
            "Creating BSA files".to_string()
        );

        // Process BSAs sequentially (to avoid resource conflicts), but parallelize within each BSA
        for directive in &self.directives {
            if self.cancellation_token.is_cancelled() {
                break;
            }

            // Pattern match on the directive type
            let result: Result<(), InstallError> = match directive.as_ref() {
                Directive::CreateBSA(_) => Ok(()), // TODO: implement BSA creation
                _ => Ok(()), // Other types don't create BSAs
            };

            result?;

            // Update progress
            {
                let mut bytes = self.processed_bytes.lock().unwrap();
                *bytes += directive.size();
            }
        }

        Ok(())
    }

    /// Phase 6: Generate patches (parallelized)
    #[allow(dead_code)]
    async fn generate_patches(&self) -> Result<(), InstallError> {
        // Filter to only merged patch directives
        let patch_directives: Vec<Arc<Directive>> = self.directives
            .iter()
            .filter(|d| matches!(d.as_ref(), Directive::MergedPatch(_)))
            .cloned()
            .collect();

        if patch_directives.is_empty() {
            return Ok(());
        }

        self.update_progress(
            InstallPhase::GeneratingPatches, 6, 7, 0, patch_directives.len(),
            "Generating merged patches".to_string()
        );

        // Process all patch directives in parallel (matches C#'s .PDoAll())
        stream::iter(patch_directives)
            .map(Ok)
            .try_for_each_concurrent(self.config.max_concurrency, |directive| {
                let install_dir = self.config.install_dir.clone();
                let extracted_modlist_dir = self.config.extracted_modlist_dir.clone();
                let downloads_dir = self.config.downloads_dir.clone();
                let game_dir = self.config.game_dir.clone();
                let processed_bytes = self.processed_bytes.clone();
                let cancellation_token = self.cancellation_token.clone();
                let vfs = Arc::new(self.vfs.clone());
                let hash_cache = self.hash_cache.clone();

                async move {
                    if cancellation_token.is_cancelled() {
                        return Ok(());
                    }

                    // Execute directive - TODO: implement patch generation
                    let computed_hash = directive.execute(
                        &install_dir,
                        &extracted_modlist_dir,
                        &downloads_dir,
                        &game_dir,
                        vfs,
                        None
                    ).await?;

                    // Cache the computed hash if one was returned
                    if let Some(hash) = computed_hash {
                        let out_path = (**install_dir).join(directive.to());
                        hash_cache.write_cache(&out_path, hash);
                    }

                    // Update progress (matches C#'s UpdateProgress(1))
                    *processed_bytes.lock().unwrap() += directive.size();

                    Ok::<(), InstallError>(())
                }
            }).await?;

        Ok(())
    }

    /// Phase 7: Finalize installation
    #[allow(dead_code)]
    async fn finalize_installation(&mut self) -> Result<(), InstallError> {
        self.update_progress(InstallPhase::Finishing, 7, 7, 0, 0, "Finalizing installation".to_string());

        // Clean up temporary files
        if (**self.config.temp_dir).exists() {
            tokio::fs::remove_dir_all(&**self.config.temp_dir).await.ok(); // Ignore errors
        }

        Ok(())
    }

    /// Update progress and notify callback if set
    fn update_progress(&self, phase: InstallPhase, current_step: usize, total_steps: usize, processed_items: usize, total_items: usize, message: String) {
     let callback = &self.progress_callback;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::directives::{FromArchive, InlineFileDirective, PatchedFromArchive};

    #[tokio::test]
    async fn test_installer_creation() {
        let vfs = VfsContext::new();
        let config = InstallerConfig::default();
        let directives: Vec<Arc<Directive>> = vec![
            Arc::new(Directive::FromArchive(FromArchive::new(
                "test.txt".to_string(),
                "abc123".to_string(),
                100,
                vec!["archive_hash".to_string(), "path/to/file".to_string()],
            ))),
            Arc::new(Directive::InlineFile(InlineFileDirective::new(
                "inline.txt".to_string(),
                "def456".to_string(),
                50,
                "inline_data_id".to_string(),
            ))),
        ];

        let installer = Installer::new(config, directives, vfs);
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

        let vfs = VfsContext::new();

        let installer = Installer::new(config, directives, vfs)
            .with_progress_callback(callback);

        // This should trigger at least one progress callback during preparation
        installer.install().await.ok();

        assert!(call_count.load(Ordering::SeqCst) > 0);
    }


    mod archive_files {
        use super::*;


    #[tokio::test]
    async fn test_install_archive_files() {
        let vfs = VfsContext::new();
        let config = InstallerConfig::default();
        let directives: Vec<Arc<Directive>> = vec![
            Arc::new(Directive::Test(crate::install::directives::TestDirective::new(
                "test.txt".to_string(),
                "abc123".to_string(),
                100,
                "test_data_id".to_string(),
            ))),
            Arc::new(Directive::FromArchive(FromArchive::new(
                "test.txt".to_string(),
                "abc123".to_string(),
                100,
                vec!["archive_hash".to_string(), "path/to/file".to_string()],
            ))),
            Arc::new(Directive::PatchedFromArchive(PatchedFromArchive::new(
                "test.txt".to_string(),
                "abc123".to_string(),
                100,
                vec!["archive_hash".to_string(), "path/to/file".to_string()],
                "def456".to_string(),
                "patch_id".to_string(),
            ))),
        ];
        let installer = Installer::new(config, directives, vfs);

        installer.install_archive_files().await.ok();
    }

    }

}

