//! Type aliases and supporting types used across the checker context.
//!
//! Cross-binder index shapes, module-resolution caches, the per-file
//! member-access / accessor / callback-mismatch memo shapes, and the
//! `ResolutionError` / `ResolutionModeOverride` helpers they depend on. Kept
//! in one file so the `pub type`/helper-type surface doesn't dilute `mod.rs`.

use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::sync::Arc;

use tsz_binder::{ModuleAugmentation, SymbolId, SymbolTable};
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

/// Per-file memo value for `find_accessor_levels_in_hierarchy`: the resolved
/// getter level, setter level, and the declaring-class node, or `None` when no
/// getter/setter pair declares the requested name in the class chain. Keyed by
/// `(class node, member-name Atom, is_static)` in
/// [`crate::context::CheckerContext::accessor_levels_cache`].
pub(crate) type AccessorLevelsCacheValue = Option<(
    Option<crate::state::MemberAccessLevel>,
    Option<crate::state::MemberAccessLevel>,
    NodeIndex,
)>;

/// Per-file memo mapping `(class node, member-name Atom, is_static)` to the
/// accessor-level classification produced by `find_accessor_levels_in_hierarchy`.
/// Backs [`crate::context::CheckerContext::accessor_levels_cache`].
pub(crate) type AccessorLevelsCache =
    RefCell<FxHashMap<(NodeIndex, Atom, bool), AccessorLevelsCacheValue>>;

#[must_use]
pub(crate) fn accessor_levels_cache_entries(cache: &AccessorLevelsCache) -> usize {
    cache.borrow().len()
}

#[must_use]
pub(crate) fn accessor_levels_cache_estimated_size_bytes(cache: &AccessorLevelsCache) -> usize {
    let cache = cache.borrow();
    cache.capacity()
        * (std::mem::size_of::<(NodeIndex, Atom, bool)>()
            + std::mem::size_of::<AccessorLevelsCacheValue>()
            + 8)
}

/// Per-file memo mapping `(class node, member-name Atom, is_static)` to the
/// access-restriction classification produced by `find_member_access_info`,
/// or `None` when the member is public/absent. Backs
/// [`crate::context::CheckerContext::member_access_info_cache`].
pub(crate) type MemberAccessInfoCache =
    RefCell<FxHashMap<(NodeIndex, Atom, bool), Option<crate::state::MemberAccessInfo>>>;

#[must_use]
pub(crate) fn member_access_info_cache_entries(cache: &MemberAccessInfoCache) -> usize {
    cache.borrow().len()
}

#[must_use]
pub(crate) fn member_access_info_cache_estimated_size_bytes(
    cache: &MemberAccessInfoCache,
) -> usize {
    let cache = cache.borrow();
    cache.capacity()
        * (std::mem::size_of::<(NodeIndex, Atom, bool)>()
            + std::mem::size_of::<Option<crate::state::MemberAccessInfo>>()
            + 8)
}

/// Per-file memo for the contextual-callback return-type mismatch derivation
/// (`raw_block_body_callback_mismatch`). Maps the inline callback argument node
/// and its expected contextual type to the stable mismatch outcome
/// `(arg index, recovery actual, expected)`, or `None` when no mismatch is
/// forced. Backs [`crate::context::CheckerContext::callback_mismatch_memo`].
pub type CallbackMismatchMemo = FxHashMap<(NodeIndex, TypeId), Option<(usize, TypeId, TypeId)>>;

#[must_use]
pub(crate) fn callback_mismatch_memo_entries(cache: &CallbackMismatchMemo) -> usize {
    cache.len()
}

#[must_use]
pub(crate) fn callback_mismatch_memo_estimated_size_bytes(cache: &CallbackMismatchMemo) -> usize {
    cache.capacity()
        * (std::mem::size_of::<(NodeIndex, TypeId)>()
            + std::mem::size_of::<Option<(usize, TypeId, TypeId)>>()
            + 8)
}

/// Flow-analysis result memo: `(FlowNodeId, SymbolId, InitialTypeId) ->
/// NarrowedTypeId`.
pub type FlowAnalysisCacheMap = FxHashMap<(tsz_binder::FlowNodeId, SymbolId, TypeId), TypeId>;

#[must_use]
pub(crate) fn flow_analysis_cache_map_entries(cache: &FlowAnalysisCacheMap) -> usize {
    cache.len()
}

#[must_use]
pub(crate) fn flow_analysis_cache_map_estimated_size_bytes(cache: &FlowAnalysisCacheMap) -> usize {
    cache.capacity()
        * (std::mem::size_of::<(tsz_binder::FlowNodeId, SymbolId, TypeId)>()
            + std::mem::size_of::<TypeId>()
            + 8)
}

/// Stable-flow confirmation memo: `(SymbolId, DeclaredTypeId)` to the last
/// `FlowNodeId` where flow analysis confirmed no narrowing.
pub type SymbolFlowConfirmedMap = FxHashMap<(SymbolId, TypeId), tsz_binder::FlowNodeId>;

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

impl From<tsz_common::ImportResolutionMode> for ResolutionModeOverride {
    fn from(mode: tsz_common::ImportResolutionMode) -> Self {
        match mode {
            tsz_common::ImportResolutionMode::Import => Self::Import,
            tsz_common::ImportResolutionMode::Require => Self::Require,
        }
    }
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

/// Per-checker cache: (target file, export name) → fully-resolved re-export target.
///
/// Maps a `(file_idx, export_name)` pair to the `(SymbolId, owning_file_idx)`
/// that the re-export walker resolves it to (or `None` when the name is not
/// exported). Only **root** resolutions — entered with an empty `visited`
/// path and no module-key override — populate this cache: at that boundary the
/// answer is the canonical, path-independent result for `(file, name)` and can
/// never be the cycle-break sentinel, so it is safe to reuse across the many
/// import/usage sites that resolve the same barrel export. Barrel-heavy
/// programs (e.g. `ts-morph`) otherwise re-walk the entire `export *` graph
/// from scratch for every distinct name, costing `O(names × export-edges)`.
pub type ReexportResolutionCache = FxHashMap<(usize, String), Option<(SymbolId, usize)>>;

#[must_use]
pub(crate) fn reexport_resolution_cache_entries(cache: &ReexportResolutionCache) -> usize {
    cache.len()
}

#[must_use]
pub(crate) fn reexport_resolution_cache_estimated_size_bytes(
    cache: &ReexportResolutionCache,
) -> usize {
    let mut size = cache.capacity()
        * (std::mem::size_of::<(usize, String)>()
            + std::mem::size_of::<Option<(SymbolId, usize)>>()
            + 8);
    for (_, export_name) in cache.keys() {
        size += export_name.capacity();
    }
    size
}

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

/// Per-checker cache for named exports reached through `export=`.
/// Keyed by `(current_file_idx, module_specifier, export_name, visited_aliases)`.
pub type ExportEqualsNamedCache =
    FxHashMap<(usize, String, String, Vec<SymbolId>), Option<SymbolId>>;

#[must_use]
pub(crate) fn export_equals_named_cache_entries(cache: &ExportEqualsNamedCache) -> usize {
    cache.len()
}

#[must_use]
pub(crate) fn export_equals_named_cache_estimated_size_bytes(
    cache: &ExportEqualsNamedCache,
) -> usize {
    let mut size = cache.capacity()
        * (std::mem::size_of::<(usize, String, String, Vec<SymbolId>)>()
            + std::mem::size_of::<Option<SymbolId>>()
            + 8);
    for (_, specifier, export_name, visited_aliases) in cache.keys() {
        size += specifier.capacity() + export_name.capacity();
        size += visited_aliases.capacity() * std::mem::size_of::<SymbolId>();
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
    fn file_session_alias_caches_report_entries_and_size() {
        let accessor_cache = AccessorLevelsCache::default();
        assert_eq!(accessor_levels_cache_entries(&accessor_cache), 0);
        assert_eq!(
            accessor_levels_cache_estimated_size_bytes(&accessor_cache),
            0
        );
        accessor_cache
            .borrow_mut()
            .insert((NodeIndex(1), Atom(2), false), None);
        assert_eq!(accessor_levels_cache_entries(&accessor_cache), 1);
        assert!(accessor_levels_cache_estimated_size_bytes(&accessor_cache) > 0);

        let member_cache = MemberAccessInfoCache::default();
        assert_eq!(member_access_info_cache_entries(&member_cache), 0);
        assert_eq!(
            member_access_info_cache_estimated_size_bytes(&member_cache),
            0
        );
        member_cache
            .borrow_mut()
            .insert((NodeIndex(3), Atom(4), true), None);
        assert_eq!(member_access_info_cache_entries(&member_cache), 1);
        assert!(member_access_info_cache_estimated_size_bytes(&member_cache) > 0);

        let mut callback_cache = CallbackMismatchMemo::default();
        assert_eq!(callback_mismatch_memo_entries(&callback_cache), 0);
        assert_eq!(
            callback_mismatch_memo_estimated_size_bytes(&callback_cache),
            0
        );
        callback_cache.insert(
            (NodeIndex(5), TypeId::STRING),
            Some((1, TypeId::NUMBER, TypeId::BOOLEAN)),
        );
        assert_eq!(callback_mismatch_memo_entries(&callback_cache), 1);
        assert!(callback_mismatch_memo_estimated_size_bytes(&callback_cache) > 0);
    }

    #[test]
    fn retained_alias_caches_report_entries_and_size() {
        let mut flow_cache = FlowAnalysisCacheMap::default();
        assert_eq!(flow_analysis_cache_map_entries(&flow_cache), 0);
        assert_eq!(flow_analysis_cache_map_estimated_size_bytes(&flow_cache), 0);
        flow_cache.insert(
            (tsz_binder::FlowNodeId(1), SymbolId(2), TypeId::STRING),
            TypeId::NUMBER,
        );
        assert_eq!(flow_analysis_cache_map_entries(&flow_cache), 1);
        assert!(flow_analysis_cache_map_estimated_size_bytes(&flow_cache) > 0);

        let mut reexport_cache = ReexportResolutionCache::default();
        assert_eq!(reexport_resolution_cache_entries(&reexport_cache), 0);
        assert_eq!(
            reexport_resolution_cache_estimated_size_bytes(&reexport_cache),
            0
        );
        reexport_cache.insert((3, "value".to_string()), Some((SymbolId(4), 5)));
        assert_eq!(reexport_resolution_cache_entries(&reexport_cache), 1);
        assert!(reexport_resolution_cache_estimated_size_bytes(&reexport_cache) > 0);
    }

    #[test]
    fn export_equals_named_cache_statistics_report_entries_and_size() {
        let mut cache = ExportEqualsNamedCache::default();
        assert_eq!(export_equals_named_cache_entries(&cache), 0);
        assert_eq!(export_equals_named_cache_estimated_size_bytes(&cache), 0);

        cache.insert(
            (1, "pkg".to_string(), "foo".to_string(), vec![]),
            Some(SymbolId(3)),
        );
        cache.insert(
            (1, "pkg".to_string(), "bar".to_string(), vec![SymbolId(7)]),
            None,
        );

        assert_eq!(export_equals_named_cache_entries(&cache), 2);
        assert!(
            export_equals_named_cache_estimated_size_bytes(&cache)
                >= 2 * (std::mem::size_of::<(usize, String, String, Vec<SymbolId>)>()
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
