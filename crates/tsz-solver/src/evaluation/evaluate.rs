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
use crate::construction::UnionComplexityCheckpoint;
use crate::def::{DefId, DefKind};
use crate::diagnostics::display_provenance::{
    self, AliasApplicationPriority, AliasApplicationProvenance,
    FreshObjectLiteralDisplayProvenance, UnionOriginProvenance,
};
use crate::evaluation::cache_stability::EvaluationCacheLimitSnapshot;
use crate::evaluation::request::EvaluationRequest;
use crate::evaluation::result::EvaluationMemoResult;
use crate::evaluation::result::EvaluationRequestStability;
use crate::evaluation::result::EvaluationResult;
use crate::evaluation::result::TerminationKind;
use crate::evaluation::session::{CompoundSubtypePairKey, EvaluationSession};
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
    evaluate_keyof, evaluate_mapped, evaluate_type, evaluate_type_result_with_request,
    evaluate_type_result_with_resolver, evaluate_type_with_request, evaluate_type_with_resolver,
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
mod compound_simplification;
mod display_alias;
mod meta_recursion_identity;
use meta_recursion_identity::MetaRecursionIdentity;
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
    /// Optional owning session for cross-evaluator depth and memo state.
    eval_session: Option<&'a EvaluationSession>,
    resolver: &'a R,
    no_unchecked_indexed_access: bool,
    exact_optional_property_types: bool,
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
    /// Operation-level recursion identities for eager `keyof` / indexed-access
    /// re-reduction. Unlike `guard`, which keys exact `TypeId` re-entry, this
    /// mirrors tsc's `getRecursionIdentity`: productive recursion can keep
    /// allocating fresh `TypeId`s while repeating the same origin.
    meta_recursion_identity_stack: Vec<MetaRecursionIdentity>,
    /// #14101 SCC-discriminating probe: ordered stack of in-flight `DefId`
    /// application entries (pushed on a successful `increment_def_depth`, popped
    /// in `decrement_def_depth`). Lets the re-entry probe count the distinct
    /// OTHER `DefId`s between two entries of the same `DefId` — 0 = single-def
    /// self-recursion, >=1 = a multi-member A->B->A SCC. Observe-only.
    def_eval_stack: Vec<DefId>,
    /// Number of currently active `DefId` expansions at or above the threshold
    /// that turns a structural recursion bailout into a real TS2589 failure.
    real_instantiation_depth_count: u32,
    /// When true, suppress `this` type substitution during Lazy type evaluation.
    /// Used during intersection evaluation to prevent premature `this` binding to
    /// individual members instead of the full intersection type.
    suppress_this_binding: bool,
    /// PERF: Cache for subtype check results used in conditional type evaluation.
    /// Key: (`check_type`, `extends_type`, `noUncheckedIndexedAccess`,
    /// `exactOptionalPropertyTypes`), Value: `is_subtype`.
    /// Deeply recursive conditional types (`DeepReadonly`, `Compute`, etc.) often check
    /// the same (check, extends) pair many times across distributed branches and
    /// tail-recursion iterations. Caching avoids redundant structural comparison.
    conditional_subtype_cache: FxHashMap<ConditionalSubtypeCacheKey, bool>,
    /// PERF: Cache whether a type contains `infer`.
    /// Recursive conditionals can revisit the same application-shaped `extends`
    /// pattern thousands of times while checking whether the application-level
    /// infer fast path applies.
    contains_infer_cache: FxHashMap<TypeId, bool>,
    /// PERF: Cache definitive subtype-pair results used only by compound
    /// union/intersection simplification. Entries are operation-local to this
    /// evaluator and keyed by the full relation configuration plus the
    /// simplifier-specific bypass/depth mode.
    compound_subtype_cache: FxHashMap<CompoundSubtypePairKey, bool>,
    /// PERF: Cache tsc's permissive-instantiation false-branch probe for
    /// conditional types. The helper substitutes every reachable type parameter
    /// with `any`, evaluates both sides, then asks the conditional relation
    /// gateway whether the false branch is definitive. That whole operation can
    /// be requested several times while one conditional reduction is deciding
    /// whether to defer. Entries are evaluator-local and published only when the
    /// probe did not move the evaluator's limit/unresolved/request state.
    permissive_false_branch_cache: FxHashMap<PermissiveFalseBranchKey, bool>,
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
    /// Which guard (if any) first cut a walk short during the current
    /// top-level request, or `None` if it ran to completion. Unlike the shared
    /// `deep_recursion_seen` bool (which a cycle / depth / iteration bail all
    /// set, so it cannot name the kind), this records the specific bail class
    /// so `evaluate_request_result` can surface `Termination::Incomplete{ kind }`
    /// (#14346 stages 2–3). Written via [`Self::note_request_termination`]
    /// (which owns the first-wins semantics and the guard set); cleared at
    /// every `evaluate_request_result` entry so the verdict is scoped to one
    /// request and never leaks across reused-evaluator requests.
    request_termination_kind: Option<TerminationKind>,
    /// Request-local counterpart of [`Self::unresolved_def_seen`]. The sticky
    /// flag blocks run-wide closed-eval writes, while this flag lets memo-result
    /// verdicts name whether the specific request observed an unresolved body.
    /// Cleared with `request_termination_kind` on every `evaluate_request_result`
    /// entry.
    request_unresolved_def_seen: bool,
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
    /// Sticky flag: set when an application's base `DefId` could not be
    /// resolved to a body by this run's resolver (no body registered yet, an
    /// `UNKNOWN` placeholder, or a first-expansion self-lazy wrapper). The
    /// result of such a run is a function of the *registration window* it ran
    /// in, not of `(DefId, args)` alone: once the declaring file registers the
    /// real body, a fresh evaluation produces a different (correct) answer.
    /// Persisting the window artifact in the project-wide `closed_eval_cache`
    /// would permanently shadow that answer, so the commit gate checks this
    /// flag.
    unresolved_def_seen: bool,
    /// Monotonic counter bumped whenever `unresolved_def_seen` is set. The
    /// counter gives `application_eval_cache` the same per-body precision as
    /// `limit_epoch`: a later unrelated application body may still be cacheable,
    /// while the body that observed a registration-window artifact is not.
    unresolved_def_epoch: u32,
    /// Snapshot of `unresolved_def_epoch` for the innermost in-flight
    /// application body. Saved/restored beside `app_body_limit_epoch`.
    app_body_unresolved_def_epoch: u32,
    /// Last observed [`TypeResolver::provisional_value_epoch`]. When the
    /// resolver serves a value derived from a mid-resolution class partial
    /// (issue #16055), the epoch moves; `observe_provisional_epoch` folds the
    /// movement into `mark_unresolved_def_seen` so every `TypeId`-keyed cache
    /// write computed across the movement is skipped and a later evaluation
    /// recomputes against the completed body.
    provisional_epoch_seen: u64,
    /// Whether this evaluator may *write* the `closed_eval_cache`. Only the
    /// checker's authoritative, context-free type-resolution pass opts in (via
    /// `with_closed_eval_writes`). Evaluators running mid-relation, mid-inference
    /// (`infer` binding), mid-narrowing, or contextual typing must NOT write —
    /// their results can depend on inference/narrowing/contextual state the
    /// `(TypeId, no_unchecked)` key does not capture. All evaluators may still
    /// *read* (the stored value is a definite context-free answer).
    closed_eval_writes_allowed: bool,
    /// Entries of `cache` whose value is a limit-truncated *stack-context
    /// artifact* rather than a stable function of the input `TypeId`: a node
    /// is tainted when a recursion/depth/iteration/divergence limit event
    /// fired within its own evaluation window (`limit_epoch` moved between
    /// entry and memo write), or when its value was an explicit cycle/depth
    /// bail-out insert. Reading a tainted entry back from `cache` records a
    /// limit event so the taint propagates to every in-flight ancestor.
    ///
    /// This is the per-entry discrimination (issue #13241, extending the
    /// PR #12902 application-eval epoch split) that lets the persistent
    /// `eval_cache` keep the *clean* intermediates of a run whose unrelated
    /// subtree hit a limit, instead of dropping the whole run's results.
    tainted: FxHashSet<TypeId>,
    /// Measurement-only id for the cross-evaluator memo-loss audit
    /// (issue #13097; see `evaluation::memo_audit`). 0 when perf counters
    /// are disabled.
    audit_evaluator_id: u64,
    /// Union-complexity event checkpoint taken at construction. A memo
    /// write-through is suppressed while the flag is newly set relative to
    /// this snapshot, mirroring the top-level boundary drain's `TS2590`
    /// gate (a cached read must not swallow the diagnostic re-derivation).
    union_complexity_at_construction: UnionComplexityCheckpoint,
    /// When true, nested `evaluate` nodes consult the persistent eval memo
    /// (`TypeApplicationEvalCache::lookup_eval_memo`) after a local-cache
    /// miss, so this
    /// evaluator reuses subtrees an earlier evaluator in the same file scope
    /// already evaluated instead of re-walking them (issue #13097).
    ///
    /// Only the plain query-backed (`NoopResolver`, no display/`this`/TS2589
    /// mode flags) evaluator construction opts in — the same context that
    /// produces every entry in that memo — so a hit is exactly the result
    /// this evaluator would have computed. Resolver-backed or mode-flagged
    /// evaluators must NOT opt in: their results can differ from the stored
    /// plain-context entries.
    persistent_memo_reads: bool,
    /// Set on a *limited-resolver* evaluator (the checker's first-pass
    /// `TypeEnvironment` evaluation, whose `Lazy` resolution is intentionally
    /// partial). Such an evaluator participates in the cross-call caches for
    /// reuse but must never *write* the resolver-independent
    /// `application_eval_cache`: even a fully-materialized application result
    /// can be context-dependent (a conditional binding `infer` against the
    /// use-site inference/contextual state), so it is not a pure function of
    /// `(DefId, args)` and a write would poison a later authoritative read.
    /// It still *reads* that cache (authoritative entries are always correct)
    /// and still shares the `instantiation_cache` (pure structural
    /// substitution is resolver- and context-independent), which is the source
    /// of its cross-block reuse. An authoritative full-resolver evaluator
    /// (`closed_eval_writes_allowed`) keeps the unconditional write.
    limited_resolver: bool,
}

/// Operation-local memo table statistics for [`TypeEvaluator`].
///
/// Owner: one evaluator request. The caches are dropped with the evaluator and
/// are never shared across resolver, substitution, or compiler-option modes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TypeEvaluatorCacheStatistics {
    /// Entries in the option-sensitive conditional subtype memo.
    pub conditional_subtype_entries: usize,
    /// Entries in the `contains infer` predicate memo keyed by `TypeId`.
    pub contains_infer_entries: usize,
    /// Entries in the compound simplification subtype memo.
    pub compound_subtype_entries: usize,
    /// Entries in the permissive-instantiation false-branch probe memo.
    pub permissive_false_branch_entries: usize,
    estimated_size_bytes: usize,
}

impl TypeEvaluatorCacheStatistics {
    /// Estimated heap bytes owned by the evaluator memo tables.
    #[must_use]
    pub const fn estimated_size_bytes(self) -> usize {
        self.estimated_size_bytes
    }
}

impl CompoundSubtypePairKey {
    pub(crate) fn from_checker<R: TypeResolver>(
        checker: &crate::relations::subtype::SubtypeChecker<'_, R>,
        source: TypeId,
        target: TypeId,
    ) -> Self {
        let resolver_identity = if checker.resolver.is_noop() {
            0
        } else {
            checker.resolver.resolver_identity()
        };
        let resolver_generation = if checker.resolver.is_noop() {
            0
        } else {
            checker.resolver.resolver_generation()
        };
        Self::new(
            checker.make_cache_key(source, target),
            checker.interner.type_database_identity(),
            resolver_identity,
            resolver_generation,
            checker.bypass_evaluation,
            checker.max_depth,
        )
    }
}

/// Operation-local cache key for tsc's permissive-instantiation false-branch
/// gate in conditional type evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PermissiveFalseBranchKey {
    check_type: TypeId,
    extends_type: TypeId,
    no_unchecked_indexed_access: bool,
    exact_optional_property_types: bool,
}

/// Operation-local cache key for definitive conditional branch subtype probes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ConditionalSubtypeCacheKey {
    check_type: TypeId,
    extends_type: TypeId,
    no_unchecked_indexed_access: bool,
    exact_optional_property_types: bool,
}

/// Snapshot of evaluator state that makes a probe result cacheable only when
/// unchanged across the probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EvaluationProbeState {
    limit_epoch: u32,
    unresolved_def_epoch: u32,
    request_termination_kind: Option<TerminationKind>,
}

#[cfg(target_arch = "wasm32")]
const DEFAULT_MAX_MAPPED_KEYS: usize = 250;
#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_MAX_MAPPED_KEYS: usize = 500;

impl<'a> TypeEvaluator<'a, NoopResolver> {
    /// Create a new evaluator without a resolver.
    ///
    /// Plain (`NoopResolver`, default mode flags) evaluators read the
    /// persistent eval memo at nested nodes (issue #13097); the mode
    /// builders below revoke that opt-in because their results are not the
    /// plain-context function of `(TypeId, options)` the memo stores.
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        static NOOP: NoopResolver = NoopResolver;
        Self::with_resolver_and_defaults(interner, &NOOP).with_persistent_eval_memo_reads()
    }
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    fn with_resolver_and_defaults(interner: &'a dyn TypeDatabase, resolver: &'a R) -> Self {
        tsz_common::perf_counters::record_eval_evaluator_construction();
        TypeEvaluator {
            interner,
            query_db: None,
            eval_session: None,
            resolver,
            no_unchecked_indexed_access: false,
            exact_optional_property_types: interner.exact_optional_property_types(),
            cache: FxHashMap::default(),
            guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::TypeEvaluation,
            ),
            keyof_constraint_guard: crate::recursion::RecursionGuard::with_profile(
                crate::recursion::RecursionProfile::TypeEvaluation,
            ),
            def_depth: FxHashMap::default(),
            meta_recursion_identity_stack: Vec::new(),
            def_eval_stack: Vec::new(),
            real_instantiation_depth_count: 0,
            suppress_this_binding: false,
            conditional_subtype_cache: FxHashMap::default(),
            contains_infer_cache: FxHashMap::default(),
            compound_subtype_cache: FxHashMap::default(),
            permissive_false_branch_cache: FxHashMap::default(),
            max_mapped_keys: DEFAULT_MAX_MAPPED_KEYS,
            flag_depth_on_app_cycle: false,
            expand_application_display_alias_args: false,
            apparent_conditional_branch: None,
            silent_depth_bailed: false,
            detection_growth_runs: FxHashMap::default(),
            deep_recursion_seen: false,
            request_termination_kind: None,
            request_unresolved_def_seen: false,
            limit_epoch: 0,
            app_body_limit_epoch: 0,
            unresolved_def_seen: false,
            unresolved_def_epoch: 0,
            app_body_unresolved_def_epoch: 0,
            provisional_epoch_seen: resolver.provisional_value_epoch(),
            closed_eval_writes_allowed: false,
            tainted: FxHashSet::default(),
            audit_evaluator_id: crate::evaluation::memo_audit::next_evaluator_id(),
            union_complexity_at_construction: interner.union_complexity_checkpoint(),
            persistent_memo_reads: false,
            limited_resolver: false,
        }
    }

    /// Return entry and size accounting for this evaluator's operation-local caches.
    #[must_use]
    pub fn cache_statistics(&self) -> TypeEvaluatorCacheStatistics {
        let conditional_subtype_entries = self.conditional_subtype_cache.len();
        let contains_infer_entries = self.contains_infer_cache.len();
        let compound_subtype_entries = self.compound_subtype_cache.len();
        let permissive_false_branch_entries = self.permissive_false_branch_cache.len();
        let type_evaluator_cache_estimated_size_bytes = conditional_subtype_entries
            .saturating_mul(std::mem::size_of::<(ConditionalSubtypeCacheKey, bool)>())
            .saturating_add(
                contains_infer_entries.saturating_mul(std::mem::size_of::<(TypeId, bool)>()),
            )
            .saturating_add(
                compound_subtype_entries
                    .saturating_mul(std::mem::size_of::<(CompoundSubtypePairKey, bool)>()),
            )
            .saturating_add(
                permissive_false_branch_entries
                    .saturating_mul(std::mem::size_of::<(PermissiveFalseBranchKey, bool)>()),
            );

        TypeEvaluatorCacheStatistics {
            conditional_subtype_entries,
            contains_infer_entries,
            compound_subtype_entries,
            permissive_false_branch_entries,
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
    /// Canonical definition in [`crate::limits`].
    const MAX_DEF_DEPTH: u32 = crate::limits::MAX_DEF_DEPTH;

    /// When the structural per-`TypeId` recursion guard hits its depth limit,
    /// surface it as TS2589 only if some `DefId` has been recursively expanded
    /// at least this many times — otherwise treat the bailout as the
    /// stack-protection cost of legitimate finite recursion and leave the type
    /// opaque. Calibration notes at the canonical definition in
    /// [`crate::limits`].
    const REAL_INSTANTIATION_BAILOUT_THRESHOLD: u32 =
        crate::limits::REAL_INSTANTIATION_BAILOUT_THRESHOLD;

    /// Whether `def_id`'s application is currently in-flight (a recursive
    /// back-edge). Used by the conditional array-arm fast paths to route a
    /// self-recursive branch through the shared tail-call loop instead of
    /// nesting through `evaluate_application` (see
    /// `eval_conditional_array_concrete`).
    pub(in crate::evaluation) fn def_application_in_flight(&self, def_id: DefId) -> bool {
        self.def_depth.get(&def_id).is_some_and(|&depth| depth > 0)
    }

    fn increment_def_depth(&mut self, def_id: DefId) -> bool {
        let depth = self.def_depth.entry(def_id).or_insert(0);
        // #14101 step-2 probe: a non-zero prior depth means this def's application
        // is re-entered while already in-flight (a recursive-heritage back-edge).
        // Pure instrumentation (probe-gated); quantifies SCC materialize-once headroom.
        if *depth >= 1 {
            crate::evaluation::eval_materialization_probe::record_def_reentry(*depth);
            // #14101 discriminating probe: distinct OTHER DefIds between this and
            // the prior same-DefId entry (0 = single-def recursion, >=1 = a
            // multi-member A->B->A SCC). Disjoint-field read of def_eval_stack
            // while `depth` borrows def_depth; gated internally on the probe.
            crate::evaluation::eval_materialization_probe::record_def_reentry_distinct(
                &self.def_eval_stack,
                def_id,
            );
        }
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
        // #14101 probe: push on success — balanced with the pop in
        // `decrement_def_depth` (the MAX_DEF_DEPTH early-return above does not push,
        // and its caller does not decrement, so the stack stays balanced). Gated on
        // the probe flag (stable per run) so a non-profiling build pays nothing.
        if tsz_common::perf_counters::enabled_fast() {
            self.def_eval_stack.push(def_id);
        }
        true
    }

    fn decrement_def_depth(&mut self, def_id: DefId) {
        // #14101 probe: pop — balanced with the gated push on a successful increment.
        if tsz_common::perf_counters::enabled_fast() {
            self.def_eval_stack.pop();
        }
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

    /// Set the owning evaluation session for fresh evaluator recursion state.
    #[must_use]
    pub const fn with_evaluation_session(mut self, session: &'a EvaluationSession) -> Self {
        self.eval_session = Some(session);
        self
    }

    /// Opt this evaluator in to *writing* the substitution-independent
    /// `closed_eval_cache`. Only the checker's authoritative, context-free
    /// type-resolution pass should call this — see `closed_eval_writes_allowed`.
    pub const fn with_closed_eval_writes(mut self) -> Self {
        self.closed_eval_writes_allowed = true;
        self
    }

    /// Mark this evaluator as backed by a *limited* resolver (the checker's
    /// first-pass `TypeEnvironment` evaluation). It shares the
    /// resolver-independent `instantiation_cache` and may *read* the
    /// `application_eval_cache`, but it never *writes* the latter, so an
    /// under-resolved or context-dependent first-pass result can never shadow
    /// the answer a later full-resolver pass would derive. See
    /// [`Self::limited_resolver`].
    pub const fn with_limited_resolver(mut self) -> Self {
        self.limited_resolver = true;
        self
    }

    /// Opt this evaluator in to reading the persistent eval memo at nested
    /// nodes (see `persistent_memo_reads`). Set by the plain `new`
    /// constructor and revoked by the mode builders; resolver-backed
    /// constructions never opt in.
    pub(crate) const fn with_persistent_eval_memo_reads(mut self) -> Self {
        self.persistent_memo_reads = true;
        self
    }

    /// Suppress `this` type substitution during Lazy type evaluation.
    /// When set, `ThisType` references inside resolved Lazy types are preserved
    /// rather than being bound to the Lazy type's own identity. This is used
    /// during interface heritage merging so that `this` can later be correctly
    /// bound to the final derived interface type.
    pub const fn with_suppress_this_binding(mut self) -> Self {
        self.suppress_this_binding = true;
        self.persistent_memo_reads = false;
        self
    }

    /// Flag `depth_exceeded` when cycle detection fires on an Application type.
    /// Used for TS2589 detection at type alias definition sites where
    /// self-referential conditional types produce the same Application TypeId
    /// on each expansion (e.g., `Foo<unknown>` → body → `Foo<unknown>`),
    /// preventing the normal per-DefId depth counter from triggering.
    pub const fn with_flag_depth_on_app_cycle(mut self) -> Self {
        self.flag_depth_on_app_cycle = true;
        // TS2589 detection must re-walk the expansion; a memo hit would
        // short-circuit the depth it is trying to observe.
        self.persistent_memo_reads = false;
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
        // Declaration-emit display aliasing is a side effect of the walk;
        // a memo hit would skip recording it.
        self.persistent_memo_reads = false;
        self
    }

    /// Drain the evaluator's internal cache, returning all intermediate results.
    /// This is for callers that inspect or discard entries. Callers persisting
    /// results into a cache whose key does not capture the ambient stack depth
    /// should use [`drain_stable_cache`](Self::drain_stable_cache).
    pub fn drain_cache(&mut self) -> impl Iterator<Item = (TypeId, TypeId)> + '_ {
        self.cache.drain()
    }

    /// Drain only cache entries whose values are stable functions of their
    /// input `TypeId`.
    ///
    /// This filters out entries whose values are limit-truncated stack-context
    /// artifacts, which must not be persisted into evaluator caches keyed only
    /// by `TypeId`.
    pub fn drain_stable_cache(&mut self) -> impl Iterator<Item = (TypeId, TypeId)> + '_ {
        let tainted = std::mem::take(&mut self.tainted);
        self.cache
            .drain()
            .filter(move |(type_id, _)| !tainted.contains(type_id))
    }

    /// Whether `type_id`'s memoized value is a limit-truncated artifact.
    pub(crate) fn is_tainted(&self, type_id: TypeId) -> bool {
        self.tainted.contains(&type_id)
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
            self.tainted.clear();
            self.compound_subtype_cache.clear();
            self.permissive_false_branch_cache.clear();
        }
        self.no_unchecked_indexed_access = enabled;
    }

    pub fn set_exact_optional_property_types(&mut self, enabled: bool) {
        if self.exact_optional_property_types != enabled {
            self.cache.clear();
            self.tainted.clear();
            self.compound_subtype_cache.clear();
            self.permissive_false_branch_cache.clear();
        }
        self.exact_optional_property_types = enabled;
    }

    pub const fn set_max_mapped_keys(&mut self, max_mapped_keys: usize) {
        self.max_mapped_keys = max_mapped_keys;
        // A non-default expansion cap changes where evaluation bails; memo
        // entries computed under the default cap must not be served here.
        self.persistent_memo_reads = false;
    }

    /// Reset per-evaluation state so this evaluator can be reused.
    ///
    /// Clears the cache, cycle detection sets, and counters while preserving
    /// configuration and borrowed references. Uses `.clear()` to reuse memory.
    #[inline]
    pub fn reset(&mut self) {
        self.cache.clear();
        self.tainted.clear();
        self.guard.reset();
        self.def_depth.clear();
        self.meta_recursion_identity_stack.clear();
        self.def_eval_stack.clear();
        self.real_instantiation_depth_count = 0;
        self.silent_depth_bailed = false;
        self.deep_recursion_seen = false;
        self.request_termination_kind = None;
        self.request_unresolved_def_seen = false;
        self.limit_epoch = 0;
        self.app_body_limit_epoch = 0;
        self.unresolved_def_seen = false;
        self.unresolved_def_epoch = 0;
        self.app_body_unresolved_def_epoch = 0;
        self.permissive_false_branch_cache.clear();
        self.compound_subtype_cache.clear();
    }

    /// Evaluate a normalized request, applying option-sensitive configuration
    /// before consulting this evaluator's local cache.
    pub fn evaluate_request(&mut self, request: EvaluationRequest) -> TypeId {
        self.evaluate_request_result(request).into_type_id()
    }

    /// Evaluate a normalized request and return the typed result stage.
    ///
    /// The sole producer of [`EvaluationResult`]. As of #14346 stage 3 it
    /// reports `Termination::Incomplete{ kind }` for any of the bail classes
    /// [`Self::note_request_termination`] records (cleared here on entry so the
    /// verdict is scoped to this single top-level request and cannot leak from
    /// a reused evaluator). `self.evaluate` still collapses each bail to the
    /// opaque, relation-preserving `TypeId` internally and
    /// `EvaluationResult::incomplete` carries that same `TypeId` as its
    /// `partial`, so every consumer's `into_type_id` collapse — and therefore
    /// the emitted type and diagnostics — is byte-identical to the pre-channel
    /// evaluator.
    pub fn evaluate_request_result(&mut self, request: EvaluationRequest) -> EvaluationResult {
        self.set_no_unchecked_indexed_access(request.no_unchecked_indexed_access());
        self.set_exact_optional_property_types(request.exact_optional_property_types());
        self.request_termination_kind = None;
        self.request_unresolved_def_seen = false;
        let type_id = self.evaluate(request.type_id());
        request_result_verdict(type_id, self.request_termination_kind)
    }

    /// Evaluate a request for a depth-agnostic memo and report whether the
    /// collapsed `TypeId` is stable enough to store.
    ///
    /// A typed incomplete verdict catches guard bails that the legacy sticky
    /// flags did not fully model (for example fuel/query-budget bails). The
    /// legacy [`Self::recursion_limit_hit`] backstop remains until #14346 makes
    /// the typed verdict the sole owner for every recursion taint class.
    pub(crate) fn evaluate_request_memo_result(
        &mut self,
        request: EvaluationRequest,
    ) -> EvaluationMemoResult {
        let result = self.evaluate_request_result(request);
        EvaluationMemoResult::for_depth_agnostic_memo(result, self.request_state_cache_stability())
    }

    /// Whether the current request state is stable enough for depth-agnostic
    /// cache publication.
    ///
    /// This centralizes the #14346 transition from loose evaluator flags to the
    /// typed request verdict. A typed incomplete verdict catches guard bails
    /// that the legacy sticky flags did not fully model (for example
    /// fuel/query-budget bails). The legacy [`Self::recursion_limit_hit`]
    /// backstop remains until every recursion-taint class is owned by the typed
    /// termination channel. The unresolved-def bit is the registration-window
    /// artifact gate: a result observed before a `DefId` body registers must
    /// not be published into a key-only cache.
    #[inline]
    pub(crate) const fn request_state_cache_stability(&self) -> EvaluationRequestStability {
        EvaluationRequestStability::from_request_state(
            self.has_incomplete_request_verdict(),
            self.recursion_limit_hit(),
            self.request_unresolved_def_seen,
        )
    }

    /// Whether the current top-level evaluation run is stable enough for
    /// run-wide cache publication.
    #[inline]
    pub(crate) const fn run_state_cache_stability(&self) -> EvaluationRequestStability {
        EvaluationRequestStability::from_request_state(
            self.has_incomplete_request_verdict(),
            self.recursion_limit_hit(),
            self.unresolved_def_seen(),
        )
    }

    #[inline]
    pub(crate) const fn request_state_is_depth_agnostic_cache_stable(&self) -> bool {
        self.request_state_cache_stability()
            .is_stable_for_depth_agnostic_cache()
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

    /// Get the owning evaluation session when one is available.
    #[inline]
    pub(crate) const fn evaluation_session(&self) -> Option<&'a EvaluationSession> {
        self.eval_session
    }

    /// Run `f` against this evaluator's owning session, falling back to the
    /// thread's current session when none was threaded in (see
    /// [`crate::evaluation::session::with_session_or_current`]).
    #[inline]
    pub(crate) fn with_evaluation_session_scope<T>(
        &self,
        f: impl FnOnce(&EvaluationSession) -> T,
    ) -> T {
        crate::evaluation::session::with_session_or_current(self.eval_session, f)
    }

    /// PERF: Look up a cached subtype result from conditional type evaluation.
    #[inline]
    pub(crate) fn cached_conditional_subtype(
        &self,
        check: TypeId,
        extends: TypeId,
    ) -> Option<bool> {
        self.conditional_subtype_cache
            .get(&self.conditional_subtype_cache_key(check, extends))
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
        let key = self.conditional_subtype_cache_key(check, extends);
        self.conditional_subtype_cache.insert(key, result);
    }

    #[inline]
    const fn conditional_subtype_cache_key(
        &self,
        check_type: TypeId,
        extends_type: TypeId,
    ) -> ConditionalSubtypeCacheKey {
        ConditionalSubtypeCacheKey {
            check_type,
            extends_type,
            no_unchecked_indexed_access: self.no_unchecked_indexed_access,
            exact_optional_property_types: self.exact_optional_property_types,
        }
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

    /// Build the option-sensitive cache key for tsc's permissive-instantiation
    /// false-branch gate.
    #[inline]
    pub(crate) const fn permissive_false_branch_key(
        &self,
        check_type: TypeId,
        extends_type: TypeId,
    ) -> PermissiveFalseBranchKey {
        PermissiveFalseBranchKey {
            check_type,
            extends_type,
            no_unchecked_indexed_access: self.no_unchecked_indexed_access,
            exact_optional_property_types: self.exact_optional_property_types,
        }
    }

    /// Look up a cached permissive false-branch probe result.
    #[inline]
    pub(crate) fn cached_permissive_false_branch(
        &self,
        key: &PermissiveFalseBranchKey,
    ) -> Option<bool> {
        self.permissive_false_branch_cache.get(key).copied()
    }

    /// Cache a permissive false-branch probe result.
    #[inline]
    pub(crate) fn cache_permissive_false_branch(
        &mut self,
        key: PermissiveFalseBranchKey,
        result: bool,
    ) {
        self.permissive_false_branch_cache.insert(key, result);
    }

    /// True when the shared permissive false-branch cache has the same
    /// resolver-independent semantics as this evaluator.
    ///
    /// Resolver-backed and limited evaluators can evaluate the instantiated
    /// permissive operands differently from the plain `NoopResolver` path, so
    /// they keep using only the operation-local mirror.
    #[inline]
    pub(crate) fn permissive_false_branch_shared_cache_allowed(&self) -> bool {
        self.persistent_memo_reads && !self.limited_resolver && self.resolver.is_noop()
    }

    /// Snapshot limit/unresolved/request state before an optional cache write.
    #[inline]
    pub(crate) const fn evaluation_probe_state(&self) -> EvaluationProbeState {
        EvaluationProbeState {
            limit_epoch: self.limit_epoch,
            unresolved_def_epoch: self.unresolved_def_epoch,
            request_termination_kind: self.request_termination_kind,
        }
    }

    /// True when a probe did not observe a new unresolved body or budget/limit
    /// event and therefore may publish an operation-local memo result.
    #[inline]
    pub(crate) fn evaluation_probe_state_is_unchanged(&self, state: EvaluationProbeState) -> bool {
        self.limit_epoch == state.limit_epoch
            && self.unresolved_def_epoch == state.unresolved_def_epoch
            && self.request_termination_kind == state.request_termination_kind
    }

    /// PERF: Exact `TypeId` containment, memoized project-wide on the interner.
    ///
    /// `contains_type_by_id` is a pure function of the immutable interned type
    /// `DAG` (no resolver, substitution env, or compiler option), so its result is
    /// permanently stable per `(root, target)` within one interner. Memoizing on
    /// the interner — rather than per `TypeEvaluator` — lets the many fresh
    /// evaluators created during recursive-alias instantiation reuse the walk
    /// instead of re-running it after each evaluator is dropped (#13097 / #8356).
    #[inline]
    pub(crate) fn cached_contains_type_by_id(&self, root: TypeId, target: TypeId) -> bool {
        if root == target {
            return true;
        }
        if let Some(cached) = self.interner.contains_type_by_id_memo(root, target) {
            return cached;
        }
        let result = crate::visitor::contains_type_by_id(self.interner, root, target);
        self.interner
            .set_contains_type_by_id_memo(root, target, result);
        result
    }

    /// Check if `no_unchecked_indexed_access` is enabled.
    #[inline]
    pub(crate) const fn no_unchecked_indexed_access(&self) -> bool {
        self.no_unchecked_indexed_access
    }

    /// Check if `exactOptionalPropertyTypes` is enabled.
    #[inline]
    pub(crate) const fn exact_optional_property_types(&self) -> bool {
        self.exact_optional_property_types
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

    /// Whether this run evaluated an application whose base `DefId` had no
    /// resolvable body (the registration-window artifact tracked by
    /// `unresolved_def_seen`).
    ///
    /// A result computed while a consumed `DefId` was still unresolved is a
    /// function of the *registration window* it ran in, not of the input
    /// `TypeId` alone: once the declaring file registers the real body, a fresh
    /// evaluation produces a different (correct) answer. Callers that persist
    /// the result in a cache keyed purely on the input `TypeId` — with no
    /// generation/registration guard — must consult this flag and skip the
    /// write, or the under-resolved answer permanently shadows the correct one.
    /// This is the public counterpart of the in-module `unresolved_def_seen`,
    /// exposed for the env-eval cache-poisoning backstop (issue #12101).
    #[inline]
    pub const fn is_unresolved_def_seen(&self) -> bool {
        self.unresolved_def_seen
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

    /// Whether the current top-level request has already recorded a typed
    /// `Termination::Incomplete` verdict. Cache writers that publish outside
    /// this evaluator must treat such results as partial even when the legacy
    /// recursion-limit backstop has not fired (for example query-budget bails).
    #[inline]
    pub(crate) const fn has_incomplete_request_verdict(&self) -> bool {
        self.request_termination_kind.is_some()
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

    /// Mark a depth-style guard bail and surface the typed request verdict.
    ///
    /// Use this for producers whose own bailout reason is
    /// [`TerminationKind::DepthExceeded`]. Producers with a more specific typed
    /// verdict, such as fuel exhaustion, should keep using
    /// [`Self::mark_depth_exceeded`] and then record their specific kind.
    #[inline]
    pub(crate) fn mark_depth_exceeded_for_request(&mut self) {
        self.mark_depth_exceeded();
        self.note_request_termination(TerminationKind::DepthExceeded);
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

    /// Record that `kind` cut the current top-level request's walk short so the
    /// `evaluate_request_result` boundary can surface
    /// `Termination::Incomplete{ kind }` (#14346 stage 3).
    ///
    /// First-wins: the verdict names the guard that *first* truncated the walk
    /// (the firing-order signal #14346 flags); a later bail in the same request
    /// does not overwrite it. Also bumps the measurement-only
    /// `record_eval_termination_guard` counter so the typed channel and the
    /// counter dump name the same set of guards — except `IterationExceeded`,
    /// which predates the counter and has no bucket (it returns before the
    /// counter call, preserving the pre-stage-3 counter behavior exactly).
    ///
    /// Byte-identical: this only records metadata; the opaque,
    /// relation-preserving `TypeId` each bail site returns is unchanged, so the
    /// universal `into_type_id` collapse — and the emitted type and diagnostics
    /// — are exactly as before.
    #[inline]
    pub(in crate::evaluation) fn note_request_termination(&mut self, kind: TerminationKind) {
        // First-wins: keep the kind that first truncated the walk.
        self.request_termination_kind.get_or_insert(kind);
        use tsz_common::perf_counters::EvaluationTerminationGuard as Guard;
        let guard = match kind {
            TerminationKind::DepthExceeded => Guard::DepthExceeded,
            TerminationKind::FuelExhausted => Guard::FuelExhausted,
            TerminationKind::SolverStackFrames => Guard::SolverStackFrames,
            TerminationKind::CrossEvalCycle => Guard::CrossEvalCycle,
            TerminationKind::QueryOpBudget => Guard::QueryOpBudget,
            // No observability counter exists for the iteration bail; it was
            // never recorded pre-stage-3 and must not start now.
            TerminationKind::IterationExceeded => return,
        };
        tsz_common::perf_counters::record_eval_termination_guard(guard);
    }

    /// Record a limit event from an evaluation-rule-local guard and surface it
    /// through the typed request verdict.
    ///
    /// Most bails flow through the evaluator's main recursion guard, whose
    /// `mark_*` helpers bump `limit_epoch` before recording the request
    /// verdict. Operation-local guards such as mapped-key constraint reduction
    /// return their own opaque fallback without touching that main guard; they
    /// must still advance `limit_epoch` so application-body cache writes do not
    /// publish a budget-truncated result.
    #[inline]
    pub(in crate::evaluation) fn record_request_limit_event(&mut self, kind: TerminationKind) {
        self.note_limit_event();
        self.note_request_termination(kind);
    }

    /// Record that an application's base `DefId` had no resolvable body in
    /// this run (registration-window artifact; see `unresolved_def_seen`).
    ///
    /// Bumps `limit_epoch` so the enclosing application's
    /// `application_eval_cache` write observes a stale
    /// `app_body_limit_epoch` snapshot and skips persisting — the same
    /// per-application precision the depth/cycle bails use. Intentionally
    /// does NOT set `deep_recursion_seen`: an unresolved def is not a depth
    /// limit and must not feed `recursion_limit_hit()` consumers
    /// (`TS2589`-adjacent bookkeeping, `depth_exceeded` cache markers).
    #[inline]
    pub(super) const fn mark_unresolved_def_seen(&mut self) {
        self.unresolved_def_seen = true;
        self.request_unresolved_def_seen = true;
        self.unresolved_def_epoch = self.unresolved_def_epoch.wrapping_add(1);
        self.note_limit_event();
    }

    /// Fold resolver-reported provisional serves into the unresolved-def
    /// machinery (see [`TypeResolver::provisional_value_epoch`]).
    ///
    /// Called before cache-write decisions (`memo_insert`) and at the end of
    /// a top-level `evaluate` so the public `is_unresolved_def_seen` reflects
    /// any provisional class partial served during this run. Each observed
    /// movement re-marks, so per-application epoch snapshots
    /// (`app_body_unresolved_def_epoch`) keep their precision: an application
    /// body evaluated entirely after the last movement stays cacheable.
    #[inline]
    pub(super) fn observe_provisional_epoch(&mut self) {
        let now = self.resolver.provisional_value_epoch();
        if now != self.provisional_epoch_seen {
            // TEMP PROBE (#16055)
            tracing::debug!(
                seen = self.provisional_epoch_seen,
                now,
                "16055 probe: evaluator observed provisional epoch movement"
            );
            self.provisional_epoch_seen = now;
            self.mark_unresolved_def_seen();
        }
    }

    /// Whether this run evaluated an application whose base `DefId` had no
    /// resolvable body (see `unresolved_def_seen`).
    #[inline]
    pub(super) const fn unresolved_def_seen(&self) -> bool {
        self.unresolved_def_seen
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

    /// Test hook: simulate a request that has already produced a typed
    /// incomplete verdict without also setting the legacy recursion-limit flags.
    /// This lets cache-boundary tests prove they consult the typed channel
    /// directly rather than passing only because `recursion_limit_hit` is true.
    #[cfg(test)]
    pub(crate) const fn simulate_incomplete_request_verdict_for_test(
        &mut self,
        kind: TerminationKind,
    ) {
        self.request_termination_kind = Some(kind);
    }

    /// Test hook: expose the typed request-result boundary without exposing the
    /// raw per-request verdict slot.
    #[cfg(test)]
    pub(crate) const fn request_result_for_test(&self, type_id: TypeId) -> EvaluationResult {
        request_result_verdict(type_id, self.request_termination_kind)
    }

    /// Global thread-local depth counter for cross-evaluator stack overflow
    /// prevention. Each `SubtypeChecker::evaluate_type` creates a fresh
    /// `TypeEvaluator`, but the OS stack accumulates across ALL of them: deep
    /// structural comparisons (e.g. `Vector<T> implements Seq<T>` with `Exclude`
    /// in an overload return) produce 100+ nested evaluate frames that overflow
    /// the 8MB default stack. This counter tracks cumulative `evaluate` frames
    /// across every `TypeEvaluator` on the call stack and bails with ERROR once
    /// it exceeds `MAX_GLOBAL_EVAL_DEPTH`. Canonical definition in
    /// [`crate::limits`].
    const MAX_GLOBAL_EVAL_DEPTH: u32 = crate::limits::MAX_GLOBAL_EVAL_DEPTH;

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
            tsz_common::perf_counters::record_eval_local_memo_hit();
            // Reading a limit-truncated artifact makes every in-flight
            // ancestor's result artifact-dependent: record a limit event so
            // the epoch-based stamping (and the application-eval epoch gate)
            // see it. The `is_empty` check keeps the common clean-run hit
            // path at a single length read.
            if !self.tainted.is_empty() && self.tainted.contains(&type_id) {
                self.note_limit_event();
            }
            return cached;
        }

        // Resolver-independent structural fixed point (issues #13250 / #8356).
        // A type containing none of the kinds `visit_type_key` rewrites and no
        // substitution-dependent leaf evaluates to itself under *every*
        // evaluator and resolver. Once any evaluator has observed and recorded
        // that fact (in `memo_insert`), every later evaluator — including the
        // resolver-backed ones that neither read nor write the persistent eval
        // memo — short-circuits the entire recursive walk here. This is the
        // single O(1) shared bit lookup that retires the ~56k identity
        // recomputes the ts-toolbelt recursive-conditional hot path performed
        // per run (the `with_resolver` evaluators dropped their local cache on
        // every relation/inference call). Mirrors the local-cache hit above:
        // no guard, budget, or epoch interaction, because the result is a
        // definite, context-free identity.
        if self.interner.structurally_eval_inert_cached(type_id) == Some(true) {
            self.cache.insert(type_id, type_id);
            return type_id;
        }

        // Substitution-independent persistent cache. See `closed_eval` module.
        if let Some(cached) = self.try_closed_eval_read(type_id) {
            self.cache.insert(type_id, cached);
            return cached;
        }

        // Persistent eval memo (issue #13097): reuse clean results an earlier
        // plain query-backed evaluator in this file scope already computed,
        // instead of re-walking the subtree. Opt-in is restricted to the same
        // plain context that wrote every stored entry (see
        // `persistent_memo_reads`), and stored entries are taint-filtered at
        // the write boundary, so a hit is exactly what this evaluator would
        // recompute. Mirrors the local-cache hit above: no guard, budget, or
        // epoch interaction.
        if self.persistent_memo_reads
            && let Some(cached) = self
                .interner
                .lookup_eval_memo(type_id, self.no_unchecked_indexed_access)
        {
            tsz_common::perf_counters::record_eval_memo_nested_hit();
            self.cache.insert(type_id, cached);
            return cached;
        }

        // Check if depth was already exceeded in a previous call
        if self.guard.is_exceeded() {
            // #14346 stage 3: surface the typed verdict for this request and
            // bump the observability counter (no-op when counters off). The
            // bail outcome (the opaque `ERROR` partial) is unchanged.
            self.note_request_termination(TerminationKind::DepthExceeded);
            return TypeId::ERROR;
        }
        // Cross-instance per-query operation budget (see `query_budget`).
        let Some(_query_frame) = self.enter_eval_query_budget() else {
            self.note_request_termination(TerminationKind::QueryOpBudget);
            return type_id;
        };

        // Cross-evaluator stack overflow prevention.
        // Only check the thread-local global depth (consolidated in
        // `crate::limits`) when the local guard depth is already significant
        // (>= 10). This avoids expensive TLS access on the vast majority of
        // shallow evaluations.
        if self.guard.depth() >= 10 {
            let global_depth = crate::limits::global_eval_depth_enter();
            if global_depth >= Self::MAX_GLOBAL_EVAL_DEPTH {
                crate::limits::global_eval_depth_leave();
                // Cross-evaluator stack protection: leave `type_id` opaque
                // rather than propagating ERROR. The outer evaluator can
                // proceed at a shallower depth without inheriting a sticky
                // exceeded flag. See the analogous DepthExceeded arm below.
                self.mark_silent_depth_bailed();
                self.note_request_termination(TerminationKind::CrossEvalCycle);
                return type_id;
            }
            let result = self.evaluate_guarded(type_id);
            crate::limits::global_eval_depth_leave();
            return result;
        }

        // Top-level frame: evaluate, then commit closed-eval cache writes.
        // See the `closed_eval` module for the safety gates.
        let limit_snapshot = EvaluationCacheLimitSnapshot::capture(self.interner);
        let result = self.evaluate_guarded(type_id);
        // Fold any provisional class-partial serve into `unresolved_def_seen`
        // before the closed-eval commit and before the caller reads
        // `is_unresolved_def_seen` (issue #16055).
        self.observe_provisional_epoch();
        self.commit_closed_eval_writes(limit_snapshot);
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
                self.note_request_termination(TerminationKind::SolverStackFrames);
                type_id
            },
        )
    }

    /// Interval for checking global evaluation fuel.
    ///
    /// We amortize the atomic load by only checking the global fuel counter
    /// every N iterations of the per-evaluator guard. This keeps the hot path
    /// fast while still catching runaway expansion within a few hundred
    /// iterations. Canonical definition in [`crate::limits`].
    const FUEL_CHECK_INTERVAL: u32 = crate::limits::EVAL_FUEL_CHECK_INTERVAL;

    /// Memoize `result` for `type_id`, stamping the entry as a stack-context
    /// artifact when a limit event fired within this node's evaluation window
    /// (`limit_epoch` moved past `epoch_at_entry`). Tainted entries must not
    /// be persisted to depth-agnostic caches; see the `tainted` field.
    #[inline]
    fn memo_insert(&mut self, epoch_at_entry: u32, type_id: TypeId, result: TypeId) {
        // A provisional class partial served during this node's evaluation
        // must gate this write like an unresolved def (issue #16055).
        self.observe_provisional_epoch();
        if self.limit_epoch != epoch_at_entry {
            self.tainted.insert(type_id);
        } else {
            // Persistent write-through (issue #13097): a clean-window entry is
            // a stable function of `(TypeId, options)` (the same per-entry
            // taint discrimination the boundary drain uses, issue #13241), so
            // plain evaluators publish it immediately instead of dropping it
            // with this evaluator. Gates mirror the boundary drain: the
            // `TS2590` union-complexity snapshot, and the limit-result-cache
            // kill switch for entries computed after a limit event this run.
            // The last clause skips the write only when the `TS2590` flag is
            // newly set relative to the construction snapshot.
            //
            // `self.limit_epoch != epoch_at_entry` above only tells us whether
            // *this node's own* window saw a NEW limit event; it says nothing
            // about a run-scoped taint an EARLIER, unrelated node already
            // recorded before this node's window opened. Once
            // `mark_unresolved_def_seen` fires anywhere in this evaluator's
            // request, every later node's own window looks "clean" by the
            // epoch-delta test alone (the epoch simply never moves again), yet
            // that later node may still be a sibling/dependent of the
            // under-resolved one — nothing about node-local epoch stability
            // proves it is not (#16553: a checker-pool partition evaluating a
            // cross-file interface union hits this after the pool's shared,
            // long-lived `eval_cache` mirrors this write cross-file). The
            // explicit `is_unresolved_def_seen()` check closes that gap; it
            // mirrors the identical guard the silent-depth-bail arm above
            // already applies to its own `memo_insert` call for the same
            // registration-window reason (#12101).
            if self.persistent_memo_reads
                && !type_id.is_intrinsic()
                && !self.is_unresolved_def_seen()
                && (self.limit_epoch == 0 || crate::limits::limit_result_cache_enabled())
                && !self
                    .interner
                    .union_complexity_changed_since(self.union_complexity_at_construction)
            {
                self.interner
                    .insert_eval_memo(type_id, self.no_unchecked_indexed_access, result);
            }
            // Resolver-independent structural fixed point (issues #13250 /
            // #8356). When a clean-window evaluation returns the input unchanged
            // (`result == type_id`) for a type that holds none of the kinds
            // `visit_type_key` rewrites nor any substitution-dependent leaf, the
            // identity is universal: every evaluator and resolver returns
            // `type_id`. Populating the shared structural-inertness bit here
            // (amortized O(1), shared, monotonic — a closed structural type's
            // inertness never changes) lets the fast path in `evaluate` retire
            // the recompute for every later evaluator, regardless of resolver.
            // It is populated unconditionally on `persistent_memo_reads`: the
            // resolver-backed `with_resolver` evaluators that drop their local
            // cache each call are exactly the ones this fixed point must reach.
            // `is_structurally_eval_inert` descends the full structural surface
            // and writes the bit as a side effect, so an unresolved `Lazy`
            // hidden in any child position keeps the type out of the cache —
            // only a genuine, resolver-independent fixed point is recorded. The
            // `is_none` guard skips the walk once the bit is already settled.
            if result == type_id
                && !type_id.is_intrinsic()
                && self
                    .interner
                    .structurally_eval_inert_cached(type_id)
                    .is_none()
            {
                let _inert =
                    crate::type_queries::is_structurally_eval_inert(self.interner, type_id);
            }
            // Measurement-only (issue #13097): record clean computes so the
            // memo audit can count cross-evaluator recomputation.
            crate::evaluation::memo_audit::record_clean_compute(
                type_id,
                self.no_unchecked_indexed_access,
                result,
                self.audit_evaluator_id,
                if self.persistent_memo_reads {
                    0
                } else if self.closed_eval_writes_allowed {
                    1
                } else {
                    2
                },
            );
        }
        self.cache.insert(type_id, result);
    }

    /// Actual evaluate logic -- separated so `stacker::maybe_grow` can wrap it.
    fn evaluate_guarded_inner(&mut self, type_id: TypeId) -> TypeId {
        use crate::recursion::RecursionResult;

        tsz_common::perf_counters::record_eval_compute_node();

        let _span =
            tracing::trace_span!("evaluate_type", ty = type_id.0, depth = self.guard.depth(),)
                .entered();

        // Snapshot for per-node taint stamping: every memo write below goes
        // through `memo_insert`, which compares the live `limit_epoch`
        // against this entry snapshot. The explicit bail-out arms call a
        // `mark_*` helper (which bumps the epoch) before inserting, so their
        // artifacts are stamped by the same comparison.
        let limit_epoch_at_entry = self.limit_epoch;

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
                    self.memo_insert(limit_epoch_at_entry, type_id, type_id);
                    return type_id;
                }
                // When checking type alias definitions for TS2589, a cycle on an
                // Application means the recursive expansion produces the same TypeId
                // each time (e.g., `Foo<unknown>` → body → `Foo<unknown>`). This is
                // effectively infinite recursion that the per-DefId counter can't
                // catch because cycle detection fires first. Flag depth_exceeded so
                // the checker can emit TS2589.
                if self.flag_depth_on_app_cycle && matches!(key, Some(TypeData::Application(_))) {
                    self.mark_depth_exceeded_for_request();
                    return TypeId::ERROR;
                }
                return type_id;
            }
            RecursionResult::DepthExceeded => {
                // Depth-bounded run (see `deep_recursion_seen`).
                self.mark_deep_recursion_seen();
                self.note_request_termination(TerminationKind::DepthExceeded);
                // The per-`TypeId` guard's depth limit is structural — it caps the
                // type-tree walk to protect the stack, not the instantiation chain.
                // tsc's `instantiationDepth` (the source of TS2589) is mirrored by
                // `def_depth`, so consult that to decide whether the bailout is a
                // real runaway (escalate) or just the structural cost of legitimate
                // finite recursion like the type-challenges `Permutation<U>` /
                // `Combination<U>` patterns (silently leave `type_id` opaque).
                if self.has_real_instantiation_depth() {
                    self.memo_insert(limit_epoch_at_entry, type_id, TypeId::ERROR);
                    return TypeId::ERROR;
                }
                self.guard.clear_exceeded();
                self.mark_silent_depth_bailed();
                // #12101 registration-window rule: a silent bail computed while
                // a consumed `DefId` had no resolvable body is a function of
                // the registration window, not of `type_id`. Memoizing the
                // opaque form would keep serving it from this long-lived
                // evaluator after the def registers, shadowing the real
                // expansion (and with it the `def_depth` divergence verdict
                // that surfaces `TS2589`).
                if !self.is_unresolved_def_seen() {
                    self.memo_insert(limit_epoch_at_entry, type_id, type_id);
                }
                return type_id;
            }
            RecursionResult::IterationExceeded => {
                // Iteration-limit bail: also a bounded run.
                self.mark_deep_recursion_seen();
                // Record the kind so the top-level `evaluate_request_result`
                // boundary can surface `Termination::Incomplete{ IterationExceeded }`
                // (#14346 stage 2). The returned `type_id` (the opaque,
                // relation-preserving partial) and the `deep_recursion_seen`
                // cache taint are unchanged, so the collapse via `into_type_id`
                // is byte-identical.
                self.note_request_termination(TerminationKind::IterationExceeded);
                self.memo_insert(limit_epoch_at_entry, type_id, type_id);
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
            self.memo_insert(limit_epoch_at_entry, type_id, TypeId::ERROR);
            self.note_request_termination(TerminationKind::FuelExhausted);
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

        // Measurement-only (issue #13250): quantify recursive-eval
        // materialization headroom for conditional/mapped/application inputs.
        // No-op (single branch) unless `TSZ_PERF_COUNTERS` is set; the lookup
        // of the result's `TypeData` is only performed under that gate.
        if tsz_common::perf_counters::enabled_fast() {
            let result_key = self.interner.lookup(result);
            crate::evaluation::eval_materialization_probe::record_compute(
                type_id,
                &key,
                result,
                result_key.as_ref(),
            );
            // #14101 / #13242 OPEN-2: instantiation-identity dedup ceiling.
            // How many distinct results would collapse if the nominal symbol
            // brand were ignored. Measurement-only, same gate.
            crate::evaluation::eval_materialization_probe::record_canon_headroom(
                &key,
                result,
                self.interner,
                self.query_db,
            );
        }

        // Symmetric cleanup: leave guard and cache result
        self.guard.leave(type_id);
        self.memo_insert(limit_epoch_at_entry, type_id, result);

        result
    }

    // Additional evaluator support methods live in the nested support module.
}

/// Translate a finished `evaluate` result plus the per-request termination
/// verdict into the typed [`EvaluationResult`] (#14346 stages 2–3).
///
/// `termination` is the first bail class that fired during the request (see
/// [`TypeEvaluator::note_request_termination`]), or `None` for a walk that ran
/// to completion. `EvaluationResult::incomplete` carries `type_id` as its
/// `partial`, so `into_type_id` is identical for both arms — the verdict is
/// additive metadata, not a value change, keeping every consumer
/// byte-identical. Factored out (and free of the evaluator's `R`/lifetime) so
/// the `Complete`/`Incomplete` selection is unit-testable without driving a
/// real bail through the full evaluator.
#[inline]
pub(crate) const fn request_result_verdict(
    type_id: TypeId,
    termination: Option<TerminationKind>,
) -> EvaluationResult {
    match termination {
        Some(kind) => EvaluationResult::incomplete(type_id, kind),
        None => EvaluationResult::complete(type_id),
    }
}

impl<R: TypeResolver> Drop for TypeEvaluator<'_, R> {
    fn drop(&mut self) {
        // Measurement-only (issue #13097): account for memo entries this
        // evaluator computed but discarded. Entries persisted through
        // `drain_cache` were removed before drop and are not counted.
        // Single branch when `TSZ_PERF_COUNTERS` is unset.
        if !tsz_common::perf_counters::enabled_fast() {
            return;
        }
        tsz_common::perf_counters::record_eval_dropped_memo_entries(self.cache.len() as u64);
        tsz_common::perf_counters::record_eval_dropped_aux_entries(
            (self.conditional_subtype_cache.len()
                + self.contains_infer_cache.len()
                + self.compound_subtype_cache.len()
                + self.permissive_false_branch_cache.len()) as u64,
        );
    }
}

// Re-enabled evaluate tests - verifying API compatibility
#[cfg(test)]
#[path = "../../tests/evaluate_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/evaluate_application_orchestrator_tests.rs"]
mod orchestrator_tests;

#[cfg(test)]
#[path = "../../tests/provisional_epoch_tests.rs"]
mod provisional_epoch_tests;

#[cfg(test)]
#[path = "../../tests/union_simplification_generic_member_tests.rs"]
mod union_simplification_generic_member_tests;
