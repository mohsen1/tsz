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
}
