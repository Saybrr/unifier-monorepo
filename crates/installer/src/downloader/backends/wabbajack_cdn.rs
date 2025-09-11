//! WabbajackCDN downloader implementation
//!
//! This module implements downloading from Wabbajack's CDN, which uses
//! a chunked file format with definition files and parallel part downloads.

use crate::downloader::{
    core::{DownloadRequest, DownloadResult, ProgressCallback, DownloadError, Result, FileValidation, ProgressEvent, ValidationType, FileOperation},
    registry::FileDownloader,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::path::Path;
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, AsyncSeekExt};
use flate2::read::GzDecoder;
use std::io::Read;

/// Domain remapping for Wabbajack CDN domains
const DOMAIN_REMAPS: &[(&str, &str)] = &[
    ("wabbajack.b-cdn.net", "authored-files.wabbajack.org"),
    ("wabbajack-mirror.b-cdn.net", "mirror.wabbajack.org"),
    ("wabbajack-patches.b-cdn.net", "patches.wabbajack.org"),
    ("wabbajacktest.b-cdn.net", "test-files.wabbajack.org"),
];

/// File definition structure from CDN
#[derive(Debug, Deserialize)]
pub struct FileDefinition {
    #[serde(rename = "MungedName")]
    pub munged_name: String,
    #[serde(rename = "Hash")]
    pub hash: String,
    #[serde(rename = "Size")]
    pub size: u64,
    #[serde(rename = "Parts")]
    pub parts: Vec<PartDefinition>,
}

/// Part definition for a file chunk
#[derive(Debug, Deserialize)]
pub struct PartDefinition {
    #[serde(rename = "Index")]
    pub index: u32,
    #[serde(rename = "Size")]
    pub size: u64,
    #[serde(rename = "Hash")]
    pub hash: String,
    #[serde(rename = "Offset")]
    pub offset: u64,
}

/// WabbajackCDN downloader implementation
pub struct WabbajackCDNDownloader {
    client: Client,
}

impl WabbajackCDNDownloader {
    /// Create a new WabbajackCDN downloader
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent("Unifier/1.0")
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    /// Apply domain remapping to a URL
    fn remap_domain(&self, url: &str) -> Result<String> {
        let parsed_url = url::Url::parse(url)
            .map_err(|e| DownloadError::InvalidUrl {
                url: url.to_string(),
                suggestion: "Check URL format and try again".to_string(),
                source: e
            })?;

        let host = parsed_url.host_str().unwrap_or("");

        for (original, remapped) in DOMAIN_REMAPS {
            if host == *original || host == *remapped {
                let mut new_url = parsed_url.clone();
                new_url.set_host(Some(remapped))
                    .map_err(|_| DownloadError::InvalidUrl {
                        url: url.to_string(),
                        suggestion: "Failed to remap domain".to_string(),
                        source: url::ParseError::EmptyHost
                    })?;

                return Ok(new_url.to_string());
            }
        }

        Ok(url.to_string())
    }

    /// Create HTTP request with proper headers
    fn create_request(&self, url: &str) -> Result<reqwest::RequestBuilder> {
        let remapped_url = self.remap_domain(url)?;
        let parsed_url = url::Url::parse(&remapped_url)
            .map_err(|e| DownloadError::InvalidUrl {
                url: url.to_string(),
                suggestion: "Check URL format and try again".to_string(),
                source: e
            })?;

        let mut request = self.client.get(&remapped_url);

        // Add Host header if domain was remapped
        if let Some(host) = parsed_url.host_str() {
            request = request.header("Host", host);
        }

        Ok(request)
    }

    /// Download and parse the file definition
    async fn get_file_definition(&self, base_url: &str) -> Result<FileDefinition> {
        let definition_url = format!("{}/definition.json.gz", base_url);
        let request = self.create_request(&definition_url)?;

        let response = request.send().await
            .map_err(|e| DownloadError::HttpRequest {
                url: definition_url.clone(),
                source: e
            })?;

        if !response.status().is_success() {
            return Err(DownloadError::Legacy(
                format!("HTTP error {}: Failed to fetch file definition from {}",
                       response.status(), definition_url)
            ));
        }

        let compressed_data = response.bytes().await
            .map_err(|e| DownloadError::HttpRequest {
                url: definition_url,
                source: e
            })?;

        // Decompress the gzipped definition
        let mut decoder = GzDecoder::new(&compressed_data[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)
            .map_err(|e| DownloadError::FileSystem {
                path: "definition.json.gz".into(),
                operation: FileOperation::Read,
                source: e
            })?;

        let definition: FileDefinition = serde_json::from_slice(&decompressed)
            .map_err(|e| DownloadError::FileSystem {
                path: "definition.json".into(),
                operation: FileOperation::Read,
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e)
            })?;

        Ok(definition)
    }

    /// Download a single part
    async fn download_part(&self, base_url: &str, part: &PartDefinition) -> Result<Vec<u8>> {
        let part_url = format!("{}/parts/{}", base_url, part.index);
        let request = self.create_request(&part_url)?;

        let response = request.send().await
            .map_err(|e| DownloadError::HttpRequest {
                url: part_url.clone(),
                source: e
            })?;

        if !response.status().is_success() {
            return Err(DownloadError::Legacy(
                format!("HTTP error {}: Failed to download part {} from {}",
                       response.status(), part.index, part_url)
            ));
        }

        let data = response.bytes().await
            .map_err(|e| DownloadError::HttpRequest {
                url: part_url,
                source: e
            })?;

        // Validate part hash
        let actual_hash = format!("{:x}", md5::compute(&data));
        if actual_hash != part.hash.to_lowercase() {
            return Err(DownloadError::ValidationFailed {
                file: format!("part_{}", part.index).into(),
                validation_type: ValidationType::Md5,
                expected: part.hash.clone(),
                actual: actual_hash,
                suggestion: "File may be corrupted, try downloading again".to_string()
            });
        }

        Ok(data.to_vec())
    }

    /// Download all parts and assemble the final file
    async fn download_chunked_file(
        &self,
        url: &str,
        dest_path: &Path,
        progress_callback: Option<ProgressCallback>,
        expected_size: Option<u64>
    ) -> Result<u64> {
        // Get file definition
        let definition = self.get_file_definition(url).await?;

        // Use expected size if provided, otherwise use definition size
        let total_size = expected_size.unwrap_or(definition.size);

        // Create output file
        let mut output_file = File::create(dest_path).await
            .map_err(|e| DownloadError::FileSystem {
                path: dest_path.to_path_buf(),
                operation: FileOperation::Create,
                source: e
            })?;

        // Download parts in sequence (could be parallelized in the future)
        let mut downloaded_bytes = 0u64;

        for part in &definition.parts {
            let part_data = self.download_part(url, part).await?;

            // Write part to output file at correct offset
            output_file.seek(tokio::io::SeekFrom::Start(part.offset)).await
                .map_err(|e| DownloadError::FileSystem {
                    path: dest_path.to_path_buf(),
                    operation: FileOperation::Write,
                    source: e
                })?;

            output_file.write_all(&part_data).await
                .map_err(|e| DownloadError::FileSystem {
                    path: dest_path.to_path_buf(),
                    operation: FileOperation::Write,
                    source: e
                })?;

            downloaded_bytes += part.size;

            // Report progress
            if let Some(ref callback) = progress_callback {
                callback(ProgressEvent::DownloadProgress {
                    url: url.to_string(),
                    downloaded: downloaded_bytes,
                    total: Some(total_size),
                    speed_bps: 0.0, // TODO: calculate actual speed
                });
            }
        }

        output_file.flush().await
            .map_err(|e| DownloadError::FileSystem {
                path: dest_path.to_path_buf(),
                operation: FileOperation::Write,
                source: e
            })?;

        Ok(total_size)
    }
}

impl Default for WabbajackCDNDownloader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FileDownloader for WabbajackCDNDownloader {
    async fn download(
        &self,
        request: &DownloadRequest,
        progress_callback: Option<ProgressCallback>,
    ) -> Result<DownloadResult> {
        let url = match &request.source {
            crate::downloader::core::DownloadSource::Url { url, .. } => url.clone(),
            crate::downloader::core::DownloadSource::Structured(structured_source) => {
                match structured_source {
                    crate::parse_wabbajack::sources::DownloadSource::WabbajackCDN(cdn_source) => {
                        cdn_source.url.clone()
                    },
                    _ => return Err(DownloadError::Configuration {
                        message: "WabbajackCDN downloader only supports WabbajackCDN structured sources".to_string(),
                        field: None,
                        suggestion: None,
                    }),
                }
            }
        };

        let dest_path = &request.destination;

        // Check if file already exists and is valid
        if let Some(result) = self.check_existing_file(dest_path, &request.validation, progress_callback.clone()).await? {
            return Ok(result);
        }

        // Create destination directory if it doesn't exist
        if let Some(parent) = dest_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| DownloadError::FileSystem {
                    path: parent.to_path_buf(),
                    operation: FileOperation::CreateDir,
                    source: e
                })?;
        }

        // Download the chunked file
        let final_size = self.download_chunked_file(&url, dest_path, progress_callback, request.expected_size).await?;

        Ok(DownloadResult::Downloaded {
            size: final_size
        })
    }

    async fn download_helper(
        &self,
        url: &str,
        dest_path: &Path,
        progress_callback: Option<ProgressCallback>,
        expected_size: Option<u64>,
    ) -> Result<u64> {
        self.download_chunked_file(url, dest_path, progress_callback, expected_size).await
    }

    async fn check_existing_file(
        &self,
        dest_path: &Path,
        _validation: &FileValidation,
        _progress_callback: Option<ProgressCallback>,
    ) -> Result<Option<DownloadResult>> {
        if !dest_path.exists() {
            return Ok(None);
        }

        // TODO: Implement proper validation for existing files
        // For now, assume existing files are valid
        let metadata = std::fs::metadata(dest_path)
            .map_err(|e| DownloadError::FileSystem {
                path: dest_path.to_path_buf(),
                operation: FileOperation::Metadata,
                source: e
            })?;

        Ok(Some(DownloadResult::AlreadyExists {
            size: metadata.len()
        }))
    }

    fn supports_url(&self, url: &str) -> bool {
        if let Ok(parsed_url) = url::Url::parse(url) {
            if let Some(host) = parsed_url.host_str() {
                return DOMAIN_REMAPS.iter().any(|(original, remapped)| {
                    host == *original || host == *remapped
                });
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_remapping() {
        let downloader = WabbajackCDNDownloader::new();

        // Test domain remapping
        let original_url = "https://wabbajack.b-cdn.net/test/file";
        let remapped = downloader.remap_domain(original_url).unwrap();
        assert_eq!(remapped, "https://authored-files.wabbajack.org/test/file");

        // Test URL that doesn't need remapping
        let normal_url = "https://example.com/test/file";
        let not_remapped = downloader.remap_domain(normal_url).unwrap();
        assert_eq!(not_remapped, normal_url);
    }

    #[test]
    fn test_supports_url() {
        let downloader = WabbajackCDNDownloader::new();

        // Should support Wabbajack CDN domains
        assert!(downloader.supports_url("https://wabbajack.b-cdn.net/test"));
        assert!(downloader.supports_url("https://authored-files.wabbajack.org/test"));
        assert!(downloader.supports_url("https://mirror.wabbajack.org/test"));

        // Should not support other domains
        assert!(!downloader.supports_url("https://example.com/test"));
        assert!(!downloader.supports_url("https://google.com/test"));
    }

    #[test]
    fn test_file_definition_deserialization() {
        let json = r#"{
            "MungedName": "test_file",
            "Hash": "abcd1234",
            "Size": 1024,
            "Parts": [
                {
                    "Index": 0,
                    "Size": 512,
                    "Hash": "hash1",
                    "Offset": 0
                },
                {
                    "Index": 1,
                    "Size": 512,
                    "Hash": "hash2",
                    "Offset": 512
                }
            ]
        }"#;

        let definition: FileDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(definition.munged_name, "test_file");
        assert_eq!(definition.size, 1024);
        assert_eq!(definition.parts.len(), 2);
        assert_eq!(definition.parts[0].index, 0);
        assert_eq!(definition.parts[1].offset, 512);
    }
}
