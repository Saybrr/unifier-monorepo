//! Download source implementations
//!
//! This module contains the actual implementation logic for different download sources.
//! The data types are defined in `crate::parse_wabbajack::sources`.

pub mod http;
pub mod wabbajack_cdn;
pub mod gamefile;
pub mod nexus;
pub mod manual;
pub mod archive;
pub mod unknown;

// Implementation modules (no re-exports needed since implementations are done via traits)
