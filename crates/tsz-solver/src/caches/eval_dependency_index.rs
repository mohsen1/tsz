use crate::caches::shared_instantiation::collect_eval_entry_def_dependencies;
use crate::def::{DefId, DefinitionStore};
use crate::evaluation::request::EvaluationCacheKey;
use crate::intern::TypeInterner;
use crate::types::TypeId;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;

#[derive(Default)]
pub(super) struct EvalDependencyIndexState {
    pub(super) reverse: FxHashMap<DefId, FxHashSet<EvaluationCacheKey>>,
    pub(super) key_dependencies: FxHashMap<EvaluationCacheKey, FxHashSet<DefId>>,
}

impl EvalDependencyIndexState {
    pub(super) fn clear(&mut self) {
        self.reverse.clear();
        self.key_dependencies.clear();
    }
}

pub(super) type EvalDependencyIndex = RefCell<EvalDependencyIndexState>;

pub(super) fn record_dependencies(
    interner: &TypeInterner,
    definition_store: Option<&DefinitionStore>,
    index: &EvalDependencyIndex,
    key: EvaluationCacheKey,
    old_result: Option<TypeId>,
    result: TypeId,
) {
    let mut index = index.borrow_mut();
    if old_result.is_some() {
        remove_dependencies(&mut index, key);
    }
    let deps = collect_eval_entry_def_dependencies(interner, definition_store, key, result);
    if deps.is_empty() {
        return;
    }
    let deps: FxHashSet<_> = deps.into_iter().collect();
    for &def_id in &deps {
        index.reverse.entry(def_id).or_default().insert(key);
    }
    index.key_dependencies.insert(key, deps);
}

pub(super) fn invalidate_for_def(
    index: &EvalDependencyIndex,
    cache: &RefCell<FxHashMap<EvaluationCacheKey, TypeId>>,
    def_id: DefId,
) {
    let Some(keys) = index.borrow_mut().reverse.remove(&def_id) else {
        return;
    };
    let mut cache = cache.borrow_mut();
    let mut index = index.borrow_mut();
    for key in keys {
        if cache.remove(&key).is_some() {
            remove_dependencies(&mut index, key);
        }
    }
}

fn remove_dependencies(index: &mut EvalDependencyIndexState, key: EvaluationCacheKey) {
    let Some(deps) = index.key_dependencies.remove(&key) else {
        return;
    };
    for def_id in deps {
        let Some(keys) = index.reverse.get_mut(&def_id) else {
            continue;
        };
        keys.remove(&key);
        if keys.is_empty() {
            index.reverse.remove(&def_id);
        }
    }
}
