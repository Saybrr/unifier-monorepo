//! Installation module
//!
//! This module handles the installation of files based on directives from modlists.

pub mod directives;
pub mod error;
pub mod lib;
pub mod vfs;

// Re-export commonly used types
pub use directives::{Directive, FromArchive, PatchedFromArchive, InlineFileDirective};
pub use error::InstallError;
