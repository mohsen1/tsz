use crate::query_boundaries::common::{self as common_query, TypeSubstitution};
use crate::query_boundaries::construct_signatures::{
    call_only_callable_type, call_signature_from_function_shape, instantiated_callable_from_base,
};
use crate::query_boundaries::state::type_resolution as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeList;
use tsz_solver::def::DefKind;
use tsz_solver::{CallSignature, TypeId};

impl<'a> CheckerState<'a> {
    /// Apply explicit type arguments to a callable type for function calls.
    ///
    /// When a function is called with explicit type arguments like `fn<T>(x: T)`,
    /// calling it as `fn<number>("hello")` should substitute `T` with `number` and
    /// then check if `"hello"` is assignable to `number`.
    ///
    /// This function creates a new callable type with the type parameters substituted,
    /// so that argument type checking can work correctly.
    pub(crate) fn apply_type_arguments_to_callable_type(
        &mut self,
        callee_type: TypeId,
        type_arguments: Option<&NodeList>,
    ) -> TypeId {
        let Some(type_arguments) = type_arguments else {
            return callee_type;
        };

        if type_arguments.nodes.is_empty() {
            return callee_type;
        }

        let mut type_args: Vec<TypeId> = Vec::with_capacity(type_arguments.nodes.len());
        for &arg_idx in &type_arguments.nodes {
            self.check_type_node_for_static_member_class_type_param_refs(arg_idx);
            type_args.push(self.get_type_from_type_node(arg_idx));
        }

        if type_args.is_empty() {
            return callee_type;
        }

        // Resolve Lazy types before classification.
        let callee_type = {
            let resolved = self.resolve_lazy_type(callee_type);
            if resolved != callee_type {
                resolved
            } else {
                callee_type
            }
        };
        let factory = self.ctx.types.factory();
        match query::classify_for_signatures(self.ctx.types, callee_type) {
            query::SignatureTypeKind::Intersection(members) => {
                let mut instantiated_members = Vec::with_capacity(members.len());
                let mut changed = false;
                for member in members {
                    let instantiated =
                        self.apply_type_arguments_to_callable_type(member, Some(type_arguments));
                    if instantiated != member {
                        changed = true;
                    }
                    instantiated_members.push(instantiated);
                }
                if changed {
                    factory.intersection(instantiated_members)
                } else {
                    callee_type
                }
            }
            query::SignatureTypeKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                let call_signatures = shape.call_signatures.clone();
                let construct_signatures = shape.construct_signatures.clone();

                // Find signatures that can accept the supplied explicit type
                // arguments. Exact arity for instantiation expressions is
                // checked before this path; ordinary calls may supply a prefix
                // when remaining type parameters have defaults or can infer.
                let matching_calls =
                    self.signatures_matching_explicit_type_args(&call_signatures, &type_args);
                let matching_constructs =
                    self.signatures_matching_explicit_type_args(&construct_signatures, &type_args);

                if matching_calls.is_empty() && matching_constructs.is_empty() {
                    return callee_type;
                }

                // Instantiate each matching signature with the type arguments.
                // When type arguments are partially supplied (fewer than type params),
                // fill in defaults that are fully determined (no remaining type param
                // references after substituting explicit args). Type parameters whose
                // defaults still reference other unsupplied params are left for the
                // solver to infer from call-site arguments.
                let mut instantiated_calls: Vec<tsz_solver::CallSignature> = matching_calls
                    .iter()
                    .map(|sig| self.instantiate_instantiation_expression_signature(sig, &type_args))
                    .collect();
                let mut instantiated_constructs: Vec<tsz_solver::CallSignature> =
                    matching_constructs
                        .iter()
                        .map(|sig| {
                            self.instantiate_instantiation_expression_signature(sig, &type_args)
                        })
                        .collect();
                // See `evaluate_substituted_signatures` for why.
                self.evaluate_substituted_signatures(&mut instantiated_calls);
                self.evaluate_substituted_signatures(&mut instantiated_constructs);

                instantiated_callable_from_base(
                    self.ctx.types,
                    &shape,
                    instantiated_calls,
                    instantiated_constructs,
                )
            }
            query::SignatureTypeKind::Function(shape_id) => {
                let shape = self.ctx.types.function_shape(shape_id);
                if type_args.len() > shape.type_params.len() {
                    return callee_type;
                }

                let sig = {
                    let mut sig =
                        call_signature_from_function_shape(shape.as_ref().clone(), shape.is_method);
                    // This path deliberately drops the `this` parameter: an
                    // instantiation expression yields a bare call signature.
                    sig.this_type = None;
                    sig
                };
                let instantiated_call = if type_args.len() < shape.type_params.len() {
                    if self.all_remaining_defaults_resolved(&sig, &type_args) {
                        // Defaults fully resolved; apply eagerly.
                        let mut args = type_args.clone();
                        for (param_index, param) in
                            sig.type_params.iter().enumerate().skip(args.len())
                        {
                            let fallback = param
                                .default
                                .or(param.constraint)
                                .unwrap_or(TypeId::UNKNOWN);
                            let substitution = TypeSubstitution::from_args(
                                self.ctx.types,
                                &sig.type_params[..param_index],
                                &args,
                            );
                            args.push(
                                crate::query_boundaries::common::instantiate_type_preserving_meta(
                                    self.ctx.types,
                                    fallback,
                                    &substitution,
                                ),
                            );
                        }
                        self.instantiate_signature(&sig, &args)
                    } else {
                        self.partially_instantiate_signature(&sig, &type_args)
                    }
                } else {
                    self.instantiate_signature(&sig, &type_args)
                };

                let mut instantiated_calls = vec![instantiated_call];
                // See `evaluate_substituted_signatures` for why.
                self.evaluate_substituted_signatures(&mut instantiated_calls);

                call_only_callable_type(self.ctx.types, instantiated_calls)
            }
            _ => callee_type,
        }
    }

    /// Evaluate the parameter, return, `this`, and predicate types of each
    /// instantiated signature in `sigs`. Used after applying type arguments to
    /// a generic callable so that mapped/conditional/index-access types in the
    /// substituted signature reduce to their structural form. Without this
    /// step, the substituted signature carries a deferred return type that
    /// downstream `ReturnType<typeof f<X>>` / `Parameters<...>` / `infer R`
    /// conditional patterns capture verbatim, and property access on the
    /// captured deferred return falls through to `any`.
    fn evaluate_substituted_signatures(&self, sigs: &mut [tsz_solver::CallSignature]) {
        use crate::query_boundaries::state::type_environment::evaluate_type_with_cache;
        let expand_aliases = self.ctx.is_declaration_file() || self.ctx.emit_declarations();
        let eval = |type_id: TypeId| -> TypeId {
            // Intrinsics (any/unknown/never/string/number/...) and ERROR are
            // already in canonical form — evaluating them allocates an
            // evaluator and produces the same TypeId. Skip the round-trip.
            if type_id.is_intrinsic() || type_id == TypeId::ERROR {
                return type_id;
            }
            let evaluated = evaluate_type_with_cache(
                self.ctx.types,
                &self.ctx,
                type_id,
                std::iter::empty(),
                false,
                expand_aliases,
                Some(self.ctx.types),
                /* authoritative */ true,
                crate::query_boundaries::state::type_environment::CacheEntryCollection::Skip,
            )
            .result;
            if evaluated == TypeId::ERROR {
                type_id
            } else {
                evaluated
            }
        };
        for sig in sigs {
            // Callers only pass signatures whose type parameters have been
            // consumed by substitution. Skip anything still generic to keep
            // partially-instantiated and uninstantiated signatures unchanged.
            if !sig.type_params.is_empty() {
                continue;
            }
            if self.substituted_return_can_stay_lazy_identity(sig.return_type) {
                continue;
            }
            // Only evaluate the return type. Params, `this`, and predicate
            // are intentionally left as substituted-but-not-evaluated:
            // contextual-typing and contravariant matching paths rely on
            // their declared shape, and forcing evaluation here regresses
            // unrelated conformance fixtures (interface declaration
            // inference, write-only-locals lint, circular-module
            // resolution).
            sig.return_type = eval(sig.return_type);
        }
    }

    fn substituted_return_can_stay_lazy_identity(&self, return_type: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        let mut saw_identity = false;
        if let Some(members) = common_query::union_members(db, return_type) {
            for member in members.iter() {
                if matches!(*member, TypeId::NULL | TypeId::UNDEFINED) {
                    continue;
                }
                if !self.type_is_lazy_class_or_interface_identity(*member) {
                    return false;
                }
                saw_identity = true;
            }
            return saw_identity;
        }
        self.type_is_lazy_class_or_interface_identity(return_type)
    }

    fn type_is_lazy_class_or_interface_identity(&self, type_id: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        let base = common_query::type_application(db, type_id)
            .map_or(type_id, |application| application.base);
        query::lazy_def_id(db, base).is_some_and(|def_id| {
            matches!(
                self.ctx.definition_store.get_kind(def_id),
                Some(DefKind::Class | DefKind::Interface)
            )
        })
    }

    fn instantiate_instantiation_expression_signature(
        &mut self,
        sig: &tsz_solver::CallSignature,
        type_args: &[TypeId],
    ) -> tsz_solver::CallSignature {
        let mut args = type_args.to_vec();
        if args.len() > sig.type_params.len() {
            args.truncate(sig.type_params.len());
        }
        if args.len() < sig.type_params.len() {
            if self.all_remaining_defaults_resolved(sig, &args) {
                for (param_index, param) in sig.type_params.iter().enumerate().skip(args.len()) {
                    let fallback = param
                        .default
                        .or(param.constraint)
                        .unwrap_or(TypeId::UNKNOWN);
                    let substitution = TypeSubstitution::from_args(
                        self.ctx.types,
                        &sig.type_params[..param_index],
                        &args,
                    );
                    args.push(
                        crate::query_boundaries::common::instantiate_type_preserving_meta(
                            self.ctx.types,
                            fallback,
                            &substitution,
                        ),
                    );
                }
                self.instantiate_signature(sig, &args)
            } else {
                self.partially_instantiate_signature(sig, &args)
            }
        } else {
            self.instantiate_signature(sig, &args)
        }
    }

    fn signatures_matching_explicit_type_args(
        &mut self,
        signatures: &[CallSignature],
        type_args: &[TypeId],
    ) -> Vec<CallSignature> {
        let arity_matches: Vec<CallSignature> = signatures
            .iter()
            .filter(|sig| sig.type_params.len() >= type_args.len())
            .cloned()
            .collect();
        let constraint_matches: Vec<CallSignature> = arity_matches
            .iter()
            .filter(|sig| {
                !self.explicit_type_args_definitely_violate_signature_constraints(sig, type_args)
            })
            .cloned()
            .collect();

        if constraint_matches.is_empty() {
            arity_matches
        } else {
            constraint_matches
        }
    }

    fn explicit_type_args_definitely_violate_signature_constraints(
        &mut self,
        sig: &CallSignature,
        type_args: &[TypeId],
    ) -> bool {
        for (param, &type_arg) in sig.type_params.iter().zip(type_args.iter()) {
            let Some(constraint) = param.constraint else {
                continue;
            };
            if matches!(type_arg, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR) {
                continue;
            }

            let substitution =
                TypeSubstitution::from_args(self.ctx.types, &sig.type_params, type_args);
            let constraint = crate::query_boundaries::common::instantiate_type_preserving_meta(
                self.ctx.types,
                constraint,
                &substitution,
            );
            let constraint = self.resolve_lazy_type(constraint);
            if query::contains_type_parameters(self.ctx.types.as_type_database(), constraint) {
                continue;
            }
            let constraint = self.evaluate_type_for_assignability(constraint);
            let db = self.ctx.types.as_type_database();
            if !common_query::is_literal_or_primitive_or_compound_of_those(db, constraint) {
                continue;
            }

            if common_query::contains_type_parameters(db, type_arg)
                || common_query::enum_def_id(db, type_arg).is_some()
            {
                continue;
            }

            if !common_query::is_literal_or_primitive_or_compound_of_those(db, type_arg) {
                if query::lazy_def_id(db, type_arg).is_some_and(|def_id| {
                    matches!(
                        self.ctx.definition_store.get_kind(def_id),
                        Some(DefKind::Class | DefKind::Interface)
                    )
                }) || common_query::is_object_like_type(db, type_arg)
                {
                    return true;
                }
                continue;
            }

            let outcome = self.type_arg_constraint_no_weak_relation_outcome(type_arg, constraint);
            if !outcome.related && !outcome.depth_exceeded && !outcome.iteration_exceeded {
                return true;
            }
        }
        false
    }
}
