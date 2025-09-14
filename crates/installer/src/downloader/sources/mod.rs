//! Download source implementations
//!
//! This module contains the actual implementation logic for different download sources.
//! The data types are defined in `sources.rs`.

pub mod sources;
pub mod http;
pub mod wabbajack_cdn;
pub mod gamefile;
pub mod nexus;
pub mod manual;
pub mod archive;
pub mod unknown;

// Re-export the types for cleaner imports
pub use sources::*;
