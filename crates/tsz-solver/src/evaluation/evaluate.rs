//! Type evaluation for meta-types (conditional, mapped, index access).
//!
//! Meta-types are "type-level functions" that compute output types from input types.
//! This module provides evaluation logic for:
//! - Conditional types: T extends U ? X : Y
//! - Distributive conditional types: (A | B) extends U ? X : Y
//! - Index access types: T[K]
//!
//! Key design:
//! - Lazy evaluation: only evaluate when needed for subtype checking
//! - Handles deferred evaluation when type parameters are unknown
//! - Supports distributivity for naked type parameters in unions

use crate::caches::db::QueryDatabase;
use crate::construction::TypeDatabase;
use crate::def::{DefId, DefKind};
use crate::diagnostics::display_provenance::{
    self, AliasApplicationPriority, AliasApplicationProvenance,
    FreshObjectLiteralDisplayProvenance, UnionOriginProvenance,
};
use crate::evaluation::request::EvaluationRequest;
use crate::evaluation::result::EvaluationResult;
use crate::instantiation::instantiate::instantiate_generic;
use crate::relations::subtype::{NoopResolver, TypeResolver};
#[cfg(test)]
use crate::types::*;
use crate::types::{
    ConditionalType, ConditionalTypeId, MappedType, MappedTypeId, StringIntrinsicKind,
    TemplateLiteralId, TemplateSpan, TupleElement, TupleListId, TypeApplicationId, TypeData,
    TypeId, TypeListId, TypeParamInfo,
};
use crate::visitors::visitor_predicates::contains_type_matching;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;

mod support;

/// Controls which subtype direction makes a member redundant when simplifying
/// a union or intersection.
enum SubtypeDirection {
    /// member[i] <: member[j] → member[i] is redundant (union semantics).
    SourceSubsumedByOther,
    /// member[j] <: member[i] → member[i] is redundant (intersection semantics).
    OtherSubsumedBySource,
}

/// Type evaluator for meta-types.
///
/// # Salsa Preparation
/// This struct uses `&mut self` methods instead of `RefCell` + `&self`.
/// This makes the evaluator thread-safe (Send) and prepares for future
/// Salsa integration where state is managed by the database runtime.
pub struct TypeEvaluator<'a, R: TypeResolver = NoopResolver> {
    interner: &'a dyn TypeDatabase,
    /// Optional query database for Salsa-backed memoization.
    query_db: Option<&'a dyn QueryDatabase>,
    resolver: &'a R,
    no_unchecked_indexed_access: bool,
    cache: FxHashMap<TypeId, TypeId>,
    /// Unified recursion guard for `TypeId` cycle detection, depth, and iteration limits.
    guard: crate::recursion::RecursionGuard<TypeId>,
    /// Recursion guard for mapped-key constraint simplification.
    pub(super) keyof_constraint_guard: crate::recursion::RecursionGuard<TypeId>,
    /// Per-DefId recursion depth counter.
    /// Allows recursive type aliases (like `TrimRight`) to expand up to `MAX_DEF_DEPTH`
    /// times before stopping, matching tsc's TS2589 "Type instantiation is excessively
    /// deep and possibly infinite" behavior. Unlike a set-based cycle detector, this
    /// permits legitimate bounded recursion where each expansion converges.
    def_depth: FxHashMap<DefId, u32>,
    /// Number of currently active `DefId` expansions at or above the threshold
    /// that turns a structural recursion bailout into a real TS2589 failure.
    real_instantiation_depth_count: u32,
    /// When true, suppress `this` type substitution during Lazy type evaluation.
    /// Used during intersection evaluation to prevent premature `this` binding to
    /// individual members instead of the full intersection type.
    suppress_this_binding: bool,
    /// PERF: Cache for subtype check results used in conditional type evaluation.
    /// Key: (`check_type`, `extends_type`), Value: `is_subtype`.
    /// Deeply recursive conditional types (`DeepReadonly`, `Compute`, etc.) often check
    /// the same (check, extends) pair many times across distributed branches and
    /// tail-recursion iterations. Caching avoids redundant structural comparison.
    conditional_subtype_cache: FxHashMap<(TypeId, TypeId), bool>,
    /// PERF: Cache whether a type contains `infer`.
    /// Recursive conditionals can revisit the same application-shaped `extends`
    /// pattern thousands of times while checking whether the application-level
    /// infer fast path applies.
    contains_infer_cache: FxHashMap<TypeId, bool>,
    /// Ceiling for eager mapped-key expansion before bailing out.
    max_mapped_keys: usize,
    /// When true, flag `depth_exceeded` on Application cycle detection.
    /// Used for TS2589 detection at type alias definition sites where
    /// self-referential conditional types produce the same Application TypeId
    /// on each expansion, preventing the per-DefId depth counter from working.
    flag_depth_on_app_cycle: bool,
    /// When true, display aliases for evaluated applications preserve expanded
    /// argument types. Declaration emit opts into this to print reusable public
    /// surfaces without changing checker diagnostic display behavior.
    expand_application_display_alias_args: bool,
    /// Set by `evaluate_conditional` when a conditional branch resolved to an
    /// Application type (via tail-call expansion or direct evaluation).
    /// `evaluate_application` reads this to store a forward display alias
    /// so the formatter shows the intermediate alias name (e.g.
    /// `DeepReadonlyObject<Part>`) rather than the outer alias (`DeepReadonly<Part>`).
    pub(super) apparent_conditional_branch: Option<TypeId>,
    /// Tracks whether ANY structural depth bailout was silently converted to an
    /// opaque (identity) result during this evaluator's lifetime. Distinct from
    /// `guard.exceeded` (cleared as part of the silent-bail policy) and from
    /// `flag_depth_on_app_cycle`. Callers that run a follow-up pass with a more
    /// powerful resolver use this to skip the retry when the original bail was
    /// structural — a more powerful resolver does not change the structural cost
    /// of recursive type-tree walks like `ts-toolbelt`'s `ComputeDeep` /
    /// `Invert` mapped+conditional bodies. See `is_silent_depth_bailed`.
    silent_depth_bailed: bool,
    /// Per-`DefId` `(max_argument_weight, new_maxima_count)` used by the TS2589
    /// detection pass to recognize a divergent (unconditionally growing)
    /// recursive alias. See `recursive_growth::detect_recursive_growth`.
    pub(super) detection_growth_runs: FxHashMap<DefId, (u64, u32)>,
}

/// Operation-local memo table statistics for [`TypeEvaluator`].
///
/// Owner: one evaluator request. The caches are dropped with the evaluator and
/// are never shared across resolver, substitution, or compiler-option modes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TypeEvaluatorCacheStatistics {
    /// Entries in the conditional subtype memo keyed by `(check_type, extends_type)`.
    pub conditional_subtype_entries: usize,
    /// Entries in the `contains infer` predicate memo keyed by `TypeId`.
    pub contains_infer_entries: usize,
    estimated_size_bytes: usize,
}

impl TypeEvaluatorCacheStatistics {
    /// Estimated heap bytes owned by the evaluator memo tables.
    #[must_use]
    pub const fn estimated_size_bytes(self) -> usize {
        self.estimated_size_bytes
    }
}

#[cfg(target_arch = "wasm32")]
const DEFAULT_MAX_MAPPED_KEYS: usize = 250;
#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_MAX_MAPPED_KEYS: usize = 500;

/// Array methods that return any (used for apparent type computation).
pub(crate) const ARRAY_METHODS_RETURN_ANY: &[&str] = &[
    "concat",
    "filter",
    "flat",
    "flatMap",
    "map",
    "reverse",
    "slice",
    "sort",
    "splice",
    "toReversed",
    "toSorted",
    "toSpliced",
    "with",
    "at",
    "find",
    "findLast",
    "pop",
    "shift",
    "entries",
    "keys",
    "values",
    "reduce",
    "reduceRight",
];
/// Array methods that return boolean.
pub(crate) const ARRAY_METHODS_RETURN_BOOLEAN: &[&str] = &["every", "includes", "some"];
/// Array methods that return number.
pub(crate) const ARRAY_METHODS_RETURN_NUMBER: &[&str] = &[
    "findIndex",
    "findLastIndex",
    "indexOf",
    "lastIndexOf",
    "push",
    "unshift",
];
/// Array methods that return void.
pub(crate) const ARRAY_METHODS_RETURN_VOID: &[&str] = &["forEach", "copyWithin", "fill"];
/// Array methods that return string.
pub(crate) const ARRAY_METHODS_RETURN_STRING: &[&str] = &["join", "toLocaleString", "toString"];

impl<'a> TypeEvaluator<'a, NoopResolver> {
    /// Create a new evaluator without a resolver.
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        static NOOP: NoopResolver = NoopResolver;
        TypeEvaluator {
            interner,
            query_db: None,
            resolver: &NOOP,
            no_unchecked_indexed_access: false,
            cache: FxHashMap::default(),
            guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::TypeEvaluation,
            ),
            keyof_constraint_guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::TypeEvaluation,
            ),
            def_depth: FxHashMap::default(),
            real_instantiation_depth_count: 0,
            suppress_this_binding: false,
            conditional_subtype_cache: FxHashMap::default(),
            contains_infer_cache: FxHashMap::default(),
            max_mapped_keys: DEFAULT_MAX_MAPPED_KEYS,
            flag_depth_on_app_cycle: false,
            expand_application_display_alias_args: false,
            apparent_conditional_branch: None,
            silent_depth_bailed: false,
            detection_growth_runs: FxHashMap::default(),
        }
    }
}

/// Snapshot of resolver/interner state needed for an `Application(base, args)`
/// evaluation. Built once by [`TypeEvaluator::application_evaluation_context`]
/// so the rest of `evaluate_application` operates on a typed bundle rather
/// than recomputing the same facts at multiple call sites.
struct ApplicationEvalContext {
    /// Formal type parameters declared on the `DefId` resolved from
    /// `app.base`, when the resolver exposes them. `None` triggers the
    /// lite-resolver fallback that extracts parameters from the resolved
    /// body's structure.
    type_params: Option<Vec<TypeParamInfo>>,
    /// The resolved body of the `DefId`, when known.
    resolved: Option<TypeId>,
    /// Set when `app.base` resolves to a `DefKind::TypeAlias` (vs a class
    /// or interface). Drives display-alias storage policy.
    is_type_alias_def: bool,
    /// Whether display-alias bookkeeping should prefer the `Application`
    /// form. True only for non-conditional type-alias applications.
    prefer_application_display_alias: bool,
    /// Set when `app.base` is a `TypeQuery` (i.e. `typeof ClassName<T>`).
    /// For `TypeQuery`-based applications the caller wants the constructor
    /// type, not the instance type, so `extract_class_instance_body` must
    /// be skipped.
    base_is_type_query: bool,
}

/// Common opening preamble for the homomorphic-mapped shortcuts:
/// `try_homomorphic_mapped_passthrough` and `try_distribute_mapped_union_arg`
/// both require `body == { [P in keyof Tᵢ]: ... }` with `Tᵢ` resolvable in
/// `type_params`. Sharing the destructure protects against drift between
/// the two call sites and avoids re-evaluating the same argument twice.
struct HomomorphicMappedArg {
    mapped: MappedType,
    source: TypeId,
    tp: TypeParamInfo,
    idx: usize,
    resolved_arg: TypeId,
}

/// Distinguishes shortcut paths in `evaluate_application` (cache hits,
/// homomorphic passthrough, mapped-union distribution) from the full
/// instantiation path.
///
/// Shortcut paths historically returned via early `decrement_def_depth` +
/// `return`, which leaves `self.apparent_conditional_branch == None` for
/// the outer caller. The full path restores the outer caller's apparent
/// branch and runs display-alias bookkeeping. The orchestrator uses this
/// outcome to apply the right cleanup without losing the historical
/// invariant.
enum ApplicationEvalOutcome {
    /// Cache hit or body-aware shortcut. Outer caller's apparent branch
    /// is NOT restored.
    ShortCircuit(TypeId),
    /// Result computed via the full instantiation pipeline. Apparent
    /// branch is restored and display-alias bookkeeping runs.
    Computed(TypeId),
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Return entry and size accounting for this evaluator's operation-local caches.
    #[must_use]
    pub fn cache_statistics(&self) -> TypeEvaluatorCacheStatistics {
        let conditional_subtype_entries = self.conditional_subtype_cache.len();
        let contains_infer_entries = self.contains_infer_cache.len();
        let type_evaluator_cache_estimated_size_bytes = conditional_subtype_entries
            .saturating_mul(std::mem::size_of::<((TypeId, TypeId), bool)>())
            .saturating_add(
                contains_infer_entries.saturating_mul(std::mem::size_of::<(TypeId, bool)>()),
            );

        TypeEvaluatorCacheStatistics {
            conditional_subtype_entries,
            contains_infer_entries,
            estimated_size_bytes: type_evaluator_cache_estimated_size_bytes,
        }
    }

    fn has_nested_complex_marker(&self, type_id: TypeId) -> bool {
        contains_type_matching(self.interner, type_id, |key| {
            matches!(
                key,
                TypeData::Conditional(_)
                    | TypeData::Mapped(_)
                    | TypeData::IndexAccess(_, _)
                    | TypeData::KeyOf(_)
                    | TypeData::TypeQuery(_)
                    | TypeData::TemplateLiteral(_)
                    | TypeData::ReadonlyType(_)
                    | TypeData::StringIntrinsic { .. }
                    | TypeData::ThisType
                    | TypeData::Lazy(_)
                    | TypeData::Application(_)
            )
        })
    }

    /// Maximum recursive expansion depth for a single `DefId`.
    /// Matches TypeScript's instantiation depth limit that triggers TS2589.
    const MAX_DEF_DEPTH: u32 = 100;

    /// When the structural per-`TypeId` recursion guard hits its depth limit,
    /// surface it as TS2589 only if some DefId has been recursively expanded at
    /// least this many times — otherwise treat the bailout as the stack-protection
    /// cost of legitimate finite recursion and leave the type opaque.
    ///
    /// Calibration: empirically, `Permutation<U>` with `|U| ≤ 3` peaks around
    /// `def_depth ≈ 33` when it hits the structural limit, while unbounded
    /// patterns like `type Foo<T,B> = { "true": Foo<T, Foo<T,B>> }[T]` saturate
    /// near `def_depth ≈ 50`.
    const REAL_INSTANTIATION_BAILOUT_THRESHOLD: u32 = 40;

    fn increment_def_depth(&mut self, def_id: DefId) -> bool {
        let depth = self.def_depth.entry(def_id).or_insert(0);
        if *depth >= Self::MAX_DEF_DEPTH {
            return false;
        }

        let was_real_instantiation_depth = *depth >= Self::REAL_INSTANTIATION_BAILOUT_THRESHOLD;
        *depth += 1;
        if !was_real_instantiation_depth && *depth >= Self::REAL_INSTANTIATION_BAILOUT_THRESHOLD {
            self.real_instantiation_depth_count += 1;
        }
        true
    }

    fn decrement_def_depth(&mut self, def_id: DefId) {
        if let Some(depth) = self.def_depth.get_mut(&def_id) {
            let was_real_instantiation_depth = *depth >= Self::REAL_INSTANTIATION_BAILOUT_THRESHOLD;
            *depth = depth.saturating_sub(1);
            if was_real_instantiation_depth && *depth < Self::REAL_INSTANTIATION_BAILOUT_THRESHOLD {
                self.real_instantiation_depth_count =
                    self.real_instantiation_depth_count.saturating_sub(1);
            }
        }
    }

    #[inline]
    const fn has_real_instantiation_depth(&self) -> bool {
        self.real_instantiation_depth_count > 0
    }

    /// Create a new evaluator with a custom resolver.
    pub fn with_resolver(interner: &'a dyn TypeDatabase, resolver: &'a R) -> Self {
        TypeEvaluator {
            interner,
            query_db: None,
            resolver,
            no_unchecked_indexed_access: false,
            cache: FxHashMap::default(),
            guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::TypeEvaluation,
            ),
            keyof_constraint_guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::TypeEvaluation,
            ),
            def_depth: FxHashMap::default(),
            real_instantiation_depth_count: 0,
            suppress_this_binding: false,
            conditional_subtype_cache: FxHashMap::default(),
            contains_infer_cache: FxHashMap::default(),
            max_mapped_keys: DEFAULT_MAX_MAPPED_KEYS,
            flag_depth_on_app_cycle: false,
            expand_application_display_alias_args: false,
            apparent_conditional_branch: None,
            silent_depth_bailed: false,
            detection_growth_runs: FxHashMap::default(),
        }
    }

    /// Set the query database for Salsa-backed memoization.
    pub fn with_query_db(mut self, db: &'a dyn QueryDatabase) -> Self {
        self.query_db = Some(db);
        self
    }

    /// Suppress `this` type substitution during Lazy type evaluation.
    /// When set, `ThisType` references inside resolved Lazy types are preserved
    /// rather than being bound to the Lazy type's own identity. This is used
    /// during interface heritage merging so that `this` can later be correctly
    /// bound to the final derived interface type.
    pub const fn with_suppress_this_binding(mut self) -> Self {
        self.suppress_this_binding = true;
        self
    }

    /// Flag `depth_exceeded` when cycle detection fires on an Application type.
    /// Used for TS2589 detection at type alias definition sites where
    /// self-referential conditional types produce the same Application TypeId
    /// on each expansion (e.g., `Foo<unknown>` → body → `Foo<unknown>`),
    /// preventing the normal per-DefId depth counter from triggering.
    pub const fn with_flag_depth_on_app_cycle(mut self) -> Self {
        self.flag_depth_on_app_cycle = true;
        self
    }

    /// True when this evaluator is running the TS2589 depth-detection pass
    /// (see `with_flag_depth_on_app_cycle`). Callers in other modules use this
    /// to drive self-referential recursion that normal evaluation defers.
    pub(crate) const fn is_depth_detection_pass(&self) -> bool {
        self.flag_depth_on_app_cycle
    }

    /// Preserve evaluated application display aliases with already-expanded
    /// type arguments. This is declaration-emitter-only behavior; checker
    /// diagnostics keep the original alias origin to avoid recursive display
    /// chains in complex conditional cases.
    pub const fn with_expanded_application_display_alias_args(mut self) -> Self {
        self.expand_application_display_alias_args = true;
        self
    }

    /// Drain the evaluator's internal cache, returning all intermediate results.
    /// This allows callers to persist intermediate evaluation results
    /// (e.g., from recursive mapped type expansion) into a longer-lived cache.
    pub fn drain_cache(&mut self) -> impl Iterator<Item = (TypeId, TypeId)> + '_ {
        self.cache.drain()
    }

    /// Pre-seed the evaluator's cache with previously computed evaluation results.
    /// This prevents re-evaluation of intermediate types (e.g., nested generic
    /// applications) that were already computed in earlier evaluator runs.
    pub fn seed_cache(&mut self, entries: impl Iterator<Item = (TypeId, TypeId)>) {
        self.cache.extend(entries);
    }

    pub fn set_no_unchecked_indexed_access(&mut self, enabled: bool) {
        if self.no_unchecked_indexed_access != enabled {
            self.cache.clear();
        }
        self.no_unchecked_indexed_access = enabled;
    }

    pub const fn set_max_mapped_keys(&mut self, max_mapped_keys: usize) {
        self.max_mapped_keys = max_mapped_keys;
    }

    /// Reset per-evaluation state so this evaluator can be reused.
    ///
    /// Clears the cache, cycle detection sets, and counters while preserving
    /// configuration and borrowed references. Uses `.clear()` to reuse memory.
    #[inline]
    pub fn reset(&mut self) {
        self.cache.clear();
        self.guard.reset();
        self.def_depth.clear();
        self.real_instantiation_depth_count = 0;
    }

    /// Evaluate a normalized request, applying option-sensitive configuration
    /// before consulting this evaluator's local cache.
    pub fn evaluate_request(&mut self, request: EvaluationRequest) -> TypeId {
        self.evaluate_request_result(request).into_type_id()
    }

    /// Evaluate a normalized request and return the typed result stage.
    pub fn evaluate_request_result(&mut self, request: EvaluationRequest) -> EvaluationResult {
        self.set_no_unchecked_indexed_access(request.no_unchecked_indexed_access());
        EvaluationResult::new(self.evaluate(request.type_id()))
    }

    // =========================================================================
    // Accessor methods for evaluate_rules modules
    // =========================================================================

    /// Get the type interner.
    #[inline]
    pub(crate) fn interner(&self) -> &'a dyn TypeDatabase {
        self.interner
    }

    /// Get the type resolver.
    #[inline]
    pub(crate) const fn resolver(&self) -> &'a R {
        self.resolver
    }

    #[inline]
    pub(crate) const fn max_mapped_keys(&self) -> usize {
        self.max_mapped_keys
    }

    /// Get the query database when one is available.
    #[inline]
    pub(crate) const fn query_db(&self) -> Option<&'a dyn QueryDatabase> {
        self.query_db
    }

    /// PERF: Look up a cached subtype result from conditional type evaluation.
    #[inline]
    pub(crate) fn cached_conditional_subtype(
        &self,
        check: TypeId,
        extends: TypeId,
    ) -> Option<bool> {
        self.conditional_subtype_cache
            .get(&(check, extends))
            .copied()
    }

    /// PERF: Cache a subtype result from conditional type evaluation.
    #[inline]
    pub(crate) fn cache_conditional_subtype(
        &mut self,
        check: TypeId,
        extends: TypeId,
        result: bool,
    ) {
        self.conditional_subtype_cache
            .insert((check, extends), result);
    }

    /// PERF: Look up whether a type contains `infer`.
    #[inline]
    pub(crate) fn cached_contains_infer(&self, type_id: TypeId) -> Option<bool> {
        self.contains_infer_cache.get(&type_id).copied()
    }

    /// PERF: Cache whether a type contains `infer`.
    #[inline]
    pub(crate) fn cache_contains_infer(&mut self, type_id: TypeId, result: bool) {
        self.contains_infer_cache.insert(type_id, result);
    }

    /// Check if `no_unchecked_indexed_access` is enabled.
    #[inline]
    pub(crate) const fn no_unchecked_indexed_access(&self) -> bool {
        self.no_unchecked_indexed_access
    }

    /// Check if depth limit was exceeded.
    #[inline]
    pub const fn is_depth_exceeded(&self) -> bool {
        self.guard.is_exceeded()
    }

    /// Whether any structural depth bailout was silently converted to an
    /// opaque (identity) result during this evaluator's lifetime.
    ///
    /// `is_depth_exceeded` is intentionally cleared when the silent-bail policy
    /// fires for legitimate finite recursion (see `RecursionResult::DepthExceeded`
    /// handling), so a follow-up pass with a more powerful resolver cannot use
    /// that flag to decide whether to retry. This counter preserves the signal.
    ///
    /// Callers that retry on the same root `type_id` should treat a silent bail
    /// as "the structural type-tree walk hit its protection limit" — running
    /// the retry will hit the same limit at the same shape and burn the same
    /// time without making additional progress.
    #[inline]
    pub const fn is_silent_depth_bailed(&self) -> bool {
        self.silent_depth_bailed
    }

    /// Mark the guard as exceeded, causing subsequent evaluations to bail out.
    ///
    /// Used when an external condition (e.g. mapped key count or distribution
    /// size exceeds its limit) means further recursive evaluation should stop.
    #[inline]
    pub(crate) const fn mark_depth_exceeded(&mut self) {
        self.guard.mark_exceeded();
    }

    /// Global thread-local depth counter for cross-evaluator stack overflow prevention.
    ///
    /// Each `SubtypeChecker::evaluate_type` creates a fresh `TypeEvaluator` with fresh
    /// per-evaluator guards. But the OS stack accumulates across ALL of them. For example,
    /// `Vector<T> implements Seq<T>` where `Opt<T>` has `toVector(): Vector<T>` and
    /// `Vector` has `Exclude<T, U>` in an overload return type: each structural comparison
    /// level creates ~8 evaluate calls, and the subtype checker recurses 10+ levels deep,
    /// producing 100+ nested evaluate frames that overflow the 8MB default stack.
    ///
    /// This counter tracks cumulative `evaluate` frames across all `TypeEvaluator` instances
    /// on the current thread's call stack. When it exceeds `MAX_GLOBAL_EVAL_DEPTH`, we
    /// bail out with ERROR to prevent stack overflow.
    const MAX_GLOBAL_EVAL_DEPTH: u32 = 200;

    /// Evaluate a type, resolving any meta-types if possible.
    /// Returns the evaluated type (may be the same if no evaluation needed).
    #[inline]
    pub fn evaluate(&mut self, type_id: TypeId) -> TypeId {
        // Fast path for intrinsics
        if type_id.is_intrinsic() {
            return type_id;
        }

        // Fast path: check local cache BEFORE depth checks.
        // Most evaluate() calls are for already-evaluated types (cache hits),
        // so checking the cache first avoids unnecessary guard operations.
        if let Some(&cached) = self.cache.get(&type_id) {
            return cached;
        }

        // Check if depth was already exceeded in a previous call
        if self.guard.is_exceeded() {
            return TypeId::ERROR;
        }

        // Cross-evaluator stack overflow prevention.
        // Only check thread-local global depth when the local guard depth
        // is already significant (>= 10). This avoids expensive TLS access
        // on the vast majority of shallow evaluations.
        if self.guard.depth() >= 10 {
            thread_local! {
                static GLOBAL_EVAL_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
            }
            let global_depth = GLOBAL_EVAL_DEPTH.with(|d| {
                let v = d.get();
                d.set(v + 1);
                v
            });
            if global_depth >= Self::MAX_GLOBAL_EVAL_DEPTH {
                GLOBAL_EVAL_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
                // Cross-evaluator stack protection: leave `type_id` opaque
                // rather than propagating ERROR. The outer evaluator can
                // proceed at a shallower depth without inheriting a sticky
                // exceeded flag. See the analogous DepthExceeded arm below.
                self.silent_depth_bailed = true;
                return type_id;
            }
            let result = self.evaluate_guarded(type_id);
            GLOBAL_EVAL_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
            return result;
        }

        self.evaluate_guarded(type_id)
    }

    /// Inner evaluate logic, called after global depth check.
    ///
    /// Wrapped with `stacker::maybe_grow()` so that deeply nested conditional/
    /// mapped type chains (ts-toolbelt, ts-essentials) can grow the stack
    /// dynamically instead of crashing even if the logical recursion guard
    /// has not yet tripped.
    fn evaluate_guarded(&mut self, type_id: TypeId) -> TypeId {
        stacker::maybe_grow(256 * 1024, 2 * 1024 * 1024, || {
            self.evaluate_guarded_inner(type_id)
        })
    }

    /// Interval for checking global evaluation fuel.
    ///
    /// We amortize the atomic load by only checking the global fuel counter
    /// every N iterations of the per-evaluator guard. This keeps the hot path
    /// fast while still catching runaway expansion within a few hundred iterations.
    const FUEL_CHECK_INTERVAL: u32 = 128;

    /// Actual evaluate logic -- separated so `stacker::maybe_grow` can wrap it.
    fn evaluate_guarded_inner(&mut self, type_id: TypeId) -> TypeId {
        use crate::recursion::RecursionResult;

        let _span =
            tracing::trace_span!("evaluate_type", ty = type_id.0, depth = self.guard.depth(),)
                .entered();

        // The entry-point `evaluate` already consulted `self.cache` and only
        // dispatched here on a miss. `evaluate_guarded_inner` is reached
        // exclusively through `evaluate_guarded`, which is itself only
        // called from `evaluate` (lines 438 and 443) — both call sites sit
        // *after* the cache check at line 411. `&mut self` is held
        // exclusively across the call, so the cache cannot have been
        // mutated in the interim and `stacker::maybe_grow` runs the
        // closure synchronously on a grown stack frame. A second
        // `cache.get` here would always miss; skip it.

        // Unified enter: checks iterations, depth, cycle detection, and visiting set size
        match self.guard.enter(type_id) {
            RecursionResult::Entered => {}
            RecursionResult::Cycle => {
                // Recursion guard for self-referential mapped/application types.
                // Recursive mapped types must stay deferred here. Collapsing them to
                // `{}` loses the constraint structure and can incorrectly make
                // self-referential generic constraints look satisfied.
                let key = self.interner.lookup(type_id);
                if matches!(key, Some(TypeData::Mapped(_))) {
                    self.cache.insert(type_id, type_id);
                    return type_id;
                }
                // When checking type alias definitions for TS2589, a cycle on an
                // Application means the recursive expansion produces the same TypeId
                // each time (e.g., `Foo<unknown>` → body → `Foo<unknown>`). This is
                // effectively infinite recursion that the per-DefId counter can't
                // catch because cycle detection fires first. Flag depth_exceeded so
                // the checker can emit TS2589.
                if self.flag_depth_on_app_cycle && matches!(key, Some(TypeData::Application(_))) {
                    self.guard.mark_exceeded();
                    return TypeId::ERROR;
                }
                return type_id;
            }
            RecursionResult::DepthExceeded => {
                // The per-`TypeId` guard's depth limit is structural — it caps the
                // type-tree walk to protect the stack, not the instantiation chain.
                // tsc's `instantiationDepth` (the source of TS2589) is mirrored by
                // `def_depth`, so consult that to decide whether the bailout is a
                // real runaway (escalate) or just the structural cost of legitimate
                // finite recursion like the type-challenges `Permutation<U>` /
                // `Combination<U>` patterns (silently leave `type_id` opaque).
                if self.has_real_instantiation_depth() {
                    self.cache.insert(type_id, TypeId::ERROR);
                    return TypeId::ERROR;
                }
                self.guard.clear_exceeded();
                self.silent_depth_bailed = true;
                self.cache.insert(type_id, type_id);
                return type_id;
            }
            RecursionResult::IterationExceeded => {
                self.cache.insert(type_id, type_id);
                return type_id;
            }
        }

        // Global fuel check: amortized to every FUEL_CHECK_INTERVAL iterations.
        // This prevents deeply recursive type libraries (ts-toolbelt, ts-essentials)
        // from consuming unbounded memory through type instantiation that creates
        // new TypeIds on each expansion. Mirrors tsc's global `instantiationCount`.
        if self
            .guard
            .iterations()
            .is_multiple_of(Self::FUEL_CHECK_INTERVAL)
            && self
                .interner
                .consume_evaluation_fuel(Self::FUEL_CHECK_INTERVAL)
        {
            self.guard.mark_exceeded();
            self.guard.leave(type_id);
            self.cache.insert(type_id, TypeId::ERROR);
            return TypeId::ERROR;
        }

        let key = match self.interner.lookup(type_id) {
            Some(k) => k,
            None => {
                self.guard.leave(type_id);
                return type_id;
            }
        };

        // Visitor pattern: dispatch to appropriate visit_* method
        let result = self.visit_type_key(type_id, &key);

        // Symmetric cleanup: leave guard and cache result
        self.guard.leave(type_id);
        self.cache.insert(type_id, result);

        result
    }

    /// Evaluate a generic type application: Base<Args>
    ///
    /// Algorithm:
    /// 1. Look up the base type - if it's a Ref, resolve it
    /// 2. Get the type parameters for the base symbol
    /// 3. If we have type params, instantiate the resolved type with args
    /// 4. Recursively evaluate the result
    fn evaluate_application(
        &mut self,
        app_id: TypeApplicationId,
        original_type_id: TypeId,
    ) -> TypeId {
        let app = self.interner.type_application(app_id);

        // Phase 1 — callee normalization. `Lazy(DefId)` is the dominant
        // shape from the binder, but `TypeQuery`, `UnresolvedTypeName`, and
        // symbol-backed objects can also reach this entrypoint after
        // cross-file lowering or value-position queries. Bases without a
        // defining `DefId` stay opaque so later passes with a richer
        // resolver can expand them.
        let Some(def_id) = self.resolve_application_def_id(app.base) else {
            return original_type_id;
        };

        tracing::trace!(
            base = app.base.0,
            ?def_id,
            num_args = app.args.len(),
            "evaluate_application"
        );

        // Phase 2 — per-DefId recursion guard. Up to MAX_DEF_DEPTH bounded
        // recursive expansions are allowed before bailing to `TypeId::ERROR`,
        // matching tsc's TS2589 behavior.
        if !self.increment_def_depth(def_id) {
            self.guard.mark_exceeded();
            return TypeId::ERROR;
        }

        // Divergence guard. MAX_DEF_DEPTH bounds the *number* of re-expansions
        // but not the *size* of each, so a growing recursive alias can build
        // enormous types within that budget. Gating on depth >= 2 keeps flat,
        // non-recursive instantiation from feeding the detector.
        if self.def_depth.get(&def_id).is_some_and(|&d| d >= 2)
            && self.detect_recursive_growth(def_id, &app.args)
        {
            self.decrement_def_depth(def_id);
            self.guard.mark_exceeded();
            return TypeId::ERROR;
        }

        // Phase 3 — build the evaluation context.
        let ctx = self.application_evaluation_context(def_id, app.base);

        // See `ApplicationEvalOutcome` for why ShortCircuit branches do not
        // restore `saved_apparent` — outer caller observes `None`.
        let saved_apparent = self.apparent_conditional_branch.take();

        // Phase 4 — raw-args cache shortcut. Lite resolvers (e.g. inside
        // `SubtypeChecker`) often return `None` for `get_lazy_type_params`,
        // so the normal expanded-args lookup never runs. Trying `app.args`
        // first lets every context benefit from previously computed results.
        if let Some(db) = self.query_db {
            let no_unchecked = self.no_unchecked_indexed_access;
            if let Some(cached) = db.lookup_application_eval_cache(def_id, &app.args, no_unchecked)
            {
                self.decrement_def_depth(def_id);
                return cached;
            }
        }

        let outcome = self.evaluate_application_body(def_id, original_type_id, &app.args, &ctx);

        // Phase 6 — outcome-dependent cleanup. ShortCircuit matches the
        // historical decrement-and-return shape; Computed restores the
        // outer apparent branch and runs display-alias bookkeeping.
        match outcome {
            ApplicationEvalOutcome::ShortCircuit(value) => {
                self.decrement_def_depth(def_id);
                value
            }
            ApplicationEvalOutcome::Computed(result) => {
                // Read the apparent conditional branch set during THIS
                // application, then restore whatever was saved for the
                // outer caller.
                let my_apparent_branch = self.apparent_conditional_branch.take();
                self.apparent_conditional_branch = saved_apparent;
                self.decrement_def_depth(def_id);

                // Phase 7 — display-alias bookkeeping. Skip entirely when
                // the result is the original `Application` itself (the
                // historical `if result != original_type_id` gate).
                if result != original_type_id {
                    self.record_application_evaluation_display_aliases(
                        result,
                        original_type_id,
                        &app.args,
                        ctx.is_type_alias_def,
                        ctx.prefer_application_display_alias,
                        my_apparent_branch,
                    );
                }
                result
            }
        }
    }

    /// Phase-1 helper: resolve an `Application` base to a [`DefId`].
    ///
    /// Returns `None` when the application's base does not normalize to a
    /// defining `DefId` (e.g. an interned base that no longer resolves, or
    /// a base whose `TypeData` shape simply has no associated `DefId`).
    /// Both cases must keep the application opaque, so the caller treats
    /// `None` the same way.
    fn resolve_application_def_id(&self, base: TypeId) -> Option<DefId> {
        let base_key = self.interner.lookup(base)?;
        match base_key {
            TypeData::Lazy(def_id) => Some(def_id),
            TypeData::TypeQuery(sym_ref) => self.resolver.symbol_to_def_id(sym_ref),
            TypeData::UnresolvedTypeName(atom) => {
                // `Application(UnresolvedTypeName(name), args)` residue from
                // cross-file lowering can resolve through the merged binder
                // graph at evaluation time — e.g. `util.OmitKeys` whose
                // lowering pass missed the imported namespace's def_id.
                let name = self.interner.resolve_atom(atom);
                self.resolver.resolve_unresolved_type_name(&name)
            }
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => self
                .interner
                .object_shape(shape_id)
                .symbol
                .and_then(|sym_id| {
                    self.resolver
                        .symbol_to_def_id(crate::types::SymbolRef(sym_id.0))
                }),
            _ => None,
        }
    }

    /// Phase-3 helper: assemble the [`ApplicationEvalContext`] for a
    /// resolved `DefId`.
    ///
    /// Reads type parameters and the resolved body from the resolver,
    /// records whether the body is a conditional alias (which drives both
    /// the marker on the base type and the display-alias policy), and
    /// emits the matching trace event the historical inline code emitted.
    fn application_evaluation_context(
        &mut self,
        def_id: DefId,
        app_base: TypeId,
    ) -> ApplicationEvalContext {
        let type_params = self.resolver.get_lazy_type_params(def_id);
        let base_is_type_query =
            matches!(self.interner.lookup(app_base), Some(TypeData::TypeQuery(_)));
        // For `typeof ClassName<T>` (TypeQuery base), use `resolve_type_query` to get
        // the constructor type rather than the instance type that `resolve_lazy` returns
        // for classes. Type-position references (`ClassName<T>`) continue to use
        // `resolve_lazy` which correctly provides the instance type.
        let resolved = if base_is_type_query {
            if let Some(TypeData::TypeQuery(sym_ref)) = self.interner.lookup(app_base) {
                self.resolver
                    .resolve_type_query(sym_ref, self.interner)
                    .or_else(|| self.resolver.resolve_lazy(def_id, self.interner))
            } else {
                self.resolver.resolve_lazy(def_id, self.interner)
            }
        } else {
            self.resolver.resolve_lazy(def_id, self.interner)
        };
        let def_kind = self.resolver.get_def_kind(def_id);
        let is_type_alias_def = matches!(def_kind, Some(DefKind::TypeAlias));
        let resolved_has_conditional_body = resolved.is_some_and(|body| {
            matches!(self.interner.lookup(body), Some(TypeData::Conditional(_)))
        });
        if is_type_alias_def && resolved_has_conditional_body {
            self.interner.mark_conditional_alias_base(app_base);
        }
        let prefer_application_display_alias = is_type_alias_def && !resolved_has_conditional_body;

        tracing::trace!(
            ?def_id,
            has_type_params = type_params.is_some(),
            type_params_count = type_params.as_ref().map(std::vec::Vec::len),
            has_resolved = resolved.is_some(),
            resolved_key = ?resolved.and_then(|r| self.interner.lookup(r)),
            "evaluate_application resolve"
        );

        ApplicationEvalContext {
            type_params,
            resolved,
            is_type_alias_def,
            prefer_application_display_alias,
            base_is_type_query,
        }
    }

    /// Phase-5 dispatch between the canonical known-params path and the
    /// lite-resolver fallback that extracts parameters from the resolved
    /// type's shape.
    fn evaluate_application_body(
        &mut self,
        def_id: DefId,
        original_type_id: TypeId,
        args: &[TypeId],
        ctx: &ApplicationEvalContext,
    ) -> ApplicationEvalOutcome {
        if let Some(type_params) = ctx.type_params.as_ref() {
            let Some(resolved) = ctx.resolved else {
                return ApplicationEvalOutcome::Computed(original_type_id);
            };
            // When the resolver returns `unknown` for the alias body, the
            // body hasn't been registered yet (e.g. cross-file alias whose
            // declaring file is still being processed in parallel
            // checking). Substituting an `unknown` body would collapse
            // `Foo<Args>` to bare `unknown` and erase its structural shape
            // downstream. Bail out and keep the original `Application`
            // opaque so later evaluator passes (with a populated body) can
            // expand it correctly.
            if resolved == TypeId::UNKNOWN {
                return ApplicationEvalOutcome::Computed(original_type_id);
            }
            self.evaluate_application_with_known_params(
                def_id,
                original_type_id,
                args,
                resolved,
                type_params,
                ctx.prefer_application_display_alias,
                ctx.base_is_type_query,
            )
        } else if let Some(resolved) = ctx.resolved {
            // Lite-resolver fallback: extract type parameters from the
            // resolved type's properties.
            //
            // For `typeof ClassExpr<T>` (TypeQuery base, Callable resolved type),
            // use per-signature instantiation so that the class type parameters
            // stored in `sig.type_params` are CONSUMED rather than SHADOWED.
            // `instantiate_generic` calls `TypeInstantiator` which calls
            // `enter_shadowing_scope(&sig.type_params)`, blocking substitution
            // of those names from the outer substitution built from extracted params.
            if ctx.base_is_type_query
                && matches!(self.interner.lookup(resolved), Some(TypeData::Callable(_)))
                && !args.is_empty()
            {
                if let Some(specialized) = self.try_instantiate_callable_type_params(resolved, args)
                {
                    let evaluated = self.evaluate(specialized);
                    return ApplicationEvalOutcome::Computed(evaluated);
                }
                return ApplicationEvalOutcome::Computed(original_type_id);
            }
            let extracted_params = self.extract_type_params_from_type(resolved);
            if !extracted_params.is_empty() && extracted_params.len() == args.len() {
                self.evaluate_application_with_extracted_params(
                    def_id,
                    original_type_id,
                    args,
                    resolved,
                    &extracted_params,
                    ctx.prefer_application_display_alias,
                )
            } else {
                ApplicationEvalOutcome::Computed(original_type_id)
            }
        } else {
            ApplicationEvalOutcome::Computed(original_type_id)
        }
    }

    /// Known-params application path: argument preparation, expanded-args
    /// cache lookup, homomorphic passthrough, class-instance extraction,
    /// mapped-union distribution, then the main `instantiate_generic` +
    /// evaluate sequence with display-alias storage.
    fn evaluate_application_with_known_params(
        &mut self,
        def_id: DefId,
        original_type_id: TypeId,
        args: &[TypeId],
        resolved: TypeId,
        type_params: &[TypeParamInfo],
        prefer_application_display_alias: bool,
        base_is_type_query: bool,
    ) -> ApplicationEvalOutcome {
        let expanded_args = self.prepare_expanded_args_for_body(resolved, args);
        let no_unchecked_indexed_access = self.no_unchecked_indexed_access;

        if let Some(db) = self.query_db
            && let Some(cached) = db.lookup_application_eval_cache(
                def_id,
                &expanded_args,
                no_unchecked_indexed_access,
            )
        {
            return ApplicationEvalOutcome::ShortCircuit(cached);
        }

        // Homomorphic mapped-type passthrough for non-object arguments.
        // tsc's `instantiateMappedType` returns the argument directly when
        // the body is `{ [P in keyof T]: ... }` and T is not an object
        // type. Runs BEFORE instantiation because `instantiate_generic`
        // eagerly evaluates `keyof T` when T is concrete, destroying the
        // structural information needed for passthrough detection later.
        if let Some(passthrough) = self.try_homomorphic_mapped_passthrough(
            def_id,
            resolved,
            type_params,
            &expanded_args,
            no_unchecked_indexed_access,
        ) {
            return ApplicationEvalOutcome::ShortCircuit(passthrough);
        }

        // Class instance extraction: when a class is used in type position
        // via `Application` (e.g. `Component<P, S>`), the INSTANCE type
        // (the first construct signature's return type) is what we want,
        // not the class constructor type. Only applies for
        // `DefKind::Class`; interfaces with construct signatures keep
        // their Callable shape intact.
        //
        // Exception: when the base is a `TypeQuery` (`typeof ClassName<T>`),
        // the caller wants the constructor type — skipping extraction keeps
        // the specialized constructor so `InstanceType<typeof Cls<T>>` can
        // correctly reduce to the class instance type via conditional infer.
        let effective_body = if base_is_type_query {
            resolved
        } else {
            self.extract_class_instance_body(def_id, resolved)
        };

        // Homomorphic mapped-type union distribution: when the alias body
        // is `{ [K in keyof T]: ... }` and T's argument resolves to a
        // union, distribute over union members BEFORE calling
        // `instantiate_generic` so the mapped evaluator can distinguish
        // the post-instantiation constraint from the declared one.
        if let Some(distributed) = self.try_distribute_mapped_union_arg(
            def_id,
            effective_body,
            type_params,
            &expanded_args,
            no_unchecked_indexed_access,
        ) {
            return ApplicationEvalOutcome::ShortCircuit(distributed);
        }

        let evaluated = self.instantiate_and_finalize_application(
            def_id,
            original_type_id,
            args,
            &expanded_args,
            effective_body,
            type_params,
            prefer_application_display_alias,
            /* record_structural_back_reference */ true,
            no_unchecked_indexed_access,
        );
        ApplicationEvalOutcome::Computed(evaluated)
    }

    /// Lite-resolver fallback application path. Used when the resolver
    /// does not surface formal type parameters (`get_lazy_type_params`
    /// returned `None`) but the resolved body itself embeds
    /// `TypeParameter` types that can be recovered structurally.
    fn evaluate_application_with_extracted_params(
        &mut self,
        def_id: DefId,
        original_type_id: TypeId,
        args: &[TypeId],
        resolved: TypeId,
        type_params: &[TypeParamInfo],
        prefer_application_display_alias: bool,
    ) -> ApplicationEvalOutcome {
        let expanded_args = self.expand_type_args(args);
        let no_unchecked_indexed_access = self.no_unchecked_indexed_access;

        if let Some(db) = self.query_db
            && let Some(cached) = db.lookup_application_eval_cache(
                def_id,
                &expanded_args,
                no_unchecked_indexed_access,
            )
        {
            return ApplicationEvalOutcome::ShortCircuit(cached);
        }

        let evaluated = self.instantiate_and_finalize_application(
            def_id,
            original_type_id,
            args,
            &expanded_args,
            resolved,
            type_params,
            prefer_application_display_alias,
            /* record_structural_back_reference */ false,
            no_unchecked_indexed_access,
        );
        ApplicationEvalOutcome::Computed(evaluated)
    }

    /// Expand `Application(base, args)` arguments based on the alias body
    /// shape.
    ///
    /// * Conditional bodies preserve `TypeParameter` args (the conditional
    ///   evaluator needs them in generic form to match at the `infer`
    ///   site) but eagerly expand concrete args.
    /// * Bodies whose extends-side is `Application(...infer...)` preserve
    ///   `Application` args so the matcher can compare at the application
    ///   level (e.g. `Promise<string>` vs `Promise<infer U>`).
    /// * Everything else uses the default `expand_type_args` which
    ///   evaluates `TypeQuery`, `Application`, and meta-types.
    fn prepare_expanded_args_for_body<'b>(
        &mut self,
        body: TypeId,
        args: &'b [TypeId],
    ) -> std::borrow::Cow<'b, [TypeId]> {
        let arg_preservation =
            crate::type_queries::classify_body_for_arg_preservation(self.interner, body);
        let body_is_conditional =
            matches!(self.interner.lookup(body), Some(TypeData::Conditional(_)));
        if body_is_conditional {
            std::borrow::Cow::Owned(
                args.iter()
                    .map(|&arg| {
                        if crate::visitor::contains_type_parameters(self.interner, arg) {
                            arg
                        } else {
                            self.try_expand_type_arg(arg)
                        }
                    })
                    .collect(),
            )
        } else if matches!(
            arg_preservation,
            crate::type_queries::BodyArgPreservation::ConditionalInfer
                | crate::type_queries::BodyArgPreservation::ConditionalApplicationInfer
        ) {
            std::borrow::Cow::Owned(self.expand_type_args_preserve_applications(args))
        } else {
            self.expand_type_args(args)
        }
    }

    /// Homomorphic mapped-type passthrough.
    ///
    /// Returns `Some(value)` (with the cache populated) when the body is a
    /// `{ [P in keyof T]: ... }` mapped type and the argument for `T`
    /// matches one of two passthrough rules:
    /// * primitive (or array-constrained any/unknown/never) — return the
    ///   argument directly;
    /// * identity body `{ [P in keyof T]: T[P] }` over `any` — return
    ///   `{ [x: string]: any; [x: number]: any }` so the result is not
    ///   assignable to `any[]`.
    fn try_homomorphic_mapped_passthrough(
        &mut self,
        def_id: DefId,
        body: TypeId,
        type_params: &[TypeParamInfo],
        expanded_args: &[TypeId],
        no_unchecked_indexed_access: bool,
    ) -> Option<TypeId> {
        let preamble = self.homomorphic_mapped_arg(body, type_params, expanded_args)?;
        let HomomorphicMappedArg {
            mapped,
            source,
            tp,
            resolved_arg,
            ..
        } = preamble;

        // Passthrough for genuine primitives. For `any`/`unknown`/`never`/
        // `error`: only passthrough when the type parameter is constrained
        // to array/tuple types (e.g. `Arrayish<T extends unknown[]>`).
        // Otherwise these top/bottom types must flow through mapped type
        // expansion so `Objectish<any>` becomes
        // `{ [x: string]: any; [x: number]: any }` (matching tsc).
        let is_any_like = resolved_arg == TypeId::ANY
            || resolved_arg == TypeId::UNKNOWN
            || resolved_arg == TypeId::NEVER
            || resolved_arg == TypeId::ERROR;
        let should_passthrough = if is_any_like {
            tp.constraint.is_some_and(|c| {
                let eval_c = self.evaluate(c);
                matches!(
                    self.interner.lookup(eval_c),
                    Some(TypeData::Array(_) | TypeData::Tuple(_))
                )
            })
        } else {
            Self::is_primitive_or_primitive_union(self.interner, resolved_arg)
        };
        if should_passthrough {
            self.insert_application_eval_cache_if_some(
                def_id,
                expanded_args,
                no_unchecked_indexed_access,
                resolved_arg,
            );
            return Some(resolved_arg);
        }

        // Objectish<any>: identity homomorphic mapped type with `any`
        // argument and non-array constraint. tsc produces
        // `{ [x: string]: any; [x: number]: any }` (NOT `any`), keeping
        // the result not assignable to `any[]`. Previously handled in
        // checker-local object construction; centralized here for
        // architectural correctness.
        if resolved_arg == TypeId::ANY
            && let Some((obj, key)) = crate::index_access_parts(self.interner, mapped.template)
            && obj == source
            && matches!(
                self.interner.lookup(key),
                Some(TypeData::TypeParameter(kp)) if kp.name == mapped.type_param.name
            )
        {
            use crate::types::{IndexSignature, ObjectShape};
            let result = self.interner.object_with_index(ObjectShape {
                flags: crate::types::ObjectFlags::empty(),
                properties: vec![],
                string_index: Some(IndexSignature {
                    key_type: TypeId::STRING,
                    value_type: TypeId::ANY,
                    readonly: false,
                    param_name: None,
                }),
                number_index: Some(IndexSignature {
                    key_type: TypeId::NUMBER,
                    value_type: TypeId::ANY,
                    readonly: false,
                    param_name: None,
                }),
                symbol: None,
            });
            self.insert_application_eval_cache_if_some(
                def_id,
                expanded_args,
                no_unchecked_indexed_access,
                result,
            );
            return Some(result);
        }

        None
    }

    /// Shared opening preamble for the two body-aware homomorphic-mapped
    /// shortcuts. Returns the structured `(mapped, source, tp, idx,
    /// resolved_arg)` tuple when `body` is `{ [P in keyof Tᵢ]: ... }` and
    /// the argument for `Tᵢ` resolves cleanly. Returns `None` if any guard
    /// in the chain fails.
    ///
    /// Extracted from the two call sites so a future change to the
    /// guard cannot drift between passthrough and union-distribute.
    fn homomorphic_mapped_arg(
        &mut self,
        body: TypeId,
        type_params: &[TypeParamInfo],
        expanded_args: &[TypeId],
    ) -> Option<HomomorphicMappedArg> {
        let TypeData::Mapped(mapped_id) = self.interner.lookup(body)? else {
            return None;
        };
        let mapped = self.interner.get_mapped(mapped_id);
        let TypeData::KeyOf(source) = self.interner.lookup(mapped.constraint)? else {
            return None;
        };
        let TypeData::TypeParameter(tp) = self.interner.lookup(source)? else {
            return None;
        };
        let idx = type_params.iter().position(|p| p.name == tp.name)?;
        if idx >= expanded_args.len() {
            return None;
        }
        let arg = expanded_args[idx];
        let resolved_arg = self.evaluate(arg);
        Some(HomomorphicMappedArg {
            mapped,
            source,
            tp,
            idx,
            resolved_arg,
        })
    }

    /// Insert into the application-eval cache iff `query_db` is connected.
    /// Folds the two-line `if let Some(db) = self.query_db { … }` idiom
    /// repeated in every body-aware shortcut and finalize helper.
    fn insert_application_eval_cache_if_some(
        &self,
        def_id: DefId,
        expanded_args: &[TypeId],
        no_unchecked_indexed_access: bool,
        evaluated: TypeId,
    ) {
        if let Some(db) = self.query_db {
            db.insert_application_eval_cache(
                def_id,
                expanded_args,
                no_unchecked_indexed_access,
                evaluated,
            );
        }
    }

    /// Extract the instance side of a class-shaped resolved body.
    ///
    /// Returns the body unchanged for interfaces and aliases. For
    /// `DefKind::Class`, returns the first construct signature's return
    /// type (the INSTANCE type) so `Component<P, S>` in type position
    /// refers to the instance rather than `typeof Component`. Interfaces
    /// with construct signatures (e.g. `ComponentClass<P>`) keep their
    /// Callable shape — only classes are unwrapped.
    fn extract_class_instance_body(&self, def_id: DefId, resolved: TypeId) -> TypeId {
        let is_class_def = matches!(
            self.resolver.get_def_kind(def_id),
            Some(crate::def::DefKind::Class)
        );
        if !is_class_def {
            return resolved;
        }
        let Some(TypeData::Callable(cs_id)) = self.interner.lookup(resolved) else {
            return resolved;
        };
        let shape = self.interner.callable_shape(cs_id);
        match shape.construct_signatures.first() {
            Some(construct_sig) => construct_sig.return_type,
            None => resolved,
        }
    }

    /// Homomorphic mapped-type distribution over a union argument.
    ///
    /// Returns `Some(union)` (with cache populated) when the body is a
    /// homomorphic mapped type and the argument for `T` resolves to a
    /// non-array/non-tuple union. Distributes per member, calling
    /// `instantiate_generic` once per non-primitive member; primitive
    /// members pass through unchanged so `Partial<string | { x: number }>`
    /// becomes `string | { x?: number }` instead of `string | string`.
    fn try_distribute_mapped_union_arg(
        &mut self,
        def_id: DefId,
        effective_body: TypeId,
        type_params: &[TypeParamInfo],
        expanded_args: &[TypeId],
        no_unchecked_indexed_access: bool,
    ) -> Option<TypeId> {
        let HomomorphicMappedArg {
            idx, resolved_arg, ..
        } = self.homomorphic_mapped_arg(effective_body, type_params, expanded_args)?;
        let TypeData::Union(list_id) = self.interner.lookup(resolved_arg)? else {
            return None;
        };
        let members = self.interner.type_list(list_id).to_vec();
        let mut distributed = Vec::with_capacity(members.len());
        for member in members {
            if crate::visitors::visitor_predicates::is_primitive_type(self.interner, member) {
                distributed.push(member);
                continue;
            }
            let mut member_args = expanded_args.to_vec();
            member_args[idx] = member;
            let instantiated =
                instantiate_generic(self.interner, effective_body, type_params, &member_args);
            distributed.push(self.evaluate(instantiated));
        }
        let evaluated = self.interner.union(distributed);
        self.insert_application_eval_cache_if_some(
            def_id,
            expanded_args,
            no_unchecked_indexed_access,
            evaluated,
        );
        Some(evaluated)
    }

    /// Instantiate + evaluate the body for an application and record the
    /// appropriate display-alias provenance.
    ///
    /// Display-alias storage is gated on `prefer_application_display_alias`:
    /// type-alias applications whose evaluation produces an intermediate
    /// `Application` form store a forward display alias so diagnostics show
    /// the apparent name (e.g. `DeepReadonlyObject<Part>`).
    ///
    /// `record_structural_back_reference` is `true` only on the known-params
    /// path where the resolver surfaced a nominal interface/class signal
    /// strong enough to back-reference from the evaluated structural form to
    /// the original `Application`. The lite-resolver fallback path keeps
    /// this off because it cannot prove the nominal origin.
    #[allow(clippy::too_many_arguments)]
    fn instantiate_and_finalize_application(
        &mut self,
        def_id: DefId,
        original_type_id: TypeId,
        original_args: &[TypeId],
        expanded_args: &[TypeId],
        body: TypeId,
        type_params: &[TypeParamInfo],
        prefer_application_display_alias: bool,
        record_structural_back_reference: bool,
        no_unchecked_indexed_access: bool,
    ) -> TypeId {
        let mut instantiated = instantiate_generic(self.interner, body, type_params, expanded_args);
        // Rebind polymorphic `this` to the concrete application so
        // interface bodies like `constraint: Constraint<this>` preserve
        // their receiver-specific invariance.
        if crate::contains_this_type(self.interner, instantiated) {
            instantiated = crate::instantiation::instantiate::substitute_this_type_cached(
                self.interner,
                self.query_db,
                instantiated,
                original_type_id,
            );
        }
        // Preserve discriminated object intersections after instantiation.
        // Re-evaluating them here distributes impossible branches again,
        // which breaks both fresh EPC and `keyof` on generic applications.
        let evaluated = if crate::type_queries::is_discriminated_object_intersection(
            self.interner,
            instantiated,
        ) {
            instantiated
        } else {
            self.evaluate(instantiated)
        };
        if prefer_application_display_alias {
            self.store_intermediate_application_display_alias(
                instantiated,
                original_type_id,
                evaluated,
                original_args,
            );
        } else if record_structural_back_reference {
            self.store_parametric_structural_back_reference(evaluated, original_type_id);
        }
        self.insert_application_eval_cache_if_some(
            def_id,
            expanded_args,
            no_unchecked_indexed_access,
            evaluated,
        );
        evaluated
    }

    /// Record display-alias provenance after a successful application
    /// evaluation.
    ///
    /// Decides whether to repaint the alias name onto the evaluated
    /// structural form. Skipping the repaint protects unrelated diagnostics
    /// from being relabeled when:
    /// * the result is a non-empty structural shape that already existed
    ///   before this application,
    /// * the result is itself one of the application arguments,
    /// * a conditional branch alias is already pinned on `result`.
    ///
    /// When `my_apparent_branch` is set by the conditional evaluator and is
    /// distinct from the original application, also installs a one-step
    /// forward alias so the formatter shows the apparent intermediate name
    /// (e.g. `DeepReadonlyObject<Part>` instead of `DeepReadonly<Part>`).
    fn record_application_evaluation_display_aliases(
        &mut self,
        result: TypeId,
        original_type_id: TypeId,
        original_args: &[TypeId],
        is_type_alias_def: bool,
        prefer_application_display_alias: bool,
        my_apparent_branch: Option<TypeId>,
    ) {
        let display_origin = if self.expand_application_display_alias_args
            && let Some(TypeData::Application(original_app_id)) =
                self.interner.lookup(original_type_id)
        {
            let original_app = self.interner.type_application(original_app_id);
            let expanded_args = self.expand_type_args(&original_app.args);
            if expanded_args.as_ref() != original_app.args.as_slice() {
                let candidate = self
                    .interner
                    .application(original_app.base, expanded_args.into_owned());
                if crate::visitor::contains_type_by_id(self.interner, candidate, result) {
                    original_type_id
                } else {
                    candidate
                }
            } else {
                original_type_id
            }
        } else {
            original_type_id
        };
        let has_param_args = original_args.iter().any(|&arg| {
            crate::type_queries::contains_generic_type_parameters_db(self.interner, arg)
        });
        // For concrete args the alias repaint is unconditional; for
        // generic args only Conditional/IndexAccess/Mapped results get
        // repainted (deferred mapped aliases retain the as-written
        // relationship needed for diagnostics like `Mapped<K>[Remapped<K>]`).
        if has_param_args
            && !matches!(
                self.interner.lookup(result),
                Some(
                    crate::types::TypeData::Conditional(_)
                        | crate::types::TypeData::IndexAccess(_, _)
                        | crate::types::TypeData::Mapped(_)
                )
            )
        {
            return;
        }

        let result_is_non_empty_structural = match self.interner.lookup(result) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                !shape.properties.is_empty()
                    || shape.string_index.is_some()
                    || shape.number_index.is_some()
            }
            Some(TypeData::Intersection(_)) => true,
            _ => false,
        };
        let result_is_application_arg = original_args.contains(&result);
        let skip_type_alias_repaint = matches!(
            self.interner.lookup(display_origin),
            Some(TypeData::Application(_))
        ) && result_is_non_empty_structural
            && (result_is_application_arg
                || (is_type_alias_def
                    && match (
                        self.interner.lookup_alloc_order(result),
                        self.interner.lookup_alloc_order(display_origin),
                    ) {
                        (Some(result_order), Some(display_order)) => result_order <= display_order,
                        _ => result.0 <= display_origin.0,
                    }));
        let keep_existing_conditional_branch_alias = is_type_alias_def
            && !prefer_application_display_alias
            && matches!(
                self.interner.lookup(display_origin),
                Some(TypeData::Application(_))
            )
            && display_provenance::display_alias(self.interner, result).is_some();
        if !skip_type_alias_repaint && !keep_existing_conditional_branch_alias {
            let priority = if prefer_application_display_alias
                || (self.expand_application_display_alias_args
                    && matches!(
                        self.interner.lookup(display_origin),
                        Some(TypeData::Application(_))
                    )) {
                AliasApplicationPriority::PreferApplication
            } else {
                AliasApplicationPriority::PreserveExisting
            };
            display_provenance::record_alias_application(
                self.interner,
                AliasApplicationProvenance {
                    evaluated: result,
                    application: display_origin,
                },
                priority,
            );
        }

        // If the conditional branch resolved to an intermediate
        // Application (e.g. `DeepReadonly<Part>` -> conditional ->
        // `DeepReadonlyObject<Part>`), store a forward display alias so
        // the formatter shows the one-step apparent type name that tsc
        // displays.
        if let Some(branch_app) = my_apparent_branch
            && branch_app != original_type_id
            && branch_app != result
            && !has_param_args
            && matches!(
                self.interner.lookup(branch_app),
                Some(crate::types::TypeData::Application(_))
            )
        {
            display_provenance::record_alias_application(
                self.interner,
                AliasApplicationProvenance {
                    evaluated: original_type_id,
                    application: branch_app,
                },
                AliasApplicationPriority::PreserveExisting,
            );
        }
    }

    fn store_intermediate_application_display_alias(
        &self,
        instantiated: TypeId,
        original_type_id: TypeId,
        evaluated: TypeId,
        original_args: &[TypeId],
    ) {
        if instantiated == original_type_id || evaluated == TypeId::ERROR {
            return;
        }
        // Only install this forward alias when the intermediate application
        // appears to have been introduced after the outer application.
        // If the instantiated application predates the outer one, it can be a
        // user-authored type occurrence and globally aliasing it risks repainting
        // unrelated diagnostics.
        let instantiated_is_new_intermediate = match (
            self.interner.lookup_alloc_order(instantiated),
            self.interner.lookup_alloc_order(original_type_id),
        ) {
            (Some(instantiated_order), Some(original_order)) => instantiated_order > original_order,
            _ => instantiated.0 > original_type_id.0,
        };
        if !instantiated_is_new_intermediate {
            return;
        }
        let instantiated_is_application = matches!(
            self.interner.lookup(instantiated),
            Some(TypeData::Application(_))
        );
        let original_is_application = matches!(
            self.interner.lookup(original_type_id),
            Some(TypeData::Application(_))
        );

        if !original_is_application {
            return;
        }

        if !instantiated_is_application {
            // Structural-body path: the type alias body resolved to a structural
            // type rather than another Application (e.g.
            // `type LinkedList<T> = T & { next: LinkedList<T> }` evaluates to an
            // Intersection). Map `evaluated → original_type_id` so diagnostics show
            // the alias name instead of the expanded structural form.
            //
            // `evaluated_is_mapped` is checked first: Mapped is a subset of structural,
            // so true short-circuits the more expensive `is_structural_display_alias_result`
            // call and avoids a duplicate `lookup(evaluated)`.
            let evaluated_is_mapped =
                matches!(self.interner.lookup(evaluated), Some(TypeData::Mapped(_)));
            if evaluated_is_mapped
                || Self::is_structural_display_alias_result(self.interner, evaluated)
            {
                // Only store the display alias when `evaluated` was freshly produced
                // by this evaluation (allocated after `original_type_id`). If it
                // pre-exists, it was already interned by a different alias and
                // overwriting its alias would corrupt diagnostics for that other alias.
                // For example, `NestedRecord<"x.y.z", string>` and `Id<...string...>`
                // can evaluate to the same structural object; the NestedRecord evaluation
                // must not replace the `Id<...>` alias that was recorded first.
                let evaluated_is_fresh = match (
                    self.interner.lookup_alloc_order(evaluated),
                    self.interner.lookup_alloc_order(original_type_id),
                ) {
                    (Some(eval_order), Some(orig_order)) => eval_order > orig_order,
                    _ => evaluated.0 > original_type_id.0,
                };
                // Safe to store in two cases:
                // 1. Recursive aliases: the recursive self-reference ensures the structural
                //    type is unique to this instantiation, so aliasing is unambiguous.
                // 2. Generic aliases whose body evaluates to a fresh Mapped type: each
                //    distinct set of type-argument TypeIds produces a distinct MappedType
                //    node (the constraint is baked into the interned key). Storing the
                //    alias lets diagnostics show e.g. `Mapped2<K>` instead of the
                //    expanded `{ [P in K as \`get${P}\`]: ... }` form, matching tsc.
                if evaluated_is_fresh
                    && (evaluated_is_mapped
                        || self.is_recursive_type_alias_application(original_type_id))
                {
                    self.interner
                        .store_display_alias_preferring_application(evaluated, original_type_id);
                }
            }
            return;
        }

        // Application→Application chain: when the outer application's args contain
        // generic type parameters, skip storing the alias. Intermediate Applications
        // in a type-alias chain (e.g. `Outer<T>` instantiated to `Inner<T>`) must not
        // displace the outer Application as the canonical display alias.
        if original_args.iter().any(|&arg| {
            crate::type_queries::contains_generic_type_parameters_db(self.interner, arg)
        }) {
            return;
        }

        if !Self::is_structural_display_alias_result(self.interner, evaluated) {
            return;
        }

        display_provenance::record_alias_application(
            self.interner,
            AliasApplicationProvenance {
                evaluated: instantiated,
                application: original_type_id,
            },
            AliasApplicationPriority::PreferApplication,
        );
    }

    fn is_recursive_type_alias_application(&self, type_id: TypeId) -> bool {
        let Some(TypeData::Application(app_id)) = self.interner.lookup(type_id) else {
            return false;
        };
        let app = self.interner.type_application(app_id);
        let Some(TypeData::Lazy(def_id)) = self.interner.lookup(app.base) else {
            return false;
        };
        if self.resolver.get_def_kind(def_id) != Some(DefKind::TypeAlias) {
            return false;
        }
        let Some(body) = self.resolver.resolve_lazy(def_id, self.interner) else {
            return false;
        };
        let mut visited = FxHashSet::default();
        self.type_reaches_alias_def(body, def_id, &mut visited)
    }

    fn type_reaches_alias_def(
        &self,
        type_id: TypeId,
        target_def_id: DefId,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if type_id.is_intrinsic() || !visited.insert(type_id) {
            return false;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Lazy(def_id))
                if self.resolver.defs_are_equivalent(def_id, target_def_id) =>
            {
                return true;
            }
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(app.base)
                    && self.resolver.defs_are_equivalent(def_id, target_def_id)
                {
                    return true;
                }
            }
            _ => {}
        }

        let mut found = false;
        crate::visitor::for_each_child_by_id(self.interner, type_id, |child| {
            if !found {
                found = self.type_reaches_alias_def(child, target_def_id, visited);
            }
        });
        found
    }

    /// Record a back-reference from an evaluated structural form to its
    /// originating parametric Application — the interface/class counterpart
    /// to `store_intermediate_application_display_alias` (which only stores
    /// for type-alias bodies that are themselves Applications).
    ///
    /// Read by `reduce_alias_body_to_application_form` to recover the
    /// Application form when the source has been eagerly evaluated to its
    /// structural shape (e.g. `Promise<{id}>` substituted into a structural
    /// Object). The downstream `store_display_alias_preferring_application`
    /// applies its own safety gates (alloc-order, intrinsic-skip, generic-
    /// args) that prevent overriding aliases for pre-existing types.
    fn store_parametric_structural_back_reference(
        &mut self,
        evaluated: TypeId,
        original_type_id: TypeId,
    ) {
        if evaluated == original_type_id || evaluated == TypeId::ERROR {
            return;
        }
        let Some(TypeData::Application(app_id)) = self.interner.lookup(original_type_id) else {
            return;
        };
        let app = self.interner.type_application(app_id);
        if app.args.is_empty() {
            return;
        }
        let app_def = match self.interner.lookup(app.base) {
            Some(TypeData::Lazy(def_id)) => self
                .resolver
                .get_def_kind(def_id)
                .map(|kind| (def_id, kind)),
            Some(TypeData::TypeQuery(sym_ref)) => {
                self.resolver.symbol_to_def_id(sym_ref).and_then(|def_id| {
                    self.resolver
                        .get_def_kind(def_id)
                        .map(|kind| (def_id, kind))
                })
            }
            _ => None,
        };
        let Some((_, app_kind)) = app_def else {
            return;
        };
        // This back-reference is for nominal parametric shapes. Type-alias
        // applications still need their evaluated structural form for displays
        // such as TS2339 on conditional helper aliases. If the resolver cannot
        // prove a nominal interface/class origin, do not repaint a structural
        // result as an arbitrary application.
        if !matches!(
            app_kind,
            crate::def::DefKind::Interface | crate::def::DefKind::Class
        ) {
            return;
        }
        if app.args.contains(&evaluated) {
            return;
        }
        // Fast path: all-intrinsic args trivially have no free type
        // parameters; skip the recursive `contains_generic_type_parameters_db`
        // traversal that fires on every parametric application evaluation.
        let all_intrinsic = app.args.iter().all(|a| a.is_intrinsic());
        if !all_intrinsic
            && app.args.iter().any(|&arg| {
                crate::type_queries::contains_generic_type_parameters_db(self.interner, arg)
            })
        {
            return;
        }
        if !Self::is_structural_display_alias_result(self.interner, evaluated) {
            return;
        }
        display_provenance::record_alias_application(
            self.interner,
            AliasApplicationProvenance {
                evaluated,
                application: original_type_id,
            },
            AliasApplicationPriority::PreferApplication,
        );
    }

    fn is_structural_display_alias_result(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
        matches!(
            interner.lookup(type_id),
            Some(
                TypeData::Object(_)
                    | TypeData::ObjectWithIndex(_)
                    | TypeData::Array(_)
                    | TypeData::Tuple(_)
                    | TypeData::Function(_)
                    | TypeData::Callable(_)
                    | TypeData::Intersection(_)
                    | TypeData::Mapped(_)
            )
        )
    }

    // Additional evaluator support methods live in the nested support module.
}

/// Convenience function for evaluating conditional types
pub fn evaluate_conditional(interner: &dyn TypeDatabase, cond: &ConditionalType) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_conditional(cond)
}

/// Convenience function for evaluating index access types
pub fn evaluate_index_access(
    interner: &dyn TypeDatabase,
    object_type: TypeId,
    index_type: TypeId,
) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_index_access(object_type, index_type)
}

/// Convenience function for evaluating index access types with options.
pub fn evaluate_index_access_with_options(
    interner: &dyn TypeDatabase,
    object_type: TypeId,
    index_type: TypeId,
    no_unchecked_indexed_access: bool,
) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
    evaluator.evaluate_index_access(object_type, index_type)
}

/// Convenience function for full type evaluation
pub fn evaluate_type(interner: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    evaluate_type_with_request(interner, EvaluationRequest::new(type_id))
}

/// Convenience function for full type evaluation with explicit request options.
pub fn evaluate_type_with_request(
    interner: &dyn TypeDatabase,
    request: EvaluationRequest,
) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_request_result(request).into_type_id()
}

/// Convenience function for evaluating mapped types
pub fn evaluate_mapped(interner: &dyn TypeDatabase, mapped: &MappedType) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_mapped(mapped)
}

/// Convenience function for evaluating keyof types
pub fn evaluate_keyof(interner: &dyn TypeDatabase, operand: TypeId) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_keyof(operand)
}

// Re-enabled evaluate tests - verifying API compatibility
#[cfg(test)]
#[path = "../../tests/evaluate_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/evaluate_application_orchestrator_tests.rs"]
mod orchestrator_tests;
