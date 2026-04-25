//! Cross-call cache for `instantiate_type`.
//!
//! Storage and key types for the `instantiate_type` memoization cache. PR 2/4
//! of the `docs/plan/ROADMAP.md` instantiation-cache workstream. This PR ships the
//! plumbing only — the cache exists on `QueryCache` and is reachable via the
//! `QueryDatabase` trait, but no production entry point probes it yet. PR 3/4
//! will wire it into the five `instantiate_type*` entry points.
//!
//! ### Key shape
//!
//! ```text
//! InstantiationCacheKey = (TypeId, CanonicalSubst, u8 mode_bits, Option<TypeId>)
//! ```
//!
//! - `TypeId` — the source type being substituted into.
//! - `CanonicalSubst` — the substitution as a `SmallVec` of `(Atom, TypeId)`
//!   pairs sorted by `Atom`, so two `TypeSubstitution`s with the same
//!   `{name -> type_id}` multiset hash and compare equal regardless of the
//!   underlying `FxHashMap` insertion order. The pairs live directly in the
//!   key — see the design doc §1 ("Why no `TypeInterner` intern handle") for
//!   why we deliberately do not intern substitutions on `TypeInterner`.
//! - `mode_bits` packs the three boolean flags on `TypeInstantiator`:
//!   - bit 0: `substitute_infer`
//!   - bit 1: `preserve_meta_types`
//!   - bit 2: `preserve_unsubstituted_type_params`
//! - `Option<TypeId>` carries `this_type` when set (used by
//!   `substitute_this_type`, where the substitution itself is empty but the
//!   `this_type` slot is populated).
//!
//! ### Why on `QueryCache` and not `TypeInterner`
//!
//! `QueryCache::clear()` is the authoritative cache-invalidation boundary;
//! `TypeInterner` survives clears and is not counted in
//! `estimated_size_bytes`. A substitution-keyed cache on `TypeInterner` would
//! grow unbounded on large repos. Per the design doc §2, cache hooks live on
//! `QueryDatabase` (not `TypeDatabase`) so the boundary stays clean.

use crate::types::TypeId;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::cell::RefCell;
use tsz_common::interner::Atom;

/// Canonical, content-hashable form of a `TypeSubstitution`.
///
/// Wraps the `SmallVec<[(Atom, TypeId); 4]>` returned by
/// `TypeSubstitution::canonical_pairs()` (added in PR 1, #1040). The pairs
/// are sorted by `Atom`, so two substitutions with the same
/// `{name -> type_id}` entries always produce equal `CanonicalSubst` values
/// regardless of insertion order.
///
/// `Hash`, `PartialEq`, `Eq`, `Clone`, and `Debug` are derived directly on
/// the wrapped `SmallVec`. The inline buffer of 4 entries matches the shape
/// of the existing `application_eval_cache` and avoids heap allocation for
/// the common case (most substitutions have 1-4 entries).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct CanonicalSubst(pub SmallVec<[(Atom, TypeId); 4]>);

impl CanonicalSubst {
    /// Construct from a canonical-pairs `SmallVec`.
    ///
    /// The caller is responsible for ensuring the input is sorted by `Atom`
    /// (typically by going through `TypeSubstitution::canonical_pairs()`).
    #[must_use]
    pub const fn from_pairs(pairs: SmallVec<[(Atom, TypeId); 4]>) -> Self {
        Self(pairs)
    }

    /// Empty substitution — used for `substitute_this_type` calls where the
    /// substitution map is empty but `this_type` is non-empty.
    #[must_use]
    pub fn empty() -> Self {
        Self(SmallVec::new())
    }

    /// Returns `true` if the substitution has no `(name, type_id)` pairs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of `(name, type_id)` pairs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Borrow the underlying canonical pairs.
    #[must_use]
    pub fn as_slice(&self) -> &[(Atom, TypeId)] {
        self.0.as_slice()
    }
}

/// Key for the `instantiate_type` cross-call cache.
///
/// See the module docs for the full key layout.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InstantiationCacheKey {
    /// Source type being substituted into.
    pub type_id: TypeId,
    /// Canonicalized substitution pairs.
    pub subst: CanonicalSubst,
    /// Packed instantiator flags (bit 0: `substitute_infer`, bit 1: `preserve_meta_types`,
    /// bit 2: `preserve_unsubstituted_type_params`).
    pub mode_bits: u8,
    /// Optional `this_type` substitution (carried by `substitute_this_type`).
    pub this_type: Option<TypeId>,
}

impl InstantiationCacheKey {
    /// Construct a cache key from its parts.
    #[must_use]
    pub const fn new(
        type_id: TypeId,
        subst: CanonicalSubst,
        mode_bits: u8,
        this_type: Option<TypeId>,
    ) -> Self {
        Self {
            type_id,
            subst,
            mode_bits,
            this_type,
        }
    }
}

/// Cross-call memoization cache for `instantiate_type`.
///
/// Owned by `QueryCache`. Single-threaded (`RefCell` rather than `RwLock`)
/// for the same reason as the surrounding caches: a per-file `QueryCache`
/// is borrowed for the duration of a check and never crossed by Rayon
/// workers.
pub struct InstantiationCache {
    inner: RefCell<FxHashMap<InstantiationCacheKey, TypeId>>,
}

impl InstantiationCache {
    /// Create an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(FxHashMap::default()),
        }
    }

    /// Look up an entry by key. Returns `None` if no entry exists.
    ///
    /// Wired into the `instantiate_type*` entry points by PR 3/4 of the
    /// cache plan. PR 2 ships the storage only.
    #[allow(dead_code)]
    pub fn lookup(&self, key: &InstantiationCacheKey) -> Option<TypeId> {
        self.inner.borrow().get(key).copied()
    }

    /// Insert (or overwrite) an entry.
    ///
    /// Wired into the `instantiate_type*` entry points by PR 3/4 of the
    /// cache plan. PR 2 ships the storage only.
    #[allow(dead_code)]
    pub fn insert(&self, key: InstantiationCacheKey, result: TypeId) {
        self.inner.borrow_mut().insert(key, result);
    }

    /// Clear all cached entries.
    pub fn clear(&self) {
        self.inner.borrow_mut().clear();
    }

    /// Number of cached entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.borrow().len()
    }

    /// Returns `true` if the cache is empty.
    #[must_use]
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.inner.borrow().is_empty()
    }

    /// Capacity of the underlying `FxHashMap`. Used by
    /// `QueryCache::estimated_size_bytes` to size-account the cache.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.inner.borrow().capacity()
    }
}

impl Default for InstantiationCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_common::interner::Atom;

    fn atom(value: u32) -> Atom {
        // Construct a synthetic Atom directly. The integer payload is opaque
        // to this cache — only `Eq`/`Hash` matter — so we don't need a real
        // `TypeInterner` to drive the keying.
        Atom(value)
    }

    fn type_id(value: u32) -> TypeId {
        TypeId(value)
    }

    fn canonical(pairs: &[(u32, u32)]) -> CanonicalSubst {
        let mut sv: SmallVec<[(Atom, TypeId); 4]> =
            pairs.iter().map(|&(a, t)| (atom(a), type_id(t))).collect();
        sv.sort_unstable_by_key(|(name, _)| *name);
        CanonicalSubst::from_pairs(sv)
    }

    #[test]
    fn test_cache_default_returns_none() {
        // An empty cache must miss on every lookup.
        let cache = InstantiationCache::new();
        let key = InstantiationCacheKey::new(type_id(10), canonical(&[(1, 100)]), 0, None);
        assert_eq!(cache.lookup(&key), None);
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_insert_lookup_roundtrip() {
        // Insert then lookup must return the inserted TypeId.
        let cache = InstantiationCache::new();
        let key = InstantiationCacheKey::new(type_id(10), canonical(&[(1, 100)]), 0, None);
        let result = type_id(200);
        cache.insert(key.clone(), result);
        assert_eq!(cache.lookup(&key), Some(result));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_cache_distinct_keys_disjoint() {
        // Different mode_bits, different this_type, and different
        // CanonicalSubst values must each produce distinct cache entries.
        let cache = InstantiationCache::new();

        let base_subst = canonical(&[(1, 100)]);
        let other_subst = canonical(&[(2, 100)]);
        let same_subst_diff_type = canonical(&[(1, 101)]);

        // Distinct mode_bits.
        let k_mode_a = InstantiationCacheKey::new(type_id(10), base_subst.clone(), 0b000, None);
        let k_mode_b = InstantiationCacheKey::new(type_id(10), base_subst.clone(), 0b001, None);
        // Distinct this_type.
        let k_this_none = InstantiationCacheKey::new(type_id(10), base_subst.clone(), 0b000, None);
        let k_this_some =
            InstantiationCacheKey::new(type_id(10), base_subst.clone(), 0b000, Some(type_id(42)));
        // Distinct CanonicalSubst (different atom and different type_id).
        let k_subst_a = InstantiationCacheKey::new(type_id(10), base_subst, 0b000, None);
        let k_subst_b = InstantiationCacheKey::new(type_id(10), other_subst, 0b000, None);
        let k_subst_c = InstantiationCacheKey::new(type_id(10), same_subst_diff_type, 0b000, None);

        cache.insert(k_mode_a.clone(), type_id(1));
        cache.insert(k_mode_b.clone(), type_id(2));
        cache.insert(k_this_some.clone(), type_id(3));
        cache.insert(k_subst_b.clone(), type_id(4));
        cache.insert(k_subst_c.clone(), type_id(5));

        // k_mode_a == k_this_none == k_subst_a; that's the same slot, so the
        // insert above for k_mode_a populates all three.
        assert_eq!(cache.lookup(&k_mode_a), Some(type_id(1)));
        assert_eq!(cache.lookup(&k_this_none), Some(type_id(1)));
        assert_eq!(cache.lookup(&k_subst_a), Some(type_id(1)));

        // The other distinct keys must hold their own values.
        assert_eq!(cache.lookup(&k_mode_b), Some(type_id(2)));
        assert_eq!(cache.lookup(&k_this_some), Some(type_id(3)));
        assert_eq!(cache.lookup(&k_subst_b), Some(type_id(4)));
        assert_eq!(cache.lookup(&k_subst_c), Some(type_id(5)));

        // 5 distinct keys (the three k_*_a aliases collapse into one entry).
        assert_eq!(cache.len(), 5);
    }

    #[test]
    fn test_cache_clear_empties() {
        let cache = InstantiationCache::new();
        let key = InstantiationCacheKey::new(type_id(10), canonical(&[(1, 100)]), 0, None);
        cache.insert(key.clone(), type_id(200));
        assert_eq!(cache.len(), 1);
        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.lookup(&key), None);
    }

    #[test]
    fn test_canonical_subst_equal_for_same_pairs() {
        // CanonicalSubst constructed from the same sorted pairs must compare
        // equal and hash equal.
        let a = canonical(&[(1, 100), (2, 200)]);
        let b = canonical(&[(2, 200), (1, 100)]); // canonical() sorts internally
        assert_eq!(a, b);

        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut ha = DefaultHasher::new();
        a.hash(&mut ha);
        let mut hb = DefaultHasher::new();
        b.hash(&mut hb);
        assert_eq!(ha.finish(), hb.finish());
    }

    #[test]
    fn test_canonical_subst_empty_helpers() {
        let empty = CanonicalSubst::empty();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        assert!(empty.as_slice().is_empty());
    }
}
