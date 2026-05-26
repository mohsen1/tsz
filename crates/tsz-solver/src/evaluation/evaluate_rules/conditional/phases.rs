//! Named phases for conditional type evaluation.

use crate::instantiation::instantiate::instantiate_generic;
use crate::relations::subtype::TypeResolver;
use crate::types::{ConditionalType, TypeData, TypeId};
use tracing::trace;

use super::super::super::evaluate::TypeEvaluator;

/// Resolved and pre-computed operands for one conditional evaluation step.
pub(super) struct ConditionalOperands {
    pub(super) check_type: TypeId,
    pub(super) extends_type: TypeId,
    pub(super) extends_has_infer: bool,
    pub(super) extends_has_type_params: bool,
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
        let extends_type = self.normalize_conditional_object_operand(evaluated_extends);
        if matches!(
            self.interner().lookup(check_type),
            Some(TypeData::Application(_))
        ) && let Some(expanded_check) =
            self.try_expand_application_for_conditional_check(check_type)
        {
            check_type = expanded_check;
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
                let args = app.args.clone();
                let expanded_args = self.expand_type_args(&args);
                let instantiated = instantiate_generic(
                    self.interner(),
                    resolved_base,
                    &type_params,
                    &expanded_args,
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

        // Thread-local depth guard: evaluating conditional types can trigger
        // subtype checks that evaluate MORE conditional types, creating an
        // Evaluator -> SubtypeChecker -> Evaluator -> ... chain where each
        // instance has fresh cycle-detection state. Without this global
        // depth limit, recursive generic types like `Vector<T> implements
        // Seq<T>` with `Exclude<T, U>` in overloads cause stack overflow.
        thread_local! {
            static CONDITIONAL_SUBTYPE_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
        }
        let prev_depth = CONDITIONAL_SUBTYPE_DEPTH.with(|d| {
            let c = d.get();
            d.set(c + 1);
            c
        });
        let result = if prev_depth >= 50 {
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
        } else {
            let mut strict_checker = self.conditional_subtype_checker();
            strict_checker.is_subtype_of(check_type, extends_type)
        };
        CONDITIONAL_SUBTYPE_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        self.cache_conditional_subtype(check_type, extends_type, result);
        result
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
