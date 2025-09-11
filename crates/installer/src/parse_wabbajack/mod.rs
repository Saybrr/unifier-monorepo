//! Wabbajack modlist parsing and operation generation
//!
//! This module handles parsing Wabbajack modlist JSON files and converting
//! them into structured download operations that the downloader can process.
//!
//! The key insight is that we want to parse the JSON into structured types
//! rather than converting to URL strings, as this provides better type safety,
//! performance, and allows for richer data representation.

pub mod sources;
pub mod operations;
pub mod parser;
pub mod integration;

// Re-export main types
pub use sources::{DownloadSource, HttpSource, NexusSource, GameFileSource, ManualSource, ArchiveSource};
pub use operations::{DownloadOperation, ArchiveManifest, OperationMetadata, ManifestMetadata};
pub use parser::{parse_modlist, ModlistParser};
pub use integration::{
    operation_to_download_request, operations_to_download_requests,
    manifest_to_download_requests, manifest_to_prioritized_download_requests,
    manifest_to_download_requests_with_stats, ConversionStats
};
