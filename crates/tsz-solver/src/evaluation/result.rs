//! Typed result boundary for type evaluation.
//!
//! The evaluator still emits a single `TypeId` today, but this wrapper names
//! the result stage so future cache/provenance metadata can be attached
//! without threading loose tuples through the evaluation engine.
//!
//! # Termination channel (#14346 scaffold)
//!
//! Beyond the evaluated `TypeId`, the result now carries an explicit
//! [`Termination`] verdict mirroring the instantiation result's typed
//! termination precedent (`crate::instantiation::result`). The verdict names
//! *whether* a bound (depth, fuel, solver-stack frame budget, cross-eval
//! cycle, query-op budget) cut the walk short and, if so, which one — instead
//! of letting an
//! outer collapse silently treat a budget-truncated partial as a finished
//! type.
//!
//! The channel is parity-safe: every consumer collapses through
//! [`EvaluationResult::into_type_id`], which returns the relation-preserving
//! `partial` (= the same opaque `TypeId` the bail produced before the channel),
//! so the emitted type and diagnostics are byte-identical. As of #14346
//! stage 3 the [`crate::evaluation::evaluate::TypeEvaluator::evaluate_request_result`]
//! producer reports [`Termination::Incomplete`] for every one of the six bail
//! classes: stage 2 wired [`TerminationKind::IterationExceeded`] (the
//! per-evaluator iteration limit), and stage 3 wired the five guards that
//! already carried a `record_eval_termination_guard` observability counter —
//! [`TerminationKind::DepthExceeded`], [`TerminationKind::FuelExhausted`],
//! [`TerminationKind::SolverStackFrames`], [`TerminationKind::CrossEvalCycle`],
//! and [`TerminationKind::QueryOpBudget`] — through the shared
//! `note_request_termination` helper (first-wins: the verdict names the guard
//! that first truncated the walk). Each still surfaces the same `partial` and
//! preserves the existing cache taint, so the collapse is byte-identical; the
//! verdict is additive metadata a later stage (cache eligibility as a property
//! of the result) can act on.

use crate::types::TypeId;

/// Which evaluation guard cut a walk short.
///
/// One discriminant per bail class in
/// `crate::evaluation::evaluate::TypeEvaluator::evaluate`: the
/// recursion-depth guard, the process-wide evaluation fuel counter, the shared
/// solver-stack-frame breaker, the cross-evaluator global-depth cycle limit,
/// the per-query operation budget, and the per-evaluator iteration limit.
/// Carried inside
/// [`Termination::Incomplete`] so a downstream consumer can distinguish a
/// genuine result from a budget-limited approximation, and (in a later stage)
/// refuse to memoize it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminationKind {
    /// The per-evaluator recursion-depth guard tripped (`guard.is_exceeded`).
    DepthExceeded,
    /// The process-wide evaluation fuel counter was exhausted.
    FuelExhausted,
    /// The shared cross-operation solver-stack-frame breaker bailed
    /// (`crate::recursion::with_solver_frame`).
    SolverStackFrames,
    /// The cross-evaluator global-depth limit (`MAX_GLOBAL_EVAL_DEPTH`)
    /// short-circuited to keep the native stack bounded.
    CrossEvalCycle,
    /// The per-query operation budget (`enter_eval_query_budget`) ran out.
    QueryOpBudget,
    /// The per-evaluator recursion guard's *iteration* limit tripped
    /// (`RecursionResult::IterationExceeded`) — a bounded run that leaves the
    /// node opaque and marks the `deep_recursion_seen` cache taint.
    IterationExceeded,
}

impl TerminationKind {
    /// Whether this bail class is a function of the evaluated *key alone*.
    ///
    /// A fresh evaluator (spun up at every cross-evaluator boundary —
    /// `SubtypeChecker::evaluate_type`, infer-pattern matching,
    /// `relations::judge`) starts its per-instance recursion guard with the
    /// iteration counter at zero. [`IterationExceeded`](Self::IterationExceeded)
    /// counts that evaluator's *total* enter attempts, so for a given boundary
    /// key it trips at the same point no matter which evaluator walks it: the
    /// opaque partial it surfaces is *reproducible from the key*, and retaining
    /// it in the window-scoped per-query memo cannot change the emitted type —
    /// it only removes the redundant cross-evaluator re-walk.
    ///
    /// The other kinds are excluded:
    /// - [`DepthExceeded`](Self::DepthExceeded) is per-evaluator too, but its
    ///   bail point is the *current stack nesting*, which depends on how the
    ///   node was reached rather than on the node alone, so a fresh boundary
    ///   walk of the same key can converge where an inline one bailed. It stays
    ///   excluded until that reproducibility is established.
    /// - [`FuelExhausted`](Self::FuelExhausted),
    ///   [`SolverStackFrames`](Self::SolverStackFrames),
    ///   [`CrossEvalCycle`](Self::CrossEvalCycle), and
    ///   [`QueryOpBudget`](Self::QueryOpBudget) are driven by process- or
    ///   run-global budgets a fresh evaluator does **not** reset; their bail
    ///   point moves with ambient state, so their partial is not reproducible
    ///   from the key and must never be reused as if it were.
    ///
    /// Used to decide whether a guard-truncated partial may be retained in the
    /// window-scoped per-query memo (see
    /// [`EvaluationMemoResult::is_stable_for_per_query_memo`]). Durable,
    /// depth-agnostic caches refuse every incomplete result regardless.
    pub(crate) const fn is_boundary_intrinsic(self) -> bool {
        matches!(self, Self::IterationExceeded)
    }
}

/// Whether an evaluation walk ran to completion or was bounded short.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Termination {
    /// The walk finished within every guard; `type_id` is the true result.
    Complete,
    /// A guard ([`TerminationKind`]) cut the walk short. `partial` is the
    /// relation-preserving approximation the bail surfaced (never a sentinel
    /// leak), kept so a depth-unaware consumer does not fall back to an
    /// un-evaluated original.
    Incomplete {
        kind: TerminationKind,
        partial: TypeId,
    },
}

/// The normalized output of an evaluation request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvaluationResult {
    type_id: TypeId,
    termination: Termination,
}

impl EvaluationResult {
    /// Construct a result for a walk that ran to completion.
    pub const fn complete(type_id: TypeId) -> Self {
        Self {
            type_id,
            termination: Termination::Complete,
        }
    }

    /// Construct a result for a walk a guard cut short, carrying the
    /// relation-preserving `partial` type and the [`TerminationKind`] that
    /// fired.
    ///
    /// Reached for every bail class as of #14346 stage 3 (stage 2 wired
    /// [`TerminationKind::IterationExceeded`]; stage 3 added the five
    /// guard bails — see `evaluate_request_result`). `type_id` is set to
    /// `partial`, so the universal [`EvaluationResult::into_type_id`] collapse
    /// is byte-identical to the pre-channel opaque bail; the verdict is
    /// additional metadata a future verdict-aware consumer can act on.
    pub const fn incomplete(partial: TypeId, kind: TerminationKind) -> Self {
        Self {
            type_id: partial,
            termination: Termination::Incomplete { kind, partial },
        }
    }

    pub const fn type_id(self) -> TypeId {
        self.type_id
    }

    /// The termination verdict for this walk.
    pub const fn termination(self) -> Termination {
        self.termination
    }

    /// Whether this walk completed without a guard-truncated verdict.
    pub const fn is_complete(self) -> bool {
        matches!(self.termination, Termination::Complete)
    }

    /// Whether a guard cut this walk short.
    pub const fn is_incomplete(self) -> bool {
        matches!(self.termination, Termination::Incomplete { .. })
    }

    /// Collapse the result to a single `TypeId`, ignoring the termination
    /// verdict. The collapse is byte-identical to the pre-channel evaluator;
    /// verdict-aware consumers (the relation layer's cache-taint gate, #14346)
    /// read [`Self::is_incomplete`] before collapsing here.
    pub const fn into_type_id(self) -> TypeId {
        self.type_id
    }

    pub fn is_identity_for(self, input: TypeId) -> bool {
        self.type_id == input
    }
}

/// Evaluation result plus the verdict for depth-agnostic memo publication.
///
/// #14346 is moving cache eligibility from loose evaluator flags into typed
/// result boundaries. A fresh-evaluator memo has two pieces of information:
/// the request's [`EvaluationResult`] and whether the request state still makes
/// the collapsed value safe to store in a depth-agnostic cache.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EvaluationMemoResult {
    result: EvaluationResult,
    request_stability: EvaluationRequestStability,
    cache_stability: EvaluationMemoStability,
}

/// Whether the evaluator request state is safe for depth-agnostic memo writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EvaluationRequestStability {
    Stable,
    /// A typed request result says a guard returned a partial answer.
    IncompleteVerdict,
    /// A legacy recursion/depth/iteration taint fired before the typed channel
    /// fully owns that family.
    RecursionLimit,
    /// A `DefId` body was unresolved, so the result is a registration-window
    /// artifact rather than a stable function of the request key.
    UnresolvedDef,
}

impl EvaluationRequestStability {
    pub(crate) const fn from_request_state(
        has_incomplete_request_verdict: bool,
        recursion_limit_hit: bool,
        unresolved_def_seen: bool,
    ) -> Self {
        if has_incomplete_request_verdict {
            Self::IncompleteVerdict
        } else if recursion_limit_hit {
            Self::RecursionLimit
        } else if unresolved_def_seen {
            Self::UnresolvedDef
        } else {
            Self::Stable
        }
    }

    pub(crate) const fn is_stable_for_depth_agnostic_cache(self) -> bool {
        matches!(self, Self::Stable)
    }

    /// Whether a complete result with this request-state verdict should remain
    /// cacheable by ordinary eval memo consumers. `UnresolvedDef` is allowed
    /// here to preserve pre-existing complete-result behavior; run-wide cache
    /// publishers such as closed-eval inspect the request state directly.
    pub(crate) const fn allows_complete_memo_result(self) -> bool {
        matches!(self, Self::Stable | Self::UnresolvedDef)
    }

    pub(crate) const fn is_stable_for_per_query_memo(self) -> bool {
        matches!(self, Self::Stable | Self::UnresolvedDef)
    }
}

/// Whether an evaluated memo result is safe to publish into caches whose key
/// does not encode ambient recursion/fuel state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EvaluationMemoStability {
    Stable,
    Unstable,
}

impl EvaluationMemoStability {
    const fn from_result(
        result: EvaluationResult,
        request_stability: EvaluationRequestStability,
    ) -> Self {
        if result.is_complete() && request_stability.allows_complete_memo_result() {
            Self::Stable
        } else {
            Self::Unstable
        }
    }

    const fn is_stable_for_depth_agnostic_cache(self) -> bool {
        matches!(self, Self::Stable)
    }
}

impl EvaluationMemoResult {
    /// Construct a memo result from a typed evaluation result and the
    /// request-state stability verdict.
    pub(crate) const fn for_depth_agnostic_memo(
        result: EvaluationResult,
        request_stability: EvaluationRequestStability,
    ) -> Self {
        Self {
            result,
            request_stability,
            cache_stability: EvaluationMemoStability::from_result(result, request_stability),
        }
    }

    /// Construct a completed memo result read from a cache that stores only
    /// stable entries.
    ///
    /// Superseded in the per-query memo read path by [`Self::from_memoized_result`],
    /// which preserves the stored termination verdict; retained for tests that
    /// pin the stable-complete shape directly.
    #[cfg(test)]
    pub(crate) const fn cached(type_id: TypeId) -> Self {
        Self {
            result: EvaluationResult::complete(type_id),
            request_stability: EvaluationRequestStability::Stable,
            cache_stability: EvaluationMemoStability::Stable,
        }
    }

    /// Construct a completed result that must not be stored in a
    /// depth-agnostic memo.
    pub(crate) const fn unstable_complete(type_id: TypeId) -> Self {
        Self {
            result: EvaluationResult::complete(type_id),
            request_stability: EvaluationRequestStability::IncompleteVerdict,
            cache_stability: EvaluationMemoStability::Unstable,
        }
    }

    #[cfg(test)]
    pub(crate) const fn evaluation_result(self) -> EvaluationResult {
        self.result
    }

    /// Whether the underlying evaluation walk was cut short by a guard
    /// ([`Termination::Incomplete`]), independent of the request-state taints
    /// that only affect memo publication.
    pub(crate) const fn is_incomplete_termination(self) -> bool {
        self.result.is_incomplete()
    }

    pub(crate) const fn type_id(self) -> TypeId {
        self.result.type_id()
    }

    /// Whether this result can be stored in caches whose key does not capture
    /// ambient recursion depth/fuel state.
    pub(crate) const fn is_stable_for_depth_agnostic_cache(self) -> bool {
        self.cache_stability.is_stable_for_depth_agnostic_cache()
    }

    /// Whether this result can be stored in the thread-local per-query memo.
    ///
    /// The per-query memo is window-scoped (cleared when a fresh top-level query
    /// begins) and read only at cross-evaluator boundaries, where every walk
    /// starts from a fresh per-instance guard. Two families qualify:
    ///
    /// * a converged result whose request state is window-safe (`Stable` /
    ///   `UnresolvedDef`) — `UnresolvedDef` remains visible to run-state gates
    ///   such as closed-eval publication, but a complete memo result is reusable
    ///   within the same coherent analysis window; and
    /// * a guard-truncated result whose bail is *boundary-intrinsic*
    ///   ([`TerminationKind::is_boundary_intrinsic`]) — a fresh evaluator
    ///   re-walking the same boundary key reproduces the identical opaque
    ///   partial, so serving it from the window memo removes the cross-evaluator
    ///   re-walk storm on deep recursive template-literal / conditional aliases
    ///   without changing the emitted type. A hit reconstructs the incomplete
    ///   verdict (see [`Self::from_memoized_result`]), so the partial stays out
    ///   of durable, depth-agnostic caches exactly as an in-line bail would.
    pub(crate) const fn is_stable_for_per_query_memo(self) -> bool {
        match self.result.termination() {
            Termination::Complete => self.request_stability.is_stable_for_per_query_memo(),
            Termination::Incomplete { kind, .. } => kind.is_boundary_intrinsic(),
        }
    }

    /// The typed evaluation result this memo wraps.
    pub(crate) const fn result(self) -> EvaluationResult {
        self.result
    }

    /// Reconstruct a memo result from an [`EvaluationResult`] previously stored
    /// in the window-scoped per-query memo.
    ///
    /// The stored result's termination verdict is preserved, so an opaque
    /// boundary-intrinsic partial keeps its incomplete verdict on the way out:
    /// [`Self::is_stable_for_depth_agnostic_cache`] stays `false` and downstream
    /// consumers (the subtype checker's `eval_cache`, closed-eval publication)
    /// refuse to promote it into a durable cache — identical to how they treat a
    /// freshly-computed bail. A converged result round-trips as `Stable`.
    pub(crate) const fn from_memoized_result(result: EvaluationResult) -> Self {
        // A converged hit round-trips `Stable` (matching the pre-existing
        // `cached()` read path); a stored boundary-intrinsic partial round-trips
        // `IncompleteVerdict` so it keeps its taint and stays out of durable
        // caches. `for_depth_agnostic_memo` owns the cache-stability derivation.
        let request_stability = if result.is_complete() {
            EvaluationRequestStability::Stable
        } else {
            EvaluationRequestStability::IncompleteVerdict
        };
        Self::for_depth_agnostic_memo(result, request_stability)
    }

    /// Collapse to the request's `TypeId` while preserving today's behavior for
    /// callers that are not yet verdict-aware.
    pub(crate) const fn into_type_id(self) -> TypeId {
        self.result.into_type_id()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EvaluationMemoResult, EvaluationMemoStability, EvaluationRequestStability,
        EvaluationResult, Termination, TerminationKind,
    };
    use crate::types::TypeId;

    #[test]
    fn complete_result_wraps_evaluated_type_id() {
        let result = EvaluationResult::complete(TypeId::STRING);

        assert_eq!(result.type_id(), TypeId::STRING);
        assert_eq!(result.into_type_id(), TypeId::STRING);
        assert_eq!(result.termination(), Termination::Complete);
        assert!(result.is_complete());
        assert!(!result.is_incomplete());
        assert!(result.is_identity_for(TypeId::STRING));
        assert!(!result.is_identity_for(TypeId::NUMBER));
    }

    #[test]
    fn incomplete_result_carries_partial_and_kind() {
        // `DepthExceeded` is a real producer as of #14346 stage 3 (the
        // `guard.is_exceeded()` prologue bail); the partial/verdict contract is
        // identical for every kind.
        let result = EvaluationResult::incomplete(TypeId::NUMBER, TerminationKind::DepthExceeded);

        assert!(!result.is_complete());
        assert!(result.is_incomplete());
        // `into_type_id` returns the relation-preserving partial regardless of
        // the verdict — the same collapse every consumer performs today.
        assert_eq!(result.into_type_id(), TypeId::NUMBER);
        assert_eq!(
            result.termination(),
            Termination::Incomplete {
                kind: TerminationKind::DepthExceeded,
                partial: TypeId::NUMBER,
            }
        );
    }

    #[test]
    fn memo_result_stability_requires_complete_result_and_clean_request_state() {
        let complete = EvaluationResult::complete(TypeId::STRING);
        let stable = EvaluationMemoResult::for_depth_agnostic_memo(
            complete,
            EvaluationRequestStability::Stable,
        );

        assert_eq!(stable.result(), complete);
        assert_eq!(stable.type_id(), TypeId::STRING);
        assert_eq!(stable.into_type_id(), TypeId::STRING);
        assert_eq!(stable.cache_stability, EvaluationMemoStability::Stable);
        assert!(stable.is_stable_for_depth_agnostic_cache());

        let request_state_tainted = EvaluationMemoResult::for_depth_agnostic_memo(
            complete,
            EvaluationRequestStability::RecursionLimit,
        );
        assert_eq!(
            request_state_tainted.cache_stability,
            EvaluationMemoStability::Unstable
        );
        assert!(!request_state_tainted.is_stable_for_depth_agnostic_cache());

        let unresolved_def_named = EvaluationMemoResult::for_depth_agnostic_memo(
            complete,
            EvaluationRequestStability::UnresolvedDef,
        );
        assert_eq!(
            unresolved_def_named.cache_stability,
            EvaluationMemoStability::Stable
        );
        assert!(unresolved_def_named.is_stable_for_depth_agnostic_cache());
        assert!(unresolved_def_named.is_stable_for_per_query_memo());

        let incomplete =
            EvaluationResult::incomplete(TypeId::NUMBER, TerminationKind::DepthExceeded);
        let typed_tainted = EvaluationMemoResult::for_depth_agnostic_memo(
            incomplete,
            EvaluationRequestStability::Stable,
        );
        assert_eq!(typed_tainted.into_type_id(), TypeId::NUMBER);
        assert_eq!(
            typed_tainted.cache_stability,
            EvaluationMemoStability::Unstable
        );
        assert!(!typed_tainted.is_stable_for_depth_agnostic_cache());
    }

    #[test]
    fn request_stability_names_request_state_reason() {
        assert_eq!(
            EvaluationRequestStability::from_request_state(false, false, false),
            EvaluationRequestStability::Stable
        );
        assert_eq!(
            EvaluationRequestStability::from_request_state(true, false, false),
            EvaluationRequestStability::IncompleteVerdict
        );
        assert_eq!(
            EvaluationRequestStability::from_request_state(false, true, false),
            EvaluationRequestStability::RecursionLimit
        );
        assert_eq!(
            EvaluationRequestStability::from_request_state(false, false, true),
            EvaluationRequestStability::UnresolvedDef
        );
        assert_eq!(
            EvaluationRequestStability::from_request_state(true, true, true),
            EvaluationRequestStability::IncompleteVerdict,
            "typed incomplete verdict should stay the primary request-state reason"
        );
        assert_eq!(
            EvaluationRequestStability::from_request_state(false, true, true),
            EvaluationRequestStability::RecursionLimit,
            "recursion-limit taint should stay primary when both legacy taints are set"
        );
        assert!(EvaluationRequestStability::Stable.is_stable_for_depth_agnostic_cache());
        assert!(
            !EvaluationRequestStability::IncompleteVerdict.is_stable_for_depth_agnostic_cache()
        );
        assert!(!EvaluationRequestStability::RecursionLimit.is_stable_for_depth_agnostic_cache());
        assert!(!EvaluationRequestStability::UnresolvedDef.is_stable_for_depth_agnostic_cache());
        assert!(EvaluationRequestStability::UnresolvedDef.is_stable_for_per_query_memo());
    }

    #[test]
    fn cached_memo_result_is_stable_and_complete() {
        let cached = EvaluationMemoResult::cached(TypeId::BOOLEAN);

        assert_eq!(cached.type_id(), TypeId::BOOLEAN);
        assert!(cached.is_stable_for_depth_agnostic_cache());
        assert_eq!(cached.result().termination(), Termination::Complete);
    }

    #[test]
    fn unstable_complete_memo_result_collapses_without_becoming_cacheable() {
        let result = EvaluationMemoResult::unstable_complete(TypeId::STRING);

        assert_eq!(result.type_id(), TypeId::STRING);
        assert_eq!(result.into_type_id(), TypeId::STRING);
        assert!(!result.is_stable_for_depth_agnostic_cache());
        assert!(!result.is_stable_for_per_query_memo());
    }

    #[test]
    fn boundary_intrinsic_names_the_iteration_bail_only() {
        // The per-evaluator total-work counter resets at every fresh-evaluator
        // boundary, so an iteration bail reproduces from the key alone.
        assert!(TerminationKind::IterationExceeded.is_boundary_intrinsic());
        // Depth is per-evaluator but reach-dependent; ambient/global budgets do
        // not reset per evaluator — all stay excluded.
        assert!(!TerminationKind::DepthExceeded.is_boundary_intrinsic());
        assert!(!TerminationKind::FuelExhausted.is_boundary_intrinsic());
        assert!(!TerminationKind::SolverStackFrames.is_boundary_intrinsic());
        assert!(!TerminationKind::CrossEvalCycle.is_boundary_intrinsic());
        assert!(!TerminationKind::QueryOpBudget.is_boundary_intrinsic());
    }

    #[test]
    fn per_query_memo_retains_boundary_intrinsic_partials_but_never_durably() {
        // A converged result keeps the pre-existing behavior: window- and
        // depth-agnostic-cacheable.
        let complete = EvaluationMemoResult::for_depth_agnostic_memo(
            EvaluationResult::complete(TypeId::STRING),
            EvaluationRequestStability::Stable,
        );
        assert!(complete.is_stable_for_per_query_memo());
        assert!(complete.is_stable_for_depth_agnostic_cache());

        // A boundary-intrinsic bail is retained in the window memo (kills the
        // re-walk storm) but stays out of every durable cache.
        let partial = EvaluationMemoResult::for_depth_agnostic_memo(
            EvaluationResult::incomplete(TypeId::NUMBER, TerminationKind::IterationExceeded),
            EvaluationRequestStability::IncompleteVerdict,
        );
        assert!(
            partial.is_stable_for_per_query_memo(),
            "iteration-exceeded partial should be window-retainable"
        );
        assert!(
            !partial.is_stable_for_depth_agnostic_cache(),
            "iteration-exceeded partial must never reach a durable cache"
        );
        assert_eq!(partial.into_type_id(), TypeId::NUMBER);

        // Reach-dependent and ambient/global-budget bails stay excluded from the
        // window memo too, so a budget-dependent partial is never reused as a
        // converged answer.
        for kind in [
            TerminationKind::DepthExceeded,
            TerminationKind::FuelExhausted,
            TerminationKind::SolverStackFrames,
            TerminationKind::CrossEvalCycle,
            TerminationKind::QueryOpBudget,
        ] {
            let partial = EvaluationMemoResult::for_depth_agnostic_memo(
                EvaluationResult::incomplete(TypeId::NUMBER, kind),
                EvaluationRequestStability::IncompleteVerdict,
            );
            assert!(
                !partial.is_stable_for_per_query_memo(),
                "{kind:?} partial must not be window-retained"
            );
            assert!(!partial.is_stable_for_depth_agnostic_cache());
        }
    }

    #[test]
    fn from_memoized_result_preserves_the_stored_verdict() {
        // A converged stored result round-trips as fully stable.
        let complete =
            EvaluationMemoResult::from_memoized_result(EvaluationResult::complete(TypeId::BOOLEAN));
        assert_eq!(complete.type_id(), TypeId::BOOLEAN);
        assert!(complete.is_stable_for_depth_agnostic_cache());
        assert!(complete.is_stable_for_per_query_memo());

        // A stored boundary-intrinsic partial comes back out still incomplete:
        // window-retainable but refused by durable caches, so a hit is never
        // promoted into a depth-agnostic cache as if it were converged.
        let partial = EvaluationMemoResult::from_memoized_result(EvaluationResult::incomplete(
            TypeId::NUMBER,
            TerminationKind::IterationExceeded,
        ));
        assert_eq!(partial.into_type_id(), TypeId::NUMBER);
        assert!(!partial.is_stable_for_depth_agnostic_cache());
        assert!(partial.is_stable_for_per_query_memo());
        assert_eq!(
            partial.result().termination(),
            Termination::Incomplete {
                kind: TerminationKind::IterationExceeded,
                partial: TypeId::NUMBER,
            }
        );
    }
}
