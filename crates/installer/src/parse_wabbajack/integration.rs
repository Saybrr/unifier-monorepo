//! Integration between parse_wabbajack and downloader modules
//!
//! This module provides conversion functions to transform parsed modlist
//! operations into download requests that the downloader system can process.

use crate::parse_wabbajack::operations::{DownloadOperation, ArchiveManifest};
use crate::downloader::core::{DownloadRequest, FileValidation};
use std::path::PathBuf;

/// Convert a DownloadOperation to a DownloadRequest
///
/// This function bridges the gap between the parsed modlist operations
/// and the downloader system, converting structured operations into
/// the format expected by the downloader.
pub fn operation_to_download_request(
    operation: &DownloadOperation,
    base_destination: &PathBuf,
) -> DownloadRequest {
    let destination = base_destination.join(&operation.filename);
    let parent_dir = destination.parent()
        .unwrap_or(base_destination)
        .to_path_buf();

    // Create validation requirements based on operation metadata
    let mut validation = FileValidation::new();

    // Add hash validation if we have a hash
    if !operation.expected_hash.is_empty() {
        validation = match operation.hash_algorithm.to_uppercase().as_str() {
            "MD5" => validation.with_md5(operation.expected_hash.clone()),
            "SHA256" => validation.with_sha256(operation.expected_hash.clone()),
            "CRC32" => {
                // Try to parse as hex, fallback to parsing as decimal
                let crc32_value = u32::from_str_radix(&operation.expected_hash, 16)
                    .or_else(|_| operation.expected_hash.parse::<u32>())
                    .unwrap_or(0);
                validation.with_crc32(crc32_value)
            },
            _ => {
                // Unknown hash algorithm, just use size validation
                validation
            }
        };
    }

    DownloadRequest::new(operation.source.clone(), parent_dir)
        .with_filename(operation.filename.clone())
        .with_validation(validation)
        .with_expected_size(operation.expected_size)
}

/// Convert multiple DownloadOperations to DownloadRequests
///
/// This is a convenience function that converts an entire list of operations
/// to download requests, filtering out any operations that require user interaction.
pub fn operations_to_download_requests(
    operations: &[DownloadOperation],
    base_destination: &PathBuf,
    include_manual: bool,
) -> Vec<DownloadRequest> {
    operations.iter()
        .filter(|op| include_manual || !op.requires_user_interaction())
        .map(|op| operation_to_download_request(op, base_destination))
        .collect()
}

/// Convert an entire ArchiveManifest to DownloadRequests
///
/// This function processes a complete modlist manifest and returns
/// download requests for all automatic operations (filtering out manual ones by default).
pub fn manifest_to_download_requests(
    manifest: &ArchiveManifest,
    base_destination: &PathBuf,
    include_manual: bool,
) -> Vec<DownloadRequest> {
    operations_to_download_requests(&manifest.operations, base_destination, include_manual)
}

/// Get download requests sorted by priority
///
/// This function converts operations to download requests and sorts them
/// by priority (lower number = higher priority).
pub fn manifest_to_prioritized_download_requests(
    manifest: &ArchiveManifest,
    base_destination: &PathBuf,
    include_manual: bool,
) -> Vec<DownloadRequest> {
    let mut operations: Vec<_> = manifest.operations.iter().collect();
    operations.sort_by_key(|op| op.priority);

    operations.iter()
        .filter(|op| include_manual || !op.requires_user_interaction())
        .map(|op| operation_to_download_request(op, base_destination))
        .collect()
}

/// Statistics about conversion from manifest to download requests
#[derive(Debug, Default)]
pub struct ConversionStats {
    pub total_operations: usize,
    pub converted_requests: usize,
    pub skipped_manual: usize,
    pub total_download_size: u64,
    pub operations_by_source: std::collections::HashMap<String, usize>,
}

/// Convert manifest to download requests with detailed statistics
pub fn manifest_to_download_requests_with_stats(
    manifest: &ArchiveManifest,
    base_destination: &PathBuf,
    include_manual: bool,
) -> (Vec<DownloadRequest>, ConversionStats) {
    let mut stats = ConversionStats::default();
    stats.total_operations = manifest.operations.len();

    let mut requests = Vec::new();

    for operation in &manifest.operations {
        // Update source statistics
        let source_type = match &operation.source {
            crate::parse_wabbajack::sources::DownloadSource::Http(_) => "HTTP",
            crate::parse_wabbajack::sources::DownloadSource::Nexus(_) => "Nexus",
            crate::parse_wabbajack::sources::DownloadSource::GameFile(_) => "GameFile",
            crate::parse_wabbajack::sources::DownloadSource::Manual(_) => "Manual",
            crate::parse_wabbajack::sources::DownloadSource::Archive(_) => "Archive",
            crate::parse_wabbajack::sources::DownloadSource::WabbajackCDN(_) => "WabbajackCDN",
        };

        *stats.operations_by_source.entry(source_type.to_string()).or_insert(0) += 1;

        // Skip manual operations if not including them
        if !include_manual && operation.requires_user_interaction() {
            stats.skipped_manual += 1;
            continue;
        }

        // Convert to download request
        let request = operation_to_download_request(operation, base_destination);
        requests.push(request);
        stats.converted_requests += 1;
        // Note: total_download_size not accumulated since we don't have reliable size info
    }

    (requests, stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_wabbajack::{
        sources::{DownloadSource, HttpSource},
        operations::DownloadOperation
    };

    #[test]
    fn test_operation_to_download_request() {
        let http_source = HttpSource::new("https://example.com/file.zip");
        let source = DownloadSource::Http(http_source);

        let operation = DownloadOperation::new(
            source,
            "test-file.zip",
            "abcd1234",
            1024, // Test file size
        )
        .with_hash_algorithm("SHA256");

        let base_destination = PathBuf::from("/downloads");
        let request = operation_to_download_request(&operation, &base_destination);

        assert_eq!(request.filename, Some("test-file.zip".to_string()));
        // All sources are now structured, so no need for this check
        // Note: expected_size removed since we don't set it from DownloadOperation anymore
        assert_eq!(request.validation.expected_size, None);
    }

    #[test]
    fn test_operations_to_download_requests_filtering() {
        let http_source = HttpSource::new("https://example.com/file.zip");
        let auto_op = DownloadOperation::new(
            DownloadSource::Http(http_source),
            "auto.zip",
            "hash1",
            2048, // Test file size
        );

        let manual_op = DownloadOperation::new(
            DownloadSource::Manual(crate::parse_wabbajack::sources::ManualSource {
                instructions: "Download manually".to_string(),
                url: None,
            }),
            "manual.zip",
            "hash2",
            4096, // Test file size
        );

        let operations = vec![auto_op, manual_op];
        let base_destination = PathBuf::from("/downloads");

        // Without manual operations
        let requests_auto_only = operations_to_download_requests(&operations, &base_destination, false);
        assert_eq!(requests_auto_only.len(), 1);

        // With manual operations
        let requests_all = operations_to_download_requests(&operations, &base_destination, true);
        assert_eq!(requests_all.len(), 2);
    }
}
