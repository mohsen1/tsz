//! Call expression checking (overload resolution, argument collection, signature instantiation).
//!
//! Decomposed by responsibility:
//! - `applicability`: Adapter for solver call/new resolution.
//! - `candidate_collection`: Argument type collection with contextual typing and spread expansion.
//! - `diagnostics`: Diagnostic filtering/rollback helpers for speculative call checking.
//! - `overload_resolution`: Overload resolution across multiple signatures.

pub(crate) mod applicability;
mod candidate_collection;
mod diagnostics;
mod non_tuple_spread_signature;
mod overload_resolution;
mod spread_arity;
mod spread_overload_selection;

use crate::query_boundaries::common::TypeResolver;
use crate::query_boundaries::common::{AssignabilityChecker, CallResult};
use crate::state::CheckerState;
use tsz_solver::TypeId;

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
    /// Whether [`CallableContext::callable_type`] is the synthetic
    /// `sigA | sigB | ...` union built for the overload-resolution first pass.
    ///
    /// That union is an *existential* alternative set: a call type-checks when
    /// *some* member admits it, not all. The non-tuple-spread arity check
    /// (TS2556) must therefore treat the union existentially here — a non-tuple
    /// spread lands on a rest position as long as *any* overload has a rest
    /// parameter at that slot. Without this, `Array.prototype.splice`'s
    /// `(start, deleteCount?)` overload (no rest) would poison a valid spread
    /// into the `(start, deleteCount, ...items)` overload's rest. A genuine
    /// union-typed *value* (not an overload set) keeps the conjunctive
    /// semantics, since calling it must satisfy every member.
    pub union_is_overload_set: bool,
}

impl CallableContext {
    pub const fn new(callable_type: TypeId) -> Self {
        Self {
            callable_type: Some(callable_type),
            union_is_overload_set: false,
        }
    }

    pub const fn none() -> Self {
        Self {
            callable_type: None,
            union_is_overload_set: false,
        }
    }

    /// As [`CallableContext::new`], but marking `callable_type` as the synthetic
    /// union of overload signatures (see
    /// [`CallableContext::union_is_overload_set`]).
    pub const fn new_overload_union(callable_type: TypeId) -> Self {
        Self {
            callable_type: Some(callable_type),
            union_is_overload_set: true,
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
    /// Overload-resolution subtype pass (tsc `chooseOverload` with
    /// `subtypeRelation`): when set, the three relation probes below route
    /// through the subtype-pass relation entries where an `any` source is
    /// not related to concrete targets at every nesting level. Non-relation
    /// adapter methods are unaffected.
    pub(super) overload_subtype_pass: bool,
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
        if self.overload_subtype_pass {
            if self
                .state
                .overload_subtype_pass_compatibility_relation_outcome(source, target)
                .related
            {
                return true;
            }
        } else if self
            .state
            .call_adapter_compatibility_relation_outcome(source, target)
            .related
        {
            return true;
        }
        if self
            .state
            .generic_indexed_options_shape_compatibility(source, target)
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
        if self.overload_subtype_pass {
            return self
                .state
                .overload_subtype_pass_strict_relation_outcome(source, target)
                .related;
        }
        self.state.strict_relation_outcome(source, target).related
    }

    fn is_assignable_to_provisional_rest_union(&mut self, source: TypeId, target: TypeId) -> bool {
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
        self.state.is_assignable_to_provisional_rest_union(
            source,
            target,
            self.overload_subtype_pass,
        )
    }

    fn is_assignable_to_generic_call(
        &mut self,
        source: TypeId,
        target: TypeId,
        strict: bool,
        provisional_rest_union: bool,
    ) -> bool {
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
        self.state.is_assignable_to_generic_call_raw(
            source,
            target,
            self.overload_subtype_pass,
            strict,
            provisional_rest_union,
        )
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
        if self.overload_subtype_pass {
            return self
                .state
                .overload_subtype_pass_bivariant_relation_outcome(source, target)
                .related;
        }
        self.state
            .bivariant_callbacks_relation_outcome(source, target)
            .related
    }

    fn evaluate_type(&mut self, type_id: TypeId) -> TypeId {
        self.state.evaluate_type_for_assignability(type_id)
    }

    fn evaluate_type_for_return_context_substitution(&mut self, type_id: TypeId) -> TypeId {
        if crate::query_boundaries::state::type_environment::application_info(
            self.state.ctx.types,
            type_id,
        )
        .is_some()
        {
            crate::query_boundaries::state::type_environment::evaluate_type_with_resolver(
                self.state.ctx.types,
                &self.state.ctx,
                type_id,
            )
        } else {
            self.state.evaluate_type_with_env(type_id)
        }
    }

    fn expand_type_alias_application(&mut self, type_id: TypeId) -> Option<TypeId> {
        use crate::query_boundaries::common::{TypeSubstitution, instantiate_type_preserving_meta};
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
        let instantiated = instantiate_type_preserving_meta(self.state.ctx.types, body, &subst);
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

    fn normalize_inferred_type(&mut self, type_id: TypeId) -> TypeId {
        self.normalize_lazy_object_identity_intersection(type_id)
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

    fn normalize_lazy_object_identity_intersection(&mut self, type_id: TypeId) -> TypeId {
        let Some(members) =
            crate::query_boundaries::common::intersection_members(self.state.ctx.types, type_id)
        else {
            return type_id;
        };

        let lazy_members: Vec<_> = members
            .iter()
            .copied()
            .filter_map(|member| {
                crate::query_boundaries::common::lazy_def_id(self.state.ctx.types, member)
                    .map(|def_id| (member, def_id))
            })
            .collect();
        if lazy_members.is_empty() {
            return type_id;
        }

        let mut changed = false;
        let retained: Vec<_> = members
            .iter()
            .copied()
            .filter(|&member| {
                let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
                    self.state.ctx.types,
                    member,
                ) else {
                    return true;
                };
                let Some(object_symbol) = shape.symbol else {
                    return true;
                };
                let is_duplicate = lazy_members
                    .iter()
                    .any(|&(_, def_id)| self.object_symbol_matches_lazy_def(object_symbol, def_id));
                changed |= is_duplicate;
                !is_duplicate
            })
            .collect();

        if changed {
            crate::query_boundaries::common::intersection_or_single(self.state.ctx.types, retained)
        } else {
            type_id
        }
    }

    fn object_symbol_matches_lazy_def(
        &self,
        object_symbol: tsz_binder::SymbolId,
        def_id: tsz_solver::DefId,
    ) -> bool {
        let Some((def_symbol, def_file_idx)) = self.state.ctx.def_symbol_identity(def_id) else {
            return false;
        };
        if object_symbol != def_symbol {
            return false;
        }

        let object_file_idx = self.state.ctx.resolve_symbol_file_index(object_symbol);
        match (def_file_idx, object_file_idx) {
            (Some(def_file_idx), Some(object_file_idx)) => def_file_idx == object_file_idx,
            (Some(def_file_idx), None) => def_file_idx == self.state.ctx.current_file_idx,
            (None, Some(_)) => false,
            (None, None) => true,
        }
    }
}

impl CheckerState<'_> {
    fn generic_indexed_options_shape_compatibility(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if !crate::query_boundaries::checkers::call::contains_generic_indexed_access_surface_for_call(
            self.ctx.types,
            target,
        ) {
            return false;
        }

        let Some(source_shape) = self.object_shape_for_call_compat(source) else {
            return false;
        };
        if source_shape.properties.is_empty() {
            return false;
        }

        let source_names: Vec<_> = source_shape
            .properties
            .iter()
            .map(|prop| self.ctx.types.resolve_atom(prop.name))
            .collect();
        source_names
            .iter()
            .all(|name| self.type_has_named_property_for_call_compat(target, name.as_ref()))
    }

    fn object_shape_for_call_compat(
        &mut self,
        type_id: TypeId,
    ) -> Option<std::sync::Arc<tsz_solver::ObjectShape>> {
        if let Some(shape) =
            crate::query_boundaries::checkers::call::object_shape_for_call(self.ctx.types, type_id)
        {
            return Some(shape);
        }

        let evaluated = self.evaluate_type_for_assignability(type_id);
        if evaluated == type_id {
            return None;
        }

        crate::query_boundaries::checkers::call::object_shape_for_call(self.ctx.types, evaluated)
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
        let access = self.resolve_property_access_with_env(type_id, name);
        crate::query_boundaries::checkers::call::property_access_is_present_for_call(&access)
            || crate::query_boundaries::checkers::call::has_property_by_str_for_call(
                self.ctx.types,
                type_id,
                name,
            )
    }
}
