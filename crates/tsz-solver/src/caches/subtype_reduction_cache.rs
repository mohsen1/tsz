//! Cross-call cache for `remove_subtypes_for_bct`.
//!
//! Mirrors the `instantiation_cache` shape. Callers build a
//! `SubtypeReductionRequest` from the input `TypeId`s and explicit
//! `SubtypeReductionOptions`; the cache key stores the sorted type list plus
//! a small `mode_bits` byte that captures request inputs other than the type
//! list which can affect the reduction result.
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
//! `types` and whether nominal hierarchy resolution is enabled; registered
//! base-type facts are stable for the lifetime of a per-file `QueryCache`,
//! so encoding that option in `mode_bits` is sufficient. The cache lives on
//! `QueryCache` (not `TypeInterner`) for the same reason as the instantiation
//! cache: `QueryCache::clear()` is the authoritative invalidation boundary.

use crate::types::TypeId;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::cell::RefCell;
use std::sync::Arc;

/// Mode bit: nominal hierarchy resolution is enabled for `remove_subtypes_for_bct`.
///
/// Nominal class-hierarchy lookups in the underlying `SubtypeChecker` can
/// change the reduction result (e.g., `Derived` is reduced away from
/// `[Base, Derived]` only when the checker can resolve the inheritance edge).
pub const MODE_NOMINAL_HIERARCHY_RESOLUTION: u8 = 0b001;

/// Option-sensitive inputs for `remove_subtypes_for_bct` cache identity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SubtypeReductionOptions {
    nominal_hierarchy_resolution: bool,
}

impl SubtypeReductionOptions {
    /// Build default options for a subtype-reduction request.
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            nominal_hierarchy_resolution: false,
        }
    }

    /// Enable or disable nominal hierarchy resolution for this request.
    #[must_use]
    pub(crate) const fn with_nominal_hierarchy_resolution(mut self, enabled: bool) -> Self {
        self.nominal_hierarchy_resolution = enabled;
        self
    }

    /// Pack option-sensitive inputs into the cache-key mode byte.
    #[must_use]
    const fn mode_bits(self) -> u8 {
        if self.nominal_hierarchy_resolution {
            MODE_NOMINAL_HIERARCHY_RESOLUTION
        } else {
            0
        }
    }
}

/// Typed request for `remove_subtypes_for_bct` cache probes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SubtypeReductionRequest<'a> {
    types: &'a [TypeId],
    options: SubtypeReductionOptions,
}

impl<'a> SubtypeReductionRequest<'a> {
    /// Build a request from the input candidate list.
    #[must_use]
    pub(crate) const fn new(types: &'a [TypeId]) -> Self {
        Self {
            types,
            options: SubtypeReductionOptions::new(),
        }
    }

    /// Override option-sensitive request inputs.
    #[must_use]
    pub(crate) const fn with_options(mut self, options: SubtypeReductionOptions) -> Self {
        self.options = options;
        self
    }

    /// Enable or disable nominal hierarchy resolution for this request.
    #[must_use]
    pub(crate) const fn with_nominal_hierarchy_resolution(self, enabled: bool) -> Self {
        self.with_options(self.options.with_nominal_hierarchy_resolution(enabled))
    }

    /// Borrow the original input candidate list.
    #[cfg(test)]
    #[must_use]
    const fn types(self) -> &'a [TypeId] {
        self.types
    }

    /// Derive the option-sensitive cache key for this request.
    #[must_use]
    pub(crate) fn cache_key(self) -> SubtypeReductionKey {
        SubtypeReductionKey::from_request(self)
    }
}

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
    /// Bit 0 (`MODE_NOMINAL_HIERARCHY_RESOLUTION`): nominal hierarchy
    /// resolution is enabled.
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

    /// Construct a cache key from a typed subtype-reduction request.
    #[must_use]
    fn from_request(request: SubtypeReductionRequest<'_>) -> Self {
        Self::new(
            SortedTypeIds::from_slice(request.types),
            request.options.mode_bits(),
        )
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
#[derive(Default)]
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

    fn cache_key(values: &[u32], options: SubtypeReductionOptions) -> SubtypeReductionKey {
        let types: Vec<TypeId> = values.iter().copied().map(type_id).collect();
        SubtypeReductionRequest::new(&types)
            .with_options(options)
            .cache_key()
    }

    fn cache_key_for_nominal_hierarchy(values: &[u32], enabled: bool) -> SubtypeReductionKey {
        cache_key(
            values,
            SubtypeReductionOptions::new().with_nominal_hierarchy_resolution(enabled),
        )
    }

    #[test]
    fn empty_cache_misses() {
        let cache = SubtypeReductionCache::new();
        let key = cache_key_for_nominal_hierarchy(&[1, 2], false);
        assert!(cache.lookup(&key).is_none());
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn insert_then_lookup_roundtrip() {
        let cache = SubtypeReductionCache::new();
        let key = cache_key_for_nominal_hierarchy(&[1, 2], false);
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
        let k_ab = cache_key_for_nominal_hierarchy(&[3, 1, 2], false);
        let k_ba = cache_key_for_nominal_hierarchy(&[1, 2, 3], false);
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
        let k_12 = cache_key_for_nominal_hierarchy(&[1, 2], false);
        let k_13 = cache_key_for_nominal_hierarchy(&[1, 3], false);
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
    fn mode_bits_isolate_nominal_hierarchy_resolution_from_default() {
        // Same TypeIds, different nominal-hierarchy-resolution flag →
        // distinct entries. This guards against caching a structural-only
        // result and serving it when class-hierarchy resolution is enabled
        // (which can change the outcome).
        let cache = SubtypeReductionCache::new();
        let default = cache_key_for_nominal_hierarchy(&[1, 2], false);
        let nominal = cache_key_for_nominal_hierarchy(&[1, 2], true);
        assert_ne!(default, nominal);
        assert_eq!(default.mode_bits, 0);
        assert_eq!(nominal.mode_bits, MODE_NOMINAL_HIERARCHY_RESOLUTION);
        cache.insert(default.clone(), arc_slice(&[1, 2]));
        cache.insert(nominal.clone(), arc_slice(&[1]));
        assert_eq!(
            &cache.lookup(&default).expect("default entry was inserted")[..],
            &[type_id(1), type_id(2)]
        );
        assert_eq!(
            &cache.lookup(&nominal).expect("nominal entry was inserted")[..],
            &[type_id(1)]
        );
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn clear_empties_cache() {
        let cache = SubtypeReductionCache::new();
        let key = cache_key_for_nominal_hierarchy(&[7], false);
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

    #[test]
    fn request_cache_key_owns_option_packing() {
        let types = [type_id(9), type_id(4), type_id(7)];
        let request = SubtypeReductionRequest::new(&types).with_nominal_hierarchy_resolution(true);
        let key = request.cache_key();

        assert_eq!(request.types(), &types);
        assert_eq!(
            key.sorted_type_ids.as_slice(),
            &[type_id(4), type_id(7), type_id(9)]
        );
        assert_eq!(key.mode_bits, MODE_NOMINAL_HIERARCHY_RESOLUTION);
    }

    #[test]
    fn default_options_use_zero_mode_bits() {
        assert_eq!(SubtypeReductionOptions::new().mode_bits(), 0);
        assert_eq!(
            SubtypeReductionOptions::new()
                .with_nominal_hierarchy_resolution(true)
                .mode_bits(),
            MODE_NOMINAL_HIERARCHY_RESOLUTION
        );
    }
}
