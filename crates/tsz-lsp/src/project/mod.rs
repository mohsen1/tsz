//! Project container for multi-file LSP operations.
//!
//! This provides a lightweight home for parsed files, binders, and line maps so
//! LSP features can be extended across multiple files.

mod core;
pub(crate) mod eviction;
pub(crate) mod features;
pub(crate) mod file_context;
pub(crate) mod imports;
pub(crate) mod module_specifiers;
pub(crate) mod operations;

#[cfg(test)]
pub(crate) use self::core::FileIdAllocator;
pub(crate) use self::core::{
    ExportMatch, ImportKind, ImportSpecifierTarget, ImportTarget, NamespaceReexportTarget,
};
pub use self::core::{
    FileRename, FileResidencyInfo, Project, ProjectFile, ProjectPerformance, ProjectRequestKind,
    ProjectRequestTiming, ProjectResidencyStats, TsConfigSettings,
};
pub use self::eviction::{EvictedFile, EvictionResult};
pub use self::file_context::{LspMinimalProviderContext, LspProviderContext};
