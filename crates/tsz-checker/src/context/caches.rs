use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz_binder::SymbolId;
use tsz_solver::TypeId;

/// Sparse cache for node-index-keyed `TypeId` lookups.
///
/// `NodeIndex` values are arena-local, so this cache is never shared across
/// parent/child checkers. It is Arc-backed for cheap speculation snapshots:
/// rollback stores a read snapshot and the active cache copy-on-writes only if
/// it is mutated after the snapshot.
#[derive(Clone, Debug)]
pub struct NodeTypeCache {
    data: Arc<FxHashMap<u32, TypeId>>,
}

impl NodeTypeCache {
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Arc::new(FxHashMap::with_capacity_and_hasher(
                capacity.min(4096),
                Default::default(),
            )),
        }
    }

    #[inline]
    pub fn new() -> Self {
        Self {
            data: Arc::new(FxHashMap::default()),
        }
    }

    #[inline]
    pub fn get(&self, key: &u32) -> Option<&TypeId> {
        self.data.get(key)
    }

    #[inline]
    pub fn insert(&mut self, key: u32, value: TypeId) {
        if key == u32::MAX {
            return;
        }
        let data = Arc::make_mut(&mut self.data);
        if value == TypeId::NONE {
            data.remove(&key);
        } else {
            data.insert(key, value);
        }
    }

    #[inline]
    pub fn contains_key(&self, key: &u32) -> bool {
        self.data.contains_key(key)
    }

    #[inline]
    pub fn remove(&mut self, key: &u32) -> Option<TypeId> {
        Arc::make_mut(&mut self.data).remove(key)
    }

    #[inline]
    pub fn or_insert(&mut self, key: u32, value: TypeId) -> TypeId {
        // Preserve the cache invariant maintained by `insert` above: NONE is
        // *never* stored as a real entry. If the caller asks to insert NONE,
        // return either the existing real value or NONE without touching the
        // map. Without this guard, `or_insert(key, NONE)` followed by `get(key)`
        // would return `Some(&NONE)` (a stale "cached" sentinel) instead of
        // `None`, and downstream callers (e.g. `type_node_resolution.rs:226`)
        // that check `if let Some(&cached) = ...get(&idx.0)` would return the
        // sentinel as if it were a real type.
        if value == TypeId::NONE {
            return self.data.get(&key).copied().unwrap_or(TypeId::NONE);
        }
        *Arc::make_mut(&mut self.data).entry(key).or_insert(value)
    }

    pub fn iter(&self) -> impl Iterator<Item = (u32, TypeId)> + '_ {
        self.data.iter().map(|(i, t)| (*i, *t))
    }

    pub fn clear(&mut self) {
        Arc::make_mut(&mut self.data).clear();
    }

    pub fn merge(&mut self, other: &Self) {
        Arc::make_mut(&mut self.data).extend(other.iter());
    }

    pub fn merge_owned(&mut self, other: Self) {
        Arc::make_mut(&mut self.data).extend(other.iter());
    }

    pub fn extend<I: IntoIterator<Item = (u32, TypeId)>>(&mut self, iter: I) {
        for (key, value) in iter {
            self.insert(key, value);
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn to_hash_map(&self) -> FxHashMap<u32, TypeId> {
        self.iter().collect()
    }
}

impl Default for NodeTypeCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Dense tristate cache for `is_narrowable_identifier` results.
#[derive(Clone, Debug)]
pub struct NarrowableIdentifierCache {
    data: Vec<u8>,
}

impl NarrowableIdentifierCache {
    const UNKNOWN: u8 = 0;
    const NOT_NARROWABLE: u8 = 1;
    const NARROWABLE: u8 = 2;

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: vec![Self::UNKNOWN; capacity],
        }
    }

    #[inline]
    pub const fn new() -> Self {
        Self { data: Vec::new() }
    }

    #[inline]
    pub fn get(&self, key: u32) -> Option<bool> {
        let idx = key as usize;
        match self.data.get(idx).copied().unwrap_or(Self::UNKNOWN) {
            Self::NARROWABLE => Some(true),
            Self::NOT_NARROWABLE => Some(false),
            _ => None,
        }
    }

    #[inline]
    pub fn insert(&mut self, key: u32, value: bool) {
        let idx = key as usize;
        if idx >= self.data.len() {
            self.data.resize(idx + 1, Self::UNKNOWN);
        }
        self.data[idx] = if value {
            Self::NARROWABLE
        } else {
            Self::NOT_NARROWABLE
        };
    }
}

impl Default for NarrowableIdentifierCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Sparse cache for `SymbolId -> TypeId` lookups.
///
/// `SymbolId`s are global after program merge, so a dense per-checker vector
/// scales with total program symbols even when a checker touches only a small
/// subset. Keep the cache sparse and Arc-backed so child checkers can inherit a
/// read snapshot cheaply; writes copy-on-write only the populated entries.
#[derive(Clone, Debug)]
pub struct SymbolTypeCache {
    data: Arc<FxHashMap<SymbolId, TypeId>>,
}

impl SymbolTypeCache {
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Arc::new(FxHashMap::with_capacity_and_hasher(
                capacity.min(4096),
                Default::default(),
            )),
        }
    }

    #[inline]
    pub fn new() -> Self {
        Self {
            data: Arc::new(FxHashMap::default()),
        }
    }

    #[inline]
    pub fn get(&self, key: &SymbolId) -> Option<&TypeId> {
        self.data.get(key)
    }

    #[inline]
    pub fn insert(&mut self, key: SymbolId, value: TypeId) {
        let data = Arc::make_mut(&mut self.data);
        if value == TypeId::NONE {
            data.remove(&key);
        } else {
            data.insert(key, value);
        }
    }

    #[inline]
    pub fn contains_key(&self, key: &SymbolId) -> bool {
        self.data.contains_key(key)
    }

    #[inline]
    pub fn remove(&mut self, key: &SymbolId) -> Option<TypeId> {
        Arc::make_mut(&mut self.data).remove(key)
    }

    #[inline]
    pub fn entry_or_insert(&mut self, key: SymbolId, value: TypeId) -> TypeId {
        // Same NONE-storage guard as `NodeTypeCache::or_insert` — `insert`
        // explicitly removes NONE entries to maintain the cache invariant
        // that `get`/`contains_key` only see real types. `entry().or_insert()`
        // with `value == NONE` would silently break that.
        if value == TypeId::NONE {
            return self.data.get(&key).copied().unwrap_or(TypeId::NONE);
        }
        *Arc::make_mut(&mut self.data).entry(key).or_insert(value)
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (SymbolId, TypeId)> + '_ {
        self.data
            .iter()
            .map(|(&symbol_id, &type_id)| (symbol_id, type_id))
    }

    pub fn to_hash_map(&self) -> FxHashMap<SymbolId, TypeId> {
        self.data.as_ref().clone()
    }

    pub fn extend(&mut self, other: Self) {
        let data = Arc::make_mut(&mut self.data);
        for (&symbol_id, &type_id) in other.data.iter() {
            if type_id != TypeId::NONE {
                data.insert(symbol_id, type_id);
            }
        }
    }
}

impl Default for SymbolTypeCache {
    fn default() -> Self {
        Self::new()
    }
}
