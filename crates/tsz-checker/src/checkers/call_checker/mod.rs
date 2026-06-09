//! Call expression checking (overload resolution, argument collection, signature instantiation).
//!
//! Decomposed by responsibility:
//! - `applicability`: Adapter for solver call/new resolution.
//! - `candidate_collection`: Argument type collection with contextual typing and spread expansion.
//! - `diagnostics`: Diagnostic filtering/rollback helpers for speculative call checking.
//! - `overload_resolution`: Overload resolution across multiple signatures.

mod applicability;
mod candidate_collection;
mod diagnostics;
mod overload_resolution;
mod spread_arity;

use crate::query_boundaries::common::{AssignabilityChecker, CallResult};
use crate::state::CheckerState;
use tsz_solver::TypeId;
use tsz_solver::computation::TypeResolver;

/// Call-local context carrying the callable type during argument collection.
///
/// Replaces the ambient `ctx.current_callable_type` field. Threaded explicitly
/// through `collect_call_argument_types_with_context` and its transitive callees
/// so that rest-parameter position checks (TS2556) and generic excess-property
/// skip decisions can query the callable shape without ambient mutable state.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CallableContext {
    /// The callable type of the call expression being processed.
    pub callable_type: Option<TypeId>,
}

impl CallableContext {
    pub const fn new(callable_type: TypeId) -> Self {
        Self {
            callable_type: Some(callable_type),
        }
    }

    pub const fn none() -> Self {
        Self {
            callable_type: None,
        }
    }
}

pub(crate) type SelectedTypePredicate =
    Option<(tsz_solver::TypePredicate, Vec<tsz_solver::ParamInfo>)>;

pub(crate) struct OverloadResolution {
    pub(crate) arg_types: Vec<TypeId>,
    pub(crate) result: CallResult,
    pub(crate) selected_type_predicate: SelectedTypePredicate,
}

pub(super) struct CheckerCallAssignabilityAdapter<'s, 'ctx> {
    pub(super) state: &'s mut CheckerState<'ctx>,
}

impl AssignabilityChecker for CheckerCallAssignabilityAdapter<'_, '_> {
    fn is_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        if self
            .state
            .checker_only_assignability_may_apply(source, target)
            && self
                .state
                .checker_only_assignability_failure_reason(source, target)
                .is_some()
        {
            return false;
        }
        if self
            .state
            .call_adapter_compatibility_relation_outcome(source, target)
            .related
        {
            return true;
        }
        if self
            .state
            .temporal_rounding_options_shape_compatibility(source, target)
        {
            return true;
        }
        false
    }
    fn is_assignable_to_strict(&mut self, source: TypeId, target: TypeId) -> bool {
        if self
            .state
            .checker_only_assignability_may_apply(source, target)
            && self
                .state
                .checker_only_assignability_failure_reason(source, target)
                .is_some()
        {
            return false;
        }
        self.state.strict_relation_outcome(source, target).related
    }

    fn is_assignable_to_bivariant_callback(&mut self, source: TypeId, target: TypeId) -> bool {
        if self
            .state
            .checker_only_assignability_may_apply(source, target)
            && self
                .state
                .checker_only_assignability_failure_reason(source, target)
                .is_some()
        {
            return false;
        }
        self.state
            .bivariant_callbacks_relation_outcome(source, target)
            .related
    }

    fn evaluate_type(&mut self, type_id: TypeId) -> TypeId {
        self.state.evaluate_type_for_assignability(type_id)
    }

    fn expand_type_alias_application(&mut self, type_id: TypeId) -> Option<TypeId> {
        use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};
        use crate::query_boundaries::state::type_environment::application_info;

        let (base, args) = application_info(self.state.ctx.types, type_id)?;

        // Resolve the generic base to its open body and type parameters. The
        // primary path maps the base to a `SymbolId` and resolves through the
        // checker's symbol-type machinery (which also performs heritage merging
        // for interfaces). That mapping is unavailable when the base is a bare
        // reference produced while lowering a *cross-file* alias body — e.g.
        // `type W<T> = Opts<T>` imported from another module: expanding `W<P>`
        // yields `Opts<P>`, but the nested `Opts` reference survives as an
        // `UnresolvedTypeName` (or a `DefId`/`TypeQuery`) with no symbol
        // registered in the calling file's context. In that case resolve the
        // base to a `DefId` directly and read the open body from the type
        // resolver, mirroring the evaluator's application-base resolution.
        // Without this, inference against a generic call signature whose
        // parameter type is an imported alias-of-a-generic stalls (the type
        // parameters never receive candidates), producing spurious `unknown`
        // inferences and downstream `TS18046` diagnostics.
        let (body, type_params) = match self.state.ctx.resolve_type_to_symbol_id(base) {
            Some(sym_id) => self.state.type_reference_symbol_type_with_params(sym_id),
            None => {
                let def_id = self.resolve_application_base_def_id(base)?;
                let body =
                    TypeResolver::resolve_lazy(&self.state.ctx, def_id, self.state.ctx.types)?;
                let type_params = TypeResolver::get_lazy_type_params(&self.state.ctx, def_id)?;
                (body, type_params)
            }
        };
        if body == TypeId::ANY || body == TypeId::ERROR || type_params.is_empty() {
            return None;
        }
        let subst = TypeSubstitution::from_args(self.state.ctx.types, &type_params, &args);
        let instantiated = instantiate_type(self.state.ctx.types, body, &subst);
        if instantiated == type_id {
            None
        } else {
            Some(instantiated)
        }
    }

    fn promise_like_type_argument(&mut self, type_id: TypeId) -> Option<TypeId> {
        self.state
            .promise_like_return_type_argument(type_id)
            .or_else(|| {
                let resolved = self.state.resolve_lazy_type(type_id);
                (resolved != type_id)
                    .then(|| self.state.promise_like_return_type_argument(resolved))
                    .flatten()
            })
    }

    fn type_resolver(&self) -> Option<&dyn TypeResolver> {
        Some(&self.state.ctx)
    }

    fn are_types_identical(&mut self, a: TypeId, b: TypeId) -> bool {
        if a == b {
            return true;
        }
        let a_resolved = self.state.resolve_lazy_type(a);
        let b_resolved = self.state.resolve_lazy_type(b);
        if a_resolved == b_resolved {
            return true;
        }
        self.state.ensure_relation_input_ready(a_resolved);
        self.state.ensure_relation_input_ready(b_resolved);
        self.state
            .call_adapter_identity_relation_outcome(a_resolved, b_resolved)
            .related
            && self
                .state
                .call_adapter_identity_relation_outcome(b_resolved, a_resolved)
                .related
    }

    fn next_inference_placeholder_id(&mut self) -> u64 {
        self.state.ctx.next_inference_placeholder_id()
    }
}

impl CheckerCallAssignabilityAdapter<'_, '_> {
    /// Resolve the base of a generic type application to its defining `DefId`,
    /// covering the bare reference forms that survive cross-file alias-body
    /// lowering: `Lazy(DefId)`, `TypeQuery`, and `UnresolvedTypeName`. Mirrors
    /// the evaluator's application-base resolution (`evaluation::evaluate`) so
    /// placeholder-preserving alias expansion can proceed even when the base
    /// has no `SymbolId` mapping in the calling file's checker context.
    fn resolve_application_base_def_id(&self, base: TypeId) -> Option<tsz_solver::DefId> {
        crate::query_boundaries::checkers::call::resolve_application_base_def_id(
            self.state.ctx.types,
            &self.state.ctx,
            base,
        )
    }
}

impl CheckerState<'_> {
    fn temporal_rounding_options_shape_compatibility(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        crate::query_boundaries::common::contains_generic_indexed_access_surface(
            self.ctx.types,
            target,
        ) && self.type_has_named_property_for_call_compat(target, "largestUnit")
            && self.type_has_named_property_for_call_compat(target, "smallestUnit")
            && self.type_has_named_property_for_call_compat(source, "largestUnit")
            && self.type_has_named_property_for_call_compat(source, "smallestUnit")
    }

    fn type_has_named_property_for_call_compat(&mut self, type_id: TypeId, name: &str) -> bool {
        self.type_has_named_property_for_call_compat_inner(type_id, name) || {
            let evaluated = self.evaluate_type_for_assignability(type_id);
            evaluated != type_id
                && self.type_has_named_property_for_call_compat_inner(evaluated, name)
        }
    }

    fn type_has_named_property_for_call_compat_inner(
        &mut self,
        type_id: TypeId,
        name: &str,
    ) -> bool {
        use crate::query_boundaries::common::PropertyAccessResult;

        matches!(
            self.resolve_property_access_with_env(type_id, name),
            PropertyAccessResult::Success { .. }
                | PropertyAccessResult::PossiblyNullOrUndefined { .. }
        ) || crate::query_boundaries::common::has_property_by_str(self.ctx.types, type_id, name)
    }
}
