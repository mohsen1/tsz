use crate::caches::shared_instantiation::collect_application_eval_entry_def_dependencies;
use crate::caches::shared_query_cache::ApplicationEvalCacheKey;
use crate::def::DefId;
use crate::intern::TypeInterner;
use crate::types::TypeId;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;

pub(super) type ApplicationEvalDependencyIndex =
    RefCell<FxHashMap<DefId, FxHashSet<ApplicationEvalCacheKey>>>;

pub(super) fn record_dependencies(
    interner: &TypeInterner,
    index: &ApplicationEvalDependencyIndex,
    key: &ApplicationEvalCacheKey,
    result: TypeId,
) {
    let deps = collect_application_eval_entry_def_dependencies(interner, key, result);
    let mut index = index.borrow_mut();
    for def_id in deps {
        index.entry(def_id).or_default().insert(key.clone());
    }
}

pub(super) fn invalidate_for_def(
    index: &ApplicationEvalDependencyIndex,
    cache: &RefCell<FxHashMap<ApplicationEvalCacheKey, TypeId>>,
    def_id: DefId,
) {
    let Some(keys) = index.borrow_mut().remove(&def_id) else {
        return;
    };
    let mut cache = cache.borrow_mut();
    for key in keys {
        cache.remove(&key);
    }
}

#[cfg(test)]
pub(super) fn key_count(index: &ApplicationEvalDependencyIndex, def_id: DefId) -> usize {
    index.borrow().get(&def_id).map_or(0, FxHashSet::len)
}
