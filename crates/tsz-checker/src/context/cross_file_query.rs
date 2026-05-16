//! Helpers for the canonical cross-file query buckets.
//!
//! Centralises the gate + bucket lookup + sentinel-filtering pattern that
//! showed up in five reader call sites and four writer call sites after
//! PRs #1922, #1926, #1932, #1934, #1937, #1939, #1943, #1949. Each
//! site re-derived the same key shape and reject rules; this module
//! owns them once.
//!
//! Bucket discriminants route through the typed
//! [`CrossFileQueryKind`](crate::state_type_analysis::cross_file::CrossFileQueryKind)
//! enum. The storage layer keys cache entries by the enum's `as_storage_kind()`
//! `u8` value, but no call site outside the storage boundary should handle
//! bare `u8` discriminants. The helpers below are the primary typed entry
//! points; a small number of call sites (such as `resolve_lazy` in
//! `context/resolver.rs`) intentionally inline `get_resolved_cross_file_query`
//! to preserve different sentinel-filtering semantics. Most helpers in this
//! module collapse `TypeId::UNKNOWN` to `None` for the common "treat as cache
//! miss" behavior, while `resolve_lazy` and
//! `cached_cross_file_class_instance_type` intentionally forward sentinel
//! values so callers can distinguish "resolved to UNKNOWN/ANY/ERROR" from
//! "no cache entry".

use tsz_binder::{SymbolId, symbol_flags};
use tsz_common::perf_counters::{
    CrossFileCacheMissCause, SourceFileSymbolArenaCacheEligibilityOutcome,
    record_cross_file_cache_miss_cause,
};

use crate::query_boundaries::common::type_id_is_known_to_db;
use crate::state_type_analysis::cross_file::CrossFileQueryKind;

use super::CheckerContext;

impl<'a> CheckerContext<'a> {
    pub fn program_has_module_augmentations(&self) -> bool {
        if self
            .global_module_augmentations_index
            .as_ref()
            .is_some_and(|index| !index.is_empty())
            || self
                .global_augmentation_targets_index
                .as_ref()
                .is_some_and(|index| !index.is_empty())
        {
            return true;
        }

        self.all_binders.as_ref().is_some_and(|binders| {
            binders.iter().any(|binder| {
                !binder.module_augmentations.is_empty()
                    || !binder.augmentation_target_modules.is_empty()
            })
        }) || !self.binder.module_augmentations.is_empty()
            || !self.binder.augmentation_target_modules.is_empty()
    }

    pub fn source_file_symbol_type_cache_scope(&self) -> u64 {
        self.definition_store.source_file_symbol_type_cache_scope()
    }

    pub fn symbol_arena_symbol_type_cache_is_stable(
        &self,
        sym_id: SymbolId,
        delegate_arena: &tsz_parser::NodeArena,
    ) -> bool {
        self.source_file_symbol_arena_cache_stability_outcome(sym_id, delegate_arena)
            == SourceFileSymbolArenaCacheEligibilityOutcome::Cacheable
    }

    pub fn source_file_symbol_arena_cache_stability_outcome(
        &self,
        sym_id: SymbolId,
        delegate_arena: &tsz_parser::NodeArena,
    ) -> SourceFileSymbolArenaCacheEligibilityOutcome {
        use SourceFileSymbolArenaCacheEligibilityOutcome as Outcome;

        let Some(symbol) = self.cross_file_cache_symbol(sym_id) else {
            return Outcome::MissingSymbol;
        };
        if symbol.declarations.len() != 1 {
            return Outcome::MultipleDeclarations;
        }

        if !symbol
            .has_any_flags(symbol_flags::CLASS | symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS)
            && !self.stable_annotated_variable_symbol(symbol, delegate_arena)
        {
            return Outcome::NotClassOrInterface;
        }

        // The `symbol_arenas` map stores one arena for the symbol, but merged
        // or augmented symbols can also have declarations in other arenas. A
        // cached symbol type is reusable only for the stable slice where the
        // single declaration is proven to belong solely to the delegated
        // source-file arena.
        let decl_idx = symbol.declarations[0];
        if !self.declaration_belongs_only_to_arena(sym_id, decl_idx, delegate_arena) {
            return Outcome::DeclarationArenaMismatch;
        }

        Outcome::Cacheable
    }

    fn stable_annotated_variable_symbol(
        &self,
        symbol: &tsz_binder::Symbol,
        delegate_arena: &tsz_parser::NodeArena,
    ) -> bool {
        if !symbol.has_any_flags(symbol_flags::VARIABLE) {
            return false;
        }
        if symbol.has_any_flags(symbol_flags::MODULE | symbol_flags::ALIAS) {
            return false;
        }

        let decl_idx = symbol.declarations[0];
        let Some(decl_node) = delegate_arena.get(decl_idx) else {
            return false;
        };
        delegate_arena
            .get_variable_declaration(decl_node)
            .is_some_and(|decl| decl.type_annotation.is_some())
    }

    fn declaration_belongs_only_to_arena(
        &self,
        sym_id: SymbolId,
        decl_idx: tsz_parser::NodeIndex,
        delegate_arena: &tsz_parser::NodeArena,
    ) -> bool {
        self.binder
            .declaration_arenas
            .get(&(sym_id, decl_idx))
            .is_some_and(|arenas| {
                arenas.len() == 1 && std::ptr::eq(arenas[0].as_ref(), delegate_arena)
            })
    }

    fn cross_file_cache_symbol(&self, sym_id: SymbolId) -> Option<&tsz_binder::Symbol> {
        if let Some(file_idx) = self.resolve_symbol_file_index(sym_id)
            && let Some(binder) = self.get_binder_for_file(file_idx)
            && let Some(sym) = binder.get_symbol(sym_id)
        {
            return Some(sym);
        }
        if let Some(sym) = self.binder.get_symbol(sym_id) {
            return Some(sym);
        }
        for lib in self.lib_contexts.iter() {
            if let Some(sym) = lib.binder.get_symbol(sym_id) {
                return Some(sym);
            }
        }
        if let Some(binders) = &self.all_binders {
            for binder in binders.iter() {
                if let Some(sym) = binder.get_symbol(sym_id) {
                    return Some(sym);
                }
            }
        }
        None
    }

    const fn source_file_symbol_type_requester_key(requester_file_idx: u32) -> u32 {
        requester_file_idx.saturating_add(1)
    }

    fn cached_symbol_type_entry(
        &self,
        sym_id: SymbolId,
        file_idx: u32,
        secondary: u32,
        args_hash: u64,
    ) -> Option<(tsz_solver::TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        if !self.share_owner_symbol_type_results {
            record_cross_file_cache_miss_cause(CrossFileCacheMissCause::GateOff);
            return None;
        }
        let Some((cached_type, params)) = self.definition_store.get_resolved_cross_file_query(
            CrossFileQueryKind::SymbolType.as_storage_kind(),
            file_idx,
            sym_id.0,
            secondary,
            args_hash,
        ) else {
            record_cross_file_cache_miss_cause(CrossFileCacheMissCause::BucketEmpty);
            return None;
        };
        if matches!(
            cached_type,
            tsz_solver::TypeId::ERROR | tsz_solver::TypeId::UNKNOWN
        ) {
            record_cross_file_cache_miss_cause(CrossFileCacheMissCause::SentinelErrorUnknown);
            return None;
        }
        if !type_id_is_known_to_db(self.types, cached_type) {
            record_cross_file_cache_miss_cause(CrossFileCacheMissCause::TypeIdNotInterned);
            return None;
        }
        Some((cached_type, params))
    }

    /// Look up a cached cross-file symbol-type via the canonical
    /// `CrossFileQueryKind::SymbolType` bucket.
    ///
    /// Returns `None` when:
    /// - the share-owner gate is off (`share_owner_symbol_type_results == false`),
    /// - the bucket has no entry for `(sym_id, file_idx)`, or
    /// - the cached value is `TypeId::ERROR` / `TypeId::UNKNOWN`,
    /// - the cached non-intrinsic `TypeId` is not interned in this checker.
    pub fn cached_cross_file_symbol_type(
        &self,
        sym_id: SymbolId,
        file_idx: u32,
    ) -> Option<(tsz_solver::TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        self.cached_symbol_type_entry(sym_id, file_idx, 0, 0)
    }

    /// Look up a cached source-file symbol-arena delegation result.
    ///
    /// Unlike genuine cross-file symbol targets, source-file symbol-arena
    /// answers can depend on the requesting file's import/diagnostic context
    /// and the virtual program that produced small file/symbol ids. Those
    /// entries therefore use requester and program scope key slots.
    pub fn cached_source_file_symbol_arena_type(
        &self,
        sym_id: SymbolId,
        file_idx: u32,
        scope: u64,
        requester_file_idx: u32,
    ) -> Option<(tsz_solver::TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        self.cached_symbol_type_entry(
            sym_id,
            file_idx,
            Self::source_file_symbol_type_requester_key(requester_file_idx),
            scope,
        )
    }

    /// Look up a cached source-file symbol-arena result that was proven
    /// requester-independent before entering this helper.
    ///
    /// This keeps the program scope key so small file/symbol ids cannot be
    /// reused across virtual programs sharing a `DefinitionStore`, but leaves
    /// the requester slot empty so other files in the same program can reuse
    /// stable single-declaration class/interface results.
    pub fn cached_stable_source_file_symbol_arena_type(
        &self,
        sym_id: SymbolId,
        file_idx: u32,
        scope: u64,
    ) -> Option<(tsz_solver::TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        self.cached_symbol_type_entry(sym_id, file_idx, 0, scope)
    }

    fn cache_symbol_type_entry(
        &self,
        sym_id: SymbolId,
        file_idx: u32,
        secondary: u32,
        args_hash: u64,
        type_id: tsz_solver::TypeId,
        type_params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        if !self.share_owner_symbol_type_results {
            return;
        }
        if matches!(
            type_id,
            tsz_solver::TypeId::ERROR | tsz_solver::TypeId::UNKNOWN
        ) {
            return;
        }
        self.definition_store.cache_resolved_cross_file_query(
            CrossFileQueryKind::SymbolType.as_storage_kind(),
            file_idx,
            sym_id.0,
            secondary,
            args_hash,
            type_id,
            type_params,
        );
    }

    /// Cache a cross-file symbol-type result in the canonical
    /// `CrossFileQueryKind::SymbolType` bucket.
    ///
    /// No-op when:
    /// - the share-owner gate is off, or
    /// - `type_id` is `TypeId::ERROR` / `TypeId::UNKNOWN` (sentinel values
    ///   would poison the cache for repeat lookups).
    ///
    /// First-writer-wins via `DashMap::entry().or_insert_with(...)`.
    pub fn cache_cross_file_symbol_type(
        &self,
        sym_id: SymbolId,
        file_idx: u32,
        type_id: tsz_solver::TypeId,
        type_params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        self.cache_symbol_type_entry(sym_id, file_idx, 0, 0, type_id, type_params);
    }

    /// Cache a source-file symbol-arena delegation result under requester and
    /// program scoped key slots. See [`cached_source_file_symbol_arena_type`].
    pub fn cache_source_file_symbol_arena_type(
        &self,
        sym_id: SymbolId,
        file_idx: u32,
        scope: u64,
        requester_file_idx: u32,
        type_id: tsz_solver::TypeId,
        type_params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        self.cache_symbol_type_entry(
            sym_id,
            file_idx,
            Self::source_file_symbol_type_requester_key(requester_file_idx),
            scope,
            type_id,
            type_params,
        );
    }

    /// Cache a proven requester-independent source-file symbol-arena result.
    /// See [`cached_stable_source_file_symbol_arena_type`].
    pub fn cache_stable_source_file_symbol_arena_type(
        &self,
        sym_id: SymbolId,
        file_idx: u32,
        scope: u64,
        type_id: tsz_solver::TypeId,
        type_params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        self.cache_symbol_type_entry(sym_id, file_idx, 0, scope, type_id, type_params);
    }

    /// Look up a cached cross-file interface-type via the canonical
    /// `CrossFileQueryKind::InterfaceType` bucket.
    ///
    /// Returns `None` when:
    /// - the share-owner gate is off,
    /// - the bucket has no entry for `(sym_id, file_idx)`, or
    /// - the cached value is `TypeId::ERROR` / `TypeId::UNKNOWN`,
    /// - the cached non-intrinsic `TypeId` is not interned in this checker.
    ///
    /// Used by `delegate_cross_arena_interface` (and similar) to skip child
    /// checker construction when a parallel worker has already lowered the
    /// interface in the same file. Mirrors `cached_cross_file_symbol_type`
    /// (see PR #1949) for a sibling bucket.
    pub fn cached_cross_file_interface_type(
        &self,
        sym_id: SymbolId,
        file_idx: u32,
    ) -> Option<tsz_solver::TypeId> {
        if !self.share_owner_symbol_type_results {
            record_cross_file_cache_miss_cause(CrossFileCacheMissCause::GateOff);
            return None;
        }
        let Some((cached_type, _params)) = self.definition_store.get_resolved_cross_file_query(
            CrossFileQueryKind::InterfaceType.as_storage_kind(),
            file_idx,
            sym_id.0,
            0,
            0,
        ) else {
            record_cross_file_cache_miss_cause(CrossFileCacheMissCause::BucketEmpty);
            return None;
        };
        if matches!(
            cached_type,
            tsz_solver::TypeId::ERROR | tsz_solver::TypeId::UNKNOWN
        ) {
            record_cross_file_cache_miss_cause(CrossFileCacheMissCause::SentinelErrorUnknown);
            return None;
        }
        if !type_id_is_known_to_db(self.types, cached_type) {
            record_cross_file_cache_miss_cause(CrossFileCacheMissCause::TypeIdNotInterned);
            return None;
        }
        Some(cached_type)
    }

    /// Cache a cross-file interface-type result in the canonical
    /// `CrossFileQueryKind::InterfaceType` bucket.
    ///
    /// No-op when:
    /// - the share-owner gate is off, or
    /// - `type_id` is `TypeId::ERROR` / `TypeId::UNKNOWN`.
    ///
    /// First-writer-wins via `DashMap::entry().or_insert_with(...)`. The
    /// bucket value's params payload is intentionally empty for interface
    /// types; per-file interface params live on the `DefId` side.
    pub fn cache_cross_file_interface_type(
        &self,
        sym_id: SymbolId,
        file_idx: u32,
        type_id: tsz_solver::TypeId,
    ) {
        if !self.share_owner_symbol_type_results {
            return;
        }
        if matches!(
            type_id,
            tsz_solver::TypeId::ERROR | tsz_solver::TypeId::UNKNOWN
        ) {
            return;
        }
        self.definition_store.cache_resolved_cross_file_query(
            CrossFileQueryKind::InterfaceType.as_storage_kind(),
            file_idx,
            sym_id.0,
            0,
            0,
            type_id,
            Vec::new(),
        );
    }

    /// Look up a cached cross-file interface-member simple type via the
    /// canonical `CrossFileQueryKind::InterfaceMemberSimpleType` bucket.
    ///
    /// Unlike the `SymbolType` / `InterfaceType` buckets (keyed by `sym_id`),
    /// this bucket is keyed by `(interface_idx, member_idx)` so a single
    /// interface's members each live under their own entry.
    ///
    /// Returns `None` when:
    /// - the share-owner gate is off,
    /// - the bucket has no entry for `(file_idx, interface_idx, member_idx)`, or
    /// - the cached value is `TypeId::ERROR` / `TypeId::UNKNOWN`,
    /// - the cached non-intrinsic `TypeId` is not interned in this checker.
    pub fn cached_cross_file_interface_member_simple_type(
        &self,
        interface_idx: tsz_parser::NodeIndex,
        member_idx: tsz_parser::NodeIndex,
        file_idx: u32,
    ) -> Option<tsz_solver::TypeId> {
        if !self.share_owner_symbol_type_results {
            record_cross_file_cache_miss_cause(CrossFileCacheMissCause::GateOff);
            return None;
        }
        let Some((cached_type, _params)) = self.definition_store.get_resolved_cross_file_query(
            CrossFileQueryKind::InterfaceMemberSimpleType.as_storage_kind(),
            file_idx,
            interface_idx.0,
            member_idx.0,
            0,
        ) else {
            record_cross_file_cache_miss_cause(CrossFileCacheMissCause::BucketEmpty);
            return None;
        };
        if matches!(
            cached_type,
            tsz_solver::TypeId::ERROR | tsz_solver::TypeId::UNKNOWN
        ) {
            record_cross_file_cache_miss_cause(CrossFileCacheMissCause::SentinelErrorUnknown);
            return None;
        }
        if !type_id_is_known_to_db(self.types, cached_type) {
            record_cross_file_cache_miss_cause(CrossFileCacheMissCause::TypeIdNotInterned);
            return None;
        }
        Some(cached_type)
    }

    /// Cache a cross-file interface-member simple type result in the
    /// canonical `CrossFileQueryKind::InterfaceMemberSimpleType` bucket.
    ///
    /// No-op when:
    /// - the share-owner gate is off, or
    /// - `type_id` is `TypeId::ERROR` / `TypeId::UNKNOWN`.
    ///
    /// First-writer-wins via `DashMap::entry().or_insert_with(...)`. The
    /// bucket value's params payload is empty — interface-member type
    /// params would live on the owning interface's `DefId`, not on the
    /// per-member entry.
    pub fn cache_cross_file_interface_member_simple_type(
        &self,
        interface_idx: tsz_parser::NodeIndex,
        member_idx: tsz_parser::NodeIndex,
        file_idx: u32,
        type_id: tsz_solver::TypeId,
    ) {
        if !self.share_owner_symbol_type_results {
            return;
        }
        if matches!(
            type_id,
            tsz_solver::TypeId::ERROR | tsz_solver::TypeId::UNKNOWN
        ) {
            return;
        }
        self.definition_store.cache_resolved_cross_file_query(
            CrossFileQueryKind::InterfaceMemberSimpleType.as_storage_kind(),
            file_idx,
            interface_idx.0,
            member_idx.0,
            0,
            type_id,
            Vec::new(),
        );
    }

    /// Look up a cached cross-file class-instance-type via the canonical
    /// `CrossFileQueryKind::ClassInstanceType` bucket.
    ///
    /// Returns `None` when:
    /// - the share-owner gate is off (`share_owner_symbol_type_results == false`), or
    /// - the bucket has no entry for `(sym_id, file_idx)`, or
    /// - the cached non-intrinsic `TypeId` is not interned in this checker.
    ///
    /// Note: this helper does **not** filter `TypeId::ERROR` /
    /// `TypeId::UNKNOWN` / `TypeId::ANY`. Class-instance bucket consumers
    /// disagree on which sentinels are meaningful (the
    /// `delegate_to_cross_arena_class_instance_lookup` site forwards the
    /// raw cached entry; the `computed_helpers_binding` site filters
    /// `ANY` / `ERROR` before populating `symbol_instance_types`). Apply
    /// per-call filtering at the call site rather than baking it into the
    /// helper.
    pub fn cached_cross_file_class_instance_type(
        &self,
        sym_id: SymbolId,
        file_idx: u32,
    ) -> Option<(tsz_solver::TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        if !self.share_owner_symbol_type_results {
            record_cross_file_cache_miss_cause(CrossFileCacheMissCause::GateOff);
            return None;
        }
        let Some((cached_type, params)) = self.definition_store.get_resolved_cross_file_query(
            CrossFileQueryKind::ClassInstanceType.as_storage_kind(),
            file_idx,
            sym_id.0,
            0,
            0,
        ) else {
            record_cross_file_cache_miss_cause(CrossFileCacheMissCause::BucketEmpty);
            return None;
        };
        // Sentinel filtering is intentionally **not** applied here — see the
        // docstring above. The next branch (`type_id_is_known_to_db`) still
        // counts as a `TypeIdNotInterned` miss, but `ERROR` / `UNKNOWN` /
        // `ANY` cached values are forwarded to the caller (which may treat
        // them as a real hit). That asymmetry is consistent with the
        // pre-instrumentation behavior; it means the
        // `SentinelErrorUnknown` bucket for this helper stays at zero
        // unless a future PR opts the class-instance reader in.
        if !type_id_is_known_to_db(self.types, cached_type) {
            record_cross_file_cache_miss_cause(CrossFileCacheMissCause::TypeIdNotInterned);
            return None;
        }
        Some((cached_type, params))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::CrossFileQueryKind;
    use crate::context::{CheckerContext, CheckerOptions};
    use tsz_binder::{BinderState, SymbolId};
    use tsz_parser::parser::{NodeArena, NodeIndex, ParserState};
    use tsz_solver::def::DefinitionStore;
    use tsz_solver::{TypeId, TypeInterner};

    fn shared_context<'a>(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        types: &'a TypeInterner,
        store: Arc<DefinitionStore>,
    ) -> CheckerContext<'a> {
        let mut ctx = CheckerContext::new_with_shared_def_store(
            arena,
            binder,
            types,
            "test.ts".to_string(),
            CheckerOptions::default(),
            store,
        );
        ctx.share_owner_symbol_type_results = true;
        ctx
    }

    fn bound_symbol_context(
        source: &str,
        symbol_name: &str,
    ) -> (Arc<NodeArena>, BinderState, TypeInterner, SymbolId) {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let arena = Arc::new(parser.get_arena().clone());
        let mut binder = BinderState::new();
        binder.bind_source_file(arena.as_ref(), root);
        let sym_id = binder
            .file_locals
            .get(symbol_name)
            .unwrap_or_else(|| panic!("symbol {symbol_name:?} not found"));
        let symbol = binder.symbols.get(sym_id).expect("symbol missing");
        let decl_idx = symbol.declarations[0];
        Arc::make_mut(&mut binder.declaration_arenas)
            .entry((sym_id, decl_idx))
            .or_default()
            .push(Arc::clone(&arena));
        (arena, binder, TypeInterner::new(), sym_id)
    }

    #[test]
    fn cross_file_cache_readers_reject_non_interned_type_ids() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let store = Arc::new(DefinitionStore::new());
        let ctx = shared_context(&arena, &binder, &types, Arc::clone(&store));
        let stale_type = TypeId(10_000);

        assert!(!crate::query_boundaries::common::type_id_is_known_to_db(
            &types, stale_type
        ));

        store.cache_resolved_cross_file_query(
            CrossFileQueryKind::SymbolType.as_storage_kind(),
            7,
            11,
            0,
            0,
            stale_type,
            Vec::new(),
        );
        store.cache_resolved_cross_file_query(
            CrossFileQueryKind::InterfaceType.as_storage_kind(),
            7,
            12,
            0,
            0,
            stale_type,
            Vec::new(),
        );
        store.cache_resolved_cross_file_query(
            CrossFileQueryKind::InterfaceMemberSimpleType.as_storage_kind(),
            7,
            21,
            22,
            0,
            stale_type,
            Vec::new(),
        );
        store.cache_resolved_cross_file_query(
            CrossFileQueryKind::ClassInstanceType.as_storage_kind(),
            7,
            13,
            0,
            0,
            stale_type,
            Vec::new(),
        );

        assert_eq!(ctx.cached_cross_file_symbol_type(SymbolId(11), 7), None);
        assert_eq!(ctx.cached_cross_file_interface_type(SymbolId(12), 7), None);
        assert_eq!(
            ctx.cached_cross_file_interface_member_simple_type(NodeIndex(21), NodeIndex(22), 7),
            None
        );
        assert_eq!(
            ctx.cached_cross_file_class_instance_type(SymbolId(13), 7),
            None
        );
    }

    #[test]
    fn stable_source_file_symbol_type_cache_accepts_annotated_variable() {
        let (arena, binder, types, sym_id) = bound_symbol_context(
            "export const leaf1: { value: number } = { value: 1 };",
            "leaf1",
        );
        let ctx = shared_context(
            arena.as_ref(),
            &binder,
            &types,
            Arc::new(DefinitionStore::new()),
        );

        assert!(ctx.symbol_arena_symbol_type_cache_is_stable(sym_id, arena.as_ref()));
    }

    #[test]
    fn stable_source_file_symbol_type_cache_accepts_type_alias() {
        let (arena, binder, types, sym_id) =
            bound_symbol_context("export type Leaf<T> = { value: T };", "Leaf");
        let ctx = shared_context(
            arena.as_ref(),
            &binder,
            &types,
            Arc::new(DefinitionStore::new()),
        );

        assert!(ctx.symbol_arena_symbol_type_cache_is_stable(sym_id, arena.as_ref()));
    }

    #[test]
    fn stable_source_file_symbol_type_cache_rejects_inferred_variable() {
        let (arena, binder, types, sym_id) =
            bound_symbol_context("export const leaf1 = { value: 1 };", "leaf1");
        let ctx = shared_context(
            arena.as_ref(),
            &binder,
            &types,
            Arc::new(DefinitionStore::new()),
        );

        assert!(!ctx.symbol_arena_symbol_type_cache_is_stable(sym_id, arena.as_ref()));
    }

    #[test]
    fn source_file_symbol_type_cache_keys_scope_and_requester() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let store = Arc::new(DefinitionStore::new());
        let ctx = shared_context(&arena, &binder, &types, store);
        let sym_id = SymbolId(11);
        let file_idx = 7;
        let scope = 0xCAFE_BABE_DEAD_BEEF;
        let requester_file_idx = 3;

        ctx.cache_cross_file_symbol_type(sym_id, file_idx, TypeId::NUMBER, Vec::new());
        ctx.cache_source_file_symbol_arena_type(
            sym_id,
            file_idx,
            scope,
            requester_file_idx,
            TypeId::STRING,
            Vec::new(),
        );

        assert_eq!(
            ctx.cached_cross_file_symbol_type(sym_id, file_idx)
                .map(|(type_id, _)| type_id),
            Some(TypeId::NUMBER)
        );
        assert_eq!(
            ctx.cached_source_file_symbol_arena_type(sym_id, file_idx, scope, requester_file_idx)
                .map(|(type_id, _)| type_id),
            Some(TypeId::STRING)
        );
        assert_eq!(
            ctx.cached_source_file_symbol_arena_type(
                sym_id,
                file_idx,
                scope,
                requester_file_idx + 1
            ),
            None
        );
        assert_eq!(
            ctx.cached_source_file_symbol_arena_type(
                sym_id,
                file_idx,
                scope + 1,
                requester_file_idx
            ),
            None
        );
    }

    #[test]
    fn stable_source_file_symbol_type_cache_key_uses_scope_without_requester() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let store = Arc::new(DefinitionStore::new());
        let ctx = shared_context(&arena, &binder, &types, store);
        let sym_id = SymbolId(11);
        let file_idx = 7;
        let scope = 0xCAFE_BABE_DEAD_BEEF;

        ctx.cache_stable_source_file_symbol_arena_type(
            sym_id,
            file_idx,
            scope,
            TypeId::STRING,
            Vec::new(),
        );

        assert_eq!(
            ctx.cached_stable_source_file_symbol_arena_type(sym_id, file_idx, scope)
                .map(|(type_id, _)| type_id),
            Some(TypeId::STRING)
        );
        assert_eq!(
            ctx.cached_stable_source_file_symbol_arena_type(sym_id, file_idx, scope + 1),
            None
        );
        assert_eq!(
            ctx.cached_cross_file_symbol_type(sym_id, file_idx)
                .map(|(type_id, _)| type_id),
            None
        );
    }
}
