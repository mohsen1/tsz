//! Atomic publication boundary for one module-augmentation batch.
//!
//! Stable declaration identity remains program-shared. Only semantic results
//! that can contain a nested cross-arena bailout are isolated: definition
//! bodies/display mappings in the solver store, checker symbol-result caches,
//! and the corresponding sparse environment result maps.

use super::state::CheckerState;
use crate::context::{CowCache, DeferredFlowEnvWrite, SymbolTypeCache};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz_binder::SymbolId;
use tsz_solver::TypeId;
use tsz_solver::def::DefinitionStore;

/// Sparse snapshots and the solver overlay for one nested augmentation batch.
#[must_use = "commit or roll back the augmentation publication transaction"]
pub(crate) struct ModuleAugmentationPublicationTransaction {
    overlay: Arc<DefinitionStore>,
    symbol_types: SymbolTypeCache,
    symbol_instance_types: SymbolTypeCache,
    enum_namespace_types: CowCache<FxHashMap<SymbolId, TypeId>>,
    namespace_module_names: FxHashMap<TypeId, String>,
    deferred_flow_env_writes_len: usize,
    deferred_eval_env_writes_len: usize,
}

impl CheckerState<'_> {
    /// Isolate bailout-contaminable semantic publication until the caller's
    /// final cross-arena bailout-epoch check succeeds.
    ///
    /// Call this only after a concrete matching augmentation has been found.
    /// The transaction is sparse: beginning it does not clone environment maps
    /// or program-wide definition data.
    pub(crate) fn begin_module_augmentation_publication(
        &mut self,
    ) -> ModuleAugmentationPublicationTransaction {
        self.ctx.flush_deferred_eval_env_writes();
        self.ctx.flush_deferred_flow_env_writes();

        self.ctx.begin_augmentation_local_transaction();
        self.ctx
            .type_env
            .borrow_mut()
            .begin_augmentation_transaction();
        self.ctx
            .type_environment
            .borrow_mut()
            .begin_augmentation_transaction();

        let overlay = self.ctx.definition_store.begin_augmentation_publication();
        let transaction = ModuleAugmentationPublicationTransaction {
            overlay: Arc::clone(&overlay),
            symbol_types: self.ctx.symbol_types.clone(),
            symbol_instance_types: self.ctx.symbol_instance_types.clone(),
            enum_namespace_types: self.ctx.enum_namespace_types.clone(),
            namespace_module_names: self.ctx.namespace_module_names.clone(),
            deferred_flow_env_writes_len: self.ctx.deferred_flow_env_writes.borrow().len(),
            deferred_eval_env_writes_len: self.ctx.deferred_eval_env_writes.borrow().len(),
        };

        self.ctx.definition_store = Arc::clone(&overlay);
        self.ctx
            .type_env
            .borrow_mut()
            .set_definition_store(Arc::clone(&overlay));
        self.ctx
            .type_environment
            .borrow_mut()
            .set_definition_store(overlay);
        transaction
    }

    /// Publish a successful batch. A nested commit replays into the outer
    /// overlay, so only the outermost commit reaches the shared base store.
    pub(crate) fn commit_module_augmentation_publication(
        &mut self,
        transaction: ModuleAugmentationPublicationTransaction,
    ) {
        self.assert_augmentation_transaction_is_current(&transaction);
        let parent = transaction
            .overlay
            .commit_augmentation_publication()
            .expect("transaction overlay must have a parent");
        self.install_augmentation_parent_store(parent);
        self.ctx
            .type_environment
            .borrow_mut()
            .commit_augmentation_transaction();
        self.ctx
            .type_env
            .borrow_mut()
            .commit_augmentation_transaction();
        self.ctx.commit_augmentation_local_transaction();
    }

    /// Discard a failed batch. Stable declaration identity writes intentionally
    /// remain shared; every bailout-contaminable result is restored or dropped.
    pub(crate) fn rollback_module_augmentation_publication(
        &mut self,
        transaction: ModuleAugmentationPublicationTransaction,
    ) {
        self.assert_augmentation_transaction_is_current(&transaction);
        let parent = transaction
            .overlay
            .rollback_augmentation_publication()
            .expect("transaction overlay must have a parent");

        self.ctx.definition_store = parent;
        self.ctx
            .type_environment
            .borrow_mut()
            .rollback_augmentation_transaction();
        self.ctx
            .type_env
            .borrow_mut()
            .rollback_augmentation_transaction();
        self.ctx.rollback_augmentation_local_transaction();
        self.ctx.symbol_types = transaction.symbol_types;
        self.ctx.symbol_instance_types = transaction.symbol_instance_types;
        self.ctx.enum_namespace_types = transaction.enum_namespace_types;
        self.ctx.namespace_module_names = transaction.namespace_module_names;
        Self::rollback_deferred_writes(
            &self.ctx.deferred_flow_env_writes,
            transaction.deferred_flow_env_writes_len,
            &self.ctx.type_environment,
        );
        Self::rollback_deferred_writes(
            &self.ctx.deferred_eval_env_writes,
            transaction.deferred_eval_env_writes_len,
            &self.ctx.type_env,
        );
    }

    fn assert_augmentation_transaction_is_current(
        &self,
        transaction: &ModuleAugmentationPublicationTransaction,
    ) {
        assert!(
            Arc::ptr_eq(&self.ctx.definition_store, &transaction.overlay),
            "augmentation publication transactions must close in LIFO order"
        );
    }

    fn install_augmentation_parent_store(&mut self, parent: Arc<DefinitionStore>) {
        self.ctx.definition_store = Arc::clone(&parent);
        self.ctx
            .type_env
            .borrow_mut()
            .set_definition_store(Arc::clone(&parent));
        self.ctx
            .type_environment
            .borrow_mut()
            .set_definition_store(Arc::clone(&parent));
        Self::retarget_deferred_store_writes(
            &mut self.ctx.deferred_eval_env_writes.borrow_mut(),
            &parent,
        );
        Self::retarget_deferred_store_writes(
            &mut self.ctx.deferred_flow_env_writes.borrow_mut(),
            &parent,
        );
    }

    fn retarget_deferred_store_writes(
        writes: &mut [DeferredFlowEnvWrite],
        store: &Arc<DefinitionStore>,
    ) {
        for write in writes {
            if matches!(write, DeferredFlowEnvWrite::SetDefinitionStore(_)) {
                *write = DeferredFlowEnvWrite::SetDefinitionStore(Arc::clone(store));
            }
        }
    }

    fn rollback_deferred_writes(
        writes: &std::cell::RefCell<Vec<DeferredFlowEnvWrite>>,
        baseline_len: usize,
        environment: &std::cell::RefCell<crate::query_boundaries::common::TypeEnvironment>,
    ) {
        let speculative = writes.borrow_mut().split_off(baseline_len);
        let mut environment = environment.borrow_mut();
        for write in speculative {
            write.apply_stable_identity(&mut environment);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CheckerOptions;
    use crate::query_boundaries::common::TypeInterner;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    #[test]
    fn nested_publication_restores_enum_and_namespace_display_caches() {
        let mut parser = ParserState::new("test.ts".to_string(), String::new());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        let arena = parser.into_arena();
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            &arena,
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions::default(),
        );
        let enum_symbol = SymbolId(1);

        checker
            .ctx
            .enum_namespace_types
            .insert(enum_symbol, TypeId::STRING);
        checker
            .ctx
            .namespace_module_names
            .insert(TypeId::STRING, "base".to_string());

        let outer = checker.begin_module_augmentation_publication();
        checker
            .ctx
            .enum_namespace_types
            .insert(enum_symbol, TypeId::NUMBER);
        checker
            .ctx
            .namespace_module_names
            .insert(TypeId::STRING, "outer".to_string());
        checker
            .ctx
            .namespace_module_names
            .insert(TypeId::NUMBER, "outer-only".to_string());

        let inner_rollback = checker.begin_module_augmentation_publication();
        checker
            .ctx
            .enum_namespace_types
            .insert(enum_symbol, TypeId::BOOLEAN);
        checker
            .ctx
            .namespace_module_names
            .insert(TypeId::STRING, "inner-rollback".to_string());
        checker.rollback_module_augmentation_publication(inner_rollback);

        assert_eq!(
            checker.ctx.enum_namespace_types.get(&enum_symbol),
            Some(&TypeId::NUMBER)
        );
        assert_eq!(
            checker
                .ctx
                .namespace_module_names
                .get(&TypeId::STRING)
                .map(String::as_str),
            Some("outer")
        );

        let inner_commit = checker.begin_module_augmentation_publication();
        checker
            .ctx
            .enum_namespace_types
            .insert(enum_symbol, TypeId::BIGINT);
        checker
            .ctx
            .namespace_module_names
            .insert(TypeId::STRING, "inner-commit".to_string());
        checker.commit_module_augmentation_publication(inner_commit);

        assert_eq!(
            checker.ctx.enum_namespace_types.get(&enum_symbol),
            Some(&TypeId::BIGINT)
        );
        assert_eq!(
            checker
                .ctx
                .namespace_module_names
                .get(&TypeId::STRING)
                .map(String::as_str),
            Some("inner-commit")
        );

        checker.rollback_module_augmentation_publication(outer);

        assert_eq!(
            checker.ctx.enum_namespace_types.get(&enum_symbol),
            Some(&TypeId::STRING)
        );
        assert_eq!(
            checker
                .ctx
                .namespace_module_names
                .get(&TypeId::STRING)
                .map(String::as_str),
            Some("base")
        );
        assert!(
            !checker
                .ctx
                .namespace_module_names
                .contains_key(&TypeId::NUMBER)
        );

        let committed = checker.begin_module_augmentation_publication();
        checker
            .ctx
            .enum_namespace_types
            .insert(enum_symbol, TypeId::BOOLEAN);
        checker
            .ctx
            .namespace_module_names
            .insert(TypeId::STRING, "committed".to_string());
        checker.commit_module_augmentation_publication(committed);

        assert_eq!(
            checker.ctx.enum_namespace_types.get(&enum_symbol),
            Some(&TypeId::BOOLEAN)
        );
        assert_eq!(
            checker
                .ctx
                .namespace_module_names
                .get(&TypeId::STRING)
                .map(String::as_str),
            Some("committed")
        );
    }
}
