use crate::query_boundaries::state::type_resolution as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeList;
use tsz_solver::{CallableShape, TypeId};

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

                // Find signatures that can accept the supplied explicit type
                // arguments. Exact arity for instantiation expressions is
                // checked before this path; ordinary calls may supply a prefix
                // when remaining type parameters have defaults or can infer.
                let matching_calls: Vec<tsz_solver::CallSignature> = shape
                    .call_signatures
                    .iter()
                    .filter(|&sig| sig.type_params.len() >= type_args.len())
                    .cloned()
                    .collect();
                let matching_constructs: Vec<tsz_solver::CallSignature> = shape
                    .construct_signatures
                    .iter()
                    .filter(|&sig| sig.type_params.len() >= type_args.len())
                    .cloned()
                    .collect();

                if matching_calls.is_empty() && matching_constructs.is_empty() {
                    return callee_type;
                }

                // Instantiate each matching signature with the type arguments.
                // When type arguments are partially supplied (fewer than type params),
                // fill in defaults that are fully determined (no remaining type param
                // references after substituting explicit args). Type parameters whose
                // defaults still reference other unsupplied params are left for the
                // solver to infer from call-site arguments.
                let instantiated_calls: Vec<tsz_solver::CallSignature> = matching_calls
                    .iter()
                    .map(|sig| self.instantiate_instantiation_expression_signature(sig, &type_args))
                    .collect();
                let instantiated_constructs: Vec<tsz_solver::CallSignature> = matching_constructs
                    .iter()
                    .map(|sig| self.instantiate_instantiation_expression_signature(sig, &type_args))
                    .collect();

                let new_shape = CallableShape {
                    call_signatures: instantiated_calls,
                    construct_signatures: instantiated_constructs,
                    properties: shape.properties.clone(),
                    string_index: shape.string_index,
                    number_index: shape.number_index,
                    symbol: None,
                    is_abstract: false,
                };
                factory.callable(new_shape)
            }
            query::SignatureTypeKind::Function(shape_id) => {
                let shape = self.ctx.types.function_shape(shape_id);
                if type_args.len() > shape.type_params.len() {
                    return callee_type;
                }

                let sig = tsz_solver::CallSignature {
                    type_params: shape.type_params.clone(),
                    params: shape.params.clone(),
                    this_type: None,
                    return_type: shape.return_type,
                    type_predicate: shape.type_predicate,
                    is_method: shape.is_method,
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
                            let substitution = tsz_solver::TypeSubstitution::from_args(
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

                let new_shape = CallableShape {
                    call_signatures: vec![instantiated_call],
                    construct_signatures: vec![],
                    properties: vec![],
                    string_index: None,
                    number_index: None,
                    symbol: None,
                    is_abstract: false,
                };
                factory.callable(new_shape)
            }
            _ => callee_type,
        }
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
                    let substitution = tsz_solver::TypeSubstitution::from_args(
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
}
