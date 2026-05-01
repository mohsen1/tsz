//! Helpers for the canonical `CROSS_FILE_QUERY_SYMBOL_TYPE` bucket.
//!
//! Centralises the gate + bucket lookup + sentinel-filtering pattern that
//! showed up in five reader call sites and four writer call sites after
//! PRs #1922, #1926, #1932, #1934, #1937, #1939, #1943, #1949. Each
//! site re-derived the same key shape and reject rules; this module
//! owns them once.

use tsz_binder::SymbolId;

use super::CheckerContext;

impl<'a> CheckerContext<'a> {
    /// Look up a cached cross-file symbol-type via the canonical
    /// `CROSS_FILE_QUERY_SYMBOL_TYPE` bucket.
    ///
    /// Returns `None` when:
    /// - the share-owner gate is off (`share_owner_symbol_type_results == false`),
    /// - the bucket has no entry for `(sym_id, file_idx)`, or
    /// - the cached value is `TypeId::ERROR` / `TypeId::UNKNOWN`.
    pub fn cached_cross_file_symbol_type(
        &self,
        sym_id: SymbolId,
        file_idx: u32,
    ) -> Option<(tsz_solver::TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        if !self.share_owner_symbol_type_results {
            return None;
        }
        let (cached_type, params) = self.definition_store.get_resolved_cross_file_query(
            crate::state_type_analysis::cross_file::CROSS_FILE_QUERY_SYMBOL_TYPE,
            file_idx,
            sym_id.0,
            0,
            0,
        )?;
        if matches!(
            cached_type,
            tsz_solver::TypeId::ERROR | tsz_solver::TypeId::UNKNOWN
        ) {
            return None;
        }
        Some((cached_type, params))
    }

    /// Cache a cross-file symbol-type result in the canonical
    /// `CROSS_FILE_QUERY_SYMBOL_TYPE` bucket.
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
            crate::state_type_analysis::cross_file::CROSS_FILE_QUERY_SYMBOL_TYPE,
            file_idx,
            sym_id.0,
            0,
            0,
            type_id,
            type_params,
        );
    }

    /// Look up a cached cross-file interface-type via the canonical
    /// `CROSS_FILE_QUERY_INTERFACE_TYPE` bucket.
    ///
    /// Returns `None` when:
    /// - the share-owner gate is off,
    /// - the bucket has no entry for `(sym_id, file_idx)`, or
    /// - the cached value is `TypeId::ERROR` / `TypeId::UNKNOWN`.
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
            return None;
        }
        let (cached_type, _params) = self.definition_store.get_resolved_cross_file_query(
            crate::state_type_analysis::cross_file::CROSS_FILE_QUERY_INTERFACE_TYPE,
            file_idx,
            sym_id.0,
            0,
            0,
        )?;
        if matches!(
            cached_type,
            tsz_solver::TypeId::ERROR | tsz_solver::TypeId::UNKNOWN
        ) {
            return None;
        }
        Some(cached_type)
    }

    /// Cache a cross-file interface-type result in the canonical
    /// `CROSS_FILE_QUERY_INTERFACE_TYPE` bucket.
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
            crate::state_type_analysis::cross_file::CROSS_FILE_QUERY_INTERFACE_TYPE,
            file_idx,
            sym_id.0,
            0,
            0,
            type_id,
            Vec::new(),
        );
    }

    /// Look up a cached cross-file interface-member simple type via the
    /// canonical `CROSS_FILE_QUERY_INTERFACE_MEMBER_SIMPLE_TYPE` bucket.
    ///
    /// Unlike the `SYMBOL_TYPE` / `INTERFACE_TYPE` buckets (keyed by `sym_id`),
    /// this bucket is keyed by `(interface_idx, member_idx)` so a single
    /// interface's members each live under their own entry.
    ///
    /// Returns `None` when:
    /// - the share-owner gate is off,
    /// - the bucket has no entry for `(file_idx, interface_idx, member_idx)`, or
    /// - the cached value is `TypeId::ERROR` / `TypeId::UNKNOWN`.
    pub fn cached_cross_file_interface_member_simple_type(
        &self,
        interface_idx: tsz_parser::NodeIndex,
        member_idx: tsz_parser::NodeIndex,
        file_idx: u32,
    ) -> Option<tsz_solver::TypeId> {
        if !self.share_owner_symbol_type_results {
            return None;
        }
        let (cached_type, _params) = self.definition_store.get_resolved_cross_file_query(
            crate::state_type_analysis::cross_file::CROSS_FILE_QUERY_INTERFACE_MEMBER_SIMPLE_TYPE,
            file_idx,
            interface_idx.0,
            member_idx.0,
            0,
        )?;
        if matches!(
            cached_type,
            tsz_solver::TypeId::ERROR | tsz_solver::TypeId::UNKNOWN
        ) {
            return None;
        }
        Some(cached_type)
    }

    /// Cache a cross-file interface-member simple type result in the
    /// canonical `CROSS_FILE_QUERY_INTERFACE_MEMBER_SIMPLE_TYPE` bucket.
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
            crate::state_type_analysis::cross_file::CROSS_FILE_QUERY_INTERFACE_MEMBER_SIMPLE_TYPE,
            file_idx,
            interface_idx.0,
            member_idx.0,
            0,
            type_id,
            Vec::new(),
        );
    }

    /// Look up a cached cross-file class-instance-type via the canonical
    /// `CROSS_FILE_QUERY_CLASS_INSTANCE_TYPE` bucket.
    ///
    /// Returns `None` when:
    /// - the share-owner gate is off (`share_owner_symbol_type_results == false`), or
    /// - the bucket has no entry for `(sym_id, file_idx)`.
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
            return None;
        }
        self.definition_store.get_resolved_cross_file_query(
            crate::state_type_analysis::cross_file::CROSS_FILE_QUERY_CLASS_INSTANCE_TYPE,
            file_idx,
            sym_id.0,
            0,
            0,
        )
    }
}
