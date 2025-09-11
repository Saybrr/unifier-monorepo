//! GameFile backend for copying files from game installations
//!
//! This backend handles GameFileSource downloads by copying files from
//! the user's game installation directory to the destination. It includes
//! game location discovery, file validation, and progress reporting.

use crate::downloader::{
    core::{
        DownloadRequest, DownloadResult, ProgressCallback, ProgressEvent,
        DownloadError, Result, FileOperation
    },
    registry::FileDownloader,
    config::DownloadConfig,
};
use crate::parse_wabbajack::sources::DownloadSource;

use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info, warn};

/// Game location discovery and file copying backend
///
/// This downloader handles GameFileSource requests by:
/// 1. Discovering the game installation directory
/// 2. Locating the source file within the game installation
/// 3. Copying the file to the destination with progress reporting
/// 4. Validating the copied file if validation is specified
pub struct GameFileDownloader {
    /// Configuration for the downloader
    config: DownloadConfig,
    /// Cache of discovered game locations to avoid repeated discovery
    game_locations: HashMap<String, PathBuf>,
}

impl GameFileDownloader {
    /// Create a new GameFile downloader with the given configuration
    pub fn new(config: DownloadConfig) -> Self {
        Self {
            config,
            game_locations: HashMap::new(),
        }
    }

    /// Discover the installation directory for a given game
    ///
    /// This method attempts to find the game installation using various methods:
    /// 1. Common Steam installation paths
    /// 2. Registry entries (Windows)
    /// 3. Environment variables
    /// 4. User-configured paths
    async fn discover_game_location(&mut self, game: &str) -> Result<PathBuf> {
        // Check cache first
        if let Some(location) = self.game_locations.get(game) {
            if location.exists() {
                debug!("Using cached game location for {}: {}", game, location.display());
                return Ok(location.clone());
            } else {
                warn!("Cached game location no longer exists: {}", location.display());
                self.game_locations.remove(game);
            }
        }

        debug!("Discovering game location for: {}", game);
        let location = self.discover_game_location_impl(game).await?;

        // Cache the discovered location
        self.game_locations.insert(game.to_string(), location.clone());
        info!("Discovered game location for {}: {}", game, location.display());

        Ok(location)
    }

    /// Implementation of game location discovery
    async fn discover_game_location_impl(&self, game: &str) -> Result<PathBuf> {
        // Try common Steam installation paths
        let steam_paths = self.get_steam_paths();
        for steam_path in steam_paths {
            let game_path = steam_path.join("steamapps").join("common").join(self.get_steam_folder_name(game));
            if game_path.exists() {
                debug!("Found game at Steam path: {}", game_path.display());
                return Ok(game_path);
            }
        }

        // Try Windows registry (if on Windows)
        #[cfg(windows)]
        if let Ok(registry_path) = self.get_game_from_registry(game).await {
            if registry_path.exists() {
                debug!("Found game via Windows registry: {}", registry_path.display());
                return Ok(registry_path);
            }
        }

        // Try environment variables
        if let Ok(env_path) = std::env::var(format!("{}_PATH", game.to_uppercase())) {
            let path = PathBuf::from(env_path);
            if path.exists() {
                debug!("Found game via environment variable: {}", path.display());
                return Ok(path);
            }
        }

        Err(DownloadError::Configuration {
            message: format!("Could not locate game installation for: {}", game),
            field: Some("game".to_string()),
            suggestion: Some(format!(
                "Please ensure {} is installed, or set the {}_PATH environment variable to point to the game directory",
                game, game.to_uppercase()
            )),
        })
    }

    /// Get common Steam installation paths
    fn get_steam_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Default Steam path
        #[cfg(windows)]
        {
            if let Ok(program_files) = std::env::var("PROGRAMFILES(X86)") {
                paths.push(PathBuf::from(program_files).join("Steam"));
            }
            if let Ok(program_files) = std::env::var("PROGRAMFILES") {
                paths.push(PathBuf::from(program_files).join("Steam"));
            }
        }

        #[cfg(unix)]
        {
            if let Ok(home) = std::env::var("HOME") {
                paths.push(PathBuf::from(home).join(".steam").join("steam"));
                paths.push(PathBuf::from(home).join(".local").join("share").join("Steam"));
            }
        }

        paths
    }

    /// Get the Steam folder name for a given game identifier
    fn get_steam_folder_name(&self, game: &str) -> String {
        match game {
            "SkyrimSpecialEdition" => "The Elder Scrolls V Skyrim Special Edition".to_string(),
            "Skyrim" => "Skyrim".to_string(),
            "Fallout4" => "Fallout 4".to_string(),
            "FalloutNewVegas" => "Fallout New Vegas".to_string(),
            "Fallout3" => "Fallout 3".to_string(),
            "Oblivion" => "Oblivion".to_string(),
            "Morrowind" => "Morrowind".to_string(),
            _ => game.to_string(), // Default to the game identifier
        }
    }

    /// Get game location from Windows registry
    #[cfg(windows)]
    async fn get_game_from_registry(&self, game: &str) -> Result<PathBuf> {
        use winreg::enums::*;
        use winreg::RegKey;

        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

        // Try different registry paths based on the game
        let registry_paths = match game {
            "SkyrimSpecialEdition" => vec![
                r"SOFTWARE\Bethesda Softworks\Skyrim Special Edition",
                r"SOFTWARE\WOW6432Node\Bethesda Softworks\Skyrim Special Edition",
            ],
            "Skyrim" => vec![
                r"SOFTWARE\Bethesda Softworks\Skyrim",
                r"SOFTWARE\WOW6432Node\Bethesda Softworks\Skyrim",
            ],
            "Fallout4" => vec![
                r"SOFTWARE\Bethesda Softworks\Fallout4",
                r"SOFTWARE\WOW6432Node\Bethesda Softworks\Fallout4",
            ],
            _ => return Err(DownloadError::Configuration {
                message: format!("No registry path configured for game: {}", game),
                field: Some("game".to_string()),
                suggestion: Some("Use environment variable or Steam installation".to_string()),
            }),
        };

        for registry_path in registry_paths {
            if let Ok(key) = hklm.open_subkey(registry_path) {
                if let Ok(install_path) = key.get_value::<String, _>("installed path") {
                    return Ok(PathBuf::from(install_path));
                }
            }
        }

        Err(DownloadError::Configuration {
            message: format!("Game not found in Windows registry: {}", game),
            field: Some("game".to_string()),
            suggestion: Some("Ensure the game is properly installed".to_string()),
        })
    }

    /// Copy a file from source to destination with progress reporting
    async fn copy_file_with_progress(
        &self,
        source_path: &Path,
        dest_path: &Path,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<u64> {
        // Get source file size
        let source_metadata = fs::metadata(source_path).await
            .map_err(|e| DownloadError::FileSystem {
                path: source_path.to_path_buf(),
                operation: FileOperation::Read,
                source: e,
            })?;

        let total_size = source_metadata.len();
        debug!("Copying file: {} -> {} ({} bytes)",
               source_path.display(), dest_path.display(), total_size);

        // Create destination directory if needed
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).await
                .map_err(|e| DownloadError::FileSystem {
                    path: parent.to_path_buf(),
                    operation: FileOperation::CreateDir,
                    source: e,
                })?;
        }

        // Report start
        if let Some(ref callback) = progress_callback {
            callback(ProgressEvent::DownloadStarted {
                url: format!("gamefile://{}", source_path.display()),
                total_size: Some(total_size),
            });
        }

        // Open source and destination files
        let mut source_file = fs::File::open(source_path).await
            .map_err(|e| DownloadError::FileSystem {
                path: source_path.to_path_buf(),
                operation: FileOperation::Read,
                source: e,
            })?;

        let mut dest_file = fs::File::create(dest_path).await
            .map_err(|e| DownloadError::FileSystem {
                path: dest_path.to_path_buf(),
                operation: FileOperation::Create,
                source: e,
            })?;

        // Copy with progress reporting
        let mut copied = 0u64;
        let mut buffer = vec![0u8; 64 * 1024]; // 64KB buffer
        let start_time = std::time::Instant::now();
        let mut last_progress_time = start_time;

        loop {
            use tokio::io::AsyncReadExt;

            let bytes_read = source_file.read(&mut buffer).await
                .map_err(|e| DownloadError::FileSystem {
                    path: source_path.to_path_buf(),
                    operation: FileOperation::Read,
                    source: e,
                })?;

            if bytes_read == 0 {
                break; // EOF
            }

            dest_file.write_all(&buffer[..bytes_read]).await
                .map_err(|e| DownloadError::FileSystem {
                    path: dest_path.to_path_buf(),
                    operation: FileOperation::Write,
                    source: e,
                })?;

            copied += bytes_read as u64;

            // Report progress at most every 100ms
            let now = std::time::Instant::now();
            if now.duration_since(last_progress_time).as_millis() >= 100 {
                if let Some(ref callback) = progress_callback {
                    let elapsed = start_time.elapsed().as_secs_f64();
                    let speed = if elapsed > 0.0 {
                        copied as f64 / elapsed
                    } else {
                        0.0
                    };

                    callback(ProgressEvent::DownloadProgress {
                        url: format!("gamefile://{}", source_path.display()),
                        downloaded: copied,
                        total: Some(total_size),
                        speed_bps: speed,
                    });
                }
                last_progress_time = now;
            }
        }

        dest_file.flush().await
            .map_err(|e| DownloadError::FileSystem {
                path: dest_path.to_path_buf(),
                operation: FileOperation::Write,
                source: e,
            })?;

        // Report completion
        if let Some(ref callback) = progress_callback {
            callback(ProgressEvent::DownloadComplete {
                url: format!("gamefile://{}", source_path.display()),
                final_size: copied,
            });
        }

        info!("Successfully copied game file: {} ({} bytes)", dest_path.display(), copied);
        Ok(copied)
    }
}

#[async_trait]
impl FileDownloader for GameFileDownloader {
    async fn download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult> {
        let gamefile_source = match &request.source {
            DownloadSource::GameFile(source) => source,
            _ => return Err(DownloadError::Configuration {
                message: "GameFile downloader only supports GameFile sources".to_string(),
                field: None,
                suggestion: None,
            }),
        };

        let filename = request.get_filename()?;
        let dest_path = request.destination.join(&filename);

        debug!("GameFile download request: {} from {} ({})",
               gamefile_source.file_path, gamefile_source.game, gamefile_source.game_version);

        // Check if file already exists and is valid
        if let Some(result) = self.check_existing_file(&dest_path, &request.validation, progress_callback.clone()).await? {
            return Ok(result);
        }

        // Create a mutable copy to handle game location discovery
        // In a production implementation, you'd want to use Arc<Mutex<HashMap>> or similar
        let mut temp_downloader = GameFileDownloader::new(self.config.clone());
        temp_downloader.game_locations = self.game_locations.clone();

        // Discover game location
        let game_dir = temp_downloader.discover_game_location(&gamefile_source.game).await?;

        // Construct source file path
        let source_path = game_dir.join(&gamefile_source.file_path);

        // Check if source file exists
        if !source_path.exists() {
            return Err(DownloadError::FileSystem {
                path: source_path.clone(),
                operation: FileOperation::Read,
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Game file not found: {}", source_path.display())
                ),
            });
        }

        // Copy the file
        let size = temp_downloader.copy_file_with_progress(&source_path, &dest_path, progress_callback.clone()).await?;

        // Validate the copied file (only if validation is specified)
        if !request.validation.is_empty() {
            debug!("Validating copied game file");
            if !request.validation.validate_file(&dest_path, progress_callback).await? {
                fs::remove_file(&dest_path).await?;
                return Err(DownloadError::ValidationFailed {
                    file: dest_path.clone(),
                    validation_type: crate::downloader::core::ValidationType::Size,
                    expected: "valid file".to_string(),
                    actual: "invalid file".to_string(),
                    suggestion: "Check game file integrity or reinstall the game".to_string(),
                });
            }
            debug!("Game file validation passed");
        }

        Ok(DownloadResult::Downloaded { size })
    }

    async fn download_helper(
        &self,
        url: &str,
        dest_path: &Path,
        progress_callback: Option<ProgressCallback>,
        _expected_size: Option<u64>,
    ) -> Result<u64> {
        // For GameFile sources, the "URL" is actually a game file path
        // This is a simplified implementation - in practice you'd want to parse the URL

        // Parse gamefile:// URL format: gamefile://GameName/path/to/file
        if !url.starts_with("gamefile://") {
            return Err(DownloadError::Configuration {
                message: format!("Invalid gamefile URL: {}", url),
                field: Some("url".to_string()),
                suggestion: Some("Use format: gamefile://GameName/path/to/file".to_string()),
            });
        }

        let path_part = &url[11..]; // Remove "gamefile://"
        let parts: Vec<&str> = path_part.splitn(2, '/').collect();
        if parts.len() != 2 {
            return Err(DownloadError::Configuration {
                message: format!("Invalid gamefile URL format: {}", url),
                field: Some("url".to_string()),
                suggestion: Some("Use format: gamefile://GameName/path/to/file".to_string()),
            });
        }

        let game = parts[0];
        let file_path = parts[1];

        let mut downloader = GameFileDownloader::new(self.config.clone());
        let game_dir = downloader.discover_game_location(game).await?;
        let source_path = game_dir.join(file_path);

        self.copy_file_with_progress(&source_path, dest_path, progress_callback).await
    }

    async fn check_existing_file(
        &self,
        dest_path: &Path,
        validation: &crate::downloader::core::FileValidation,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<Option<DownloadResult>> {
        if !dest_path.exists() {
            return Ok(None);
        }

        debug!("File already exists: {}", dest_path.display());

        // If no validation required, assume file is good
        if validation.is_empty() {
            let metadata = fs::metadata(dest_path).await
                .map_err(|e| DownloadError::FileSystem {
                    path: dest_path.to_path_buf(),
                    operation: FileOperation::Read,
                    source: e,
                })?;

            info!("Using existing file: {} ({} bytes)", dest_path.display(), metadata.len());
            return Ok(Some(DownloadResult::AlreadyExists {
                size: metadata.len()
            }));
        }

        // Validate existing file
        if validation.validate_file(dest_path, progress_callback).await? {
            let metadata = fs::metadata(dest_path).await
                .map_err(|e| DownloadError::FileSystem {
                    path: dest_path.to_path_buf(),
                    operation: FileOperation::Read,
                    source: e,
                })?;

            info!("Using existing validated file: {} ({} bytes)", dest_path.display(), metadata.len());
            Ok(Some(DownloadResult::AlreadyExists {
                size: metadata.len()
            }))
        } else {
            warn!("Existing file failed validation, will re-copy: {}", dest_path.display());
            // Remove invalid file
            fs::remove_file(dest_path).await
                .map_err(|e| DownloadError::FileSystem {
                    path: dest_path.to_path_buf(),
                    operation: FileOperation::Delete,
                    source: e,
                })?;
            Ok(None)
        }
    }

    fn supports_url(&self, url: &str) -> bool {
        url.starts_with("gamefile://")
    }
}

impl Default for GameFileDownloader {
    fn default() -> Self {
        Self::new(DownloadConfig::default())
    }
}
