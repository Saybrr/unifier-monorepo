//! HTTP-based file downloader with resume support

use crate::downloader::{
    core::{DownloadRequest, DownloadResult, FileValidation, DownloadError, Result, ProgressCallback, ProgressEvent},
    config::DownloadConfig,
    registry::FileDownloader,
};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, warn, info_span, Instrument};

/// HTTP-based file downloader
pub struct HttpDownloader {
    client: Client,
    config: DownloadConfig,
}

impl HttpDownloader {
    pub fn new(config: DownloadConfig) -> Self {
        let client = Client::builder()
            .timeout(config.timeout)
            .user_agent(&config.user_agent)
            .build()
            .expect("Failed to create HTTP client");

        Self { client, config }
    }

    async fn get_file_size(&self, url: &str) -> Result<Option<u64>> {
        debug!("Getting file size for: {}", url);
        let response = self.client.head(url).send().await?;
        response.error_for_status_ref()?;

        Ok(response.content_length())
    }

    async fn download_file(
        &self,
        url: &str,
        dest_path: &Path,
        progress_callback: Option<ProgressCallback>,
        expected_size: Option<u64>,
    ) -> Result<u64> {
        // Check for existing partial file
        let temp_path = dest_path.with_extension("part");
        let start_byte = if self.config.allow_resume && temp_path.exists() {
            let size = fs::metadata(&temp_path).await?.len();
            debug!("Found partial file, resuming from byte {}", size);
            size
        } else {
            0
        };

        // Get file size for progress tracking
        // Use expected size if provided, otherwise try to get from server
        let mut total_size = if let Some(expected) = expected_size {
            debug!("Using expected size from validation: {} bytes", expected);
            Some(expected)
        } else {
            debug!("No expected size provided, querying server");
            self.get_file_size(url).await?
        };

        // Build request with range header for resume
        let mut request = self.client.get(url);
        if start_byte > 0 {
            request = request.header("Range", format!("bytes={}-", start_byte));
            debug!("Requesting range: bytes={}-", start_byte);
        }

        let response = request.send().await?;
        response.error_for_status_ref()?;

        // If we didn't get size from HEAD request, try to get it from GET response
        if total_size.is_none() {
            total_size = response.content_length();
            debug!("Got content length from GET response: {:?}", total_size);
        }

        // Adjust total size if resuming
        if let Some(size) = total_size {
            if start_byte > 0 {
                total_size = Some(start_byte + size);
            }
        }

        if let Some(ref callback) = progress_callback {
            callback(ProgressEvent::DownloadStarted {
                url: url.to_string(),
                total_size,
            });
        }

        // Open file for writing
        let mut file = if start_byte > 0 {
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&temp_path)
                .await?
        } else {
            fs::File::create(&temp_path).await?
        };

        // Download with progress tracking
        let mut stream = response.bytes_stream();
        let mut downloaded = start_byte;
        let start_time = std::time::Instant::now();
        let mut last_progress_time = start_time;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;

            // Report progress at most every 100ms to avoid spam
            let now = std::time::Instant::now();
            if now.duration_since(last_progress_time).as_millis() >= 100 {
                if let Some(ref callback) = progress_callback {
                    let elapsed = start_time.elapsed().as_secs_f64();
                    let speed = if elapsed > 0.0 {
                        (downloaded - start_byte) as f64 / elapsed
                    } else {
                        0.0
                    };

                    callback(ProgressEvent::DownloadProgress {
                        url: url.to_string(),
                        downloaded,
                        total: total_size,
                        speed_bps: speed,
                    });
                }
                last_progress_time = now;
            }
        }

        file.flush().await?;

        // Move temp file to final destination
        fs::rename(&temp_path, dest_path).await?;

        if let Some(ref callback) = progress_callback {
            callback(ProgressEvent::DownloadComplete {
                url: url.to_string(),
                final_size: downloaded,
            });
        }

        debug!("Download completed: {} bytes", downloaded);
        Ok(downloaded)
    }
}


#[async_trait]
impl FileDownloader for HttpDownloader {
    async fn download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult> {
        // Extract URL first for tracing
        let url = match &request.source {
            crate::downloader::core::DownloadSource::Url { url, .. } => url.as_str(),
            crate::downloader::core::DownloadSource::Structured(structured) => {
                match structured {
                    crate::parse_wabbajack::sources::DownloadSource::Http(http_source) => {
                        &http_source.url
                    },
                    _ => {
                        return Err(DownloadError::UnsupportedUrl {
                            url: "non-http structured source".to_string(),
                            scheme: "structured".to_string(),
                            supported_schemes: "http, https".to_string(),
                        });
                    }
                }
            }
        };

        async move {
            let filename = request.get_filename()?;
            let dest_path = request.destination.join(&filename);

            debug!("Downloading {} to {}", url, dest_path.display());

            // Check existing file first
            if let Some(result) = self.check_existing_file(&dest_path, &request.validation, progress_callback.clone()).await? {
                return Ok(result);
            }
            let size = self.download_helper(url, &dest_path, progress_callback.clone(), request.expected_size).await?;

            // Validate the downloaded file (only if validation is specified)
            if !request.validation.is_empty() {
                debug!("Validating downloaded file");
                if !request.validation.validate_file(&dest_path, progress_callback).await? {
                    fs::remove_file(&dest_path).await?;
                    return Err(DownloadError::ValidationFailed {
                        file: dest_path.clone(),
                        validation_type: crate::downloader::core::ValidationType::Size, // Default validation type
                        expected: "valid file".to_string(),
                        actual: "invalid file".to_string(),
                        suggestion: "Check file integrity or download again".to_string(),
                    });
                }
                debug!("File validation passed");
            }

            Ok(DownloadResult::Downloaded { size })
        }
        .instrument(info_span!("http_download", url = %url))
        .await
    }

    /// Download file only without any validation - pure download logic
    async fn download_helper(
        &self,
        url: &str,
        dest_path: &std::path::Path,
        progress_callback: Option<ProgressCallback>,
        expected_size: Option<u64>,
    ) -> Result<u64> {
        debug!("Download: {} to {}", url, dest_path.display());

        // Create destination directory if it doesn't exist
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).await?;
            debug!("Created directory: {}", parent.display());
        }

        let size = self.download_file(url, dest_path, progress_callback, expected_size).await?;
        debug!("Pure download completed: {} bytes", size);
        Ok(size)
    }

    /// Check if file exists and handle validation if needed
    async fn check_existing_file(
        &self,
        dest_path: &std::path::Path,
        validation: &FileValidation,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<Option<DownloadResult>> {
        if dest_path.exists() {
            let size = fs::metadata(dest_path).await?.len();

            if validation.is_empty() {
                // No validation needed, file exists
                debug!("File exists and no validation required");
                return Ok(Some(DownloadResult::AlreadyExists { size }));
            } else if validation.validate_file(dest_path, progress_callback).await? {
                debug!("File exists and is valid");
                return Ok(Some(DownloadResult::AlreadyExists { size }));
            } else {
                // Remove invalid file
                warn!("Existing file is invalid, removing: {}", dest_path.display());
                fs::remove_file(dest_path).await?;
            }
        }
        Ok(None)
    }

    fn supports_url(&self, url: &str) -> bool {
        url.starts_with("http://") || url.starts_with("https://")
    }
}

/// Create optimized HTTP downloader for different use cases
impl HttpDownloader {
    /// Create downloader optimized for large files
    pub fn for_large_files() -> Self {
        let config = crate::downloader::config::DownloadConfigBuilder::new()
            .high_performance()
            .timeout(std::time::Duration::from_secs(300)) // 5 minutes for large files
            .chunk_size(32768) // 32KB chunks
            .build();

        Self::new(config)
    }

    /// Create downloader optimized for many small files
    pub fn for_small_files() -> Self {
        let config = crate::downloader::config::DownloadConfigBuilder::new()
            .timeout(std::time::Duration::from_secs(30))
            .chunk_size(4096) // 4KB chunks
            .max_concurrent_validations(8)
            .build();

        Self::new(config)
    }

    /// Create reliable downloader with more retries
    pub fn reliable() -> Self {
        let config = crate::downloader::config::DownloadConfigBuilder::new()
            .reliable()
            .build();

        Self::new(config)
    }
}
