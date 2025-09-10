//! Downloader registry for managing multiple download implementations
//!
//! The registry pattern allows supporting different download protocols and sources
//! while providing a unified interface. This is the layer between batch operations
//! and specific downloader implementations.

use crate::downloader::{
    core::{DownloadRequest, DownloadResult, ProgressCallback, DownloadError, Result, FileValidation},
    config::DownloadConfig,
    backends::HttpDownloader,
};
use async_trait::async_trait;

/// Trait for different download implementations
///
/// This trait defines the interface that all downloaders must implement.
/// Each downloader can support different protocols (HTTP, FTP, etc.) or
/// sources (direct URLs, Google Drive, etc.)
#[async_trait]
pub trait FileDownloader: Send + Sync {
    /// Download a file according to the request specification
    ///
    /// This is the main method that implements the complete download logic
    /// including validation, progress reporting, and error handling
    async fn download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult>;

    /// Download file without any validation - pure download logic
    ///
    /// This is a helper method for just the download part, without
    /// validation or other features. Used internally by the download method.
    async fn download_helper(
        &self,
        url: &str,
        dest_path: &std::path::Path,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<u64>;

    /// Check if file exists and handle validation if needed
    ///
    /// This method checks if a file already exists at the destination
    /// and validates it if necessary. Returns Some(result) if the file
    /// is already valid, None if download is needed.
    async fn check_existing_file(
        &self,
        dest_path: &std::path::Path,
        validation: &FileValidation,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<Option<DownloadResult>>;

    /// Check if this downloader supports the given URL scheme
    ///
    /// This is used by the registry to find the appropriate downloader
    /// for a given URL. For example, HttpDownloader supports "http" and "https".
    fn supports_url(&self, url: &str) -> bool;
}

/// Registry for managing multiple downloader implementations
///
/// The registry maintains a list of available downloaders and routes
/// download requests to the appropriate implementation based on URL scheme.
/// This allows supporting multiple protocols seamlessly.
pub struct DownloaderRegistry {
    downloaders: Vec<Box<dyn FileDownloader>>,
}

impl DownloaderRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            downloaders: Vec::new(),
        }
    }

    /// Register a new downloader implementation
    ///
    /// This method uses the builder pattern to allow chaining multiple
    /// downloader registrations fluently.
    pub fn register<D: FileDownloader + 'static>(mut self, downloader: D) -> Self {
        self.downloaders.push(Box::new(downloader));
        self
    }

    /// Add an HTTP downloader with the given configuration
    ///
    /// This is a convenience method for the most common case of adding
    /// HTTP/HTTPS support to the registry.
    pub fn with_http_downloader(self, config: DownloadConfig) -> Self {
        self.register(HttpDownloader::new(config))
    }

    /// Find the appropriate downloader for a given URL
    ///
    /// This method iterates through registered downloaders and returns
    /// the first one that supports the URL's scheme. If no downloader
    /// supports the URL, returns an UnsupportedUrl error.
    pub async fn find_downloader(&self, url: &str) -> Result<&dyn FileDownloader> {
        self.downloaders
            .iter()
            .find(|d| d.supports_url(url))
            .map(|d| d.as_ref())
            .ok_or_else(|| {
                let parsed_url = url::Url::parse(url);
                let scheme = parsed_url.map(|u| u.scheme().to_string()).unwrap_or_else(|_| "unknown".to_string());
                DownloadError::UnsupportedUrl {
                    url: url.to_string(),
                    scheme,
                    supported_schemes: "http, https".to_string(),
                }
            })
    }

    /// Attempt to download a file using the appropriate downloader
    ///
    /// This method combines finding the right downloader and performing
    /// the download in one convenient call. This is the main method used
    /// by batch operations.
    pub async fn attempt_download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult> {
        let downloader = self.find_downloader(&request.url).await?;
        downloader.download(request, progress_callback).await
    }
}

impl Default for DownloaderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
