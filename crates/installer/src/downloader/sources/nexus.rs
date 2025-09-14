//! Nexus Mods download source implementation

use reqwest::Client;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info, warn};
use once_cell::sync::OnceCell;

use crate::downloader::api::nexus_api::NexusAPI;
use crate::downloader::core::{
    DownloadRequest, DownloadResult, ProgressCallback, Result,
    DownloadError, ProgressEvent, FileOperation
};

/// Global Nexus authentication client instance
static NEXUS_API: OnceCell<NexusAPI> = OnceCell::new();

/// Initialize the global Nexus authentication client
pub async fn initialize_nexus_api() -> Result<()> {
    // Load environment variables if .env file exists
    if let Ok(_) = dotenv::dotenv() {
        debug!("Loaded environment variables from .env file");
    }

    let api = NexusAPI::new()?;

    // Validate the API key on initialization
    match api.validate_user().await {
        Ok(user) => {
            info!("Nexus authentication initialized for user: {} (Premium: {})", user.name, user.is_premium);
        }
        Err(e) => {
            warn!("Failed to validate Nexus API key: {}", e);
            return Err(e);
        }
    }

    NEXUS_API.set(api).map_err(|_|
        DownloadError::Legacy("Failed to initialize global Nexus authentication".to_string())
    )?;
    Ok(())
}

/// Get the global Nexus authentication client
fn get_nexus_api() -> Result<&'static NexusAPI> {
    NEXUS_API.get().ok_or_else(||
        DownloadError::Configuration {
            message: "Nexus authentication not initialized".to_string(),
            field: Some("NEXUS_API_KEY".to_string()),
            suggestion: Some("Call initialize_nexus_auth() before using Nexus downloads".to_string()),
        }
    )
}

/// Nexus Mods download source
#[derive(Debug, Clone, PartialEq)]
pub struct NexusSource {
    /// Nexus mod ID
    pub mod_id: u32,
    /// Nexus file ID
    pub file_id: u32,
    /// Game name (e.g., "SkyrimSpecialEdition")
    pub game_name: String,
    /// Mod name for display
    pub mod_name: String,
    /// Mod author
    pub author: String,
    /// Mod version
    pub version: String,
    /// Mod description
    pub description: String,
    /// Whether the mod is marked NSFW
    pub is_nsfw: bool,
}

impl NexusSource {
    /// Download file from Nexus Mods
    pub async fn download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<ProgressCallback>,
        config: &crate::downloader::config::DownloadConfig,
    ) -> Result<DownloadResult> {
        let filename = request.get_filename()?;
        let dest_path = request.destination.join(&filename);

        debug!("Nexus downloading mod {} file {} to {}",
               self.mod_id, self.file_id, dest_path.display());

        // Check if file already exists and is valid
        if let Some(result) = self.check_existing_file(&dest_path, &request.validation, progress_callback.clone()).await? {
            return Ok(result);
        }

        // Get Nexus authentication
        let api = get_nexus_api()?;

        // Get download links
        let download_links = api.get_download_links(&self.game_name, self.mod_id, self.file_id).await
            .map_err(|e| {
                warn!("Failed to get download links for {}:{}:{}: {}",
                      self.game_name, self.mod_id, self.file_id, e);
                e
            })?;

        if download_links.is_empty() {
            return Err(DownloadError::Legacy(
                format!("No download links available for mod {} file {}", self.mod_id, self.file_id)
            ));
        }

        // Select best download link
        let download_link = api.select_best_download_link(&download_links)
            .ok_or_else(|| DownloadError::Legacy("No suitable download link found".to_string()))?;

        info!("Using CDN: {} for mod {} file {}",
              download_link.name, self.mod_id, self.file_id);

        // Create destination directory if it doesn't exist
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| DownloadError::FileSystem {
                path: parent.to_path_buf(),
                operation: FileOperation::CreateDir,
                source: e,
            })?;
        }

        // Download the file using the download link
        let final_size = self.download_from_url(&download_link.uri, &dest_path,
                                               progress_callback.clone(), config).await?;

        // Validate the downloaded file
        self.validate_downloaded_file(&dest_path, &request.validation, progress_callback).await?;

        Ok(DownloadResult::Downloaded { size: final_size })
    }

    /// Download file from the provided URL
    async fn download_from_url(
        &self,
        url: &str,
        dest_path: &Path,
        progress_callback: Option<ProgressCallback>,
        config: &crate::downloader::config::DownloadConfig,
    ) -> Result<u64> {
        let client = Client::builder()
            .user_agent(&config.user_agent)
            .timeout(config.timeout)
            .build()
            .map_err(|e| DownloadError::Legacy(format!("Failed to create HTTP client: {}", e)))?;

        debug!("Starting download from: {}", url);

        let response = client.get(url).send().await
            .map_err(|e| DownloadError::HttpRequest {
                url: url.to_string(),
                source: e,
            })?;

        if !response.status().is_success() {
            return Err(DownloadError::HttpRequest {
                url: url.to_string(),
                source: reqwest::Error::from(response.error_for_status().unwrap_err()),
            });
        }

        let total_size = response.content_length().unwrap_or(0);
        debug!("Content length: {} bytes", total_size);

        // Create the output file
        let mut file = fs::File::create(dest_path).await.map_err(|e| DownloadError::FileSystem {
            path: dest_path.to_path_buf(),
            operation: FileOperation::Create,
            source: e,
        })?;

        let mut downloaded = 0u64;
        let mut stream = response.bytes_stream();

        // Download with progress reporting
        use futures::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| DownloadError::HttpRequest {
                url: url.to_string(),
                source: e,
            })?;

            file.write_all(&chunk).await.map_err(|e| DownloadError::FileSystem {
                path: dest_path.to_path_buf(),
                operation: FileOperation::Write,
                source: e,
            })?;

            downloaded += chunk.len() as u64;

            // Report progress
            if let Some(ref callback) = progress_callback {
                callback(ProgressEvent::DownloadProgress {
                    url: url.to_string(),
                    downloaded,
                    total: if total_size > 0 { Some(total_size) } else { None },
                    speed_bps: 0.0, // TODO: calculate actual speed
                });
            }
        }

        file.flush().await.map_err(|e| DownloadError::FileSystem {
            path: dest_path.to_path_buf(),
            operation: FileOperation::Write,
            source: e,
        })?;

        file.sync_all().await.map_err(|e| DownloadError::FileSystem {
            path: dest_path.to_path_buf(),
            operation: FileOperation::Write,
            source: e,
        })?;

        debug!("Download completed: {} bytes", downloaded);
        Ok(downloaded)
    }

    /// Validate the downloaded file
    async fn validate_downloaded_file(
        &self,
        file_path: &Path,
        validation: &crate::downloader::core::FileValidation,
        progress_callback: Option<ProgressCallback>
    ) -> Result<()> {
        // Validate size if expected
        let metadata = fs::metadata(file_path).await.map_err(|e| DownloadError::FileSystem {
            path: file_path.to_path_buf(),
            operation: FileOperation::Metadata,
            source: e,
        })?;

        if let Some(expected_size) = validation.expected_size {
            if metadata.len() != expected_size {
                // Delete the invalid file
                let _ = fs::remove_file(file_path).await;

                return Err(DownloadError::SizeMismatch {
                    file: file_path.to_path_buf(),
                    expected: expected_size,
                    actual: metadata.len(),
                    diff: metadata.len() as i64 - expected_size as i64,
                });
            }
        }

        // Validate hash if provided
        if let Some(ref expected_hash) = validation.xxhash64_base64 {
            let file_data = fs::read(file_path).await.map_err(|e| DownloadError::FileSystem {
                path: file_path.to_path_buf(),
                operation: FileOperation::Read,
                source: e,
            })?;

            // Compute xxHash64 hash
            let computed_hash = xxhash_rust::xxh64::xxh64(&file_data, 0);

            // Convert computed hash to base64
            use base64::Engine;
            let computed_hash_bytes = computed_hash.to_le_bytes();
            let computed_hash_base64 = base64::engine::general_purpose::STANDARD.encode(&computed_hash_bytes);

            // Compare base64 hashes
            if &computed_hash_base64 != expected_hash {
                // Delete the invalid file
                let _ = fs::remove_file(file_path).await;

                return Err(DownloadError::ValidationFailed {
                    file: file_path.to_path_buf(),
                    validation_type: crate::downloader::core::ValidationType::XxHash64,
                    expected: expected_hash.clone(),
                    actual: computed_hash_base64,
                    suggestion: "File may be corrupted, try downloading again".to_string()
                });
            }
        }

        // Report validation success
        if let Some(ref callback) = progress_callback {
            callback(ProgressEvent::ValidationComplete {
                file: file_path.display().to_string(),
                valid: true,
            });
        }

        Ok(())
    }

    /// Check if file exists and is valid
    async fn check_existing_file(
        &self,
        dest_path: &Path,
        validation: &crate::downloader::core::FileValidation,
        _progress_callback: Option<ProgressCallback>,
    ) -> Result<Option<DownloadResult>> {
        if !dest_path.exists() {
            return Ok(None);
        }

        debug!("File already exists, checking validity: {}", dest_path.display());

        let metadata = fs::metadata(dest_path).await.map_err(|e| DownloadError::FileSystem {
            path: dest_path.to_path_buf(),
            operation: FileOperation::Metadata,
            source: e,
        })?;

        // Quick size check
        if let Some(expected_size) = validation.expected_size {
            if metadata.len() != expected_size {
                debug!("Existing file has wrong size: {} vs {}", metadata.len(), expected_size);
                return Ok(None);
            }
        }

        // If we have a hash, validate it
        if validation.xxhash64_base64.is_some() {
            if let Err(_) = self.validate_downloaded_file(dest_path, validation, None).await {
                debug!("Existing file failed validation, will re-download");
                return Ok(None);
            }
        }

        debug!("Existing file is valid, skipping download");
        Ok(Some(DownloadResult::AlreadyExists {
            size: metadata.len()
        }))
    }
}

impl NexusSource {
    pub fn new(mod_id: u32, file_id: u32, game_name: String) -> Self {
        Self {
            mod_id,
            file_id,
            game_name,
            mod_name: String::new(),
            author: String::new(),
            version: String::new(),
            description: String::new(),
            is_nsfw: false,
        }
    }

    pub fn with_metadata<S: Into<String>>(
        mut self,
        name: S,
        author: S,
        version: S,
        description: S,
        is_nsfw: bool,
    ) -> Self {
        self.mod_name = name.into();
        self.author = author.into();
        self.version = version.into();
        self.description = description.into();
        self.is_nsfw = is_nsfw;
        self
    }
}
