use crate::caches::shared_instantiation::collect_application_eval_entry_def_dependencies;
use crate::caches::shared_query_cache::ApplicationEvalCacheKey;
use crate::def::{DefId, DefinitionStore};
use crate::intern::TypeInterner;
use crate::types::TypeId;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;

#[derive(Default)]
pub(super) struct ApplicationEvalDependencyIndexState {
    pub(super) reverse: FxHashMap<DefId, FxHashSet<ApplicationEvalCacheKey>>,
    pub(super) key_dependencies: FxHashMap<ApplicationEvalCacheKey, FxHashSet<DefId>>,
}

impl ApplicationEvalDependencyIndexState {
    pub(super) fn clear(&mut self) {
        self.reverse.clear();
        self.key_dependencies.clear();
    }
}

pub(super) type ApplicationEvalDependencyIndex = RefCell<ApplicationEvalDependencyIndexState>;

pub(super) fn record_dependencies(
    interner: &TypeInterner,
    definition_store: Option<&DefinitionStore>,
    index: &ApplicationEvalDependencyIndex,
    key: &ApplicationEvalCacheKey,
    old_result: Option<TypeId>,
    result: TypeId,
) {
    let mut index = index.borrow_mut();
    if old_result.is_some() {
        remove_dependencies(&mut index, key);
    }
    let deps =
        collect_application_eval_entry_def_dependencies(interner, definition_store, key, result);
    if deps.is_empty() {
        return;
    }
    let deps: FxHashSet<_> = deps.into_iter().collect();
    for &def_id in &deps {
        index.reverse.entry(def_id).or_default().insert(key.clone());
    }
    index.key_dependencies.insert(key.clone(), deps);
}

pub(super) fn invalidate_for_def(
    index: &ApplicationEvalDependencyIndex,
    cache: &RefCell<FxHashMap<ApplicationEvalCacheKey, TypeId>>,
    def_id: DefId,
) {
    let Some(keys) = index.borrow_mut().reverse.remove(&def_id) else {
        return;
    };
    let mut cache = cache.borrow_mut();
    let mut index = index.borrow_mut();
    for key in keys {
        if cache.remove(&key).is_some() {
            remove_dependencies(&mut index, &key);
        }
    }
}

#[cfg(test)]
pub(super) fn key_count(index: &ApplicationEvalDependencyIndex, def_id: DefId) -> usize {
    index
        .borrow()
        .reverse
        .get(&def_id)
        .map_or(0, FxHashSet::len)
}

fn remove_dependencies(
    index: &mut ApplicationEvalDependencyIndexState,
    key: &ApplicationEvalCacheKey,
) {
    let Some(deps) = index.key_dependencies.remove(key) else {
        return;
    };
    for def_id in deps {
        let Some(keys) = index.reverse.get_mut(&def_id) else {
            continue;
        };
        keys.remove(key);
        if keys.is_empty() {
            index.reverse.remove(&def_id);
        }
    }
}
