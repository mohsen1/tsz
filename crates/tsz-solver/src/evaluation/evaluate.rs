//! Type evaluation for meta-types (conditional, mapped, index access) and
//! generic type applications.
//!
//! Meta-types are "type-level functions" that compute output types from input types.
//! This module provides evaluation logic for:
//! - Conditional types: T extends U ? X : Y
//! - Distributive conditional types: (A | B) extends U ? X : Y
//! - Index access types: T[K]
//! - Generic type applications: `Base<Args>` (see the `application` submodule)
//!
//! Key design:
//! - Lazy evaluation: only evaluate when needed for subtype checking
//! - Handles deferred evaluation when type parameters are unknown
//! - Supports distributivity for naked type parameters in unions

mod api;
pub(in crate::evaluation) mod application_types;
mod array_methods;

use crate::caches::db::QueryDatabase;
use crate::construction::TypeDatabase;
use crate::def::{DefId, DefKind};
use crate::diagnostics::display_provenance::{
    self, AliasApplicationPriority, AliasApplicationProvenance,
    FreshObjectLiteralDisplayProvenance, UnionOriginProvenance,
};
use crate::evaluation::request::EvaluationRequest;
use crate::evaluation::result::EvaluationResult;
#[cfg(test)]
#[allow(unused_imports)]
use crate::instantiation::instantiate::instantiate_generic;
use crate::relations::subtype::{NoopResolver, TypeResolver};
#[cfg(test)]
use crate::types::*;
use crate::types::{
    ConditionalTypeId, MappedTypeId, StringIntrinsicKind, TemplateLiteralId, TemplateSpan,
    TupleElement, TupleListId, TypeApplicationId, TypeData, TypeId, TypeListId, TypeParamInfo,
};
use crate::visitors::visitor_predicates::contains_type_matching;
pub use api::{
    evaluate_conditional, evaluate_index_access, evaluate_index_access_with_options,
    evaluate_keyof, evaluate_mapped, evaluate_type, evaluate_type_with_request,
    evaluate_type_with_resolver,
};
use application_types::{ApplicationEvalContext, ApplicationEvalOutcome, HomomorphicMappedArg};
pub(crate) use array_methods::{
    ARRAY_METHODS_RETURN_ANY, ARRAY_METHODS_RETURN_BOOLEAN, ARRAY_METHODS_RETURN_NUMBER,
    ARRAY_METHODS_RETURN_STRING, ARRAY_METHODS_RETURN_VOID,
};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;

mod application;
mod closed_eval;
mod display_alias;
mod query_budget;
mod support;

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
    /// Sticky flag: set when this run hit any recursion limit (a `DefId` reached
    /// `MAX_DEF_DEPTH`, a structural cycle/depth/iteration bail). Such depth-
    /// bounded runs must not be persisted in `closed_eval_cache`, or a later read
    /// could short-circuit the expansion that re-derives `TS2589`. See the
    /// `closed_eval` module.
    deep_recursion_seen: bool,
    /// Monotonic counter of *limit events* (cycle / depth / iteration / divergence
    /// bails) seen so far in this run. Unlike the sticky `deep_recursion_seen` /
    /// `silent_depth_bailed` booleans — which, once set by the first bail anywhere,
    /// stay set for the rest of the run and would disable every later cache write —
    /// this epoch lets a write be gated on whether a NEW limit event fired during
    /// the *specific application body* being finalized (see `app_body_limit_epoch`
    /// and `application_eval_result_cacheable`). Bumped at every limit-event site.
    limit_epoch: u32,
    /// Snapshot of `limit_epoch` taken when the application currently being
    /// evaluated entered its body. Saved/restored around each nested
    /// `evaluate_application` call so that, at an `application_eval_cache` write
    /// site, it always reflects the innermost in-flight application. When it still
    /// equals `limit_epoch`, that application's whole body subtree evaluated
    /// without truncation, so its result is a complete, stack-independent function
    /// of `(DefId, args)` and is safe to persist even if an *earlier, unrelated*
    /// sibling already bailed.
    app_body_limit_epoch: u32,
    /// Whether this evaluator may *write* the `closed_eval_cache`. Only the
    /// checker's authoritative, context-free type-resolution pass opts in (via
    /// `with_closed_eval_writes`). Evaluators running mid-relation, mid-inference
    /// (`infer` binding), mid-narrowing, or contextual typing must NOT write —
    /// their results can depend on inference/narrowing/contextual state the
    /// `(TypeId, no_unchecked)` key does not capture. All evaluators may still
    /// *read* (the stored value is a definite context-free answer).
    closed_eval_writes_allowed: bool,
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

impl<'a> TypeEvaluator<'a, NoopResolver> {
    /// Create a new evaluator without a resolver.
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        static NOOP: NoopResolver = NoopResolver;
        Self::with_resolver_and_defaults(interner, &NOOP)
    }
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    fn with_resolver_and_defaults(interner: &'a dyn TypeDatabase, resolver: &'a R) -> Self {
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
            deep_recursion_seen: false,
            limit_epoch: 0,
            app_body_limit_epoch: 0,
            closed_eval_writes_allowed: false,
        }
    }

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
            // Depth-bounded run (see `deep_recursion_seen`).
            self.mark_deep_recursion_seen();
            return false;
        }

        let was_real_instantiation_depth = *depth >= Self::REAL_INSTANTIATION_BAILOUT_THRESHOLD;
        *depth += 1;
        if !was_real_instantiation_depth && *depth >= Self::REAL_INSTANTIATION_BAILOUT_THRESHOLD {
            self.real_instantiation_depth_count += 1;
            self.mark_deep_recursion_seen();
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
        Self::with_resolver_and_defaults(interner, resolver)
    }

    /// Set the query database for Salsa-backed memoization.
    pub fn with_query_db(mut self, db: &'a dyn QueryDatabase) -> Self {
        self.query_db = Some(db);
        self
    }

    /// Opt this evaluator in to *writing* the substitution-independent
    /// `closed_eval_cache`. Only the checker's authoritative, context-free
    /// type-resolution pass should call this — see `closed_eval_writes_allowed`.
    pub const fn with_closed_eval_writes(mut self) -> Self {
        self.closed_eval_writes_allowed = true;
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

    /// Whether this run hit any recursion / depth / iteration limit.
    ///
    /// Single source of truth for the three flags that mark a result as a
    /// *stack-context artifact* rather than a stable, key-determined answer:
    ///
    /// - `guard.is_exceeded()` — a genuine `MAX_DEF_DEPTH` / divergent-growth /
    ///   mapped-key bail; while set, `evaluate` returns `ERROR` for every node.
    /// - `silent_depth_bailed` — the structural stack-protection bail that
    ///   `clear_exceeded`s the guard and leaves the type opaque (unexpanded).
    /// - `deep_recursion_seen` — a cycle/iteration bail returned an opaque
    ///   cycle-breaker, or a `DefId` crossed the real-instantiation threshold.
    ///
    /// A run in any of these states must not persist results to caches whose
    /// key does not capture the ambient stack depth (`closed_eval_cache`,
    /// `application_eval_cache`); see the respective limit gates. `pub(crate)`
    /// so the per-query cross-evaluator memo can tell a stable result (safe to
    /// memoize) from a stack-context artifact that must be recomputed (#11586).
    #[inline]
    pub(crate) const fn recursion_limit_hit(&self) -> bool {
        self.guard.is_exceeded() || self.silent_depth_bailed || self.deep_recursion_seen
    }

    /// Mark the guard as exceeded, causing subsequent evaluations to bail out.
    ///
    /// Used when an external condition (e.g. mapped key count or distribution
    /// size exceeds its limit) means further recursive evaluation should stop.
    #[inline]
    pub(crate) const fn mark_depth_exceeded(&mut self) {
        self.guard.mark_exceeded();
        self.note_limit_event();
    }

    /// Record that a recursion/depth/iteration/divergence limit just fired.
    ///
    /// Bumping the monotonic `limit_epoch` is how a later
    /// `application_eval_cache` write learns that *its* body subtree was
    /// truncated. Over-bumping (calling this where no application body is in
    /// flight, or twice for one logical event) only ever makes a write more
    /// conservative — never unsound — so every limit-event site routes through
    /// here.
    #[inline]
    const fn note_limit_event(&mut self) {
        self.limit_epoch = self.limit_epoch.wrapping_add(1);
    }

    /// Set the sticky `deep_recursion_seen` flag and record the limit event.
    #[inline]
    const fn mark_deep_recursion_seen(&mut self) {
        self.deep_recursion_seen = true;
        self.note_limit_event();
    }

    /// Set the sticky `silent_depth_bailed` flag and record the limit event.
    #[inline]
    const fn mark_silent_depth_bailed(&mut self) {
        self.silent_depth_bailed = true;
        self.note_limit_event();
    }

    /// Test hook: simulate an *earlier, unrelated* recursive alias having bailed
    /// (a cycle / silent-depth / iteration bail that latches the sticky
    /// recursion-limit state without poisoning the guard). Lets a test exercise
    /// the [`application_eval_result_cacheable`](Self::application_eval_result_cacheable)
    /// boundary without first constructing a real divergent type whose own bail
    /// path (`guard.mark_exceeded`) would short-circuit every later `evaluate`.
    #[cfg(test)]
    pub(crate) const fn simulate_unrelated_recursion_bail_for_test(&mut self) {
        self.mark_deep_recursion_seen();
    }

    /// Global thread-local depth counter for cross-evaluator stack overflow
    /// prevention. Each `SubtypeChecker::evaluate_type` creates a fresh
    /// `TypeEvaluator`, but the OS stack accumulates across ALL of them: deep
    /// structural comparisons (e.g. `Vector<T> implements Seq<T>` with `Exclude`
    /// in an overload return) produce 100+ nested evaluate frames that overflow
    /// the 8MB default stack. This counter tracks cumulative `evaluate` frames
    /// across every `TypeEvaluator` on the call stack and bails with ERROR once
    /// it exceeds `MAX_GLOBAL_EVAL_DEPTH`.
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

        // Substitution-independent persistent cache. See `closed_eval` module.
        if let Some(cached) = self.try_closed_eval_read(type_id) {
            self.cache.insert(type_id, cached);
            return cached;
        }

        // Check if depth was already exceeded in a previous call
        if self.guard.is_exceeded() {
            return TypeId::ERROR;
        }
        // Cross-instance per-query operation budget (see `query_budget`).
        let Some(_query_frame) = self.enter_eval_query_budget() else {
            return type_id;
        };

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
                self.mark_silent_depth_bailed();
                return type_id;
            }
            let result = self.evaluate_guarded(type_id);
            GLOBAL_EVAL_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
            return result;
        }

        // Top-level frame: evaluate, then commit closed-eval cache writes.
        // See the `closed_eval` module for the safety gates.
        let union_too_complex_before = self.interner.is_union_too_complex();
        let result = self.evaluate_guarded(type_id);
        self.commit_closed_eval_writes(union_too_complex_before);
        result
    }

    /// Inner evaluate logic, called after global depth check.
    ///
    /// Wrapped with `stacker::maybe_grow()` so that deeply nested conditional/
    /// mapped type chains (ts-toolbelt, ts-essentials) can grow the stack
    /// dynamically instead of crashing even if the logical recursion guard
    /// has not yet tripped.
    ///
    /// The shared cross-operation [`crate::recursion::with_solver_frame`] breaker
    /// additionally bounds the combined
    /// `evaluate -> subtype -> instantiate -> evaluate` cycle whose interleaved
    /// frames slip past every per-instance guard (issue #7574). When the budget
    /// is exhausted we leave `type_id` opaque — the same graceful, non-`ERROR`
    /// bail the cross-evaluator `MAX_GLOBAL_EVAL_DEPTH` guard uses — so an outer
    /// evaluator can still make progress at a shallower depth.
    fn evaluate_guarded(&mut self, type_id: TypeId) -> TypeId {
        crate::recursion::with_solver_frame(|| self.evaluate_guarded_inner(type_id)).unwrap_or_else(
            || {
                self.mark_silent_depth_bailed();
                type_id
            },
        )
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
                // Recursion-bounded run: do not persist its intermediates (see
                // `deep_recursion_seen`).
                self.mark_deep_recursion_seen();
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
                    self.note_limit_event();
                    return TypeId::ERROR;
                }
                return type_id;
            }
            RecursionResult::DepthExceeded => {
                // Depth-bounded run (see `deep_recursion_seen`).
                self.mark_deep_recursion_seen();
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
                self.mark_silent_depth_bailed();
                self.cache.insert(type_id, type_id);
                return type_id;
            }
            RecursionResult::IterationExceeded => {
                // Iteration-limit bail: also a bounded run.
                self.mark_deep_recursion_seen();
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
            self.mark_depth_exceeded();
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

    // Additional evaluator support methods live in the nested support module.
}

// Re-enabled evaluate tests - verifying API compatibility
#[cfg(test)]
#[path = "../../tests/evaluate_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/evaluate_application_orchestrator_tests.rs"]
mod orchestrator_tests;
