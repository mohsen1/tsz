#[cfg(test)]
mod tests {
    use super::{CheckerContext, TypeCache};
    use rustc_hash::{FxHashMap, FxHashSet};
    use std::sync::Arc;
    use tsz_binder::BinderState;
    use tsz_binder::SymbolId;
    use tsz_common::checker_options::CheckerOptions;
    use tsz_parser::parser::NodeIndex;
    use tsz_parser::parser::node::NodeArena;
    use tsz_solver::TypeId;
    use tsz_solver::construction::TypeInterner;
    use tsz_solver::def::{DefinitionInfo, DefinitionStore};

    fn empty_cache() -> TypeCache {
        TypeCache {
            symbol_types: crate::context::SymbolTypeCache::new(),
            symbol_instance_types: crate::context::SymbolTypeCache::new(),
            node_types: crate::context::NodeTypeCache::new(),
            symbol_dependencies: FxHashMap::default(),
            def_to_symbol: FxHashMap::default(),
            def_to_name: FxHashMap::default(),
            def_types: FxHashMap::default(),
            def_type_params: FxHashMap::default(),
            boxed_types: FxHashMap::default(),
            boxed_def_ids: FxHashMap::default(),
            well_known_symbol_names: FxHashMap::default(),
            flow_analysis_cache: FxHashMap::default(),
            class_instance_type_to_decl: FxHashMap::default(),
            class_instance_type_cache: FxHashMap::default(),
            class_constructor_type_cache: FxHashMap::default(),
            type_only_nodes: FxHashSet::default(),
            namespace_module_names: FxHashMap::default(),
        }
    }

    #[test]
    fn type_cache_merge_keeps_constructor_type_cache() {
        let mut lhs = empty_cache();
        let mut rhs = empty_cache();

        rhs.class_constructor_type_cache
            .insert(NodeIndex(42), TypeId::STRING);

        lhs.merge(rhs);

        assert_eq!(
            lhs.class_constructor_type_cache.get(&NodeIndex(42)),
            Some(&TypeId::STRING)
        );
    }

    #[test]
    fn type_cache_merge_keeps_error_class_type_cache_entries() {
        let mut lhs = empty_cache();
        let mut rhs = empty_cache();

        rhs.class_instance_type_cache
            .insert(NodeIndex(10), TypeId::ERROR);
        rhs.class_constructor_type_cache
            .insert(NodeIndex(11), TypeId::ERROR);

        lhs.merge(rhs);

        assert_eq!(
            lhs.class_instance_type_cache.get(&NodeIndex(10)),
            Some(&TypeId::ERROR)
        );
        assert_eq!(
            lhs.class_constructor_type_cache.get(&NodeIndex(11)),
            Some(&TypeId::ERROR)
        );
    }

    #[test]
    fn invalidate_symbols_clears_class_type_caches() {
        let mut cache = empty_cache();
        let sym = SymbolId(7);
        cache
            .symbol_dependencies
            .insert(sym, FxHashSet::<SymbolId>::default());
        cache
            .class_instance_type_cache
            .insert(NodeIndex(1), TypeId::NUMBER);
        cache
            .class_constructor_type_cache
            .insert(NodeIndex(2), TypeId::STRING);
        cache
            .class_instance_type_to_decl
            .insert(TypeId::BOOLEAN, NodeIndex(3));

        let affected = cache.invalidate_symbols(&[sym]);

        assert_eq!(affected, 1);
        assert!(cache.class_instance_type_cache.is_empty());
        assert!(cache.class_constructor_type_cache.is_empty());
        assert!(cache.class_instance_type_to_decl.is_empty());
    }

    #[test]
    fn extract_cache_keeps_definition_names_without_symbol_mapping() {
        let arena = NodeArena::new();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let store = Arc::new(DefinitionStore::new());
        let name = types.intern_string("ConcatArray");
        let def_id = store.register(DefinitionInfo::interface(name, Vec::new(), Vec::new()));

        let ctx = CheckerContext::new_with_shared_def_store(
            &arena,
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions::default(),
            store,
        );

        let cache = ctx.extract_cache();

        assert_eq!(
            cache.def_to_name.get(&def_id).map(String::as_str),
            Some("ConcatArray")
        );
    }
}

impl super::ProgramContext {
    /// Build the shared `SymbolId` → file-index map from `symbol_file_targets`.
    ///
    /// Call this once after populating `symbol_file_targets`. The resulting
    /// `Arc<FxHashMap>` is shared (O(1) clone) across all checkers, eliminating
    /// the per-checker O(N) copy into `cross_file_symbol_targets`.
    pub fn build_global_symbol_file_index(&mut self) {
        let mut map: FxHashMap<SymbolId, usize> =
            FxHashMap::with_capacity_and_hasher(self.symbol_file_targets.len(), Default::default());
        for &(sym_id, file_idx) in self.symbol_file_targets.iter() {
            map.insert(sym_id, file_idx);
        }
        self.global_symbol_file_index = Some(Arc::new(map));
    }

    /// Build global indices only when the skeleton fingerprint has changed.
    ///
    /// Compares `new_fingerprint` against `self.last_skeleton_fingerprint`.
    /// If they match, the global indices are already valid and the expensive
    /// O(N) binder scan is skipped entirely. If they differ (or this is the
    /// first build), delegates to `build_global_indices` and stores the new
    /// fingerprint for future comparisons.
    ///
    /// Returns `true` if indices were rebuilt, `false` if cached.
    pub fn build_global_indices_if_changed(&mut self, new_fingerprint: u64) -> bool {
        if self.last_skeleton_fingerprint == Some(new_fingerprint) {
            // All global indices (name-based + arena) + skeleton indices are still valid.
            return false;
        }
        self.build_global_indices();
        self.last_skeleton_fingerprint = Some(new_fingerprint);
        true
    }
}
