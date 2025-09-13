//! GameFile download source implementation

use async_trait::async_trait;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, warn};

use crate::downloader::core::{
    Downloadable, DownloadRequest, DownloadResult, ProgressCallback, Result,
    DownloadError, ValidationType, ProgressEvent
};
use crate::parse_wabbajack::sources::GameFileSource;

#[async_trait]
impl Downloadable for GameFileSource {
    async fn download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<ProgressCallback>,
        _config: &crate::downloader::config::DownloadConfig,
    ) -> Result<DownloadResult> {
        let filename = request.get_filename()?;
        let dest_path = request.destination.join(&filename);

        debug!("GameFile downloading {} from {} ({})",
               self.file_path, self.game, self.game_version);

        // Check if file already exists and is valid
        if let Some(result) = self.check_existing_file(&dest_path, &request.validation, progress_callback.clone()).await? {
            return Ok(result);
        }

        // Discover game location
        let game_dir = self.discover_game_location().await?;

        // Construct source file path
        let source_path = game_dir.join(&self.file_path);

        // Check if source file exists
        if !source_path.exists() {
            return Err(DownloadError::Legacy(
                format!("Game file not found: {}", source_path.display())
            ));
        }

        // Copy the file
        let size = self.copy_file_with_progress(&source_path, &dest_path, progress_callback.clone()).await?;

        // Validate the copied file (only if validation is specified)
        if !request.validation.is_empty() {
            debug!("Validating copied game file: {} (expected_size: {:?})",
                   dest_path.display(), request.validation.expected_size);

            match request.validation.validate_file(&dest_path, progress_callback).await {
                Ok(true) => {
                    debug!("Game file validation passed");
                },
                Ok(false) => {
                    // This shouldn't happen as validate_file returns Err for failures
                    fs::remove_file(&dest_path).await?;
                    return Err(DownloadError::ValidationFailed {
                        file: dest_path.clone(),
                        validation_type: ValidationType::Size,
                        expected: "valid file".to_string(),
                        actual: "invalid file".to_string(),
                        suggestion: "Check game file integrity or reinstall the game".to_string(),
                    });
                },
                Err(e) => {
                    // Log the specific validation error (like SizeMismatch)
                    debug!("Game file validation failed with error: {}", e);
                    fs::remove_file(&dest_path).await?;
                    return Err(e); // Propagate the specific error (e.g., SizeMismatch)
                }
            }
        }

        Ok(DownloadResult::Downloaded { size })
    }

    fn description(&self) -> String {
        format!("Game file: {} from {}", self.file_path, self.game)
    }

    fn requires_external_dependencies(&self) -> bool {
        true
    }
}

impl GameFileSource {
    /// Discover the installation directory for this game
    async fn discover_game_location(&self) -> Result<std::path::PathBuf> {
        debug!("Discovering game location for: {}", self.game);

        // Try common Steam installation paths
        let steam_paths = self.get_steam_paths();
        for steam_path in steam_paths {
            let game_path = steam_path.join("steamapps").join("common").join(self.get_steam_folder_name());
            if game_path.exists() {
                debug!("Found game at Steam path: {}", game_path.display());
                return Ok(game_path);
            }
        }

        // Try Windows registry (if on Windows)
        #[cfg(windows)]
        if let Ok(registry_path) = self.get_game_from_registry().await {
            if registry_path.exists() {
                debug!("Found game via Windows registry: {}", registry_path.display());
                return Ok(registry_path);
            }
        }

        // Try environment variables
        if let Ok(env_path) = std::env::var(format!("{}_PATH", self.game.to_uppercase())) {
            let path = std::path::PathBuf::from(env_path);
            if path.exists() {
                debug!("Found game via environment variable: {}", path.display());
                return Ok(path);
            }
        }

        Err(DownloadError::Legacy(
            format!("Could not locate game installation for: {}. Please ensure {} is installed, or set the {}_PATH environment variable to point to the game directory",
                   self.game, self.game, self.game.to_uppercase())
        ))
    }

    /// Get common Steam installation paths
    fn get_steam_paths(&self) -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();

        // Default Steam path
        #[cfg(windows)]
        {
            if let Ok(program_files) = std::env::var("PROGRAMFILES(X86)") {
                paths.push(std::path::PathBuf::from(program_files).join("Steam"));
            }
            if let Ok(program_files) = std::env::var("PROGRAMFILES") {
                paths.push(std::path::PathBuf::from(program_files).join("Steam"));
            }
        }

        #[cfg(unix)]
        {
            if let Ok(home) = std::env::var("HOME") {
                paths.push(std::path::PathBuf::from(home).join(".steam").join("steam"));
                paths.push(std::path::PathBuf::from(home).join(".local").join("share").join("Steam"));
            }
        }

        paths
    }

    /// Get the Steam folder name for this game
    fn get_steam_folder_name(&self) -> String {
        match self.game.as_str() {
            "SkyrimSpecialEdition" => "The Elder Scrolls V Skyrim Special Edition".to_string(),
            "Skyrim" => "Skyrim".to_string(),
            "Fallout4" => "Fallout 4".to_string(),
            "FalloutNewVegas" => "Fallout New Vegas".to_string(),
            "Fallout3" => "Fallout 3".to_string(),
            "Oblivion" => "Oblivion".to_string(),
            "Morrowind" => "Morrowind".to_string(),
            _ => self.game.clone(), // Default to the game identifier
        }
    }

    /// Get game location from Windows registry
    #[cfg(windows)]
    async fn get_game_from_registry(&self) -> Result<std::path::PathBuf> {
        use winreg::enums::*;
        use winreg::RegKey;

        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

        // Try different registry paths based on the game
        let registry_paths = match self.game.as_str() {
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
            _ => return Err(DownloadError::Legacy(
                format!("No registry path configured for game: {}", self.game)
            )),
        };

        for registry_path in registry_paths {
            if let Ok(key) = hklm.open_subkey(registry_path) {
                if let Ok(install_path) = key.get_value::<String, _>("installed path") {
                    return Ok(std::path::PathBuf::from(install_path));
                }
            }
        }

        Err(DownloadError::Legacy(
            format!("Game not found in Windows registry: {}", self.game)
        ))
    }

    /// Copy a file from source to destination with progress reporting
    async fn copy_file_with_progress(
        &self,
        source_path: &Path,
        dest_path: &Path,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<u64> {
        // Get source file size
        let source_metadata = fs::metadata(source_path).await?;
        let total_size = source_metadata.len();
        debug!("Copying file: {} -> {} ({} bytes)",
               source_path.display(), dest_path.display(), total_size);

        // Create destination directory if needed
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Report start
        if let Some(ref callback) = progress_callback {
            callback(ProgressEvent::DownloadStarted {
                url: format!("gamefile://{}", source_path.display()),
                total_size: Some(total_size),
            });
        }

        // Open source and destination files
        let mut source_file = fs::File::open(source_path).await?;
        let mut dest_file = fs::File::create(dest_path).await?;

        // Copy with progress reporting
        let mut copied = 0u64;
        let mut buffer = vec![0u8; 64 * 1024]; // 64KB buffer
        let start_time = std::time::Instant::now();
        let mut last_progress_time = start_time;

        loop {
            use tokio::io::AsyncReadExt;

            let bytes_read = source_file.read(&mut buffer).await?;
            if bytes_read == 0 {
                break; // EOF
            }

            dest_file.write_all(&buffer[..bytes_read]).await?;
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

        dest_file.flush().await?;

        // Report completion
        if let Some(ref callback) = progress_callback {
            callback(ProgressEvent::DownloadComplete {
                url: format!("gamefile://{}", source_path.display()),
                final_size: copied,
            });
        }

        debug!("Successfully copied game file: {} ({} bytes)", dest_path.display(), copied);
        Ok(copied)
    }

    /// Check if file exists and is valid
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
            let metadata = fs::metadata(dest_path).await?;
            debug!("Using existing file: {} ({} bytes)", dest_path.display(), metadata.len());
            return Ok(Some(DownloadResult::AlreadyExists {
                size: metadata.len()
            }));
        }

        // Validate existing file
        if validation.validate_file(dest_path, progress_callback).await? {
            let metadata = fs::metadata(dest_path).await?;
            debug!("Using existing validated file: {} ({} bytes)", dest_path.display(), metadata.len());
            Ok(Some(DownloadResult::AlreadyExists {
                size: metadata.len()
            }))
        } else {
            warn!("Existing file failed validation, will re-copy: {}", dest_path.display());
            // Remove invalid file
            fs::remove_file(dest_path).await?;
            Ok(None)
        }
    }
}
