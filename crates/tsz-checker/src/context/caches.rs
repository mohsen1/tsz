use rustc_hash::FxHashMap;
use tsz_binder::SymbolId;
use tsz_solver::TypeId;

/// Dense flat-vec cache for node-index-keyed `TypeId` lookups.
#[derive(Clone, Debug)]
pub struct NodeTypeCache {
    data: Vec<TypeId>,
}

impl NodeTypeCache {
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: vec![TypeId::NONE; capacity],
        }
    }

    #[inline]
    pub const fn new() -> Self {
        Self { data: Vec::new() }
    }

    #[inline]
    pub fn get(&self, key: &u32) -> Option<&TypeId> {
        let idx = *key as usize;
        self.data.get(idx).filter(|t| **t != TypeId::NONE)
    }

    #[inline]
    pub fn insert(&mut self, key: u32, value: TypeId) {
        if key == u32::MAX {
            return;
        }
        let idx = key as usize;
        if idx >= self.data.len() {
            self.data.resize(idx + 1, TypeId::NONE);
        }
        self.data[idx] = value;
    }

    #[inline]
    pub fn contains_key(&self, key: &u32) -> bool {
        let idx = *key as usize;
        self.data.get(idx).is_some_and(|t| *t != TypeId::NONE)
    }

    #[inline]
    pub fn remove(&mut self, key: &u32) -> Option<TypeId> {
        let idx = *key as usize;
        if let Some(slot) = self.data.get_mut(idx)
            && *slot != TypeId::NONE
        {
            let old = *slot;
            *slot = TypeId::NONE;
            return Some(old);
        }
        None
    }

    #[inline]
    pub fn or_insert(&mut self, key: u32, value: TypeId) -> TypeId {
        let idx = key as usize;
        if idx >= self.data.len() {
            self.data.resize(idx + 1, TypeId::NONE);
        }
        if self.data[idx] == TypeId::NONE {
            self.data[idx] = value;
        }
        self.data[idx]
    }

    pub fn iter(&self) -> impl Iterator<Item = (u32, TypeId)> + '_ {
        self.data
            .iter()
            .enumerate()
            .filter(|(_, t)| **t != TypeId::NONE)
            .map(|(i, t)| (i as u32, *t))
    }

    pub fn clear(&mut self) {
        self.data.fill(TypeId::NONE);
    }

    pub fn merge(&mut self, other: &Self) {
        if other.data.len() > self.data.len() {
            self.data.resize(other.data.len(), TypeId::NONE);
        }
        for (i, &t) in other.data.iter().enumerate() {
            if t != TypeId::NONE {
                self.data[i] = t;
            }
        }
    }

    pub fn merge_owned(&mut self, other: Self) {
        if other.data.len() > self.data.len() {
            self.data.resize(other.data.len(), TypeId::NONE);
        }
        for (i, t) in other.data.into_iter().enumerate() {
            if t != TypeId::NONE {
                self.data[i] = t;
            }
        }
    }

    pub fn extend<I: IntoIterator<Item = (u32, TypeId)>>(&mut self, iter: I) {
        for (key, value) in iter {
            self.insert(key, value);
        }
    }

    pub fn len(&self) -> usize {
        self.data.iter().filter(|t| **t != TypeId::NONE).count()
    }

    pub fn is_empty(&self) -> bool {
        self.data.iter().all(|t| *t == TypeId::NONE)
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

/// Dense flat-vec cache for `SymbolId -> TypeId` lookups.
#[derive(Clone, Debug)]
pub struct SymbolTypeCache {
    data: Vec<TypeId>,
}

impl SymbolTypeCache {
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: vec![TypeId::NONE; capacity],
        }
    }

    #[inline]
    pub const fn new() -> Self {
        Self { data: Vec::new() }
    }

    #[inline]
    pub fn get(&self, key: &SymbolId) -> Option<&TypeId> {
        let idx = key.0 as usize;
        self.data.get(idx).filter(|t| **t != TypeId::NONE)
    }

    #[inline]
    pub fn insert(&mut self, key: SymbolId, value: TypeId) {
        let idx = key.0 as usize;
        if idx >= self.data.len() {
            self.data.resize(idx + 1, TypeId::NONE);
        }
        self.data[idx] = value;
    }

    #[inline]
    pub fn contains_key(&self, key: &SymbolId) -> bool {
        let idx = key.0 as usize;
        self.data.get(idx).is_some_and(|t| *t != TypeId::NONE)
    }

    #[inline]
    pub fn remove(&mut self, key: &SymbolId) -> Option<TypeId> {
        let idx = key.0 as usize;
        if let Some(slot) = self.data.get_mut(idx)
            && *slot != TypeId::NONE
        {
            let old = *slot;
            *slot = TypeId::NONE;
            return Some(old);
        }
        None
    }

    #[inline]
    pub fn entry_or_insert(&mut self, key: SymbolId, value: TypeId) -> TypeId {
        let idx = key.0 as usize;
        if idx >= self.data.len() {
            self.data.resize(idx + 1, TypeId::NONE);
        }
        if self.data[idx] == TypeId::NONE {
            self.data[idx] = value;
        }
        self.data[idx]
    }

    pub fn len(&self) -> usize {
        self.data.iter().filter(|t| **t != TypeId::NONE).count()
    }

    pub fn is_empty(&self) -> bool {
        self.data.iter().all(|t| *t == TypeId::NONE)
    }

    pub fn iter(&self) -> impl Iterator<Item = (SymbolId, TypeId)> + '_ {
        self.data
            .iter()
            .enumerate()
            .filter(|(_, t)| **t != TypeId::NONE)
            .map(|(i, t)| (SymbolId(i as u32), *t))
    }

    pub fn to_hash_map(&self) -> FxHashMap<SymbolId, TypeId> {
        self.iter().collect()
    }

    pub fn extend(&mut self, other: Self) {
        if other.data.len() > self.data.len() {
            self.data.resize(other.data.len(), TypeId::NONE);
        }
        for (i, t) in other.data.into_iter().enumerate() {
            if t != TypeId::NONE {
                self.data[i] = t;
            }
        }
    }
}

impl Default for SymbolTypeCache {
    fn default() -> Self {
        Self::new()
    }
}
