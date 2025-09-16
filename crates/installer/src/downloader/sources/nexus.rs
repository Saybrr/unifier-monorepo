//! Nexus Mods download source implementation

use tracing::{debug, info};
use once_cell::sync::OnceCell;
use serde::Deserialize;

use crate::downloader::api::nexus_api::NexusAPI;
use crate::downloader::core::{
    DownloadRequest, DownloadResult, ProgressCallback, Result,
    DownloadError, ProgressEvent
};
use crate::downloader::core::http::HttpClient;
use crate::downloader::core::files::check_existing_file;

/// Raw Nexus archive state from JSON parsing
#[derive(Debug, Deserialize, Clone)]
pub struct NexusArchiveState {
    #[serde(rename = "ModID")]
    pub mod_id: u32,
    #[serde(rename = "FileID")]
    pub file_id: u32,
    #[serde(rename = "GameName")]
    pub game_name: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Author")]
    pub author: Option<String>,
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "Description")]
    pub description: String,
    #[serde(rename = "IsNSFW")]
    pub is_nsfw: bool,
    #[serde(rename = "ImageURL")]
    pub image_url: Option<String>,
}

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
            // Log error instead of warning since this is initialization
            // Initialization failures should be handled by the caller
            debug!("Failed to validate Nexus API key during initialization: {}", e);
            return Err(e);
        }
    }

    NEXUS_API.set(api).map_err(|_|
        DownloadError::Legacy("Failed to initialize global Nexus authentication".to_string())
    )?;
    Ok(())
}

/// Get the global Nexus authentication client
pub fn get_nexus_api() -> Result<&'static NexusAPI> {
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
        config: &crate::downloader::core::config::DownloadConfig,
    ) -> Result<DownloadResult> {
        let filename = request.get_filename()?;
        let dest_path = request.destination.join(&filename);

        debug!("Nexus downloading mod {} file {} to {}",
               self.mod_id, self.file_id, dest_path.display());

        // Check if file already exists and is valid using common utility
        if let Some(result) = check_existing_file(&dest_path, &request.validation, progress_callback.clone()).await? {
            return Ok(result);
        }

        // Get Nexus authentication
        let api = get_nexus_api()?;

        // Get download links
        let download_links = api.get_download_links(&self.game_name, self.mod_id, self.file_id).await
            .map_err(|e| {
                // Report warning through progress callback
                if let Some(ref callback) = progress_callback {
                    let url = format!("nexus://{}:{}", self.mod_id, self.file_id);
                    callback(ProgressEvent::Warning {
                        url,
                        message: format!("Failed to get download links for {}:{}:{}: {}",
                                       self.game_name, self.mod_id, self.file_id, e),
                    });
                }
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

        debug!("Using CDN: {} for mod {} file {}",
              download_link.name, self.mod_id, self.file_id);

        // Download the file using centralized logic with retry
        let expected_size = request.validation.expected_size;


             // Create HTTP client with appropriate timeout
        let timeout = expected_size
            .map(|size| config.get_timeout_for_size(size))
            .unwrap_or(config.timeout);
         let http_client = HttpClient::with_timeout(config, timeout)?;

         // Use HttpClient's built-in retry logic
         let final_size = http_client.download_with_retry(&download_link.uri, &dest_path, expected_size, progress_callback.clone(), config).await?;

        // Return result with file path for centralized validation
        Ok(DownloadResult::Downloaded {
            size: final_size,
            file_path: dest_path
        })
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
