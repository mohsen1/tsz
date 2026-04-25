//! Type aliases and supporting types used across the checker context.
//!
//! Cross-binder index shapes, module-resolution caches, and the
//! `ResolutionError` / `ResolutionModeOverride` helpers they depend on. Kept
//! in one file so the `pub type`/helper-type surface doesn't dilute `mod.rs`.

use rustc_hash::FxHashMap;
use std::sync::Arc;

use tsz_binder::{ModuleAugmentation, SymbolId};

/// Represents a failed module resolution with specific error details.
#[derive(Clone, Debug)]
pub struct ResolutionError {
    pub code: u32,
    pub message: String,
}

/// Explicit module-resolution override carried by import attributes / import types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResolutionModeOverride {
    Import,
    Require,
}

/// Global cross-binder index: identifier name → list of `(file_idx, SymbolId)`
/// where the name appears in a binder's `file_locals`.
pub type GlobalFileLocalsIndex = Arc<FxHashMap<String, Vec<(usize, SymbolId)>>>;

/// Per-module export map: export name → list of `(file_idx, SymbolId)` where
/// the export is declared. The value shape inside a `GlobalModuleExportsIndex`.
pub type ModuleExportsByName = FxHashMap<String, Vec<(usize, SymbolId)>>;

/// Owned (non-`Arc`) form of the cross-binder module exports index.
/// Used while the index is being built before it is wrapped in `Arc`.
pub type ModuleExportsIndexMap = FxHashMap<String, ModuleExportsByName>;

/// Global cross-binder index: module specifier → export name → list of
/// `(file_idx, SymbolId)` where the export is declared.
pub type GlobalModuleExportsIndex = Arc<ModuleExportsIndexMap>;

/// Global cross-binder index: module specifier → list of `(file_idx, augmentation)`
/// entries that contribute to that module's merged type.
pub type GlobalModuleAugmentationsIndex = Arc<FxHashMap<String, Vec<(usize, ModuleAugmentation)>>>;

/// Global cross-binder index: module specifier → list of `(symbol, file_idx)`
/// identifying the symbols targeted by each augmentation of that module.
pub type GlobalAugmentationTargetsIndex = Arc<FxHashMap<String, Vec<(SymbolId, usize)>>>;

pub type ResolvedModulePathMap = FxHashMap<(usize, String), usize>;
pub type ResolvedModuleErrorMap = FxHashMap<(usize, String), ResolutionError>;
pub type ResolvedModuleRequestPathMap =
    FxHashMap<(usize, String, Option<ResolutionModeOverride>), usize>;
pub type ResolvedModuleRequestErrorMap =
    FxHashMap<(usize, String, Option<ResolutionModeOverride>), ResolutionError>;

/// Per-`(source_file_idx, specifier)` flag mirroring tsc's
/// `resolvedUsingTsExtension`: `true` when the resolver consumed a TS source
/// extension from the specifier via a literal package.json `exports`/`imports`
/// key (e.g. `"./*.ts"` or `"#foo.ts"`). Used by the import-extension gate
/// (TS2877) to suppress the warning when the package author opted into the
/// `.ts` mapping.
pub type ResolvedModuleTsExtensionMap = FxHashMap<(usize, String), bool>;

/// Program-wide type-only wildcard re-exports map: module specifier → entries of
/// (re-exported module specifier, is-type-only flag). Mirrors
/// `tsz_binder::Binder::wildcard_reexports_type_only` but wrapped in `Arc` so
/// cross-file lookup binders can share one allocation.
pub type ProgramWildcardReexportsTypeOnly = Arc<FxHashMap<String, Vec<(String, bool)>>>;
