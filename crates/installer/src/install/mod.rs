//! Installation module
//!
//! This module handles the installation of files based on directives from modlists.

pub mod directives;
pub mod error;

// Re-export commonly used types
pub use directives::{Directive, FromArchiveDirective, PatchedFromArchiveDirective, InlineFileDirective};
pub use error::InstallError;
