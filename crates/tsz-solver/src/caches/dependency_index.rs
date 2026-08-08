use crate::def::DefId;
use crate::types::TypeId;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::hash::Hash;

/// Reverse dependency index shared by the evaluation and application-eval
/// caches, generic over the cache key `K`.
///
/// `reverse` maps each `DefId` to the set of cache keys that depend on it, so a
/// `DefId`-scoped invalidation can find every entry to drop. `key_dependencies`
/// maps each cache key back to its dependency list.
///
/// The dependency list is already de-duplicated by `collect_def_dependencies`'
/// `seen` set, and it is only ever iterated (never membership-queried), so it is
/// stored as a compact boxed slice rather than re-hashed into a second
/// `FxHashSet` on every cache write.
pub(super) struct DependencyIndexState<K> {
    pub(super) reverse: FxHashMap<DefId, FxHashSet<K>>,
    pub(super) key_dependencies: FxHashMap<K, Box<[DefId]>>,
}

impl<K> Default for DependencyIndexState<K> {
    fn default() -> Self {
        Self {
            reverse: FxHashMap::default(),
            key_dependencies: FxHashMap::default(),
        }
    }
}

impl<K> DependencyIndexState<K> {
    pub(super) fn clear(&mut self) {
        self.reverse.clear();
        self.key_dependencies.clear();
    }
}

pub(super) type DependencyIndex<K> = RefCell<DependencyIndexState<K>>;

/// Record the dependency list for `key`. `remove_old` requests that the entry's
/// previous dependencies be dropped first (callers pass `old_result.is_some()`).
/// `deps` must already be de-duplicated.
pub(super) fn record_dependencies<K: Eq + Hash + Clone>(
    index: &DependencyIndex<K>,
    key: K,
    remove_old: bool,
    deps: Vec<DefId>,
) {
    let mut index = index.borrow_mut();
    if remove_old {
        remove_dependencies(&mut index, &key);
    }
    if deps.is_empty() {
        return;
    }
    for &def_id in &deps {
        index.reverse.entry(def_id).or_default().insert(key.clone());
    }
    index.key_dependencies.insert(key, deps.into_boxed_slice());
}

/// Drop every cache key that depends on `def_id`, from both `cache` and the
/// index itself.
pub(super) fn invalidate_for_def<K: Eq + Hash>(
    index: &DependencyIndex<K>,
    cache: &RefCell<FxHashMap<K, TypeId>>,
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

/// Drop `key`'s dependency edges from the index without touching any cache.
/// Callers that evict a single entry directly (rather than through
/// [`invalidate_for_def`]) use this to keep the reverse index consistent.
pub(super) fn forget_key<K: Eq + Hash>(index: &DependencyIndex<K>, key: &K) {
    remove_dependencies(&mut index.borrow_mut(), key);
}

fn remove_dependencies<K: Eq + Hash>(index: &mut DependencyIndexState<K>, key: &K) {
    let Some(deps) = index.key_dependencies.remove(key) else {
        return;
    };
    for def_id in deps.iter() {
        let Some(keys) = index.reverse.get_mut(def_id) else {
            continue;
        };
        keys.remove(key);
        if keys.is_empty() {
            index.reverse.remove(def_id);
        }
    }
}
