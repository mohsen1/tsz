use crate::caches::dependency_index::{self, DependencyIndex, DependencyIndexState};
use crate::caches::shared_instantiation::collect_eval_entry_def_dependencies;
use crate::def::{DefId, DefinitionStore};
use crate::evaluation::request::EvaluationCacheKey;
use crate::intern::TypeInterner;
use crate::types::TypeId;
use rustc_hash::FxHashMap;
use std::cell::RefCell;

pub(super) type EvalDependencyIndexState = DependencyIndexState<EvaluationCacheKey>;
pub(super) type EvalDependencyIndex = DependencyIndex<EvaluationCacheKey>;

pub(super) fn record_dependencies(
    interner: &TypeInterner,
    definition_store: Option<&DefinitionStore>,
    index: &EvalDependencyIndex,
    key: EvaluationCacheKey,
    old_result: Option<TypeId>,
    result: TypeId,
) {
    let deps = collect_eval_entry_def_dependencies(interner, definition_store, key, result);
    dependency_index::record_dependencies(index, key, old_result.is_some(), deps);
}

pub(super) fn invalidate_for_def(
    index: &EvalDependencyIndex,
    cache: &RefCell<FxHashMap<EvaluationCacheKey, TypeId>>,
    def_id: DefId,
) {
    dependency_index::invalidate_for_def(index, cache, def_id);
}
