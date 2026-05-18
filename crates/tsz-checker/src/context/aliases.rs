//! Type aliases and supporting types used across the checker context.
//!
//! Cross-binder index shapes, module-resolution caches, and the
//! `ResolutionError` / `ResolutionModeOverride` helpers they depend on. Kept
//! in one file so the `pub type`/helper-type surface doesn't dilute `mod.rs`.

use rustc_hash::FxHashMap;
use std::sync::Arc;

use tsz_binder::{ModuleAugmentation, SymbolId, SymbolTable};

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

/// Syntactic request kind used by the driver when resolving a module specifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResolutionRequestKind {
    EsmImport,
    DynamicImport,
    CjsRequire,
    EsmReExport,
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

/// Per-checker cache: (requesting file, module specifier) → resolved cross-file namespace exports.
pub type NamespaceExportsCache = FxHashMap<(usize, String), Option<SymbolTable>>;

#[must_use]
pub(crate) fn namespace_exports_cache_entries(cache: &NamespaceExportsCache) -> usize {
    cache.len()
}

#[must_use]
pub(crate) fn namespace_exports_cache_estimated_size_bytes(cache: &NamespaceExportsCache) -> usize {
    let mut size = cache.capacity()
        * (std::mem::size_of::<(usize, String)>() + std::mem::size_of::<Option<SymbolTable>>() + 8);
    for ((_, specifier), table) in cache {
        size += specifier.capacity();
        if let Some(table) = table {
            size += symbol_table_estimated_size_bytes(table);
        }
    }
    size
}

/// Per-checker positive cache for named exports reached through `export=`.
/// Keyed by `(current_file_idx, module_specifier, export_name)`.
pub type ExportEqualsNamedCache = FxHashMap<(usize, String, String), Option<SymbolId>>;

#[must_use]
pub(crate) fn export_equals_named_cache_entries(cache: &ExportEqualsNamedCache) -> usize {
    cache.len()
}

#[must_use]
pub(crate) fn export_equals_named_cache_estimated_size_bytes(
    cache: &ExportEqualsNamedCache,
) -> usize {
    let mut size = cache.capacity()
        * (std::mem::size_of::<(usize, String, String)>()
            + std::mem::size_of::<Option<SymbolId>>()
            + 8);
    for ((_, specifier, export_name), _) in cache {
        size += specifier.capacity() + export_name.capacity();
    }
    size
}

/// Per-checker cache: nested namespace name → candidate `(file_idx, SymbolId)` entries.
pub type NestedNamespaceCandidatesCache = FxHashMap<String, Vec<(usize, SymbolId)>>;

#[must_use]
pub(crate) fn nested_namespace_candidates_cache_entries(
    cache: &NestedNamespaceCandidatesCache,
) -> usize {
    cache.len()
}

#[must_use]
pub(crate) fn nested_namespace_candidates_cache_estimated_size_bytes(
    cache: &NestedNamespaceCandidatesCache,
) -> usize {
    let mut size = cache.capacity()
        * (std::mem::size_of::<String>() + std::mem::size_of::<Vec<(usize, SymbolId)>>() + 8);
    for (namespace, candidates) in cache {
        size += namespace.capacity();
        size += candidates.capacity() * std::mem::size_of::<(usize, SymbolId)>();
    }
    size
}

/// Per-checker cache: namespace name → member name → resolved cross-binder symbol.
pub type NamespaceMemberResolutionCache = FxHashMap<String, FxHashMap<String, Option<SymbolId>>>;

#[must_use]
pub(crate) fn namespace_member_resolution_cache_entries(
    cache: &NamespaceMemberResolutionCache,
) -> usize {
    cache.values().map(FxHashMap::len).sum()
}

#[must_use]
pub(crate) fn namespace_member_resolution_cache_estimated_size_bytes(
    cache: &NamespaceMemberResolutionCache,
) -> usize {
    let mut size = cache.capacity()
        * (std::mem::size_of::<String>()
            + std::mem::size_of::<FxHashMap<String, Option<SymbolId>>>()
            + 8);
    for (namespace, members) in cache {
        size += namespace.capacity();
        size += members.capacity()
            * (std::mem::size_of::<String>() + std::mem::size_of::<Option<SymbolId>>() + 8);
        for member in members.keys() {
            size += member.capacity();
        }
    }
    size
}

fn symbol_table_estimated_size_bytes(table: &SymbolTable) -> usize {
    let mut size = std::mem::size_of::<SymbolTable>();
    for (name, _) in table.iter() {
        size += name.capacity() + std::mem::size_of::<SymbolId>();
    }
    size
}

/// Global cross-binder index: module specifier → list of `(file_idx, augmentation)`
/// entries that contribute to that module's merged type.
pub type GlobalModuleAugmentationsIndex = Arc<FxHashMap<String, Vec<(usize, ModuleAugmentation)>>>;

/// Global cross-binder index: module specifier → list of `(symbol, file_idx)`
/// identifying the symbols targeted by each augmentation of that module.
pub type GlobalAugmentationTargetsIndex = Arc<FxHashMap<String, Vec<(SymbolId, usize)>>>;

pub type ResolvedModulePathMap = FxHashMap<(usize, String), usize>;
pub type ResolvedModuleErrorMap = FxHashMap<(usize, String), ResolutionError>;
pub type ResolvedModuleRequestPathMap = FxHashMap<
    (
        usize,
        String,
        Option<ResolutionModeOverride>,
        ResolutionRequestKind,
    ),
    usize,
>;
pub type ResolvedModuleRequestErrorMap = FxHashMap<
    (
        usize,
        String,
        Option<ResolutionModeOverride>,
        ResolutionRequestKind,
    ),
    ResolutionError,
>;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_exports_cache_statistics_report_entries_and_size() {
        let mut table = SymbolTable::new();
        table.set("Exported".to_string(), SymbolId(7));

        let mut cache = NamespaceExportsCache::default();
        assert_eq!(namespace_exports_cache_entries(&cache), 0);
        assert_eq!(namespace_exports_cache_estimated_size_bytes(&cache), 0);

        cache.insert((1, "pkg".to_string()), Some(table));
        cache.insert((2, "missing".to_string()), None);

        assert_eq!(namespace_exports_cache_entries(&cache), 2);
        assert!(
            namespace_exports_cache_estimated_size_bytes(&cache)
                >= 2 * (std::mem::size_of::<(usize, String)>()
                    + std::mem::size_of::<Option<SymbolTable>>())
        );
    }

    #[test]
    fn export_equals_named_cache_statistics_report_entries_and_size() {
        let mut cache = ExportEqualsNamedCache::default();
        assert_eq!(export_equals_named_cache_entries(&cache), 0);
        assert_eq!(export_equals_named_cache_estimated_size_bytes(&cache), 0);

        cache.insert((1, "pkg".to_string(), "foo".to_string()), Some(SymbolId(3)));
        cache.insert((1, "pkg".to_string(), "bar".to_string()), None);

        assert_eq!(export_equals_named_cache_entries(&cache), 2);
        assert!(
            export_equals_named_cache_estimated_size_bytes(&cache)
                >= 2 * (std::mem::size_of::<(usize, String, String)>()
                    + std::mem::size_of::<Option<SymbolId>>())
        );
    }

    #[test]
    fn nested_namespace_candidates_cache_statistics_report_entries_and_size() {
        let mut cache = NestedNamespaceCandidatesCache::default();
        assert_eq!(nested_namespace_candidates_cache_entries(&cache), 0);
        assert_eq!(
            nested_namespace_candidates_cache_estimated_size_bytes(&cache),
            0
        );

        cache.insert("A.B".to_string(), vec![(1, SymbolId(2)), (3, SymbolId(4))]);
        cache.insert("C.D".to_string(), vec![(5, SymbolId(6))]);

        assert_eq!(nested_namespace_candidates_cache_entries(&cache), 2);
        assert!(
            nested_namespace_candidates_cache_estimated_size_bytes(&cache)
                >= 3 * std::mem::size_of::<(usize, SymbolId)>()
        );
    }

    #[test]
    fn namespace_member_resolution_cache_statistics_report_entries_and_size() {
        let mut cache = NamespaceMemberResolutionCache::default();
        assert_eq!(namespace_member_resolution_cache_entries(&cache), 0);
        assert_eq!(
            namespace_member_resolution_cache_estimated_size_bytes(&cache),
            0
        );

        let mut pkg_members = FxHashMap::default();
        pkg_members.insert("foo".to_string(), Some(SymbolId(1)));
        pkg_members.insert("missing".to_string(), None);
        let mut other_members = FxHashMap::default();
        other_members.insert("bar".to_string(), Some(SymbolId(2)));
        cache.insert("pkg".to_string(), pkg_members);
        cache.insert("other".to_string(), other_members);

        assert_eq!(namespace_member_resolution_cache_entries(&cache), 3);
        assert!(
            namespace_member_resolution_cache_estimated_size_bytes(&cache)
                >= 3 * (std::mem::size_of::<String>() + std::mem::size_of::<Option<SymbolId>>())
        );
    }
}
