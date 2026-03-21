//! Definition identifiers and resolution for the solver.
//!
//! This module groups the DefId system:
//! - `core`: DefId allocation, `DefinitionStore`, `DefKind`, `DefInfo`
//! - `resolver`: `TypeResolver` trait and `TypeEnvironment`
//! - `incremental`: File-change coordination and definition invalidation

mod core;
pub mod incremental;
pub mod resolver;

pub use self::core::*;
pub use self::incremental::{FileChange, FileChangeSet, InvalidationSummary, diff_fingerprints};
