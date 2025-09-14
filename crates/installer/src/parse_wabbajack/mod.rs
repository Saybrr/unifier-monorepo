//! Wabbajack modlist parsing and operation generation
//!
//! This module handles parsing Wabbajack modlist JSON files and converting
//! them into structured download operations that the downloader can process.
//!
//! The key insight is that we want to parse the JSON into structured types
//! rather than converting to URL strings, as this provides better type safety,
//! performance, and allows for richer data representation.

pub mod operations;
pub mod parser;

// Re-export main types
pub use crate::downloader::sources::{DownloadSource, HttpSource, NexusSource, GameFileSource, ManualSource, ArchiveSource};
pub use operations::{ArchiveManifest, ManifestMetadata};
pub use parser::{parse_modlist, ModlistParser};
