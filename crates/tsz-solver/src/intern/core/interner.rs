//! Core implementation of the type interning engine.
//!
//! This module owns the `TypeInterner` struct, its intern/lookup hot paths, and
//! the component accessors. Concern-specific pieces live in submodules:
//! - `storage`: sharded `TypeData` storage and the slice/value component interners
//!   (pure data layout).
//! - `display`: diagnostic display provenance (fresh-literal properties, alias
//!   names, union member origin).
//! - `cache`: the thread-local intern/lookup fast-path cache.

use crate::def::DefId;
use crate::types::{
    CallableShape, CallableShapeId, ConditionalType, ConditionalTypeId, FunctionShape,
    FunctionShapeId, IntrinsicKind, LiteralValue, MappedType, MappedTypeId, ObjectFlags,
    ObjectShape, ObjectShapeId, PropertyInfo, PropertyLookup, TemplateLiteralId, TemplateSpan,
    TupleElement, TupleListId, TypeApplication, TypeApplicationId, TypeData, TypeId, TypeListId,
    TypeParamInfo,
};
use crate::utils::RwLockExt;
use crate::visitor::is_identity_comparable_type;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHasher};
use smallvec::SmallVec;
use std::hash::{Hash, Hasher};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, AtomicU32, Ordering},
};
use tsz_common::interner::{Atom, ShardedInterner};

/// A universe-shared variance result: the def's variance mask plus the
/// resolution-gap fingerprint (defs whose resolution failed during the walk)
/// that gates replaying it under a different resolver.
pub type SharedDefVariance = (Arc<[crate::types::Variance]>, Arc<[DefId]>);

mod cache;
mod display;
mod storage;

pub(super) use storage::{CachedUnionMember, TypeShard};
use storage::{ConcurrentSliceInterner, ConcurrentValueInterner, write_id_slot};

/// Global counter for assigning unique `instance_id`s to `TypeInterner`
/// instances. `0` is reserved as "empty/no-interner" so it will never match
/// a real entry stored in the thread-local cache.
static NEXT_INTERNER_INSTANCE_ID: AtomicU32 = AtomicU32::new(1);

/// Clear the thread-local type interner cache.
///
/// This MUST be called between independent compilation sessions (e.g., in batch
/// mode) to prevent stale cached entries from a previous `TypeInterner` instance
/// from being returned for `TypeId` values that have been reused by a new interner.
/// Without this, the lookup cache may return `TypeData` from a dropped interner,
/// causing incorrect type resolution and panics.
pub fn clear_thread_local_cache() {
    cache::clear_thread_local_cache();
}

pub(super) const SHARD_BITS: u32 = 6;
pub(super) const SHARD_COUNT: usize = 1 << SHARD_BITS; // 64 shards
pub(super) const SHARD_MASK: u32 = (SHARD_COUNT as u32) - 1;
pub(crate) const PROPERTY_MAP_THRESHOLD: usize = 24;
pub(super) const TYPE_LIST_INLINE: usize = 8;

/// Maximum template literal expansion limit.
/// WASM environments have limited linear memory, so we use a much lower limit
/// to prevent OOM. Native CLI can handle more.
#[cfg(target_arch = "wasm32")]
pub(crate) const TEMPLATE_LITERAL_EXPANSION_LIMIT: usize = 2_000;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const TEMPLATE_LITERAL_EXPANSION_LIMIT: usize = 100_000;

/// Maximum number of interned types before the interner returns `TypeId::ERROR`.
///
/// Prevents OOM on pathological inputs (e.g., DOM types + module augmentation
/// that create millions of intermediate types via heritage merging and
/// function shape instantiation). With roughly 200-300 bytes per interned entry
/// (DashMap overhead, `Arc`, shapes), 8M types is roughly a 1.6-2.4GB
/// interner budget before fallback; WASM keeps the historical 500k
/// (~100-150MB) because the 32-bit heap cannot host a multi-GB interner.
///
/// The native value is sized for legitimate large programs, not as a working
/// budget: a 1.2k-file slice of the `large-ts-repo` benchmark (with aws-sdk
/// dependency surface) legitimately interns >500k unique types, and the full
/// 12k-file row needs several million. The old shared 500k cap made the cap a
/// *semantic* cliff on real projects rather than a pathological-input guard.
///
/// When the count is exceeded, new non-intrinsic interning poisons the
/// interner and returns `TypeId::ERROR`. Already-computed ids remain readable
/// (`lookup` and existing-key `intern` still succeed) so later diagnostics,
/// relations, and shared cross-file caches keep working; only *new* type
/// construction degrades to `ERROR`.
#[cfg(target_arch = "wasm32")]
pub(crate) const MAX_INTERNED_TYPES: usize = 500_000;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const MAX_INTERNED_TYPES: usize = 8_000_000;

// The evaluation-fuel budget (`MAX_EVALUATION_FUEL`) lives in the
// consolidated `crate::limits` module (issue #13091).

pub(crate) type TypeListBuffer = SmallVec<[TypeId; TYPE_LIST_INLINE]>;
type ObjectPropertyIndex = DashMap<ObjectShapeId, Arc<FxHashMap<Atom, usize>>, FxBuildHasher>;
type ObjectPropertyMap = OnceLock<ObjectPropertyIndex>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InternedTypeLimitContext {
    pub(crate) current_count: usize,
    pub(crate) max_interned_types: usize,
    pub(crate) fallback_type: TypeId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PredicateCacheKind {
    ContainsThis = 0,
    ContainsInfer = 1,
    ContainsTypeQuery = 2,
    ContainsTypeParams = 3,
    ContainsLazyOrRecursive = 4,
    ContainsUnresolvedApplication = 5,
    ContainsResolverDependent = 6,
    ContainsConditional = 7,
    ContainsParamOrInferRoot = 8,
    ContainsGenericParamsRoot = 9,
    EvalContainsInfer = 10,
    ContainsFileRelative = 11,
    IsGenericWithUnionConstraint = 12,
    IsGenericWithoutNullableConstraint = 13,
    /// `type_id` contains no node whose evaluation depends on the resolver or
    /// substitution environment (no `Conditional`/`IndexAccess`/`Mapped`/
    /// `KeyOf`/`TypeQuery`/`Application`/`TemplateLiteral`/`Lazy`/`Recursive`/
    /// `StringIntrinsic`/`NoInfer`/`UnresolvedTypeName`/`TypeParameter`/`Infer`/
    /// `ThisType`/`BoundParameter`). Such a type evaluates to itself under every
    /// evaluator and resolver in a project run, so a recorded identity result is
    /// a permanent structural fixed point. Backs the evaluator's resolver-
    /// independent fixed-point fast path (issues #13250 / #8356).
    StructurallyEvalInert = 14,
}

impl PredicateCacheKind {
    const fn bit(self) -> u16 {
        1u16 << (self as u8)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct PredicateCacheEntry {
    known: u16,
    truthy: u16,
}

impl PredicateCacheEntry {
    #[inline]
    const fn get(self, kind: PredicateCacheKind) -> Option<bool> {
        let bit = kind.bit();
        if self.known & bit == 0 {
            None
        } else {
            Some(self.truthy & bit != 0)
        }
    }

    #[inline]
    const fn set(&mut self, kind: PredicateCacheKind, result: bool) {
        let bit = kind.bit();
        self.known |= bit;
        if result {
            self.truthy |= bit;
        } else {
            self.truthy &= !bit;
        }
    }

    #[inline]
    const fn has(self, kind: PredicateCacheKind) -> bool {
        self.known & kind.bit() != 0
    }
}

/// Type interning table with lock-free concurrent access.
///
/// Uses sharded `DashMap` structures for all internal storage, enabling
/// true parallel type checking without lock contention.
///
/// All internal structures use lazy initialization via `OnceLock` to minimize
/// startup overhead - `DashMaps` are only allocated when first accessed.
pub struct TypeInterner {
    /// Sharded storage for user-defined types (lazily initialized)
    pub(super) shards: Vec<TypeShard>,
    /// String interner for property names and string literals (already lock-free)
    pub string_interner: ShardedInterner,
    /// Concurrent interners for type components (lazily initialized)
    pub(super) type_lists: ConcurrentSliceInterner<TypeId>,
    pub(super) tuple_lists: ConcurrentSliceInterner<TupleElement>,
    pub(super) template_lists: ConcurrentSliceInterner<TemplateSpan>,
    pub(super) object_shapes: ConcurrentValueInterner<ObjectShape>,
    /// Object property maps: lazily initialized `DashMap`
    pub(super) object_property_maps: ObjectPropertyMap,
    pub(super) function_shapes: ConcurrentValueInterner<FunctionShape>,
    pub(super) callable_shapes: ConcurrentValueInterner<CallableShape>,
    pub(super) conditional_types: ConcurrentValueInterner<ConditionalType>,
    pub(super) mapped_types: ConcurrentValueInterner<MappedType>,
    pub(super) applications: ConcurrentValueInterner<TypeApplication>,
    /// Cache for `is_identity_comparable_type` checks (memoized O(1) lookup after first computation)
    pub(super) identity_comparable_cache: DashMap<TypeId, bool, FxBuildHasher>,
    /// Result memo for the canonical semantic `widen_type` entry (flags
    /// `widen_boolean_intrinsics=true`, all others false). Widening is a pure
    /// function of the immutable interned type structure, but `widen_type`
    /// allocates a fresh per-call recursion guard, so widening the same wide
    /// union once per reference is O(N) repeated — O(N^2) over an N-arm
    /// discriminated-union switch (#13598). This memo collapses repeats of the
    /// same root `TypeId` to O(1). Only the `widen_type` entry uses it; the
    /// `widen_type_deep`/display variants compute different results for the same
    /// `TypeId` and must not share it.
    pub(super) widen_type_cache: DashMap<TypeId, TypeId, FxBuildHasher>,
    /// Packed per-`TypeId` caches for immutable structural content predicates.
    ///
    /// Each bit records one predicate's known/truthy state. This preserves the
    /// existing accessor surface while replacing the independent
    /// `DashMap<TypeId, bool>` tables for content predicates with one shared
    /// table. Predicate identity remains part of the key through the bit, so
    /// distinct walks do not alias each other.
    pub(crate) predicate_cache: DashMap<TypeId, PredicateCacheEntry, FxBuildHasher>,
    /// Result memo for `normalize_union`, keyed by the exact flattened
    /// pre-normalization member list. Normalization (semantic sort, dedup,
    /// absorption passes, subtype reduction) is deterministic in the input
    /// list over immutable interned types; evaluation rebuilds the same
    /// unions constantly. Inputs longer than
    /// `UNION_NORMALIZE_CACHE_MAX_LEN` bypass this memo so the sticky
    /// TS2590 `union_too_complex` flag is never swallowed by a hit.
    pub(crate) union_normalize_cache: DashMap<Box<[TypeId]>, TypeId, FxBuildHasher>,
    /// The global Array base type (e.g., Array<T> from lib.d.ts).
    /// Uses `AtomicU32` (with `u32::MAX` as sentinel for `None`) instead of
    /// `RwLock` so file checkers can overwrite the prime checker's value without
    /// lock contention on this frequently-read field.
    pub(super) array_base_type: AtomicU32,
    /// Display-order Array base type used for keyof/mapped diagnostics.
    /// This may differ from `array_base_type` when the semantic base and the
    /// lib-merged display surface are not the same lowered type.
    pub(super) array_display_base_type: AtomicU32,
    /// Type parameters for the Array base type.
    /// Kept as `OnceLock` since params don't contain `DefIds` and are stable
    /// across checkers (the interner allocates `TypeParam` `TypeIds` centrally).
    pub(super) array_base_type_params: OnceLock<Vec<TypeParamInfo>>,
    /// The global ReadonlyArray base type (e.g., `ReadonlyArray<T>` from lib.d.ts).
    /// Used by property access resolution to correctly reject mutating methods
    /// (`push`, `pop`, etc.) on `readonly T[]` types.
    pub(super) readonly_array_base_type: AtomicU32,
    /// Boxed interface types for primitives (e.g., String interface for `string`).
    /// Registered from lib.d.ts during primordial type setup.
    pub(super) boxed_types: DashMap<IntrinsicKind, TypeId, FxBuildHasher>,
    /// `DefIds` known to be boxed types (e.g., the DefId for the Function interface).
    /// Registered alongside `boxed_types` so subtype checking can identify boxed
    /// types even when `TypeEnvironment` is unavailable.
    pub(super) boxed_def_ids: DashMap<IntrinsicKind, Vec<DefId>, FxBuildHasher>,
    /// `DefIds` known to be the `ThisType` marker interface from lib.d.ts.
    /// Used by `ThisTypeMarkerExtractor` to identify `ThisType<T>` applications
    /// when the base type is `Lazy(DefId)`.
    pub(super) this_type_marker_def_ids: DashMap<DefId, (), FxBuildHasher>,
    /// Global allocation counter for deterministic type ordering.
    /// The sharded interner embeds shard index in TypeId low bits, so raw TypeId
    /// comparison is hash-dependent. This counter provides allocation-order
    /// comparison that approximates tsc's source-order type ID allocation.
    pub(super) alloc_counter: AtomicU32,
    /// Circuit breaker: once set, all intern/lookup calls return early.
    pub(super) poisoned: std::sync::atomic::AtomicBool,
    /// Effective value for `noUncheckedIndexedAccess` used by query-boundary helpers.
    pub(super) no_unchecked_indexed_access: AtomicBool,
    /// Effective value for `exactOptionalPropertyTypes` used by query-boundary helpers.
    pub(super) exact_optional_property_types: AtomicBool,
    /// Display properties for fresh object literal types.
    ///
    /// When object literal properties are widened (e.g., `"hello"` → `string`),
    /// the pre-widened types are stored here for display in error messages.
    /// This implements tsc's "freshness" model where error messages show
    /// literal types (`{ x: "hello" }`) even though the type system uses
    /// widened types (`{ x: string }`).
    ///
    /// Key: `ObjectShapeId` of the widened (interned) shape.
    /// Value: Vec of `PropertyInfo` with original (non-widened) `type_ids`.
    pub(super) display_properties: DashMap<TypeId, Arc<Vec<PropertyInfo>>, FxBuildHasher>,
    /// Reverse mapping from evaluated Application results back to their
    /// original Application TypeId for diagnostic display.
    ///
    /// When `Application(Lazy(Dictionary), [string])` evaluates to
    /// `ObjectWithIndex({ [index: string]: string })`, this maps
    /// the `ObjectWithIndex` TypeId back to the Application TypeId.
    /// The formatter checks this to show `Dictionary<string>` instead
    /// of `{ [index: string]: string; }` in error messages.
    pub(super) display_alias: DashMap<TypeId, TypeId, FxBuildHasher>,
    /// Semantic provenance: evaluated structural result -> originating
    /// `Application` TypeId.
    ///
    /// Unlike `display_alias`, this map is recorded unconditionally for
    /// nominal (class/interface) application evaluations and is consumed by
    /// the relation layer to recover generic identity for the accept-only
    /// variance fast path (tsc `relateVariances` on same-reference
    /// instantiations whose tsz forms were eagerly evaluated). It is never
    /// read by the printer, so it carries no display-repaint heuristics.
    pub(super) application_eval_origin: DashMap<TypeId, TypeId, FxBuildHasher>,
    /// Application bases whose type-alias body is a conditional type.
    ///
    /// Conditional aliases often evaluate to a branch with its own display
    /// surface. Keep this small provenance bit so application-preferring alias
    /// storage can avoid repainting an already-recorded branch intersection.
    pub(super) conditional_alias_bases: DashMap<TypeId, (), FxBuildHasher>,
    /// As-written origin members for a Union TypeId, used to preserve top-level
    /// alias names that would otherwise be lost during union flattening.
    ///
    /// When a user writes `T | null` and `T` is a type alias whose body is itself
    /// a union (e.g., `type T = "a" | "b" | undefined`), tsc's `getUnionType`
    /// flattens the inputs into `"a" | "b" | undefined | null`, but the printer
    /// still displays `T | null` by consulting the union's `origin` field.
    ///
    /// tsz captures the equivalent information here: the checker records the
    /// *unflattened* member list (e.g., `[Lazy(T), null]`) for the resulting
    /// flattened union. The formatter consults this map before falling through
    /// to structural display.
    ///
    /// Key: the flattened Union `TypeId` returned to the checker.
    /// Value: the unflattened input member list, in the order the user wrote.
    pub(super) display_union_origin: DashMap<TypeId, Arc<Vec<TypeId>>, FxBuildHasher>,
    /// Flag set when union normalization detects that a union type is too complex
    /// to represent (would require > 1M pairwise subtype comparisons during
    /// reduction). Mirrors tsc's `removeSubtypes` complexity heuristic that
    /// emits TS2590. The checker reads and clears this flag to emit the diagnostic.
    pub(super) union_too_complex: AtomicBool,
    /// Flag set when tuple synthesis detects that a spread would produce a tuple
    /// with more than `MAX_REPRESENTABLE_TUPLE_LENGTH` elements. The checker reads
    /// and clears this to emit TS2799 instead of TS2589.
    pub(super) tuple_too_large: AtomicBool,
    /// Universe-wide declared-variance masks for generic definitions.
    ///
    /// Keyed by `DefId`; the value is `(mask, gap_defs)`. Populated by the
    /// variance computer (`relations/variance.rs`) only with canonical masks
    /// (the walk never depended on an in-flight def below its own frame).
    /// `gap_defs` is the walk's resolution-failure fingerprint: the set of
    /// `DefId`s whose lazy resolution failed during the walk. A mask is a
    /// pure function of (def structure, failure set): a consumer may replay
    /// it iff every fingerprint def still fails to resolve under the
    /// consumer's resolver — validated on read. Masks therefore live on the
    /// interner — the one shared type universe — instead of any per-checker
    /// `QueryCache`, and survive across files and child checkers. Values hold
    /// no `TypeId`s, only `Variance` bitmasks and `DefId`s.
    pub(super) def_variance_masks: DashMap<DefId, SharedDefVariance, FxBuildHasher>,
    /// Unique identifier scoping this interner's entries in the thread-local
    /// lookup/intern cache. See `NEXT_INTERNER_INSTANCE_ID` for context.
    pub(super) instance_id: u32,
}

// The per-thread evaluation fuel counter lives in the consolidated
// `crate::limits` thread-local budget state (issue #13091); the methods
// below remain the stable access surface for checker/database callers.

/// Entry-count snapshot for retained `TypeInterner` predicate caches.
///
/// These caches memoize immutable per-`TypeId` content predicates. The snapshot
/// is observability-only: it does not change cache keys, invalidation, fuel, or
/// predicate answers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TypePredicateCacheStatistics {
    /// Number of memoized identity-comparability predicate results.
    pub identity_comparable_cache_entries: usize,
    /// Number of memoized union-normalization results.
    pub union_normalize_cache_entries: usize,
    /// Number of memoized `ThisType` containment predicate results.
    pub contains_this_cache_entries: usize,
    /// Number of memoized `infer` containment predicate results.
    pub contains_infer_cache_entries: usize,
    /// Number of memoized `typeof` query containment predicate results.
    pub contains_type_query_cache_entries: usize,
    /// Number of memoized type-parameter containment predicate results.
    pub contains_type_params_cache_entries: usize,
    /// Number of memoized lazy-or-recursive containment predicate results.
    pub contains_lazy_or_recursive_cache_entries: usize,
    /// Number of memoized unresolved-application containment predicate results.
    pub contains_unresolved_application_cache_entries: usize,
    /// Number of memoized resolver-dependent containment predicate results.
    pub contains_resolver_dependent_cache_entries: usize,
    /// Number of memoized conditional-type containment predicate results.
    pub contains_conditional_cache_entries: usize,
    /// Number of memoized narrow param-or-infer containment results.
    pub contains_param_or_infer_root_cache_entries: usize,
    /// Number of memoized depth-limited generic-params root walk results.
    pub contains_generic_params_root_cache_entries: usize,
    /// Number of memoized evaluator `type_contains_infer` walk results.
    pub eval_contains_infer_cache_entries: usize,
    /// Number of memoized file-relative containment predicate results.
    pub contains_file_relative_cache_entries: usize,
}

impl std::fmt::Debug for TypeInterner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypeInterner")
            .field("shards", &self.shards.len())
            .finish_non_exhaustive()
    }
}

impl TypeInterner {
    /// Capture retained predicate-cache entry counts for perf attribution.
    #[must_use]
    pub fn type_predicate_cache_statistics(&self) -> TypePredicateCacheStatistics {
        TypePredicateCacheStatistics {
            identity_comparable_cache_entries: self.identity_comparable_cache.len(),
            union_normalize_cache_entries: self.union_normalize_cache.len(),
            contains_this_cache_entries: self
                .predicate_cache_entries_for(PredicateCacheKind::ContainsThis),
            contains_infer_cache_entries: self
                .predicate_cache_entries_for(PredicateCacheKind::ContainsInfer),
            contains_type_query_cache_entries: self
                .predicate_cache_entries_for(PredicateCacheKind::ContainsTypeQuery),
            contains_type_params_cache_entries: self
                .predicate_cache_entries_for(PredicateCacheKind::ContainsTypeParams),
            contains_lazy_or_recursive_cache_entries: self
                .predicate_cache_entries_for(PredicateCacheKind::ContainsLazyOrRecursive),
            contains_unresolved_application_cache_entries: self
                .predicate_cache_entries_for(PredicateCacheKind::ContainsUnresolvedApplication),
            contains_resolver_dependent_cache_entries: self
                .predicate_cache_entries_for(PredicateCacheKind::ContainsResolverDependent),
            contains_conditional_cache_entries: self
                .predicate_cache_entries_for(PredicateCacheKind::ContainsConditional),
            contains_param_or_infer_root_cache_entries: self
                .predicate_cache_entries_for(PredicateCacheKind::ContainsParamOrInferRoot),
            contains_generic_params_root_cache_entries: self
                .predicate_cache_entries_for(PredicateCacheKind::ContainsGenericParamsRoot),
            eval_contains_infer_cache_entries: self
                .predicate_cache_entries_for(PredicateCacheKind::EvalContainsInfer),
            contains_file_relative_cache_entries: self
                .predicate_cache_entries_for(PredicateCacheKind::ContainsFileRelative),
        }
    }

    #[inline]
    pub(crate) fn predicate_cache_get(
        &self,
        type_id: TypeId,
        kind: PredicateCacheKind,
    ) -> Option<bool> {
        self.predicate_cache
            .get(&type_id)
            .and_then(|entry| entry.get(kind))
    }

    #[inline]
    pub(crate) fn predicate_cache_set(
        &self,
        type_id: TypeId,
        kind: PredicateCacheKind,
        result: bool,
    ) {
        match self.predicate_cache.entry(type_id) {
            Entry::Occupied(mut entry) => entry.get_mut().set(kind, result),
            Entry::Vacant(entry) => {
                let mut value = PredicateCacheEntry::default();
                value.set(kind, result);
                entry.insert(value);
            }
        }
    }

    fn predicate_cache_entries_for(&self, kind: PredicateCacheKind) -> usize {
        self.predicate_cache
            .iter()
            .filter(|entry| entry.value().has(kind))
            .count()
    }

    /// Create a new type interner with pre-registered intrinsics.
    ///
    /// Uses lazy initialization for all `DashMap` structures to minimize
    /// startup overhead. `DashMaps` are only allocated when first accessed.
    pub fn new() -> Self {
        let shards: Vec<TypeShard> = (0..SHARD_COUNT).map(|_| TypeShard::new()).collect();

        Self {
            shards,
            // String interner - common strings are interned on-demand for faster startup
            string_interner: ShardedInterner::new(),
            type_lists: ConcurrentSliceInterner::new(),
            tuple_lists: ConcurrentSliceInterner::new(),
            template_lists: ConcurrentSliceInterner::new(),
            object_shapes: ConcurrentValueInterner::new(),
            object_property_maps: OnceLock::new(),
            function_shapes: ConcurrentValueInterner::new(),
            callable_shapes: ConcurrentValueInterner::new(),
            conditional_types: ConcurrentValueInterner::new(),
            mapped_types: ConcurrentValueInterner::new(),
            applications: ConcurrentValueInterner::new(),
            identity_comparable_cache: DashMap::with_hasher(FxBuildHasher),
            predicate_cache: DashMap::with_hasher(FxBuildHasher),
            widen_type_cache: DashMap::with_hasher(FxBuildHasher),
            union_normalize_cache: DashMap::with_hasher(FxBuildHasher),
            array_base_type: AtomicU32::new(u32::MAX),
            array_display_base_type: AtomicU32::new(u32::MAX),
            array_base_type_params: OnceLock::new(),
            readonly_array_base_type: AtomicU32::new(u32::MAX),
            boxed_types: DashMap::with_hasher(FxBuildHasher),
            boxed_def_ids: DashMap::with_hasher(FxBuildHasher),
            this_type_marker_def_ids: DashMap::with_hasher(FxBuildHasher),
            alloc_counter: AtomicU32::new(0),
            poisoned: std::sync::atomic::AtomicBool::new(false),
            no_unchecked_indexed_access: AtomicBool::new(false),
            exact_optional_property_types: AtomicBool::new(false),
            display_properties: DashMap::with_hasher(FxBuildHasher),
            display_alias: DashMap::with_hasher(FxBuildHasher),
            application_eval_origin: DashMap::with_hasher(FxBuildHasher),
            conditional_alias_bases: DashMap::with_hasher(FxBuildHasher),
            display_union_origin: DashMap::with_hasher(FxBuildHasher),
            union_too_complex: AtomicBool::new(false),
            tuple_too_large: AtomicBool::new(false),
            def_variance_masks: DashMap::with_hasher(FxBuildHasher),
            instance_id: NEXT_INTERNER_INSTANCE_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Read a universe-shared variance mask for a generic definition.
    ///
    /// Returns `(mask, gap_defs)`. Only canonical masks are stored, together
    /// with their resolution-failure fingerprint (see `def_variance_masks`);
    /// callers must validate the fingerprint against their resolver before
    /// replaying the mask.
    #[inline]
    pub fn shared_def_variance(&self, def_id: DefId) -> Option<SharedDefVariance> {
        self.def_variance_masks
            .get(&def_id)
            .map(|entry| entry.value().clone())
    }

    /// Store a universe-shared variance mask for a generic definition with
    /// its resolution-failure fingerprint.
    ///
    /// Callers must only insert canonical masks whose every resolution gap is
    /// listed in `gaps`. First write wins, keeping replays deterministic
    /// within a session.
    #[inline]
    pub fn insert_shared_def_variance(
        &self,
        def_id: DefId,
        mask: Arc<[crate::types::Variance]>,
        gaps: Arc<[DefId]>,
    ) {
        self.def_variance_masks
            .entry(def_id)
            .or_insert((mask, gaps));
    }

    #[inline]
    pub fn no_unchecked_indexed_access(&self) -> bool {
        self.no_unchecked_indexed_access.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn set_no_unchecked_indexed_access(&self, enabled: bool) {
        self.no_unchecked_indexed_access
            .store(enabled, Ordering::Relaxed);
    }

    #[inline]
    pub fn exact_optional_property_types(&self) -> bool {
        self.exact_optional_property_types.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn set_exact_optional_property_types(&self, enabled: bool) {
        self.exact_optional_property_types
            .store(enabled, Ordering::Relaxed);
    }

    /// Atomically read and clear the "union too complex" flag.
    ///
    /// Returns `true` if a union construction was aborted due to complexity
    /// since the last call to this method. The flag is cleared after reading.
    /// The checker uses this to emit TS2590.
    #[inline]
    pub fn take_union_too_complex(&self) -> bool {
        self.union_too_complex.swap(false, Ordering::Relaxed)
    }

    /// Mark that a union construction was aborted due to complexity.
    /// Called from `reduce_union_subtypes` when pairwise comparisons would exceed 1M.
    #[inline]
    pub(crate) fn set_union_too_complex(&self) {
        self.union_too_complex.store(true, Ordering::Relaxed);
    }

    /// Peek at the union-too-complex flag without clearing it (so the checker
    /// still observes it). The evaluator uses this to skip caching an evaluation
    /// that tripped the `TS2590` limit.
    #[inline]
    pub fn is_union_too_complex(&self) -> bool {
        self.union_too_complex.load(Ordering::Relaxed)
    }

    /// Atomically read and clear the "tuple too large" flag.
    ///
    /// Returns `true` if a tuple spread synthesis was aborted because the result
    /// would exceed `MAX_REPRESENTABLE_TUPLE_LENGTH` elements. The checker uses
    /// this to emit TS2799 instead of TS2589.
    #[inline]
    pub fn take_tuple_too_large(&self) -> bool {
        self.tuple_too_large.swap(false, Ordering::Relaxed)
    }

    /// Mark that a tuple spread synthesis was aborted due to exceeding the
    /// representable tuple length limit.
    #[inline]
    pub(crate) fn set_tuple_too_large(&self) {
        self.tuple_too_large.store(true, Ordering::Relaxed);
    }

    /// Set the global Array base type (e.g., Array<T> from lib.d.ts).
    ///
    /// The `TypeId` uses `AtomicU32` so each file checker can overwrite the prime
    /// checker's value with one containing correct `DefIds` for its own
    /// `DefinitionStore`. The params use `OnceLock` since they don't contain
    /// `DefIds` and are stable across checkers.
    pub fn set_array_base_type(&self, type_id: TypeId, params: Vec<TypeParamInfo>) {
        self.array_base_type.store(type_id.0, Ordering::Relaxed);
        let _ = self.array_base_type_params.set(params);
    }

    /// Set the global `ReadonlyArray<T>` base type from lib.d.ts.
    pub fn set_readonly_array_base_type(&self, type_id: TypeId) {
        self.readonly_array_base_type
            .store(type_id.0, Ordering::Relaxed);
    }

    /// Get the global `ReadonlyArray<T>` base type, if it has been set.
    #[inline]
    pub fn get_readonly_array_base_type(&self) -> Option<TypeId> {
        let raw = self.readonly_array_base_type.load(Ordering::Relaxed);
        if raw == u32::MAX {
            None
        } else {
            Some(TypeId(raw))
        }
    }

    /// Set the Array base type used for display-order-sensitive queries.
    pub fn set_array_display_base_type(&self, type_id: TypeId) {
        self.array_display_base_type
            .store(type_id.0, Ordering::Relaxed);
    }

    /// Get the global Array base type, if it has been set.
    #[inline]
    pub fn get_array_base_type(&self) -> Option<TypeId> {
        let raw = self.array_base_type.load(Ordering::Relaxed);
        if raw == u32::MAX {
            None
        } else {
            Some(TypeId(raw))
        }
    }

    /// Get the Array base type used for display-order-sensitive queries.
    #[inline]
    pub fn get_array_display_base_type(&self) -> Option<TypeId> {
        let raw = self.array_display_base_type.load(Ordering::Relaxed);
        if raw == u32::MAX {
            None
        } else {
            Some(TypeId(raw))
        }
    }

    /// Get the type parameters for the global Array base type, if it has been set.
    #[inline]
    pub fn get_array_base_type_params(&self) -> &[TypeParamInfo] {
        self.array_base_type_params
            .get()
            .map_or(&[], |v| v.as_slice())
    }

    /// Set a boxed interface type for a primitive intrinsic kind.
    ///
    /// Called during primordial type setup when lib.d.ts is processed.
    /// For example, `set_boxed_type(IntrinsicKind::String, type_id_of_String_interface)`
    /// enables property access on `string` values to resolve through the String interface.
    pub fn set_boxed_type(&self, kind: IntrinsicKind, type_id: TypeId) {
        self.boxed_types.insert(kind, type_id);
    }

    /// Get the boxed interface type for a primitive intrinsic kind.
    #[inline]
    pub fn get_boxed_type(&self, kind: IntrinsicKind) -> Option<TypeId> {
        self.boxed_types.get(&kind).map(|r| *r)
    }

    /// Register a DefId as belonging to a boxed type.
    pub fn register_boxed_def_id(&self, kind: IntrinsicKind, def_id: DefId) {
        let mut def_ids = self.boxed_def_ids.entry(kind).or_default();
        if !def_ids.contains(&def_id) {
            def_ids.push(def_id);
        }
    }

    /// Check if a DefId corresponds to a boxed type of the given kind.
    pub fn is_boxed_def_id(&self, def_id: DefId, kind: IntrinsicKind) -> bool {
        self.boxed_def_ids
            .get(&kind)
            .is_some_and(|ids| ids.contains(&def_id))
    }

    /// Register a DefId as belonging to the `ThisType` marker interface.
    pub fn register_this_type_def_id(&self, def_id: DefId) {
        self.this_type_marker_def_ids.insert(def_id, ());
    }

    /// Check if a DefId corresponds to the `ThisType` marker interface.
    pub fn is_this_type_marker_def_id(&self, def_id: DefId) -> bool {
        self.this_type_marker_def_ids.contains_key(&def_id)
    }

    /// Get the object property maps, initializing on first access
    #[inline]
    fn get_object_property_maps(&self) -> &ObjectPropertyIndex {
        self.object_property_maps
            .get_or_init(|| DashMap::with_hasher(FxBuildHasher))
    }

    /// Check if a type can be compared by `TypeId` identity alone (O(1) equality).
    /// Results are cached for O(1) lookup after first computation.
    /// This is used for optimization in BCT and subtype checking.
    #[inline]
    pub fn is_identity_comparable_type(&self, type_id: TypeId) -> bool {
        // Fast path: check cache first
        if let Some(cached) = self.identity_comparable_cache.get(&type_id) {
            return *cached;
        }
        // Compute and cache
        let result = is_identity_comparable_type(self, type_id);
        self.identity_comparable_cache.insert(type_id, result);
        result
    }

    /// Look up the memoized canonical `widen_type` result for `type_id`.
    #[inline]
    pub fn widen_type_memo(&self, type_id: TypeId) -> Option<TypeId> {
        self.widen_type_cache.get(&type_id).map(|v| *v)
    }

    /// Record the canonical `widen_type` result for `type_id`.
    #[inline]
    pub fn set_widen_type_memo(&self, type_id: TypeId, result: TypeId) {
        self.widen_type_cache.insert(type_id, result);
    }

    /// Intern a string into an Atom.
    /// This is used when constructing types with property names or string literals.
    #[inline]
    pub fn intern_string(&self, s: &str) -> Atom {
        tsz_common::perf_counters::record_interner_string_intern_call();
        self.string_interner.intern(s)
    }

    /// Resolve an Atom back to its string value.
    /// This is used when formatting types for error messages.
    pub fn resolve_atom(&self, atom: Atom) -> String {
        self.string_interner.resolve(atom).to_string()
    }

    /// Resolve an Atom without allocating a new String.
    pub fn resolve_atom_ref(&self, atom: Atom) -> Arc<str> {
        self.string_interner.resolve(atom)
    }

    #[inline]
    pub fn type_list(&self, id: TypeListId) -> Arc<[TypeId]> {
        self.type_lists
            .get(id.0)
            .unwrap_or_else(|| self.type_lists.empty())
    }

    #[inline]
    pub fn tuple_list(&self, id: TupleListId) -> Arc<[TupleElement]> {
        self.tuple_lists
            .get(id.0)
            .unwrap_or_else(|| self.tuple_lists.empty())
    }

    #[inline]
    pub fn template_list(&self, id: TemplateLiteralId) -> Arc<[TemplateSpan]> {
        self.template_lists
            .get(id.0)
            .unwrap_or_else(|| self.template_lists.empty())
    }

    #[inline]
    pub fn object_shape(&self, id: ObjectShapeId) -> Arc<ObjectShape> {
        self.object_shapes.get(id.0).unwrap_or_else(|| {
            // Use a cached static empty shape to avoid heap allocation on every miss.
            static EMPTY_SHAPE: OnceLock<Arc<ObjectShape>> = OnceLock::new();
            Arc::clone(EMPTY_SHAPE.get_or_init(|| {
                Arc::new(ObjectShape {
                    flags: ObjectFlags::empty(),
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                    symbol: None,
                })
            }))
        })
    }

    pub fn object_property_index(&self, shape_id: ObjectShapeId, name: Atom) -> PropertyLookup {
        let shape = self.object_shape(shape_id);
        if shape.properties.len() < PROPERTY_MAP_THRESHOLD {
            return PropertyLookup::Uncached;
        }

        match self.object_property_map(shape_id, &shape) {
            Some(map) => match map.get(&name) {
                Some(&idx) => PropertyLookup::Found(idx),
                None => PropertyLookup::NotFound,
            },
            None => PropertyLookup::Uncached,
        }
    }

    /// Get or create a property map for an object shape.
    ///
    /// This uses a lock-free pattern with `DashMap` to avoid the read-then-write
    /// deadlock that existed in the previous `RwLock`<Vec> implementation.
    fn object_property_map(
        &self,
        shape_id: ObjectShapeId,
        shape: &ObjectShape,
    ) -> Option<Arc<FxHashMap<Atom, usize>>> {
        if shape.properties.len() < PROPERTY_MAP_THRESHOLD {
            return None;
        }

        let maps = self.get_object_property_maps();

        // Try to get existing map (lock-free read)
        if let Some(map) = maps.get(&shape_id) {
            return Some(std::sync::Arc::clone(&map));
        }

        // Build the property map
        let mut map = FxHashMap::default();
        for (idx, prop) in shape.properties.iter().enumerate() {
            map.insert(prop.name, idx);
        }
        let map = Arc::new(map);

        // Try to insert - if another thread inserted first, use theirs
        match maps.entry(shape_id) {
            Entry::Vacant(e) => {
                e.insert(std::sync::Arc::clone(&map));
                Some(map)
            }
            Entry::Occupied(e) => Some(std::sync::Arc::clone(e.get())),
        }
    }

    #[inline]
    pub fn function_shape(&self, id: FunctionShapeId) -> Arc<FunctionShape> {
        self.function_shapes.get(id.0).unwrap_or_else(|| {
            Arc::new(FunctionShape {
                type_params: Vec::new(),
                params: Vec::new(),
                this_type: None,
                return_type: TypeId::ERROR,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            })
        })
    }

    #[inline]
    pub fn callable_shape(&self, id: CallableShapeId) -> Arc<CallableShape> {
        self.callable_shapes.get(id.0).unwrap_or_else(|| {
            Arc::new(CallableShape {
                call_signatures: Vec::new(),
                construct_signatures: Vec::new(),
                properties: Vec::new(),
                ..Default::default()
            })
        })
    }

    /// Get a conditional type by value (no Arc clone overhead).
    /// Preferred over `conditional_type()` since `ConditionalType` is Copy.
    #[inline]
    pub fn get_conditional(&self, id: ConditionalTypeId) -> ConditionalType {
        self.conditional_types
            .get_copy(id.0)
            .unwrap_or(ConditionalType {
                check_type: TypeId::ERROR,
                extends_type: TypeId::ERROR,
                true_type: TypeId::ERROR,
                false_type: TypeId::ERROR,
                is_distributive: false,
            })
    }

    /// Get a mapped type by value (no Arc clone overhead).
    /// Preferred over `mapped_type()` since `MappedType` is Copy.
    #[inline]
    pub fn get_mapped(&self, id: MappedTypeId) -> MappedType {
        self.mapped_types.get_copy(id.0).unwrap_or(MappedType {
            type_param: TypeParamInfo {
                is_const: false,
                name: self.intern_string("_"),
                constraint: None,
                default: None,
                origin: crate::types::TypeParamOrigin::User,
            },
            constraint: TypeId::ERROR,
            name_type: None,
            template: TypeId::ERROR,
            readonly_modifier: None,
            optional_modifier: None,
        })
    }

    #[inline]
    pub fn conditional_type(&self, id: ConditionalTypeId) -> Arc<ConditionalType> {
        self.conditional_types.get(id.0).unwrap_or_else(|| {
            Arc::new(ConditionalType {
                check_type: TypeId::ERROR,
                extends_type: TypeId::ERROR,
                true_type: TypeId::ERROR,
                false_type: TypeId::ERROR,
                is_distributive: false,
            })
        })
    }

    #[inline]
    pub fn mapped_type(&self, id: MappedTypeId) -> Arc<MappedType> {
        self.mapped_types.get(id.0).unwrap_or_else(|| {
            Arc::new(MappedType {
                type_param: TypeParamInfo {
                    is_const: false,
                    name: self.intern_string("_"),
                    constraint: None,
                    default: None,
                    origin: crate::types::TypeParamOrigin::User,
                },
                constraint: TypeId::ERROR,
                name_type: None,
                template: TypeId::ERROR,
                readonly_modifier: None,
                optional_modifier: None,
            })
        })
    }

    #[inline]
    pub fn type_application(&self, id: TypeApplicationId) -> Arc<TypeApplication> {
        self.applications.get(id.0).unwrap_or_else(|| {
            Arc::new(TypeApplication {
                base: TypeId::ERROR,
                args: Vec::new(),
            })
        })
    }

    /// Intern a type key and return its `TypeId`.
    /// If the key already exists, returns the existing `TypeId`.
    /// Otherwise, creates a new `TypeId` and stores the key.
    ///
    /// This uses a lock-free pattern with `DashMap` for concurrent access.
    ///
    /// Consults a thread-local cache scoped by this interner's `instance_id`
    /// before falling through to the `DashMap` lookup.
    #[inline]
    pub fn intern(&self, key: TypeData) -> TypeId {
        // NOTE: a poisoned interner (type-count limit) is handled in
        // `intern_slow` *after* the existing-key read paths, so already
        // interned keys keep resolving to their existing ids. Only new
        // allocations degrade to `TypeId::ERROR`.
        //
        // T2.4 instrumentation. Semantics:
        //   intern_calls   = number of non-poisoned `intern()` entries
        //   intern_hits    = returned an existing `TypeId` (intrinsic, TL
        //                    hit, shard read hit, or race-loss occupied
        //                    insert)
        //   intern_misses  = stored a new `TypeData` (vacant insert)
        // Invariant:
        //   intern_calls = intern_hits + intern_misses + slow_path_errors
        // where `slow_path_errors` is the count of calls that hit the
        // `intern_slow` circuit breakers (max-types, u32-overflow). It is
        // observable as the residual `intern_calls - intern_hits -
        // intern_misses` and is not separately bucketed today.
        //
        // We gate once with `enabled_fast()` (one `OnceLock<bool>` read)
        // and cache the resulting `&'static PerfCounters` pointer in `pc`.
        // An enabled run pays the gate read plus one `counters()`
        // `OnceLock<PerfCounters>` deref per `intern()` call (vs. one per
        // increment). A disabled run pays only the gate read: subsequent
        // `if let Some(c) = pc` checks are predictable branches on a
        // local `None`, so the increment body is consistently skipped.
        let pc = if tsz_common::perf_counters::enabled_fast() {
            Some(tsz_common::perf_counters::counters())
        } else {
            None
        };
        if let Some(c) = pc {
            c.interner_intern_calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(id) = self.get_intrinsic_id(&key) {
            if let Some(c) = pc {
                c.interner_intern_hits
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            return id;
        }

        let mut hasher = FxHasher::default();
        key.hash(&mut hasher);
        let hash = hasher.finish();

        // Fast path: thread-local cache hit scoped by this interner's
        // instance_id.
        if let Some(id) = cache::intern_probe(hash, self.instance_id, &key) {
            if let Some(c) = pc {
                c.interner_intern_hits
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            return id;
        }

        let result = self.intern_slow(key, hash, pc);
        if result != TypeId::ERROR {
            cache::intern_insert(hash, self.instance_id, key, result);
        }
        result
    }

    /// Allocate a fresh `TypeId` for declaration-scoped types that carry
    /// identity beyond their structural payload.
    ///
    /// The stored `TypeData` is still available through `lookup`, but this
    /// intentionally bypasses `key_to_index` and the thread-local intern cache
    /// so two declarations with the same surface name and constraint do not
    /// collapse to one semantic type parameter.
    pub(crate) fn intern_fresh(&self, key: TypeData) -> TypeId {
        if self.poisoned.load(std::sync::atomic::Ordering::Relaxed) {
            return TypeId::ERROR;
        }
        if self.interned_type_limit_exceeded() {
            return self.poison_due_to_interned_type_limit();
        }

        let mut hasher = FxHasher::default();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        let shard_idx = (hash as usize) & (SHARD_COUNT - 1);
        let shard = &self.shards[shard_idx];
        let inner = shard.get_inner();

        let local_index = shard.next_index.fetch_add(1, Ordering::Relaxed);
        if local_index > (u32::MAX >> SHARD_BITS) {
            return TypeId::ERROR;
        }

        let order = self.alloc_counter.fetch_add(1, Ordering::Relaxed);
        {
            let mut vec = tsz_common::perf_counters::time_shard_write(shard_idx as u32, || {
                inner.index_to_key.write_unpoisoned("interner.index_to_key")
            });
            let mut ord = tsz_common::perf_counters::time_shard_write(shard_idx as u32, || {
                inner.alloc_order.write_unpoisoned("interner.alloc_order")
            });
            write_id_slot(&mut vec, local_index as usize, key, || TypeData::Error);
            write_id_slot(&mut ord, local_index as usize, order, || u32::MAX);
        }

        self.make_id(local_index, shard_idx as u32)
    }

    /// Slow path for `intern`: goes through `DashMap` and RwLock-protected storage.
    ///
    /// `pc` is the cached counter pointer from the public `intern()` entry,
    /// `Some` only when `enabled_fast()` was true at the call site. Threading
    /// it through avoids re-deref'ing the `OnceLock` and re-checking the gate
    /// in this slow path, and lets the caller make the lifetime of the cache
    /// pointer explicit.
    #[inline(never)]
    fn intern_slow(
        &self,
        key: TypeData,
        hash: u64,
        pc: Option<&'static tsz_common::perf_counters::PerfCounters>,
    ) -> TypeId {
        let shard_idx = (hash as usize) & (SHARD_COUNT - 1);
        let shard = &self.shards[shard_idx];
        let inner = shard.get_inner();

        // Try to get existing ID (lock-free read). This runs even when the
        // interner is poisoned: already-interned keys must keep resolving to
        // their existing ids so program semantics and shared caches survive
        // a type-count-limit event (only *new* types degrade to ERROR).
        if let Some(entry) = inner.key_to_index.get(&key) {
            let local_index = *entry.value();
            if let Some(c) = pc {
                c.interner_intern_hits
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            return self.make_id(local_index, shard_idx as u32);
        }

        // Circuit breaker 1: type count limit. Returning `TypeId::ERROR` here
        // intentionally does not credit a hit or miss — the residual
        // `calls - hits - misses` exposes circuit-breaker activations.
        if self.interned_type_limit_exceeded() {
            return self.poison_due_to_interned_type_limit();
        }

        // Allocate new index
        let local_index = shard.next_index.fetch_add(1, Ordering::Relaxed);
        if local_index > (u32::MAX >> SHARD_BITS) {
            // Circuit breaker 2: u32 overflow. Same rationale as #1: not
            // credited as hit or miss; observable via the residual.
            return TypeId::ERROR;
        }

        // Double-check: another thread might have inserted while we allocated
        match inner.key_to_index.entry(key) {
            Entry::Vacant(e) => {
                // Record allocation order for deterministic union member sorting.
                let order = self.alloc_counter.fetch_add(1, Ordering::Relaxed);
                {
                    // T2.4 instrumentation: time the shard's write-lock
                    // acquisitions. With `perf-counters-timing` ON, each
                    // observation lands in the lock-wait histogram. With it
                    // OFF (default) the wrapper compiles to a direct call —
                    // no `Instant::now()`, no atomic touch.
                    let mut vec =
                        tsz_common::perf_counters::time_shard_write(shard_idx as u32, || {
                            inner.index_to_key.write_unpoisoned("interner.index_to_key")
                        });
                    let mut ord =
                        tsz_common::perf_counters::time_shard_write(shard_idx as u32, || {
                            inner.alloc_order.write_unpoisoned("interner.alloc_order")
                        });
                    write_id_slot(&mut vec, local_index as usize, key, || TypeData::Error);
                    write_id_slot(&mut ord, local_index as usize, order, || u32::MAX);
                }
                // Publish the index only after its slot is readable so a
                // concurrent `key_to_index` hit can never observe an
                // unwritten `index_to_key` slot via `lookup`.
                e.insert(local_index);
                if let Some(c) = pc {
                    c.interner_intern_misses
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                self.make_id(local_index, shard_idx as u32)
            }
            Entry::Occupied(e) => {
                // Another thread inserted first, use their ID. We bumped
                // `next_index` above and won't recycle it, so this is a hit
                // from the caller's POV (no new TypeData was stored).
                let existing_index = *e.get();
                if let Some(c) = pc {
                    c.interner_intern_hits
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                self.make_id(existing_index, shard_idx as u32)
            }
        }
    }

    /// Look up the `TypeData` for a given `TypeId`.
    ///
    /// Uses a thread-local direct-mapped cache for O(1) lookups on cache hits,
    /// falling back to `RwLock`-protected shard storage on misses. Cache
    /// entries are scoped by `self.instance_id` so a stale entry from a
    /// previous `TypeInterner` on the same thread (conformance runner, batch
    /// mode) is detected and treated as a miss.
    #[inline]
    pub fn lookup(&self, id: TypeId) -> Option<TypeData> {
        // Intentionally NOT gated on `poisoned`: already-interned ids must
        // remain readable after a type-count-limit event so previously
        // computed program types (and the cross-file caches that hold their
        // ids) do not collapse to opaque misses. Only new interning degrades.
        if id.is_intrinsic() || id.is_error() {
            return self.get_intrinsic_key(id);
        }

        // Fast path: thread-local cache hit scoped by this interner's
        // instance_id.
        if let Some(data) = cache::lookup_probe(id, self.instance_id) {
            return Some(data);
        }

        let data = self.lookup_slow(id)?;
        cache::lookup_insert(id, self.instance_id, data);
        Some(data)
    }

    /// Slow path for `lookup`: goes through RwLock-protected shard storage.
    #[inline(never)]
    fn lookup_slow(&self, id: TypeId) -> Option<TypeData> {
        let raw_val = id.0.checked_sub(TypeId::FIRST_USER)?;
        let shard_idx = (raw_val & SHARD_MASK) as usize;
        let local_index = raw_val >> SHARD_BITS;

        let shard = self.shards.get(shard_idx)?;
        // If shard is empty, no types have been interned there yet
        if shard.is_empty() {
            return None;
        }
        // Use inner.get() instead of get_or_init() -- if shard is non-empty,
        // inner is guaranteed initialized (intern sets it before incrementing counter).
        let inner = shard.inner.get()?;
        let vec = inner.index_to_key.read().ok()?;
        vec.get(local_index as usize).copied()
    }

    /// Look up the allocation order for a given `TypeId`.
    /// Returns `None` for intrinsic/error types (they have no alloc order).
    #[inline]
    pub(crate) fn lookup_alloc_order(&self, id: TypeId) -> Option<u32> {
        if id.is_intrinsic() || id.is_error() {
            return None;
        }
        let raw_val = id.0.checked_sub(TypeId::FIRST_USER)?;
        let shard_idx = (raw_val & SHARD_MASK) as usize;
        let local_index = raw_val >> SHARD_BITS;
        let shard = self.shards.get(shard_idx)?;
        if shard.is_empty() {
            return None;
        }
        let inner = shard.inner.get()?;
        let ord = inner.alloc_order.read().ok()?;
        let val = ord.get(local_index as usize).copied()?;
        if val == u32::MAX { None } else { Some(val) }
    }

    pub(in crate::intern) fn intern_type_list(&self, members: Vec<TypeId>) -> TypeListId {
        tsz_common::perf_counters::record_interner_type_list_intern_call();
        TypeListId(self.type_lists.intern(&members))
    }

    /// Intern a type list from a slice, avoiding Vec conversion when the caller
    /// already has a `SmallVec` or slice reference.
    pub(in crate::intern) fn intern_type_list_from_slice(&self, members: &[TypeId]) -> TypeListId {
        tsz_common::perf_counters::record_interner_type_list_intern_call();
        TypeListId(self.type_lists.intern(members))
    }

    pub(super) fn intern_tuple_list(&self, elements: Vec<TupleElement>) -> TupleListId {
        TupleListId(self.tuple_lists.intern(&elements))
    }

    pub(crate) fn intern_template_list(&self, spans: Vec<TemplateSpan>) -> TemplateLiteralId {
        TemplateLiteralId(self.template_lists.intern(&spans))
    }

    pub fn intern_object_shape(&self, shape: ObjectShape) -> ObjectShapeId {
        tsz_common::perf_counters::record_interner_object_shape_intern_call();
        ObjectShapeId(self.object_shapes.intern(shape))
    }

    pub(super) fn intern_function_shape(&self, shape: FunctionShape) -> FunctionShapeId {
        tsz_common::perf_counters::record_interner_function_shape_intern_call();
        FunctionShapeId(self.function_shapes.intern(shape))
    }

    pub(in crate::intern) fn intern_callable_shape(&self, shape: CallableShape) -> CallableShapeId {
        tsz_common::perf_counters::record_interner_callable_shape_intern_call();
        CallableShapeId(self.callable_shapes.intern(shape))
    }

    pub(super) fn intern_conditional_type(
        &self,
        conditional: ConditionalType,
    ) -> ConditionalTypeId {
        tsz_common::perf_counters::record_interner_conditional_intern_call();
        ConditionalTypeId(self.conditional_types.intern(conditional))
    }

    pub(super) fn intern_mapped_type(&self, mapped: MappedType) -> MappedTypeId {
        tsz_common::perf_counters::record_interner_mapped_intern_call();
        MappedTypeId(self.mapped_types.intern(mapped))
    }

    pub(super) fn intern_application(&self, application: TypeApplication) -> TypeApplicationId {
        tsz_common::perf_counters::record_interner_application_intern_call();
        TypeApplicationId(self.applications.intern(application))
    }

    /// Get the number of interned types (lock-free read)
    pub fn len(&self) -> usize {
        let mut total = TypeId::FIRST_USER as usize;
        for shard in &self.shards {
            total += shard.next_index.load(Ordering::Relaxed) as usize;
        }
        total
    }

    /// Check if the interner is empty (only has intrinsics)
    pub fn is_empty(&self) -> bool {
        self.len() <= TypeId::FIRST_USER as usize
    }

    /// Get an approximate count of interned types.
    /// This is cheaper than `len()` as it samples only a few shards.
    /// Used for the circuit breaker to avoid OOM.
    /// Uses the global allocation counter for an exact count (single atomic load)
    /// instead of sampling shards and extrapolating.
    #[inline]
    fn approximate_count(&self) -> usize {
        self.alloc_counter.load(Ordering::Relaxed) as usize
    }

    #[inline]
    const fn interned_type_limit_exceeded_for_count(count: usize) -> bool {
        count > MAX_INTERNED_TYPES
    }

    #[inline]
    fn interned_type_limit_exceeded(&self) -> bool {
        Self::interned_type_limit_exceeded_for_count(self.approximate_count())
    }

    #[inline]
    fn interned_type_limit_context(&self) -> InternedTypeLimitContext {
        InternedTypeLimitContext {
            current_count: self.approximate_count(),
            max_interned_types: MAX_INTERNED_TYPES,
            fallback_type: TypeId::ERROR,
        }
    }

    #[inline]
    fn poison_due_to_interned_type_limit(&self) -> TypeId {
        let context = self.interned_type_limit_context();
        if self
            .poisoned
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            tracing::warn!(
                target: "tsz::solver::interner",
                interned_type_count = context.current_count,
                max_interned_types = context.max_interned_types,
                fallback_type_id = context.fallback_type.0,
                fallback_type = "TypeId::ERROR",
                "interned type limit exceeded; poisoning type interner"
            );
        }
        context.fallback_type
    }

    /// Consume evaluation fuel and return whether fuel is exhausted.
    ///
    /// This is a per-thread budget across the `TypeEvaluator` instances of
    /// the file-check session running on this thread (see
    /// [`crate::limits::consume_evaluation_fuel`]). When exhausted, the
    /// current evaluation should bail out with ERROR, but the interner
    /// remains readable so already-computed project types do not turn into
    /// opaque `Type(N)` placeholders in later diagnostics.
    #[inline]
    pub fn consume_evaluation_fuel(&self, amount: u32) -> bool {
        crate::limits::consume_evaluation_fuel(amount)
    }

    /// Reset this thread's evaluation fuel counter.
    ///
    /// Called at the start of each top-level file check session. `tsc` resets
    /// its `instantiationCount` per checked source element, so the fuel limit
    /// must bound *per-check* runaway instantiation rather than accumulate
    /// across the whole program — a cumulative budget starves the tail files
    /// of any multi-thousand-file program into blanket `TypeId::ERROR`.
    #[inline]
    pub fn reset_evaluation_fuel(&self) {
        crate::limits::reset_evaluation_fuel();
    }

    /// Check whether this thread's evaluation fuel is exhausted without
    /// consuming any.
    #[inline]
    pub fn is_evaluation_fuel_exhausted(&self) -> bool {
        crate::limits::is_evaluation_fuel_exhausted()
    }

    #[inline]
    fn make_id(&self, local_index: u32, shard_idx: u32) -> TypeId {
        let raw_val = (local_index << SHARD_BITS) | (shard_idx & SHARD_MASK);
        let id = TypeId(TypeId::FIRST_USER + raw_val);

        // SAFETY: Assert that we're not overflowing into the local ID space (MSB=1).
        // Global TypeIds must have MSB=0 (0x7FFFFFFF-) to allow ScopedTypeInterner
        // to use the upper half (0x80000000+) for ephemeral types.
        debug_assert!(
            id.is_global(),
            "Global TypeId overflow: {id:?} - would conflict with local ID space"
        );

        id
    }

    const fn get_intrinsic_id(&self, key: &TypeData) -> Option<TypeId> {
        match key {
            TypeData::Intrinsic(kind) => Some(kind.to_type_id()),
            TypeData::Error => Some(TypeId::ERROR),
            // Map boolean literals to their intrinsic IDs to avoid duplicates
            TypeData::Literal(LiteralValue::Boolean(true)) => Some(TypeId::BOOLEAN_TRUE),
            TypeData::Literal(LiteralValue::Boolean(false)) => Some(TypeId::BOOLEAN_FALSE),
            _ => None,
        }
    }

    const fn get_intrinsic_key(&self, id: TypeId) -> Option<TypeData> {
        match id {
            TypeId::NONE | TypeId::ERROR => Some(TypeData::Error),
            TypeId::NEVER => Some(TypeData::Intrinsic(IntrinsicKind::Never)),
            TypeId::UNKNOWN => Some(TypeData::Intrinsic(IntrinsicKind::Unknown)),
            TypeId::ANY => Some(TypeData::Intrinsic(IntrinsicKind::Any)),
            TypeId::VOID => Some(TypeData::Intrinsic(IntrinsicKind::Void)),
            TypeId::UNDEFINED => Some(TypeData::Intrinsic(IntrinsicKind::Undefined)),
            TypeId::NULL => Some(TypeData::Intrinsic(IntrinsicKind::Null)),
            TypeId::BOOLEAN => Some(TypeData::Intrinsic(IntrinsicKind::Boolean)),
            TypeId::NUMBER => Some(TypeData::Intrinsic(IntrinsicKind::Number)),
            TypeId::STRING => Some(TypeData::Intrinsic(IntrinsicKind::String)),
            TypeId::BIGINT => Some(TypeData::Intrinsic(IntrinsicKind::Bigint)),
            TypeId::SYMBOL => Some(TypeData::Intrinsic(IntrinsicKind::Symbol)),
            TypeId::OBJECT | TypeId::PROMISE_BASE => {
                Some(TypeData::Intrinsic(IntrinsicKind::Object))
            }
            TypeId::BOOLEAN_TRUE => Some(TypeData::Literal(LiteralValue::Boolean(true))),
            TypeId::BOOLEAN_FALSE => Some(TypeData::Literal(LiteralValue::Boolean(false))),
            TypeId::FUNCTION => Some(TypeData::Intrinsic(IntrinsicKind::Function)),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "interner_tests.rs"]
mod tests;
