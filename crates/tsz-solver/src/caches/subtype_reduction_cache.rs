//! Cross-call cache for `remove_subtypes_for_bct`.
//!
//! Mirrors the `instantiation_cache` shape (PR 3/4 of
//! `docs/plan/perf-instantiate-type-cache-design.md`). The key is the
//! sorted list of input `TypeId`s plus a small `mode_bits` byte that
//! captures any inputs other than the type list which can affect the
//! reduction result (currently: whether a `TypeResolver` was provided,
//! which enables nominal class-hierarchy subtype resolution).
//!
//! ### Why a memo cache here
//!
//! `remove_subtypes_for_bct` is the O(N²) hot loop in `compute_best_common_type`
//! (see `crates/tsz-solver/src/operations/expression_ops.rs`). For BCT
//! workloads with ~200 sibling candidate classes (e.g., the
//! `BCT candidates=200` bench fixture), the function performs 200 × 199
//! pairwise subtype checks per call site, and the same fixture exercises
//! four call sites with very similar 200-element lists. Caching the
//! reduced result by sorted-`TypeId` collapses the second through fourth
//! calls to O(1).
//!
//! Subtype reduction is correctness-critical: the value cached here flows
//! into `interner.union(reduced)`, so the cache key must capture every
//! input that affects the result. `remove_subtypes_for_bct` reads only
//! `types` and (optionally) `resolver`; the resolver's identity is stable
//! for the lifetime of a per-file `QueryCache`, so encoding "resolver
//! present / absent" in `mode_bits` is sufficient. The cache lives on
//! `QueryCache` (not `TypeInterner`) for the same reason as the
//! instantiation cache: `QueryCache::clear()` is the authoritative
//! invalidation boundary.

use crate::types::TypeId;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::cell::RefCell;
use std::sync::Arc;

/// Mode bit: a `TypeResolver` was provided to `remove_subtypes_for_bct`.
///
/// The presence of a resolver enables nominal class-hierarchy lookups in
/// the underlying `SubtypeChecker`, which can change the reduction result
/// (e.g., `Derived` is reduced away from `[Base, Derived]` only when the
/// checker can resolve the inheritance edge).
pub const MODE_HAS_RESOLVER: u8 = 0b001;

/// Canonical, content-hashable form of a sorted `&[TypeId]` input.
///
/// The `SmallVec` inline buffer of 8 keeps the common case (small
/// element-count BCT calls from array literals, conditionals, etc.)
/// allocation-free; large lists (the BCT stress fixture uses ~200) spill
/// to heap exactly once when the key is first constructed and are then
/// kept inside the cache by `Arc`-cloning the value side.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct SortedTypeIds(pub SmallVec<[TypeId; 8]>);

impl SortedTypeIds {
    /// Construct a `SortedTypeIds` by copying and sorting an input slice.
    ///
    /// `O(N log N)` once per cache probe — paid only on the first call;
    /// subsequent identical calls hit the cache in `O(N)` (hash) without
    /// re-running the O(N²) subtype loop.
    #[must_use]
    pub fn from_slice(types: &[TypeId]) -> Self {
        let mut buf: SmallVec<[TypeId; 8]> = SmallVec::from_slice(types);
        buf.sort_unstable_by_key(|id| id.0);
        Self(buf)
    }

    /// Number of `TypeId`s in the canonical key.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the canonical key has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Borrow the underlying sorted `TypeId` slice.
    #[must_use]
    pub fn as_slice(&self) -> &[TypeId] {
        self.0.as_slice()
    }
}

/// Key for the `remove_subtypes_for_bct` cross-call cache.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SubtypeReductionKey {
    /// Sorted input `TypeId`s — equivalent to tsc's `getTypeListId(types)`.
    pub sorted_type_ids: SortedTypeIds,
    /// Bitfield of inputs other than `types` that can affect the result.
    /// Bit 0 (`MODE_HAS_RESOLVER`): a `TypeResolver` was provided.
    pub mode_bits: u8,
}

impl SubtypeReductionKey {
    /// Construct a cache key from its parts.
    #[must_use]
    pub const fn new(sorted_type_ids: SortedTypeIds, mode_bits: u8) -> Self {
        Self {
            sorted_type_ids,
            mode_bits,
        }
    }

    /// Convenience constructor that sorts the input slice on the caller's
    /// behalf and packs the resolver-present flag.
    #[must_use]
    pub fn build(types: &[TypeId], has_resolver: bool) -> Self {
        let mode_bits = if has_resolver { MODE_HAS_RESOLVER } else { 0 };
        Self::new(SortedTypeIds::from_slice(types), mode_bits)
    }
}

/// Cross-call memoization cache for `remove_subtypes_for_bct`.
///
/// Owned by `QueryCache`. Single-threaded (`RefCell`) for the same reason
/// as the surrounding caches: a per-file `QueryCache` is borrowed for the
/// duration of a check and never crossed by Rayon workers.
///
/// The value side is `Arc<[TypeId]>` so cache hits return a cheap clone of
/// a heap-allocated slice instead of re-allocating a `Vec`.
pub struct SubtypeReductionCache {
    inner: RefCell<FxHashMap<SubtypeReductionKey, Arc<[TypeId]>>>,
}

impl SubtypeReductionCache {
    /// Create an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: RefCell::new(FxHashMap::default()),
        }
    }

    /// Look up an entry by key. Returns `None` if no entry exists.
    pub fn lookup(&self, key: &SubtypeReductionKey) -> Option<Arc<[TypeId]>> {
        self.inner.borrow().get(key).cloned()
    }

    /// Insert (or overwrite) an entry.
    pub fn insert(&self, key: SubtypeReductionKey, result: Arc<[TypeId]>) {
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

impl Default for SubtypeReductionCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_id(value: u32) -> TypeId {
        TypeId(value)
    }

    fn arc_slice(values: &[u32]) -> Arc<[TypeId]> {
        let v: Vec<TypeId> = values.iter().copied().map(type_id).collect();
        Arc::from(v)
    }

    #[test]
    fn empty_cache_misses() {
        let cache = SubtypeReductionCache::new();
        let key = SubtypeReductionKey::build(&[type_id(1), type_id(2)], false);
        assert!(cache.lookup(&key).is_none());
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn insert_then_lookup_roundtrip() {
        let cache = SubtypeReductionCache::new();
        let key = SubtypeReductionKey::build(&[type_id(1), type_id(2)], false);
        let value = arc_slice(&[1, 2]);
        cache.insert(key.clone(), value.clone());
        let got = cache.lookup(&key).expect("hit");
        assert_eq!(&got[..], &value[..]);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn order_independence_of_input_slice() {
        // Two slices with the same set of TypeIds in different orders must
        // hash to the same cache slot — that's the whole point of the
        // sorted-key form (mirrors tsc's getTypeListId).
        let cache = SubtypeReductionCache::new();
        let k_ab = SubtypeReductionKey::build(&[type_id(3), type_id(1), type_id(2)], false);
        let k_ba = SubtypeReductionKey::build(&[type_id(1), type_id(2), type_id(3)], false);
        assert_eq!(k_ab, k_ba);
        cache.insert(k_ab, arc_slice(&[1, 2, 3]));
        assert!(cache.lookup(&k_ba).is_some());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn distinct_lists_do_not_alias() {
        // {1, 2} and {1, 3} must produce distinct cache entries even
        // though they share an element.
        let cache = SubtypeReductionCache::new();
        let k_12 = SubtypeReductionKey::build(&[type_id(1), type_id(2)], false);
        let k_13 = SubtypeReductionKey::build(&[type_id(1), type_id(3)], false);
        assert_ne!(k_12, k_13);
        cache.insert(k_12.clone(), arc_slice(&[1, 2]));
        cache.insert(k_13.clone(), arc_slice(&[1, 3]));
        let v_12 = cache.lookup(&k_12).expect("hit");
        let v_13 = cache.lookup(&k_13).expect("hit");
        assert_eq!(&v_12[..], &[type_id(1), type_id(2)]);
        assert_eq!(&v_13[..], &[type_id(1), type_id(3)]);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn mode_bits_isolate_resolver_present_from_absent() {
        // Same TypeIds, different `has_resolver` flag → distinct entries.
        // This guards against caching a no-resolver result and serving it
        // when class-hierarchy resolution is enabled (which can change the
        // outcome).
        let cache = SubtypeReductionCache::new();
        let no_res = SubtypeReductionKey::build(&[type_id(1), type_id(2)], false);
        let with_res = SubtypeReductionKey::build(&[type_id(1), type_id(2)], true);
        assert_ne!(no_res, with_res);
        cache.insert(no_res.clone(), arc_slice(&[1, 2]));
        cache.insert(with_res.clone(), arc_slice(&[1]));
        assert_eq!(
            &cache.lookup(&no_res).expect("no_res entry was inserted")[..],
            &[type_id(1), type_id(2)]
        );
        assert_eq!(
            &cache
                .lookup(&with_res)
                .expect("with_res entry was inserted")[..],
            &[type_id(1)]
        );
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn clear_empties_cache() {
        let cache = SubtypeReductionCache::new();
        let key = SubtypeReductionKey::build(&[type_id(7)], false);
        cache.insert(key.clone(), arc_slice(&[7]));
        assert_eq!(cache.len(), 1);
        cache.clear();
        assert!(cache.is_empty());
        assert!(cache.lookup(&key).is_none());
    }

    #[test]
    fn sorted_type_ids_helpers() {
        let s = SortedTypeIds::from_slice(&[type_id(3), type_id(1), type_id(2)]);
        assert_eq!(s.len(), 3);
        assert!(!s.is_empty());
        assert_eq!(s.as_slice(), &[type_id(1), type_id(2), type_id(3)]);
    }
}
