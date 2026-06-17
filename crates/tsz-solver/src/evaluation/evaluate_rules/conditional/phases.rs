//! Named phases for conditional type evaluation.

use crate::instantiation::instantiate::instantiate_generic_cached;
use crate::relations::subtype::TypeResolver;
use crate::types::{ConditionalType, PropertyInfo, TypeData, TypeId};
use tracing::trace;

use super::super::super::evaluate::TypeEvaluator;

/// Resolved and pre-computed operands for one conditional evaluation step.
pub(super) struct ConditionalOperands {
    pub(super) check_type: TypeId,
    pub(super) extends_type: TypeId,
    pub(super) extends_has_infer: bool,
    pub(super) extends_has_type_params: bool,
}

thread_local! {
    /// Re-entrant conditional-subtype recursion depth on this thread.
    ///
    /// The counter is consulted by [`ConditionalSubtypeDepthGuard`] to cap the
    /// `Evaluator -> SubtypeChecker -> Evaluator -> ...` chain at depth 50.
    /// It MUST be zero between compilations: a leaked positive depth would make
    /// later conditional-subtype checks on a reused batch/merge-group worker
    /// conservatively take the false branch, so the same code would select
    /// different conditional branches run-to-run (#13368). The guard restores
    /// the depth on every exit including a panic-unwind a caller swallows, so
    /// no manual post-call decrement (which the unwind skips) is needed.
    static CONDITIONAL_SUBTYPE_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// RAII recursion-depth guard for [`check_conditional_subtype`].
///
/// [`enter`](Self::enter) increments [`CONDITIONAL_SUBTYPE_DEPTH`] and returns
/// the depth observed *before* the increment plus the guard; `Drop` decrements
/// it, so the counter is restored on the normal return path and when the
/// guarded subtype walk unwinds via a caught panic. The counter is
/// function-private state that the batch boundary reset cannot reach, so this
/// self-cleaning guard is the only correct cross-compilation isolation for it.
#[must_use]
struct ConditionalSubtypeDepthGuard;

impl ConditionalSubtypeDepthGuard {
    /// Cap above which the conditional-subtype relation conservatively returns
    /// `false`, matching tsc returning the deferred conditional once the
    /// instantiation depth is exceeded.
    const LIMIT: u32 = 50;

    fn enter() -> (u32, Self) {
        let prev_depth = CONDITIONAL_SUBTYPE_DEPTH.with(|d| {
            let c = d.get();
            d.set(c + 1);
            c
        });
        (prev_depth, Self)
    }
}

impl Drop for ConditionalSubtypeDepthGuard {
    fn drop(&mut self) {
        CONDITIONAL_SUBTYPE_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Result from tail-call dispatch in conditional evaluation.
pub(super) enum TailCallStep {
    /// Continue the loop with this conditional (direct or via `Application`).
    Continue(ConditionalType),
    /// An `Application` expanded to a non-conditional type; caller emits alias.
    InstantiatedApp { original: TypeId, resolved: TypeId },
    /// Branch is a bare `Application` (inside limit, not expandable to conditional).
    BareApplication,
    /// No tail-call pattern detected (at limit or branch is not `Application`/`Conditional`).
    NoTailCall,
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Decide whether a conditional with an `infer`-bearing extends pattern and a
    /// generic-tuple check type must stay deferred.
    ///
    /// tsc only defers a conditional whose check type is generic when the
    /// extends relation's outcome actually depends on how the free type
    /// parameter is later instantiated. For a tuple check type matched against a
    /// tuple `infer` pattern, that dependence exists only when a generic element
    /// is aligned with a pattern position that imposes a structural shape or a
    /// constraint the element might not satisfy (e.g. `[T] extends [[infer U]]`,
    /// where `T` may or may not be a one-tuple). When every generic element lines
    /// up with an `infer` position it always satisfies (a bare `infer`, or a
    /// constrained `infer X extends C` whose `C` already bounds the element),
    /// the pattern matches for any instantiation, so tsc resolves the true
    /// branch eagerly with the element captured by the `infer`. Deferring in that
    /// case strands the `infer` variables unbound in the true branch.
    ///
    /// The relaxation is intentionally conservative: it only skips deferral for
    /// rest-free, equal-length positional tuples whose every generic element
    /// faces such an always-matching `infer`. Any rest element, length mismatch,
    /// or generic element facing a non-`infer` / not-provably-satisfied position
    /// keeps the original deferral so shape/constraint-dependent patterns
    /// (`[T] extends [[infer U]]`) are unaffected. The downstream infer-pattern
    /// match (Step 3) and its own generic-check-type deferral then own the
    /// resolved case.
    pub(super) fn generic_tuple_infer_defer_required(
        &self,
        check_type: TypeId,
        extends_unwrapped: TypeId,
    ) -> bool {
        let Some(TypeData::Tuple(check_list)) = self.interner().lookup(check_type) else {
            return false;
        };
        let check_elems = self.interner().tuple_list(check_list);
        let has_generic_element = check_elems
            .iter()
            .any(|element| Self::is_generic_ref(self.interner(), element.type_id));
        if !has_generic_element {
            // Not a generic tuple at all — the original gate would not fire.
            return false;
        }

        // Only relax for a rest-free, equal-length positional tuple pattern.
        let Some(TypeData::Tuple(pattern_list)) = self.interner().lookup(extends_unwrapped) else {
            return true;
        };
        let pattern_elems = self.interner().tuple_list(pattern_list);
        if check_elems.len() != pattern_elems.len() {
            return true;
        }
        if check_elems.iter().any(|e| e.rest) || pattern_elems.iter().any(|e| e.rest) {
            return true;
        }

        // Defer unless every generic check element faces an `infer` position
        // that matches it for *every* instantiation. A bare (unconstrained)
        // `infer` matches any type. A constrained `infer X extends C` matches the
        // generic element for all instantiations only when the element's own
        // upper bound is already a subtype of `C` (so no instantiation can fall
        // outside `C`). Both cases make the conditional resolve identically
        // regardless of the free parameter, so eager resolution is sound and
        // matches tsc; deferring would strand the `infer` variables unbound.
        for (check_elem, pattern_elem) in check_elems.iter().zip(pattern_elems.iter()) {
            if !Self::is_generic_ref(self.interner(), check_elem.type_id) {
                continue;
            }
            let Some(TypeData::Infer(info)) = self.interner().lookup(pattern_elem.type_id) else {
                return true;
            };
            let Some(infer_constraint) = info.constraint else {
                // Bare infer: matches any instantiation of the element.
                continue;
            };
            // Constrained infer: only safe when the element provably satisfies
            // the constraint for every instantiation, i.e. its upper bound is a
            // subtype of the infer's constraint.
            if !self
                .generic_element_satisfies_infer_constraint(check_elem.type_id, infer_constraint)
            {
                return true;
            }
        }
        false
    }

    /// True when a generic check-tuple element provably satisfies a constrained
    /// `infer`'s upper bound for *every* instantiation — i.e. the element's own
    /// declared constraint (upper bound) is assignable to `infer_constraint`.
    /// Used by [`Self::generic_tuple_infer_defer_required`] to decide whether a
    /// constrained `infer` position matches unconditionally. A `false` result
    /// keeps the conditional deferred, never widening the resolved set.
    fn generic_element_satisfies_infer_constraint(
        &self,
        element: TypeId,
        infer_constraint: TypeId,
    ) -> bool {
        let Some(TypeData::TypeParameter(param)) = self.interner().lookup(element) else {
            return false;
        };
        let Some(element_constraint) = param.constraint else {
            // An unconstrained type parameter's upper bound is `unknown`, which
            // is not a subtype of a non-trivial `infer_constraint`.
            return false;
        };
        let mut checker = self.conditional_subtype_checker();
        checker.allow_bivariant_rest = true;
        checker.is_subtype_of(element_constraint, infer_constraint)
    }

    /// Resolve and pre-compute operands for one conditional evaluation step.
    ///
    /// Evaluates `check_type` and `extends_type`, normalises object shapes, expands
    /// `Application` check types, and caches the `extends_has_infer` /
    /// `extends_has_type_params` predicates so they are computed only once per loop
    /// iteration.
    pub(super) fn resolve_operands(&mut self, cond: &ConditionalType) -> ConditionalOperands {
        let evaluated_check = self.evaluate(cond.check_type);
        let mut check_type = self.normalize_conditional_object_operand(evaluated_check);
        let evaluated_extends = self.evaluate(cond.extends_type);
        let mut extends_type = self.normalize_conditional_object_operand(evaluated_extends);
        if matches!(
            self.interner().lookup(check_type),
            Some(TypeData::Application(_))
        ) && let Some(expanded_check) =
            self.try_expand_application_for_conditional_check(check_type)
        {
            check_type = expanded_check;
        }
        if !crate::visitors::visitor_predicates::contains_infer_types(
            self.interner(),
            cond.extends_type,
        ) && matches!(
            self.interner().lookup(extends_type),
            Some(TypeData::Application(_))
        ) && let Some(expanded_extends) =
            self.try_expand_application_for_conditional_check(extends_type)
        {
            extends_type = expanded_extends;
        }

        // When check_type is an unresolvable Application (e.g., Promise<string>
        // where Promise is referenced via TypeQuery with no DefId yet), try to
        // resolve it structurally. This is critical for Awaited<T>-style patterns
        // where the conditional needs to see Promise's structural members (like
        // `then`) for infer pattern matching.
        //
        // Uses get_type_params + resolve_ref on the SymbolRef directly, bypassing
        // the DefId path which may not be available yet during lazy evaluation.
        if let Some(TypeData::Application(app_id)) = self.interner().lookup(check_type) {
            let app = self.interner().type_application(app_id);
            if let Some(TypeData::TypeQuery(sym_ref)) = self.interner().lookup(app.base)
                && let Some(type_params) = self.resolver().get_type_params(sym_ref)
                && let Some(resolved_base) = self.resolver().resolve_ref(sym_ref, self.interner())
                && !type_params.is_empty()
                && type_params.len() == app.args.len()
            {
                let expanded_args = self.expand_type_args(&app.args);
                let instantiated = instantiate_generic_cached(
                    self.interner(),
                    self.query_db(),
                    resolved_base,
                    &type_params,
                    expanded_args.as_ref(),
                );
                let resolved = self.evaluate(instantiated);
                if resolved != check_type {
                    check_type = resolved;
                }
            }
        }

        trace!(
            check_raw = cond.check_type.0,
            check_eval = check_type.0,
            check_key = ?self.interner().lookup(check_type),
            extends_raw = cond.extends_type.0,
            extends_eval = extends_type.0,
            extends_key = ?self.interner().lookup(extends_type),
            "evaluate_conditional"
        );

        // PERF: Cache predicate results for extends_type once per iteration.
        // type_contains_infer is called up to 5 times and contains_free_type_parameters
        // at least once, each creating fresh FxHashSet/FxHashMap allocations.
        let extends_has_infer =
            self.type_contains_infer(extends_type) || self.type_contains_infer(cond.extends_type);
        // Use the FREE-type-parameter query: type parameters bound by inner
        // function/callable signatures (e.g., the `T` in `<T>() => ...`) are
        // already resolved within their own scope, so they must not force the
        // surrounding conditional to stay deferred. Without this distinction,
        // `(<T>() => T extends any ? 1 : 2) extends (<T>() => T extends Y ? 1 : 2)`
        // — the structural shape of the type-challenges `Equal<X, Y>` trick —
        // is incorrectly held deferred whenever either side embeds a generic
        // function literal.
        let extends_has_type_params =
            crate::visitor::contains_free_type_parameters(self.interner(), extends_type)
                || crate::visitor::contains_free_type_parameters(
                    self.interner(),
                    cond.extends_type,
                );

        ConditionalOperands {
            check_type,
            extends_type,
            extends_has_infer,
            extends_has_type_params,
        }
    }

    /// Settle a conditional whose evaluated operands carry an error or an
    /// unresolved reference, before the structural relation check runs.
    ///
    /// - A *genuine* error type in the extends position (e.g. a failed indexed
    ///   access that minted `TypeData::Error`) collapses the conditional to its
    ///   false branch — tsc parity, and it preserves structural modifiers
    ///   (readonly) instead of collapsing to `T`.
    /// - An `UnresolvedTypeName` is NOT a genuine error: it is a cross-module /
    ///   cross-arena reference the current resolver generation could not yet
    ///   bind to a `DefId` (the same residue the check-side `visit_conditional`
    ///   excludes via `is_genuine_error_type`). The relation machinery treats
    ///   such a name as related to everything (error/`any`-like), so a definitive
    ///   branch here would be schedule-dependent: `T extends Builtin ? T : …`
    ///   over a still-unresolved imported `Builtin` reports `T <: Builtin` true
    ///   and collapses to `T`, while `Filter extends AnyRecord ? {…} : never`
    ///   collapses to `never`. Defer instead (mirroring the `Lazy`/`Application`
    ///   deferral) so the resolver generation that binds the reference decides
    ///   the branch, and mark the unresolved-reference event so the deferred
    ///   result is not persisted to the depth-agnostic caches and a later pass
    ///   recomputes it rather than reusing a stale deferral.
    ///
    /// Returns `Some(result)` when the conditional is settled or deferred here.
    pub(super) fn resolve_conditional_error_or_unresolved(
        &mut self,
        cond: &ConditionalType,
        check_type: TypeId,
        extends_type: TypeId,
    ) -> Option<TypeId> {
        if crate::visitor::is_genuine_error_type(self.interner(), extends_type) {
            return Some(self.evaluate(cond.false_type));
        }

        // Only a *bare* `UnresolvedTypeName` is intercepted here; an unresolved
        // reference wrapped in an `Application`/`Lazy` is deferred downstream by
        // the indeterminate-relation block after the subtype check.
        if matches!(
            self.interner().lookup(extends_type),
            Some(TypeData::UnresolvedTypeName(_))
        ) || matches!(
            self.interner().lookup(check_type),
            Some(TypeData::UnresolvedTypeName(_))
        ) {
            self.mark_unresolved_def_seen();
            return Some(self.deferred_conditional(cond, check_type, extends_type));
        }

        None
    }

    /// Re-intern `cond` as a deferred conditional carrying the *evaluated*
    /// `check_type`/`extends_type` while preserving its original branches and
    /// distributivity. Shared by the operand-deferral sites in this module so the
    /// five-field reconstruction lives in one place.
    fn deferred_conditional(
        &self,
        cond: &ConditionalType,
        check_type: TypeId,
        extends_type: TypeId,
    ) -> TypeId {
        self.interner().conditional(ConditionalType {
            check_type,
            extends_type,
            true_type: cond.true_type,
            false_type: cond.false_type,
            is_distributive: cond.is_distributive,
        })
    }

    /// Defer a conditional whose evaluated CHECK type is an opaque, resolver-less
    /// `Application`, instead of letting it vacuously take its true branch.
    ///
    /// A resolvable application evaluates to its structural form, so an
    /// `Application` that *survives* evaluation in the check position is opaque:
    /// the active resolver lacked the body needed to reduce it (e.g. a
    /// distributive utility whose body could not expand because an inner
    /// reference was still unresolved under this resolver — the
    /// `resolver_generation()==0` / registration-window family). With no
    /// structure to compare, the structural subtype walk degrades the opaque
    /// application toward the bottom type, so `is_sub` is vacuously `true` and the
    /// conditional collapses into its TRUE branch. That is the mechanism by which
    /// the mapped key-remap `IsOptionalKeyOf<O, K> extends false ? never : K`
    /// filters every key to `never`, leaving `RequiredKeysOf`/`OptionalKeysOf`
    /// with the wrong key set and false-failing `TS2344` against a well-typed
    /// default-options argument (#13609).
    ///
    /// The guard is gated on [`Self::unresolved_def_seen`] so it fires only in
    /// that resolver-less window — a *generic* application whose base the resolver
    /// can expand (deferred only pending instantiation) is left to the normal
    /// branch logic, which is what keeps generic-call inference / higher-order
    /// re-generalization unaffected. (The extends-side `Application` guard in
    /// `conditional.rs` needs no such gate: it lives in the `is_sub == false`
    /// branch where deferring is already the conservative choice, whereas this
    /// check-side guard must override a vacuous `is_sub == true` and so requires
    /// the resolver-less signal to tell that apart from a genuine match.) When it
    /// does fire it mirrors that extends-side guard and the
    /// bare-`UnresolvedTypeName` deferral: defer so a later resolver pass expands
    /// the application and decides the branch, and
    /// re-mark the unresolved-def event so neither this evaluator nor the
    /// top-level evaluator that commits its results persists the resolver-less
    /// branch into the substitution-independent / application caches (a cold pass
    /// that left the application opaque must not shadow the answer a resolved pass
    /// derives). The `TS2589` depth-detection pass is exempt: it intentionally
    /// drives recursive alias bodies with their parameters left free.
    ///
    /// Returns `Some(deferred_conditional)` when the guard applies.
    pub(super) fn defer_resolver_less_application_check(
        &mut self,
        cond: &ConditionalType,
        check_type: TypeId,
        extends_type: TypeId,
        is_sub: bool,
    ) -> Option<TypeId> {
        if is_sub
            && self.unresolved_def_seen()
            && !self.is_depth_detection_pass()
            && matches!(
                self.interner().lookup(check_type),
                Some(TypeData::Application(_))
            )
        {
            self.mark_unresolved_def_seen();
            return Some(self.deferred_conditional(cond, check_type, extends_type));
        }
        None
    }

    /// Subtype check with cache lookup and thread-local depth guard.
    ///
    /// Returns `true` if `check_type <: extends_type`, consulting the evaluator's
    /// `conditional_subtype_cache` first and falling back to a full structural check
    /// guarded by a thread-local recursion counter that caps at depth 50.
    pub(super) fn check_conditional_subtype(
        &mut self,
        check_type: TypeId,
        extends_type: TypeId,
    ) -> bool {
        if let Some(cached) = self.cached_conditional_subtype(check_type, extends_type) {
            return cached;
        }

        // Depth guard: evaluating conditional types can trigger subtype checks
        // that evaluate MORE conditional types, creating an
        // Evaluator -> SubtypeChecker -> Evaluator -> ... chain where each
        // instance has fresh cycle-detection state. Without this global depth
        // limit, recursive generic types like `Vector<T> implements Seq<T>`
        // with `Exclude<T, U>` in overloads cause stack overflow. The guard is
        // RAII (see `ConditionalSubtypeDepthGuard`) so the depth is restored on
        // every exit including a caught panic-unwind, keeping the relation
        // schedule-independent across batch-worker reuse (#13368).
        let (prev_depth, depth_guard) = ConditionalSubtypeDepthGuard::enter();
        let result = if prev_depth >= ConditionalSubtypeDepthGuard::LIMIT {
            // At excessive depth, conservatively assume not a subtype
            // (takes the false/else branch of the conditional).
            // This matches tsc's behavior of returning the deferred
            // conditional when instantiation depth is exceeded.
            false
        } else if Self::is_primitive_vs_function(self.interner(), check_type, extends_type) {
            // Fast-path: primitive types (string, number, boolean, bigint,
            // symbol) are never subtypes of Function. The structural subtype
            // checker may incorrectly autobox the primitive to its wrapper
            // type (String, Number, etc.) and find structural compatibility
            // with the evaluated Function interface. This fast-path prevents
            // `string extends Function` from incorrectly taking the true
            // branch, matching tsc's behavior where primitives never extend
            // Function.
            false
        } else if Self::function_intrinsic_extends_callable_target(
            self.interner(),
            check_type,
            extends_type,
        ) {
            // In conditional types, tsc treats the global `Function`
            // intrinsic as satisfying callable targets. Ordinary
            // assignment intentionally remains stricter.
            true
        } else if self.object_literals_have_conflicting_required_property(check_type, extends_type)
        {
            // `Extract<Union, { kind: "x" }>` and similar discriminant filters
            // distribute over every union member. If both sides expose the same
            // required property with distinct literal values, the relation is
            // definitively false, so avoid the full structural subtype walk.
            false
        } else {
            let mut strict_checker = self.conditional_subtype_checker();
            strict_checker.is_subtype_of(check_type, extends_type)
        };
        // Restore the depth before the cache write to preserve the original
        // decrement ordering; `Drop` would otherwise run at end of scope, but
        // either way the depth is restored on a panic-unwind exit.
        drop(depth_guard);
        self.cache_conditional_subtype(check_type, extends_type, result);
        result
    }

    fn object_literals_have_conflicting_required_property(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let source_shape_id = match self.interner().lookup(source) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => shape_id,
            _ => return false,
        };
        let target_shape_id = match self.interner().lookup(target) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => shape_id,
            _ => return false,
        };

        let source_shape = self.interner().object_shape(source_shape_id);
        let target_shape = self.interner().object_shape(target_shape_id);

        target_shape
            .properties
            .iter()
            .filter(|prop| !prop.optional)
            .any(|target_prop| {
                let Some(source_prop) =
                    PropertyInfo::find_in_slice(&source_shape.properties, target_prop.name)
                else {
                    return false;
                };
                Self::literal_values_are_disjoint(
                    self.interner().lookup(source_prop.type_id),
                    self.interner().lookup(target_prop.type_id),
                )
            })
    }

    fn literal_values_are_disjoint(source: Option<TypeData>, target: Option<TypeData>) -> bool {
        match (source, target) {
            (Some(TypeData::Literal(source)), Some(TypeData::Literal(target))) => {
                source.primitive_type_id() == target.primitive_type_id() && source != target
            }
            _ => false,
        }
    }

    /// Detect a tail-call pattern in `branch` and return the continuation step.
    ///
    /// Decides whether the conditional evaluation loop should continue (tail-call
    /// elimination), return an instantiated application result, or fall through to
    /// a normal `evaluate` call.
    ///
    /// `tail_application_branch` is updated in-place when a bare `Application`
    /// expands to a `Conditional` (so the display alias survives across iterations).
    pub(super) fn try_dispatch_tail_call(
        &mut self,
        branch: TypeId,
        tail_application_branch: &mut Option<TypeId>,
        tail_recursion_count: usize,
    ) -> TailCallStep {
        if tail_recursion_count >= Self::MAX_TAIL_RECURSION_DEPTH {
            return TailCallStep::NoTailCall;
        }

        match self.interner().lookup(branch) {
            Some(TypeData::Conditional(next_cond_id)) => {
                TailCallStep::Continue(self.interner().get_conditional(next_cond_id))
            }
            Some(TypeData::Application(_)) => {
                if let Some(instantiated) = self.try_instantiate_application_for_tail_call(branch) {
                    if let Some(TypeData::Conditional(next_cond_id)) =
                        self.interner().lookup(instantiated)
                    {
                        tail_application_branch.get_or_insert(branch);
                        TailCallStep::Continue(self.interner().get_conditional(next_cond_id))
                    } else {
                        TailCallStep::InstantiatedApp {
                            original: branch,
                            resolved: instantiated,
                        }
                    }
                } else {
                    TailCallStep::BareApplication
                }
            }
            _ => TailCallStep::NoTailCall,
        }
    }
}

#[cfg(test)]
mod conditional_subtype_depth_guard_tests {
    use super::{CONDITIONAL_SUBTYPE_DEPTH, ConditionalSubtypeDepthGuard};

    fn current_depth() -> u32 {
        CONDITIONAL_SUBTYPE_DEPTH.with(std::cell::Cell::get)
    }

    #[test]
    fn enter_reports_prior_depth_and_drop_restores() {
        assert_eq!(current_depth(), 0, "counter starts clean");
        let (prev0, g0) = ConditionalSubtypeDepthGuard::enter();
        assert_eq!(prev0, 0, "first entry observes depth 0");
        assert_eq!(current_depth(), 1);
        {
            let (prev1, _g1) = ConditionalSubtypeDepthGuard::enter();
            assert_eq!(prev1, 1, "nested entry observes the outer depth");
            assert_eq!(current_depth(), 2);
        }
        assert_eq!(current_depth(), 1, "nested drop restores one level");
        drop(g0);
        assert_eq!(current_depth(), 0, "outer drop restores the clean slate");
    }

    /// #13368: the guard must restore the depth even when the guarded subtype
    /// walk unwinds via a panic a caller (`try_tsz`, LSP) catches, so a stale
    /// positive depth can never leak into the next compilation on a reused
    /// batch/merge-group worker thread (which would force later
    /// conditional-subtype checks onto the conservative false branch).
    #[test]
    fn depth_is_restored_on_unwind() {
        assert_eq!(current_depth(), 0, "counter starts clean");
        let result = std::panic::catch_unwind(|| {
            let (_prev, _guard) = ConditionalSubtypeDepthGuard::enter();
            assert_eq!(current_depth(), 1);
            panic!("simulated mid-subtype-walk panic");
        });
        assert!(result.is_err(), "the closure panicked");
        assert_eq!(
            current_depth(),
            0,
            "guard Drop must restore the depth during unwind"
        );
    }
}
