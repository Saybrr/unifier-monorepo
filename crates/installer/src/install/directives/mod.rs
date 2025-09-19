//! Installation directive implementations
//!
//! This module contains individual directive types and their implementations.
//! Each directive type is defined in its own file along with its execute method.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use crate::install::error::InstallError;
use crate::install::vfs::VfsContext;

// Individual directive type modules
pub mod from_archive;
pub mod patched_from_archive;
pub mod inline_file;
pub mod remapped_inline_file;
pub mod transformed_texture;
pub mod create_bsa;
pub mod merged_patch;
pub mod property_file;
pub mod archive_meta;
pub mod ignored_directly;
pub mod no_match;
pub mod test_directive;

// Re-export all directive types for cleaner imports
pub use from_archive::FromArchive;
pub use patched_from_archive::PatchedFromArchive;
pub use inline_file::InlineFileDirective;
pub use remapped_inline_file::RemappedInlineFile;
pub use transformed_texture::TransformedTextureDirective;
pub use create_bsa::CreateBSADirective;
pub use merged_patch::{MergedPatchDirective, SourcePatch};
pub use property_file::{PropertyFileDirective, PropertyType};
pub use archive_meta::ArchiveMetaDirective;
pub use ignored_directly::IgnoredDirectlyDirective;
pub use no_match::NoMatchDirective;
pub use test_directive::TestDirective;


/// Unified directive enum for type-safe directive processing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "crate::parse_wabbajack::parser::Directive")]
pub enum Directive {
    FromArchive(FromArchive),
    PatchedFromArchive(PatchedFromArchive),
    InlineFile(InlineFileDirective),
    RemappedInlineFile(RemappedInlineFile),
    TransformedTexture(TransformedTextureDirective),
    CreateBSA(CreateBSADirective),
    MergedPatch(MergedPatchDirective),
    PropertyFile(PropertyFileDirective),
    ArchiveMeta(ArchiveMetaDirective),
    IgnoredDirectly(IgnoredDirectlyDirective),
    NoMatch(NoMatchDirective),
    Test(TestDirective),
}

impl Directive {
    /// Get the destination path for any directive type
    pub async fn execute(&self, install_dir: &Arc<PathBuf>,
        extracted_modlist_dir: &Arc<PathBuf>,
        downloads_dir: &Arc<PathBuf>,
        vfs_context: Arc<VfsContext>,
        progress_callback: Option<Box<dyn Fn(u64, u64) + Send + Sync>>,
        ) -> Result<(), InstallError> {

        match self {
            Directive::FromArchive(d) => d.execute(install_dir, extracted_modlist_dir, vfs_context, progress_callback).await,
            Directive::PatchedFromArchive(d) => d.execute(install_dir, vfs_context, extracted_modlist_dir, progress_callback).await,
            Directive::InlineFile(d) => d.execute(install_dir, extracted_modlist_dir, progress_callback).await,
            Directive::RemappedInlineFile(d) => d.execute(install_dir, extracted_modlist_dir, downloads_dir, progress_callback).await,
            Directive::TransformedTexture(d) => d.execute(install_dir, vfs_context, progress_callback).await,
            Directive::CreateBSA(d) => d.execute(install_dir, extracted_modlist_dir, progress_callback).await,
            Directive::MergedPatch(d) => d.execute(install_dir, extracted_modlist_dir, progress_callback).await,
            Directive::PropertyFile(d) => d.execute(install_dir, extracted_modlist_dir, progress_callback).await,
            Directive::ArchiveMeta(d) => d.execute(install_dir, extracted_modlist_dir, progress_callback).await,
            Directive::IgnoredDirectly(d) => d.execute(install_dir, progress_callback).await,
            Directive::NoMatch(d) => d.execute(install_dir, progress_callback).await,
            Directive::Test(d) => d.execute(install_dir, extracted_modlist_dir, progress_callback, vfs_context).await,
        }

    }
    pub fn to(&self) -> &str {
        match self {
            Directive::FromArchive(d) => &d.to,
            Directive::PatchedFromArchive(d) => &d.to,
            Directive::InlineFile(d) => &d.to,
            Directive::RemappedInlineFile(d) => &d.to,
            Directive::TransformedTexture(d) => &d.to,
            Directive::CreateBSA(d) => &d.to,
            Directive::MergedPatch(d) => &d.to,
            Directive::PropertyFile(d) => &d.to,
            Directive::ArchiveMeta(d) => &d.to,
            Directive::IgnoredDirectly(d) => &d.to,
            Directive::NoMatch(d) => &d.to,
            Directive::Test(d) => &d.to,
        }
    }

    /// Get the content hash for any directive type
    pub fn hash(&self) -> &str {
        match self {
            Directive::FromArchive(d) => &d.hash,
            Directive::PatchedFromArchive(d) => &d.hash,
            Directive::InlineFile(d) => &d.hash,
            Directive::RemappedInlineFile(d) => &d.hash,
            Directive::TransformedTexture(d) => &d.hash,
            Directive::CreateBSA(d) => &d.hash,
            Directive::MergedPatch(d) => &d.hash,
            Directive::PropertyFile(d) => &d.hash,
            Directive::ArchiveMeta(d) => &d.hash,
            Directive::IgnoredDirectly(d) => &d.hash,
            Directive::NoMatch(d) => &d.hash,
            Directive::Test(d) => &d.hash,
        }
    }

    /// Get the file size for any directive type
    pub fn size(&self) -> u64 {
        match self {
            Directive::FromArchive(d) => d.size,
            Directive::PatchedFromArchive(d) => d.size,
            Directive::InlineFile(d) => d.size,
            Directive::RemappedInlineFile(d) => d.size,
            Directive::TransformedTexture(d) => d.size,
            Directive::CreateBSA(d) => d.size,
            Directive::MergedPatch(d) => d.size,
            Directive::PropertyFile(d) => d.size,
            Directive::ArchiveMeta(d) => d.size,
            Directive::IgnoredDirectly(d) => d.size,
            Directive::NoMatch(d) => d.size,
            Directive::Test(d) => d.size,
        }
    }

    // /// Check if this directive requires VFS (archive-based installation)
    // pub fn requires_vfs(&self) -> bool {
    //     matches!(self,
    //         Directive::FromArchive(_) |
    //         Directive::PatchedFromArchive(_) |
    //         Directive::TransformedTexture(_) |
    //         Directive::Test(_)
    //     )
    // }

    // /// Check if this directive is an inline file (embedded data)
    // pub fn is_inline(&self) -> bool {
    //     matches!(self,
    //         Directive::InlineFile(_) |
    //         Directive::RemappedInlineFile(_) |
    //         Directive::PropertyFile(_) |
    //         Directive::ArchiveMeta(_) |
    //         Directive::Test(_)
    //     )
    // }

    // /// Check if this directive should be processed during installation
    // pub fn should_install(&self) -> bool {
    //     !matches!(self,
    //         Directive::IgnoredDirectly(_) |
    //         Directive::NoMatch(_) |
    //         Directive::Test(_)
    //     )
    // }
}


// Conversion from parser directive to installer directive

impl From<crate::parse_wabbajack::parser::Directive> for Directive {
    fn from(parser_directive: crate::parse_wabbajack::parser::Directive) -> Self {
        use crate::parse_wabbajack::parser::Directive as PD;

        match parser_directive {
            PD::FromArchive(directive) => {
                Directive::FromArchive(directive)
            },
            PD::PatchedFromArchive(directive) => {
                Directive::PatchedFromArchive(directive)
            },
            PD::InlineFile(directive) => {
                Directive::InlineFile(directive)
            },
            PD::RemappedInlineFile(directive) => {
                Directive::RemappedInlineFile(directive)
            },
            PD::TransformedTexture(directive) => {
                Directive::TransformedTexture(directive)
            },
            PD::CreateBSA(directive) => {
                Directive::CreateBSA(directive)
            },
            PD::MergedPatch(directive) => {
                Directive::MergedPatch(directive)
            },
            PD::PropertyFile(directive) => {
                Directive::PropertyFile(directive)
            },
            PD::ArchiveMeta(directive) => {
                Directive::ArchiveMeta(directive)
            },
            PD::IgnoredDirectly(directive) => {
                Directive::IgnoredDirectly(directive)
            },
            PD::NoMatch(directive) => {
                Directive::NoMatch(directive)
            },
        }
    }
}
