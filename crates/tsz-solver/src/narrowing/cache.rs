use crate::narrowing::generation_memo::GenerationMemo;
use crate::narrowing::request::NarrowTypeStableCacheKey;
use crate::types::TypeId;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use std::cell::{Cell, RefCell};
use std::sync::Arc;
use tsz_common::interner::Atom;

type SplitNullishParts = (Option<TypeId>, Option<TypeId>);
type DiscriminantMembers = FxHashMap<TypeId, Vec<TypeId>>;
type DiscriminantIndex = FxHashMap<(TypeId, Atom), Arc<DiscriminantMembers>>;
type PropertyCacheKey = (TypeId, Atom);
type NarrowedPropertyCache = GenerationMemo<PropertyCacheKey, Option<CachedPropertyType>>;
type RequiredPropertyCache = GenerationMemo<PropertyCacheKey, bool>;
type OptionalChainCache = GenerationMemo<PropertyCacheKey, TypeId>;
type OptionalPropertyChainCache = GenerationMemo<OptionalPropertyChainKey, TypeId>;

/// Cache key for a successful identifier-rooted optional property chain.
///
/// The root is semantic (`TypeId`), while the path uses interned property
/// atoms plus a bit mask for which path segments used `?.`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OptionalPropertyChainKey {
    pub root_type: TypeId,
    pub properties: Vec<Atom>,
    pub optional_mask: u64,
    pub no_unchecked_indexed_access: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CachedPropertyType {
    pub type_id: TypeId,
    pub from_index_signature: bool,
}

impl CachedPropertyType {
    pub const fn new(type_id: TypeId, from_index_signature: bool) -> Self {
        Self {
            type_id,
            from_index_signature,
        }
    }

    pub const fn explicit(type_id: TypeId) -> Self {
        Self {
            type_id,
            from_index_signature: false,
        }
    }

    pub const fn index_signature(type_id: TypeId) -> Self {
        Self {
            type_id,
            from_index_signature: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NarrowingCacheStatistics {
    pub resolve_cache_entries: usize,
    pub narrowed_property_cache_entries: usize,
    pub required_property_cache_entries: usize,
    pub split_nullish_cache_entries: usize,
    pub contains_type_parameters_cache_entries: usize,
    pub optional_chain_cache_entries: usize,
    pub optional_property_chain_cache_entries: usize,
    pub contextual_resolve_cache_entries: usize,
    pub discriminant_index_entries: usize,
    pub narrow_type_cache_entries: usize,
    pub narrow_excluding_cache_entries: usize,
    pub narrow_assignable_cache_entries: usize,
    pub narrow_subtype_cache_entries: usize,
    pub generation_stamped_cache_keys: usize,
    pub max_generation_slots_per_cache_key: usize,
    pub estimated_size_bytes: usize,
}

impl NarrowingCacheStatistics {
    #[must_use]
    pub const fn total_entries(self) -> usize {
        self.resolve_cache_entries
            + self.narrowed_property_cache_entries
            + self.required_property_cache_entries
            + self.split_nullish_cache_entries
            + self.contains_type_parameters_cache_entries
            + self.optional_chain_cache_entries
            + self.optional_property_chain_cache_entries
            + self.contextual_resolve_cache_entries
            + self.discriminant_index_entries
            + self.narrow_type_cache_entries
            + self.narrow_excluding_cache_entries
            + self.narrow_assignable_cache_entries
            + self.narrow_subtype_cache_entries
    }
}

/// Narrowing context for type guards and control flow analysis.
/// Shared across multiple narrowing contexts to persist resolution results.
#[derive(Default, Clone, Debug)]
pub struct NarrowingCache {
    /// Cache for type resolution (Lazy/App/Template -> Structural)
    pub resolve_cache: RefCell<FxHashMap<TypeId, TypeId>>,
    /// In-progress type resolution set. `resolve_cache` only records completed
    /// resolutions, so recursive `keyof` / indexed-access / conditional graphs
    /// can re-enter before a cache entry exists. Returning the original deferred
    /// type on a cycle preserves generic form and prevents stack overflow.
    pub resolve_visiting: RefCell<FxHashSet<TypeId>>,
    /// Cache for top-level property type lookups (`TypeId`, resolver generation, `PropName`) -> `PropType`
    pub property_cache: RefCell<NarrowedPropertyCache>,
    /// Cache for required-property checks in `in`-operator negative narrowing
    /// (`obj` in `!("prop" in obj)`).
    pub required_property_cache: RefCell<RequiredPropertyCache>,
    /// Cache for split-nullish decomposition (TypeId -> (`non_nullish`, nullish)).
    /// Reused by checker optional-chain/property-access hot paths.
    pub split_nullish_cache: RefCell<FxHashMap<TypeId, SplitNullishParts>>,
    /// Cache for "type contains type parameters" checks.
    pub contains_type_parameters_cache: RefCell<FxHashMap<TypeId, bool>>,
    /// Cache for optional chain property access results.
    /// Keyed by `(object_type_with_nullish, property_atom, resolver_generation)`
    /// -> final result `TypeId`.
    /// Unlike `property_cache` which is keyed by resolved (non-nullish) base type,
    /// this caches the COMPLETE result including nullish union and undefined addition.
    /// This skips `split_nullish`, `resolve_type`, `contains_type_params`, and property
    /// lookup on cache hits, eliminating 4+ `RefCell` borrows per repeated access.
    pub optional_chain_cache: RefCell<OptionalChainCache>,
    /// Cache for full optional property chains such as
    /// `options?.nested?.transport?.backoff?.base`.
    ///
    /// This is keyed by semantic root type, atomized path, and resolver
    /// generation rather than by AST node, so repeated textual chains in
    /// generated code can reuse the final successful read result without
    /// re-walking every segment while still observing lazy resolver changes.
    pub optional_property_chain_cache: RefCell<OptionalPropertyChainCache>,
    /// Cache for contextual type resolution in object literal property typing.
    /// Maps raw contextual `TypeId` -> fully resolved `TypeId` after the
    /// evaluate/resolve/lazy/application chain. Avoids repeating the expensive
    /// chain for each property of the same object literal.
    pub contextual_resolve_cache: RefCell<FxHashMap<TypeId, TypeId>>,
    /// Discriminant index for fast switch-case narrowing.
    /// Key: (`union_type`, `discriminant_property`) -> Map of `literal_value` -> matching members.
    /// Built once per (union, property) pair, then O(1) lookup per case clause.
    /// Without this, each case clause iterates ALL union members (O(N) per case = O(N^2) total).
    pub discriminant_index: RefCell<DiscriminantIndex>,
    /// Cache for applying a semantic predicate guard to an input type.
    ///
    /// Keyed by input `TypeId`, predicate payload, branch sense, compiler
    /// option bits, and resolver generation so lazy alias changes cannot reuse
    /// stale predicate results. Other guard kinds keep their existing dynamic
    /// paths because their results depend on structural lookups that are already
    /// cached at narrower query boundaries.
    pub(crate) narrow_type_cache: RefCell<GenerationMemo<NarrowTypeStableCacheKey, TypeId>>,
    /// Memo for `NarrowingContext::narrow_excluding_type` keyed by
    /// `(source, excluded, resolver_generation)`.
    ///
    /// False-branch type-predicate narrowing over a recursive-schema union
    /// drives `narrow_excluding_type` into an exponential self-recursion:
    /// every intersection / type-parameter member re-enters the function on
    /// `(member, excluded)`, and recursive alias members expand the same
    /// `(source, excluded)` subtree at each depth. Memoizing collapses that
    /// re-expansion to linear; combined with `narrow_excluding_visiting` it is
    /// the structural fix for the non-terminating typebox row.
    pub(crate) narrow_excluding_cache: RefCell<GenerationMemo<NarrowExcludingStableKey, TypeId>>,
    /// In-progress `(source, excluded, resolver_generation)` set for
    /// `narrow_excluding_type`.
    pub(crate) narrow_excluding_visiting: RefCell<FxHashSet<NarrowExcludingKey>>,
    /// Memo for the narrowing-boundary assignability check keyed by
    /// `(source, target, resolver_generation)`.
    pub(crate) narrow_assignable_cache: RefCell<GenerationMemo<NarrowExcludingStableKey, bool>>,
    /// Memo for the narrowing-boundary subtype check keyed by
    /// `(source, target, resolver_generation)`.
    pub(crate) narrow_subtype_cache: RefCell<GenerationMemo<NarrowExcludingStableKey, bool>>,
    /// Re-entrancy depth of the exclusion-narrowing families.
    pub(crate) narrow_excluding_depth: Cell<u32>,
    /// Remaining cumulative work for the current outermost exclusion-narrowing
    /// request.
    pub(crate) narrow_excluding_fuel: Cell<u32>,
    /// Per-request work cap used to prime [`Self::narrow_excluding_fuel`]. `0`
    /// means "use [`NARROW_EXCLUDING_WORK_BUDGET`]"; tests and tuning may lower
    /// it to exercise the bail path deterministically.
    pub(crate) narrow_excluding_budget: Cell<u32>,
}

/// Per-request cumulative work bound shared by the exclusion-narrowing families.
///
/// One unit is charged per *fresh* (un-memoized) exclusion narrow. Real flow
/// narrowing performs at most a few hundred such steps per request, so the bound
/// is pure headroom on conforming code and only bites pathological breadth-fan
/// recursion.
pub(crate) const NARROW_EXCLUDING_WORK_BUDGET: u32 = 1_000_000;

/// Cache key for `NarrowingContext::narrow_excluding_type` and
/// `NarrowingContext::is_assignable_to`.
///
/// A `(source, target, resolver_generation)` triple. `resolver_generation` is
/// folded in so a later resolver that resolves a Lazy alias differently cannot
/// reuse a stale result, matching the keying discipline of
/// `NarrowTypeCacheKey`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub(crate) struct NarrowExcludingKey {
    pub(crate) source: TypeId,
    pub(crate) excluded: TypeId,
    pub(crate) resolver_generation: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub(crate) struct NarrowExcludingStableKey {
    pub(crate) source: TypeId,
    pub(crate) excluded: TypeId,
}

/// RAII guard for one exclusion-narrowing recursion frame.
pub(in crate::narrowing) struct ExclusionFrame<'b> {
    depth: &'b Cell<u32>,
    fuel: &'b Cell<u32>,
    prior: u32,
}

impl Drop for ExclusionFrame<'_> {
    fn drop(&mut self) {
        self.depth.set(self.prior);
        if self.prior == 0 {
            self.fuel.set(0);
        }
    }
}

/// RAII guard for one in-progress type-resolution key.
pub(in crate::narrowing) struct ResolveVisitGuard<'b> {
    visiting: &'b RefCell<FxHashSet<TypeId>>,
    type_id: TypeId,
}

impl Drop for ResolveVisitGuard<'_> {
    fn drop(&mut self) {
        self.visiting.borrow_mut().remove(&self.type_id);
    }
}

/// RAII guard for one in-progress exclusion-narrowing key.
pub(in crate::narrowing) struct NarrowExcludingVisitGuard<'b> {
    visiting: &'b RefCell<FxHashSet<NarrowExcludingKey>>,
    key: NarrowExcludingKey,
}

impl Drop for NarrowExcludingVisitGuard<'_> {
    fn drop(&mut self) {
        self.visiting.borrow_mut().remove(&self.key);
    }
}

impl NarrowingCache {
    pub fn new() -> Self {
        Self {
            resolve_cache: RefCell::new(FxHashMap::with_capacity_and_hasher(1024, FxBuildHasher)),
            resolve_visiting: RefCell::new(FxHashSet::default()),
            property_cache: RefCell::new(GenerationMemo::default()),
            required_property_cache: RefCell::new(GenerationMemo::default()),
            split_nullish_cache: RefCell::new(FxHashMap::with_capacity_and_hasher(
                512,
                FxBuildHasher,
            )),
            contains_type_parameters_cache: RefCell::new(FxHashMap::with_capacity_and_hasher(
                1024,
                FxBuildHasher,
            )),
            optional_chain_cache: RefCell::new(GenerationMemo::default()),
            optional_property_chain_cache: RefCell::new(GenerationMemo::default()),
            contextual_resolve_cache: RefCell::new(FxHashMap::with_capacity_and_hasher(
                256,
                FxBuildHasher,
            )),
            discriminant_index: RefCell::new(FxHashMap::default()),
            narrow_type_cache: RefCell::new(GenerationMemo::default()),
            narrow_excluding_cache: RefCell::new(GenerationMemo::default()),
            narrow_excluding_visiting: RefCell::new(FxHashSet::default()),
            narrow_assignable_cache: RefCell::new(GenerationMemo::default()),
            narrow_subtype_cache: RefCell::new(GenerationMemo::default()),
            narrow_excluding_depth: Cell::new(0),
            narrow_excluding_fuel: Cell::new(0),
            narrow_excluding_budget: Cell::new(0),
        }
    }

    #[must_use]
    pub fn cache_statistics(&self) -> NarrowingCacheStatistics {
        let property_cache = self.property_cache.borrow();
        let required_property_cache = self.required_property_cache.borrow();
        let narrow_type_cache = self.narrow_type_cache.borrow();
        let narrow_excluding_cache = self.narrow_excluding_cache.borrow();
        let narrow_assignable_cache = self.narrow_assignable_cache.borrow();
        let narrow_subtype_cache = self.narrow_subtype_cache.borrow();
        let generation_stamped_cache_keys = property_cache.key_count()
            + required_property_cache.key_count()
            + self.optional_chain_cache.borrow().key_count()
            + self.optional_property_chain_cache.borrow().key_count()
            + narrow_type_cache.key_count()
            + narrow_excluding_cache.key_count()
            + narrow_assignable_cache.key_count()
            + narrow_subtype_cache.key_count();
        let max_generation_slots_per_cache_key = [
            property_cache.max_slots_per_key(),
            required_property_cache.max_slots_per_key(),
            self.optional_chain_cache.borrow().max_slots_per_key(),
            self.optional_property_chain_cache
                .borrow()
                .max_slots_per_key(),
            narrow_type_cache.max_slots_per_key(),
            narrow_excluding_cache.max_slots_per_key(),
            narrow_assignable_cache.max_slots_per_key(),
            narrow_subtype_cache.max_slots_per_key(),
        ]
        .into_iter()
        .max()
        .unwrap_or(0);

        NarrowingCacheStatistics {
            resolve_cache_entries: self.resolve_cache.borrow().len(),
            narrowed_property_cache_entries: property_cache.len(),
            required_property_cache_entries: required_property_cache.len(),
            split_nullish_cache_entries: self.split_nullish_cache.borrow().len(),
            contains_type_parameters_cache_entries: self
                .contains_type_parameters_cache
                .borrow()
                .len(),
            optional_chain_cache_entries: self.optional_chain_cache.borrow().len(),
            optional_property_chain_cache_entries: self
                .optional_property_chain_cache
                .borrow()
                .len(),
            contextual_resolve_cache_entries: self.contextual_resolve_cache.borrow().len(),
            discriminant_index_entries: self.discriminant_index.borrow().len(),
            narrow_type_cache_entries: narrow_type_cache.len(),
            narrow_excluding_cache_entries: narrow_excluding_cache.len(),
            narrow_assignable_cache_entries: narrow_assignable_cache.len(),
            narrow_subtype_cache_entries: narrow_subtype_cache.len(),
            generation_stamped_cache_keys,
            max_generation_slots_per_cache_key,
            estimated_size_bytes: self.estimated_size_bytes(),
        }
    }

    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        const BUCKET_OVERHEAD: usize = 64;

        let mut size = std::mem::size_of::<Self>();

        {
            let map = self.resolve_cache.borrow();
            size += map.capacity() * (BUCKET_OVERHEAD + std::mem::size_of::<(TypeId, TypeId)>());
        }
        {
            let set = self.resolve_visiting.borrow();
            size += set.capacity() * (BUCKET_OVERHEAD + std::mem::size_of::<TypeId>());
        }
        {
            let map = self.property_cache.borrow();
            size += map.estimated_size_bytes(BUCKET_OVERHEAD);
        }
        {
            let map = self.required_property_cache.borrow();
            size += map.estimated_size_bytes(BUCKET_OVERHEAD);
        }
        {
            let map = self.split_nullish_cache.borrow();
            size += map.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<TypeId>()
                    + std::mem::size_of::<SplitNullishParts>());
        }
        {
            let map = self.contains_type_parameters_cache.borrow();
            size += map.capacity() * (BUCKET_OVERHEAD + std::mem::size_of::<(TypeId, bool)>());
        }
        {
            let map = self.optional_chain_cache.borrow();
            size += map.estimated_size_bytes(BUCKET_OVERHEAD);
        }
        {
            let map = self.optional_property_chain_cache.borrow();
            size += map.estimated_size_bytes(BUCKET_OVERHEAD);
            size += map.key_extra_size_bytes(|key| {
                key.properties.capacity() * std::mem::size_of::<Atom>()
            });
        }
        {
            let map = self.contextual_resolve_cache.borrow();
            size += map.capacity() * (BUCKET_OVERHEAD + std::mem::size_of::<(TypeId, TypeId)>());
        }
        {
            let map = self.discriminant_index.borrow();
            size += map.capacity()
                * (BUCKET_OVERHEAD
                    + std::mem::size_of::<(TypeId, Atom)>()
                    + std::mem::size_of::<Arc<DiscriminantMembers>>());
            for members in map.values() {
                size += members.capacity()
                    * (BUCKET_OVERHEAD
                        + std::mem::size_of::<TypeId>()
                        + std::mem::size_of::<Vec<TypeId>>());
                size += members
                    .values()
                    .map(|variants| variants.capacity() * std::mem::size_of::<TypeId>())
                    .sum::<usize>();
            }
        }
        {
            let map = self.narrow_type_cache.borrow();
            size += map.estimated_size_bytes(BUCKET_OVERHEAD);
        }
        {
            let map = self.narrow_excluding_cache.borrow();
            size += map.estimated_size_bytes(BUCKET_OVERHEAD);
        }
        {
            let set = self.narrow_excluding_visiting.borrow();
            size += set.capacity() * (BUCKET_OVERHEAD + std::mem::size_of::<NarrowExcludingKey>());
        }
        {
            let map = self.narrow_assignable_cache.borrow();
            size += map.estimated_size_bytes(BUCKET_OVERHEAD);
        }
        {
            let map = self.narrow_subtype_cache.borrow();
            size += map.estimated_size_bytes(BUCKET_OVERHEAD);
        }

        size
    }

    pub(in crate::narrowing) fn enter_exclusion_frame(&self) -> ExclusionFrame<'_> {
        let prior = self.narrow_excluding_depth.get();
        if prior == 0 {
            let cap = self.narrow_excluding_budget.get();
            let cap = if cap == 0 {
                NARROW_EXCLUDING_WORK_BUDGET
            } else {
                cap
            };
            self.narrow_excluding_fuel.set(cap);
        }
        self.narrow_excluding_depth.set(prior + 1);
        ExclusionFrame {
            depth: &self.narrow_excluding_depth,
            fuel: &self.narrow_excluding_fuel,
            prior,
        }
    }

    pub(in crate::narrowing) fn charge_exclusion_work(&self) -> bool {
        let fuel = self.narrow_excluding_fuel.get();
        if fuel == 0 {
            return false;
        }
        self.narrow_excluding_fuel.set(fuel - 1);
        true
    }

    pub(in crate::narrowing) const fn exclusion_within_budget(&self) -> bool {
        self.narrow_excluding_fuel.get() > 0
    }

    #[cfg(test)]
    pub(crate) fn set_narrow_excluding_budget(&self, budget: u32) {
        self.narrow_excluding_budget.set(budget);
    }

    pub(in crate::narrowing) fn resolve_visit_guard(
        &self,
        type_id: TypeId,
    ) -> Option<ResolveVisitGuard<'_>> {
        if !self.resolve_visiting.borrow_mut().insert(type_id) {
            return None;
        }
        Some(ResolveVisitGuard {
            visiting: &self.resolve_visiting,
            type_id,
        })
    }

    pub(in crate::narrowing) fn narrow_excluding_visit_guard(
        &self,
        key: NarrowExcludingKey,
    ) -> Option<NarrowExcludingVisitGuard<'_>> {
        if !self.narrow_excluding_visiting.borrow_mut().insert(key) {
            return None;
        }
        Some(NarrowExcludingVisitGuard {
            visiting: &self.narrow_excluding_visiting,
            key,
        })
    }
}

#[cfg(test)]
#[path = "cache/cache_visibility_tests.rs"]
mod cache_visibility_tests;
