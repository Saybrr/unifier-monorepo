//! Authentication modules for various download sources

pub mod nexus_api;

// Re-export common authentication types
pub use nexus_api::{NexusAPI, UserValidation, NexusMod, NexusFile, NexusDownloadLink};
