//! Download operation types and archive manifest
//!
//! This module defines the structured operations that result from parsing
//! a Wabbajack modlist and can be processed by the downloader system.

use crate::parse_wabbajack::sources::DownloadSource;

/// A complete download operation with all metadata
///
/// This represents a single file that needs to be obtained, along with
/// all the information needed to download, validate, and store it.
#[derive(Debug, Clone)]
pub struct DownloadOperation {
    /// The structured download source (HTTP, Nexus, etc.)
    pub source: DownloadSource,
    /// Final filename for the downloaded file
    pub filename: String,
    /// Expected file hash for validation
    pub expected_hash: String,
    /// Hash algorithm used (e.g., "SHA256", "MD5")
    pub hash_algorithm: String,
    /// Priority for download ordering (lower = higher priority)
    pub priority: u32,
    /// Optional metadata for display/logging purposes
    pub metadata: OperationMetadata,
}

/// Additional metadata for a download operation
#[derive(Debug, Clone, Default)]
pub struct OperationMetadata {
    /// Human-readable description
    pub description: String,
    /// Category/group this operation belongs to
    pub category: String,
    /// Whether this is required or optional
    pub required: bool,
    /// Tags for filtering/grouping
    pub tags: Vec<String>,
}

/// Complete manifest of all download operations from a modlist
///
/// This represents the parsed and processed modlist with all archives
/// converted to structured download operations.
#[derive(Debug)]
pub struct ArchiveManifest {
    /// All download operations to perform
    pub operations: Vec<DownloadOperation>,
    /// Manifest metadata
    pub metadata: ManifestMetadata,
    /// Operation statistics
    pub stats: ManifestStats,
}

/// Metadata about the entire manifest
#[derive(Debug, Clone, Default)]
pub struct ManifestMetadata {
    /// Modlist name
    pub name: String,
    /// Modlist version
    pub version: String,
    /// Modlist author
    pub author: String,
    /// Game the modlist is for
    pub game: String,
    /// Optional description
    pub description: String,
}

/// Statistics about the manifest operations
#[derive(Debug, Default)]
pub struct ManifestStats {
    /// Total number of operations
    pub total_operations: usize,
    /// Operations by source type
    pub http_operations: usize,
    pub nexus_operations: usize,
    pub gamefile_operations: usize,
    pub manual_operations: usize,
    pub archive_operations: usize,
    pub wabbajack_cdn_operations: usize,
    /// Total expected download size in bytes
    pub total_download_size: u64,
    /// Operations requiring user interaction
    pub user_interaction_required: usize,
    /// Operations requiring external dependencies
    pub external_dependencies_required: usize,
}

impl DownloadOperation {
    /// Create a new download operation
    pub fn new<S: Into<String>>(
        source: DownloadSource,
        filename: S,
        expected_hash: S,
    ) -> Self {
        Self {
            source,
            filename: filename.into(),
            expected_hash: expected_hash.into(),
            hash_algorithm: "SHA256".to_string(), // Default to SHA256
            priority: 0,
            metadata: OperationMetadata::default(),
        }
    }

    /// Set the hash algorithm and return self for chaining
    pub fn with_hash_algorithm<S: Into<String>>(mut self, algorithm: S) -> Self {
        self.hash_algorithm = algorithm.into();
        self
    }

    /// Set the priority and return self for chaining
    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    /// Set metadata and return self for chaining
    pub fn with_metadata(mut self, metadata: OperationMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Check if this operation requires user interaction
    pub fn requires_user_interaction(&self) -> bool {
        self.source.requires_user_interaction()
    }

    /// Check if this operation requires external dependencies
    pub fn requires_external_dependencies(&self) -> bool {
        self.source.requires_external_dependencies()
    }

}

impl ArchiveManifest {
    /// Create a new empty manifest
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
            metadata: ManifestMetadata::default(),
            stats: ManifestStats::default(),
        }
    }

    /// Add an operation to the manifest
    pub fn add_operation(&mut self, operation: DownloadOperation) {
        self.operations.push(operation);
        self.update_stats();
    }

    /// Add multiple operations to the manifest
    pub fn add_operations(&mut self, operations: Vec<DownloadOperation>) {
        self.operations.extend(operations);
        self.update_stats();
    }

    /// Update statistics based on current operations
    pub fn update_stats(&mut self) {
        let mut stats = ManifestStats::default();

        stats.total_operations = self.operations.len();
        // Note: total_download_size removed since we don't have reliable size info from Wabbajack
        stats.total_download_size = 0;

        for operation in &self.operations {
            match &operation.source {
                DownloadSource::Http(_) => stats.http_operations += 1,
                DownloadSource::Nexus(_) => stats.nexus_operations += 1,
                DownloadSource::GameFile(_) => stats.gamefile_operations += 1,
                DownloadSource::Manual(_) => stats.manual_operations += 1,
                DownloadSource::Archive(_) => stats.archive_operations += 1,
                DownloadSource::WabbajackCDN(_) => stats.wabbajack_cdn_operations += 1,
            }

            if operation.requires_user_interaction() {
                stats.user_interaction_required += 1;
            }

            if operation.requires_external_dependencies() {
                stats.external_dependencies_required += 1;
            }
        }

        self.stats = stats;
    }

    /// Get operations that can be downloaded automatically
    pub fn automatic_operations(&self) -> Vec<&DownloadOperation> {
        self.operations.iter()
            .filter(|op| !op.requires_user_interaction())
            .collect()
    }

    /// Get operations that require user interaction
    pub fn manual_operations(&self) -> Vec<&DownloadOperation> {
        self.operations.iter()
            .filter(|op| op.requires_user_interaction())
            .collect()
    }

    /// Get operations sorted by priority (lower priority number = higher priority)
    pub fn operations_by_priority(&self) -> Vec<&DownloadOperation> {
        let mut ops: Vec<_> = self.operations.iter().collect();
        ops.sort_by_key(|op| op.priority);
        ops
    }

}

impl Default for ArchiveManifest {
    fn default() -> Self {
        Self::new()
    }
}

impl ManifestStats {
    /// Get total download size in human readable format
    pub fn total_download_size_human(&self) -> String {
        let size = self.total_download_size as f64;
        if size < 1024.0 {
            format!("{} B", size)
        } else if size < 1024.0 * 1024.0 {
            format!("{:.1} KB", size / 1024.0)
        } else if size < 1024.0 * 1024.0 * 1024.0 {
            format!("{:.1} MB", size / (1024.0 * 1024.0))
        } else {
            format!("{:.1} GB", size / (1024.0 * 1024.0 * 1024.0))
        }
    }

    /// Get percentage of operations that are automatic vs manual
    pub fn automation_percentage(&self) -> f64 {
        if self.total_operations == 0 {
            return 0.0;
        }

        let automatic = self.total_operations - self.user_interaction_required;
        (automatic as f64 / self.total_operations as f64) * 100.0
    }
}
