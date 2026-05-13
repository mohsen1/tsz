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
//! to preserve different sentinel-filtering semantics — `resolve_lazy`
//! forwards `TypeId::UNKNOWN` so callers can distinguish "lazy reference
//! resolved but symbol type is genuinely unknown" from "lazy reference not
//! resolved" (`None`), while these helpers collapse `UNKNOWN` to `None` for
//! the (more common) "treat as cache miss" semantics.

use tsz_binder::SymbolId;
use tsz_common::perf_counters::{CrossFileCacheMissCause, record_cross_file_cache_miss_cause};

use crate::query_boundaries::common::type_id_is_known_to_db;
use crate::state_type_analysis::cross_file::CrossFileQueryKind;

use super::CheckerContext;

impl<'a> CheckerContext<'a> {
    fn cross_file_symbol_type_secondary(requester_file_idx: Option<u32>) -> u32 {
        requester_file_idx.map_or(0, |idx| idx.saturating_add(1))
    }

    fn cached_cross_file_symbol_type_with_requester(
        &self,
        sym_id: SymbolId,
        file_idx: u32,
        requester_file_idx: Option<u32>,
    ) -> Option<(tsz_solver::TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        let secondary = Self::cross_file_symbol_type_secondary(requester_file_idx);

        if !self.share_owner_symbol_type_results {
            record_cross_file_cache_miss_cause(CrossFileCacheMissCause::GateOff);
            return None;
        }
        let Some((cached_type, params)) = self.definition_store.get_resolved_cross_file_query(
            CrossFileQueryKind::SymbolType.as_storage_kind(),
            file_idx,
            sym_id.0,
            secondary,
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
        self.cached_cross_file_symbol_type_with_requester(sym_id, file_idx, None)
    }

    /// Look up a cached source-file symbol-arena delegation result. Unlike
    /// genuine cross-file symbol targets, source-file symbol-arena answers can
    /// depend on the requesting file's import and diagnostic context, so the
    /// requester file is part of the key.
    pub fn cached_source_file_symbol_arena_type(
        &self,
        sym_id: SymbolId,
        file_idx: u32,
        requester_file_idx: u32,
    ) -> Option<(tsz_solver::TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        self.cached_cross_file_symbol_type_with_requester(
            sym_id,
            file_idx,
            Some(requester_file_idx),
        )
    }

    fn cache_cross_file_symbol_type_with_requester(
        &self,
        sym_id: SymbolId,
        file_idx: u32,
        requester_file_idx: Option<u32>,
        type_id: tsz_solver::TypeId,
        type_params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        let secondary = Self::cross_file_symbol_type_secondary(requester_file_idx);

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
            0,
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
        self.cache_cross_file_symbol_type_with_requester(
            sym_id,
            file_idx,
            None,
            type_id,
            type_params,
        );
    }

    /// Cache a source-file symbol-arena delegation result under a requester-
    /// scoped key. See [`cached_source_file_symbol_arena_type`].
    pub fn cache_source_file_symbol_arena_type(
        &self,
        sym_id: SymbolId,
        file_idx: u32,
        requester_file_idx: u32,
        type_id: tsz_solver::TypeId,
        type_params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        self.cache_cross_file_symbol_type_with_requester(
            sym_id,
            file_idx,
            Some(requester_file_idx),
            type_id,
            type_params,
        );
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
    use tsz_parser::parser::{NodeArena, NodeIndex};
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
}
