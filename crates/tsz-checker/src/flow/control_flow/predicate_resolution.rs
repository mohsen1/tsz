//! Generic type-predicate resolution: instantiate a guard's predicate target
//! from the call's actual argument types.
//!
//! Split out of `narrowing.rs` to keep that module under the source-size cap.
//! These methods own the checker-side orchestration of predicate-target
//! instantiation; the underlying type inference is answered through the
//! `flow_analysis` query boundary so the semantic operation stays solver-owned.

use tsz_common::interner::Atom;
use tsz_parser::parser::node::CallExprData;
use tsz_solver::{ParamInfo, TypeId, TypePredicate};

use super::FlowAnalyzer;
use crate::query_boundaries::flow as flow_boundary;
use crate::query_boundaries::flow_analysis::{self as flow_query, union_members_for_type};

impl<'a> FlowAnalyzer<'a> {
    /// Resolve a generic assertion predicate's type from the call's actual argument types.
    ///
    /// For `assertEqual<T>(value: any, type: T): asserts value is T` called as
    /// `assertEqual(animal.type, 'cat' as const)`, the predicate's `type_id` is the
    /// unresolved type parameter T. This method finds which parameter shares that type
    /// and resolves it to the corresponding argument's concrete type (e.g., literal 'cat').
    pub(crate) fn resolve_generic_predicate(
        &self,
        predicate: &TypePredicate,
        params: &[ParamInfo],
        call: &CallExprData,
        callee_type: TypeId,
        node_types: &crate::context::NodeTypeCache,
    ) -> TypePredicate {
        let Some(pred_type) = predicate.type_id else {
            return *predicate;
        };

        // Extract type params from the predicate signature via solver query
        let type_params = flow_query::extract_predicate_signature(self.interner, callee_type)
            .map(|sig| sig.type_params)
            .unwrap_or_default();
        if type_params.is_empty() {
            return *predicate;
        }

        let args = match call.arguments.as_ref() {
            Some(args) => args.nodes.as_slice(),
            None => return *predicate,
        };

        // Case 1: Direct match — predicate type IS a type parameter (e.g., `x is T`)
        for (i, param) in params.iter().enumerate() {
            if param.type_id == pred_type
                && let Some(&arg_idx) = args.get(i)
                && let Some(&arg_type) = node_types.get(&arg_idx.0)
            {
                return TypePredicate {
                    type_id: Some(arg_type),
                    ..*predicate
                };
            }
        }

        // Case 1b: Predicate type is a type parameter T, but the parameter type is a
        // union containing T (e.g., `isSuccess<T>(result: Result<T>): result is T`
        // where `type Result<T> = T | "FAILURE"`).
        // Infer T by subtracting the non-T union members from the argument type.
        // The parameter type may be a type alias (Lazy/Application) that needs
        // evaluation to expose the underlying union.
        let pred_is_type_param = type_params.iter().any(|tp| {
            flow_query::type_param_info(self.interner, pred_type)
                .is_some_and(|info| info.name == tp.name)
        });
        if pred_is_type_param {
            for (i, param) in params.iter().enumerate() {
                // Evaluate the parameter type in case it's a type alias like Result<T>.
                // Use the flow query boundary to expand type applications
                // (e.g., Result<T> -> T | "FAILURE").
                let evaluated_param = if let Some(env) = &self.type_environment {
                    let env_borrow = env.borrow();
                    flow_query::evaluate_application_type(self.interner, &env_borrow, param.type_id)
                } else {
                    flow_query::evaluate_type_structure(self.interner, param.type_id)
                };
                if let Some(param_members) = union_members_for_type(self.interner, evaluated_param)
                    && param_members.contains(&pred_type)
                    && let Some(&arg_idx) = args.get(i)
                    && let Some(&arg_type) = node_types.get(&arg_idx.0)
                {
                    let concrete_members: Vec<TypeId> = param_members
                        .iter()
                        .filter(|&&m| m != pred_type)
                        .copied()
                        .collect();
                    if concrete_members.iter().any(|&m| {
                        let evaluated = flow_query::evaluate_type_structure(self.interner, m);
                        flow_query::contains_type_parameters(self.interner, m)
                            || flow_query::contains_type_parameters(self.interner, evaluated)
                            || crate::query_boundaries::state::checking::object_shape(
                                self.interner,
                                evaluated,
                            )
                            .is_some()
                    }) {
                        continue;
                    }
                    let inferred_t = if let Some(arg_members) =
                        union_members_for_type(self.interner, arg_type)
                    {
                        let remaining: Vec<TypeId> = arg_members
                            .iter()
                            .filter(|&&m| !concrete_members.contains(&m))
                            .copied()
                            .collect();
                        match remaining.len() {
                            0 => arg_type,
                            1 => remaining[0],
                            _ => self.interner.factory().union(remaining),
                        }
                    } else {
                        arg_type
                    };
                    return TypePredicate {
                        type_id: Some(inferred_t),
                        ..*predicate
                    };
                }
            }
        }

        // Case 2: Complex predicate type CONTAINS type parameters (e.g., mapped types
        // like `target is { readonly [K in P]: unknown }`). Build a substitution from
        // function type params to call argument types and instantiate the predicate type.
        let mut substitution = flow_query::TypeSubstitution::new();
        for tp in &type_params {
            for (i, param) in params.iter().enumerate() {
                if let Some(info) = flow_query::type_param_info(self.interner, param.type_id)
                    && info.name == tp.name
                {
                    if let Some(&arg_idx) = args.get(i)
                        && let Some(&arg_type) = node_types.get(&arg_idx.0)
                    {
                        substitution.insert(tp.name, arg_type);
                    }
                    break;
                }
            }
        }

        // Case 2b: Infer remaining type params from callback function predicates.
        // For a parameter like `predicate: (arg: unknown) => arg is ValueT`, if the
        // argument is `isB: (arg: unknown) => arg is 'B'`, infer ValueT = 'B' by
        // matching the type predicates of the parameter type and argument type.
        for tp in &type_params {
            if substitution.get(tp.name).is_some() {
                continue; // Already substituted
            }
            for (i, param) in params.iter().enumerate() {
                // Check if the parameter type is a function with a type predicate
                // containing the unsubstituted type param
                if let Some(param_fn_shape) =
                    flow_query::function_shape_for_type(self.interner, param.type_id)
                    && let Some(ref param_pred) = param_fn_shape.type_predicate
                    && let Some(param_pred_type) = param_pred.type_id
                    && flow_query::type_param_info(self.interner, param_pred_type)
                        .is_some_and(|info| info.name == tp.name)
                {
                    // The param's predicate type IS this type param (e.g., `arg is ValueT`).
                    // Now check if the argument is also a function with a type predicate.
                    if let Some(&arg_idx) = args.get(i)
                        && let Some(&arg_type) = node_types.get(&arg_idx.0)
                        && let Some(arg_fn_shape) =
                            flow_query::function_shape_for_type(self.interner, arg_type)
                        && let Some(ref arg_pred) = arg_fn_shape.type_predicate
                        && let Some(arg_pred_type) = arg_pred.type_id
                    {
                        // Infer: ValueT = arg's predicate type (e.g., 'B')
                        substitution.insert(tp.name, arg_pred_type);
                        break;
                    }
                }
            }
        }

        // Case 2c: The predicate target is a bare type parameter whose corresponding
        // parameter type is a *union* containing that type parameter (a wrapper such
        // as `Result<T>` rather than just `T`). Case 1 failed because
        // `param.type_id != pred_type`, and Case 2 failed because `param.type_id` is
        // not directly a `TypeParameter`.
        //
        // Handle the common pattern where the parameter type is a union containing
        // the type parameter:
        //   `function isSuccess<T>(result: T | "FAILURE"): result is T`
        //   param type = T | "FAILURE", arg type = number | "FAILURE"
        //   → infer T = number (subtract fixed union members from arg type)
        //
        // This union-subtraction path must run *before* the general structural
        // inference below and keep ownership of the bare-union case: structural
        // inference would bind `T` to the whole argument union (`number | "FAILURE"`)
        // rather than the subtracted member (`number`), which would also leave the
        // false branch narrowing wrong.
        if let Some(pred_param_info) = flow_query::type_param_info(self.interner, pred_type) {
            let pred_param_name = pred_param_info.name;
            if type_params.iter().any(|tp| tp.name == pred_param_name) {
                for (i, param) in params.iter().enumerate() {
                    if let Some(&arg_idx) = args.get(i)
                        && let Some(&arg_type) = node_types.get(&arg_idx.0)
                        && let Some(inferred) = self.infer_type_param_from_union(
                            param.type_id,
                            arg_type,
                            pred_param_name,
                        )
                    {
                        return TypePredicate {
                            type_id: Some(inferred),
                            ..*predicate
                        };
                    }
                }
            }
        }

        // Case 2d: Structural inference for a type parameter that none of the earlier
        // cases could bind. The preceding cases bind a type parameter when it appears
        // *directly* as a parameter type (`value: T`), as a bare member of a union
        // parameter (`T | "x"`, handled by Cases 1b/2c above), or as a callback
        // predicate target.
        //
        // The remaining gap is a type parameter mentioned only from a *nested*
        // parameter position. Two shapes reach here:
        //   * the predicate target is a *compound* type mentioning the type parameter
        //     — a generic application or wrapper such as `AsyncIterable<T>`, `Box<T>`,
        //     `Promise<T>`, or `Map<K, V>`, often via an alias like
        //     `MaybeAsync<T> = T | AsyncIterable<T>`; or
        //   * the predicate target is a *bare* type parameter (`value is T`) whose `T`
        //     is inferable only from another parameter's nested shape
        //     (`witness: T[]`, `() => T`, a struct field).
        // None of the shortcuts fire for these, so the predicate would otherwise be
        // left generic and narrowing would intersect the value with the raw type
        // parameter. The union-subtraction Case 2c already returned for bare targets
        // reachable through a union parameter, so it does not preempt this fallback.
        //
        // Recover the bindings the same way call resolution does: structurally match
        // each declared parameter type against its argument type. This is a solver
        // concern, so it is answered through the inference query boundary rather than
        // re-implemented here.
        let predicate_still_generic = type_params.iter().any(|tp| {
            substitution.get(tp.name).is_none()
                && flow_query::contains_type_parameter_named(self.interner, pred_type, tp.name)
        });
        if predicate_still_generic {
            let param_arg_pairs: Vec<(TypeId, TypeId)> = params
                .iter()
                .enumerate()
                .filter_map(|(i, param)| {
                    let &arg_idx = args.get(i)?;
                    let &arg_type = node_types.get(&arg_idx.0)?;
                    Some((param.type_id, arg_type))
                })
                .collect();
            for (name, inferred) in flow_query::infer_type_arguments_from_param_args(
                self.interner,
                &type_params,
                &param_arg_pairs,
            ) {
                if substitution.get(name).is_none() {
                    substitution.insert(name, inferred);
                }
            }
        }

        if !substitution.is_empty() {
            let instantiated =
                flow_query::instantiate_type(self.interner, pred_type, &substitution);
            if instantiated != pred_type {
                // Evaluate to resolve mapped types (e.g., `{ [K in "length"]: unknown }` -> `{ length: unknown }`)
                let evaluated = flow_query::evaluate_type_structure(self.interner, instantiated);
                return TypePredicate {
                    type_id: Some(evaluated),
                    ..*predicate
                };
            }
        }

        *predicate
    }

    /// Attempt to infer a type parameter from a union-typed parameter.
    ///
    /// When a parameter type is a union like `T | "FAILURE"` and the argument type
    /// is `number | "FAILURE"`, we can infer T = number by subtracting the fixed
    /// (non-type-parameter) union members from the argument type.
    ///
    /// Returns `Some(inferred_type)` if inference succeeds, `None` otherwise.
    pub(crate) fn infer_type_param_from_union(
        &self,
        param_type: TypeId,
        arg_type: TypeId,
        target_param_name: Atom,
    ) -> Option<TypeId> {
        // Evaluate/expand the parameter type to get its structural form.
        // Type aliases like `Result<T>` may be represented as Application types
        // that need expansion to reveal the underlying union `T | "FAILURE"`.
        // Use the type environment's resolver for Application types.
        let expanded_param = if let Some(env_ref) = &self.type_environment {
            let env = env_ref.borrow();
            let result = flow_query::evaluate_application_type(self.interner, &env, param_type);
            if result == param_type {
                flow_query::evaluate_type_structure(self.interner, param_type)
            } else {
                result
            }
        } else {
            flow_query::evaluate_type_structure(self.interner, param_type)
        };
        // Get union members of the (expanded) parameter type
        let param_members = union_members_for_type(self.interner, expanded_param)?;

        // Check if any member is the target type parameter
        let is_target_param = |m: TypeId| -> bool {
            flow_query::type_param_info(self.interner, m)
                .is_some_and(|info| info.name == target_param_name)
        };
        let has_target_param = param_members.iter().any(|&m| is_target_param(m));
        if !has_target_param {
            return None;
        }

        // Collect the fixed (non-type-parameter) members from the parameter type.
        // Evaluate each to resolve Lazy/Application types to their concrete forms
        // (e.g., `FAILURE` alias → `"FAILURE"` literal) so TypeId comparison works
        // when subtracting from arg members.
        let fixed_members: Vec<TypeId> = param_members
            .iter()
            .copied()
            .filter(|&m| !is_target_param(m))
            .map(|m| {
                // Resolve Lazy/Application types to their concrete forms.
                // Lazy(DefId) types are type aliases that need resolver lookup.
                if let Some(env_ref) = &self.type_environment {
                    let env = env_ref.borrow();
                    let resolved =
                        flow_boundary::resolve_lazy_def_with_env(self.interner, Some(&*env), m);
                    if resolved != m {
                        return resolved;
                    }
                    // Then try Application evaluation
                    let result = flow_query::evaluate_application_type(self.interner, &env, m);
                    if result != m {
                        return result;
                    }
                }
                flow_query::evaluate_type_structure(self.interner, m)
            })
            .collect();

        // Get union members of the argument type (or treat as single-member)
        let arg_members = union_members_for_type(self.interner, arg_type)
            .unwrap_or_else(|| vec![arg_type].into());

        // Subtract the fixed members from the argument type
        let remaining: Vec<TypeId> = arg_members
            .iter()
            .copied()
            .filter(|arg_m| !fixed_members.contains(arg_m))
            .collect();

        if remaining.is_empty() {
            return None;
        }

        // Wrapper-pattern fallback: if the non-T side is a structural wrapper like
        // `{ data: T }`, exact member subtraction leaves both `T` and `{ data: T }`
        // in `remaining` because the wrapper member still contains the type parameter.
        // Prefer the candidates that do NOT have the wrapper's property set.
        let wrapper_props = param_members
            .iter()
            .copied()
            .filter(|&m| !is_target_param(m))
            .find_map(|m| {
                let evaluated = if let Some(env_ref) = &self.type_environment {
                    let env = env_ref.borrow();
                    let applied = flow_query::evaluate_application_type(self.interner, &env, m);
                    if applied != m {
                        applied
                    } else {
                        flow_query::evaluate_type_structure(self.interner, m)
                    }
                } else {
                    flow_query::evaluate_type_structure(self.interner, m)
                };
                crate::query_boundaries::state::checking::object_shape(self.interner, evaluated)
                    .map(|shape| {
                        shape
                            .properties
                            .iter()
                            .map(|prop| prop.name)
                            .collect::<Vec<_>>()
                    })
            })
            .unwrap_or_default();

        if !wrapper_props.is_empty() && remaining.len() > 1 {
            let unwrapped: Vec<TypeId> = remaining
                .iter()
                .copied()
                .filter(|candidate| {
                    let evaluated = if let Some(env_ref) = &self.type_environment {
                        let env = env_ref.borrow();
                        let applied =
                            flow_query::evaluate_application_type(self.interner, &env, *candidate);
                        if applied != *candidate {
                            applied
                        } else {
                            flow_query::evaluate_type_structure(self.interner, *candidate)
                        }
                    } else {
                        flow_query::evaluate_type_structure(self.interner, *candidate)
                    };
                    wrapper_props.iter().all(|prop_name| {
                        let prop_text = self.interner.resolve_atom_ref(*prop_name);
                        crate::query_boundaries::state::checking::find_property_in_object_by_str(
                            self.interner,
                            evaluated,
                            prop_text.as_ref(),
                        )
                        .is_none()
                    })
                })
                .collect();

            if !unwrapped.is_empty() {
                return Some(if unwrapped.len() == 1 {
                    unwrapped[0]
                } else {
                    flow_query::union_types(self.interner, unwrapped)
                });
            }
        }

        // Build the inferred type from the remaining members
        let inferred = if remaining.len() == 1 {
            remaining[0]
        } else {
            flow_query::union_types(self.interner, remaining)
        };
        Some(inferred)
    }
}
