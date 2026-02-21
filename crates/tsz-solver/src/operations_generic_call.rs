//! Generic function call inference.
//!
//! Contains the core generic call resolution logic, including:
//! - Multi-pass type argument inference (Round 1 + Round 2)
//! - Contextual type computation for lambda arguments
//! - Trivial single-type-param fast path
//! - Placeholder normalization

use crate::infer::{InferenceContext, InferenceError};
use crate::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::{AssignabilityChecker, CallEvaluator, CallResult};
use crate::types::{
    FunctionShape, ParamInfo, TupleElement, TypeData, TypeId, TypeParamInfo, TypePredicate,
};
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::{debug, trace};

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub(crate) fn resolve_generic_call(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> CallResult {
        let previous_defaulted = std::mem::take(&mut self.defaulted_placeholders);
        let result = self.resolve_generic_call_inner(func, arg_types);
        self.defaulted_placeholders = previous_defaulted;
        result
    }

    fn resolve_generic_call_inner(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> CallResult {
        // Check argument count BEFORE type inference
        // This prevents false positive TS2554 errors for generic functions with optional/rest params
        let (min_args, max_args) = self.arg_count_bounds(&func.params);

        if arg_types.len() < min_args {
            return CallResult::ArgumentCountMismatch {
                expected_min: min_args,
                expected_max: max_args,
                actual: arg_types.len(),
            };
        }

        if let Some(max) = max_args
            && arg_types.len() > max
        {
            return CallResult::ArgumentCountMismatch {
                expected_min: min_args,
                expected_max: Some(max),
                actual: arg_types.len(),
            };
        }

        if let Some(result) = self.resolve_trivial_single_type_param_call(func, arg_types) {
            return result;
        }

        let mut infer_ctx = InferenceContext::new(self.interner.as_type_database());
        let mut substitution = TypeSubstitution::new();
        let mut var_map: FxHashMap<TypeId, crate::infer::InferenceVar> = FxHashMap::default();
        let mut type_param_vars = Vec::with_capacity(func.type_params.len());

        self.constraint_pairs.borrow_mut().clear();
        *self.constraint_recursion_depth.borrow_mut() = 0;

        // Reusable visited set for type_contains_placeholder checks (avoids per-iteration alloc)
        let mut placeholder_visited = FxHashSet::default();
        // Reusable buffer for placeholder names (avoids per-iteration String allocation)
        let mut placeholder_buf = String::with_capacity(24);

        // 1. Create inference variables and placeholders for each type parameter
        for tp in &func.type_params {
            // Allocate an inference variable first, then create a *unique* placeholder type
            // for that variable. We register the placeholder name (not the original type
            // parameter name) with the inference context so occurs-checks don't get confused
            // by identically-named type parameters from outer scopes (e.g., `T` inside `T`).
            let var = infer_ctx.fresh_var();
            type_param_vars.push(var);

            // Create a unique placeholder type for this inference variable
            // We use a TypeParameter with a special name to track it during constraint collection
            use std::fmt::Write;
            placeholder_buf.clear();
            write!(placeholder_buf, "__infer_{}", var.0).unwrap();
            let placeholder_atom = self.interner.intern_string(&placeholder_buf);
            infer_ctx.register_type_param(placeholder_atom, var, tp.is_const);
            let placeholder_key = TypeData::TypeParameter(TypeParamInfo {
                is_const: tp.is_const,
                name: placeholder_atom,
                constraint: tp.constraint,
                default: None,
            });
            let placeholder_id = self.interner.intern(placeholder_key);

            substitution.insert(tp.name, placeholder_id);
            var_map.insert(placeholder_id, var);

            // Add the type parameter constraint as an upper bound, but only if the
            // constraint is concrete (doesn't reference other type params via placeholders).
            // Constraints like `keyof T` that depend on other type params can't be evaluated
            // during resolution since T may not be resolved yet. These are validated in the
            // post-resolution constraint check below.
            if let Some(constraint) = tp.constraint {
                let inst_constraint = instantiate_type(self.interner, constraint, &substitution);
                placeholder_visited.clear();
                if !self.type_contains_placeholder(
                    inst_constraint,
                    &var_map,
                    &mut placeholder_visited,
                ) {
                    infer_ctx.add_upper_bound(var, inst_constraint);
                }
            }

            if tp.default.is_some() {
                self.defaulted_placeholders.insert(placeholder_id);
            }
        }

        // 2. Instantiate parameters with placeholders
        let instantiated_params: Vec<ParamInfo> = func
            .params
            .iter()
            .map(|p| {
                let instantiated = instantiate_type(self.interner, p.type_id, &substitution);
                if let Some(name_atom) = p.name {
                    let param_name = self.interner.resolve_atom(name_atom);
                    debug!(
                        param_name = %param_name.as_str(),
                        original_type_id = p.type_id.0,
                        original_type_key = ?self.interner.lookup(p.type_id),
                        instantiated_type_id = instantiated.0,
                        instantiated_type_key = ?self.interner.lookup(instantiated),
                        "Instantiated param"
                    );
                }
                // If this is a function type, also log its return type
                if let Some(TypeData::Function(shape_id)) = self.interner.lookup(instantiated) {
                    let shape = self.interner.function_shape(shape_id);
                    debug!(
                        return_type_id = shape.return_type.0,
                        return_type_key = ?self.interner.lookup(shape.return_type),
                        "Instantiated function return type"
                    );
                }
                ParamInfo {
                    name: p.name,
                    type_id: instantiated,
                    optional: p.optional,
                    rest: p.rest,
                }
            })
            .collect();

        // 2.5. Seed contextual constraints from return type BEFORE argument processing
        // This enables downward inference: `let x: string = id(...)` should infer T = string
        // Contextual hints use lower priority so explicit arguments can override
        // Skip `any` and `unknown` contextual types — they carry no inference information
        // and can interfere with constraint-based inference (e.g., causing T to resolve to
        // `any` instead of using its constraint like `(arg: string) => any`).
        if let Some(ctx_type) = self.contextual_type
            && ctx_type != TypeId::ANY
            && ctx_type != TypeId::UNKNOWN
        {
            let return_type_with_placeholders =
                instantiate_type(self.interner, func.return_type, &substitution);
            // CORRECT: return_type <: ctx_type
            // In assignment `let x: Target = Source`, the relation is `Source <: Target`
            // Therefore, the return value must be assignable to the expected type
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                return_type_with_placeholders, // source
                ctx_type,                      // target
                crate::types::InferencePriority::ReturnType,
            );
        }

        // 3. Multi-pass constraint collection for proper contextual typing

        // Prepare rest tuple inference info
        let rest_tuple_inference =
            self.rest_tuple_inference_target(&instantiated_params, arg_types, &var_map);
        let rest_tuple_start = rest_tuple_inference.as_ref().map(|(start, _, _)| *start);
        let mut has_context_sensitive_args = false;

        // === Round 1: Process non-contextual arguments ===
        // These are arguments like arrays, primitives, and objects that don't need
        // contextual typing. Processing them first allows us to infer type parameters
        // that contextual arguments (lambdas) can then use.
        for (i, &arg_type) in arg_types.iter().enumerate() {
            if rest_tuple_start.is_some_and(|start| i >= start) {
                continue;
            }
            let Some(target_type) =
                self.param_type_for_arg_index(&instantiated_params, i, arg_types.len())
            else {
                break;
            };

            // Skip contextually sensitive arguments (will process in Round 2)
            if self.is_contextually_sensitive(arg_type) {
                has_context_sensitive_args = true;
                continue;
            }

            // Direct placeholders (inference variables) are validated by final
            // constraint resolution below. Skipping eager checks here avoids
            // duplicate expensive assignability work on hot generic-call paths.
            if !var_map.contains_key(&target_type) {
                placeholder_visited.clear();
                if !self.type_contains_placeholder(target_type, &var_map, &mut placeholder_visited)
                {
                    // No placeholder in target_type - check assignability directly
                    if !self.checker.is_assignable_to(arg_type, target_type)
                        && !self.is_function_union_compat(arg_type, target_type)
                    {
                        return CallResult::ArgumentTypeMismatch {
                            index: i,
                            expected: target_type,
                            actual: arg_type,
                        };
                    }
                } else {
                    // Target type contains placeholders - check against their constraints
                    if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(target_type)
                        && let Some(constraint) = tp.constraint
                    {
                        let inst_constraint =
                            instantiate_type(self.interner, constraint, &substitution);
                        placeholder_visited.clear();
                        if !self.type_contains_placeholder(
                            inst_constraint,
                            &var_map,
                            &mut placeholder_visited,
                        ) {
                            // Constraint is fully concrete - safe to check now
                            if !self.checker.is_assignable_to(arg_type, inst_constraint)
                                && !self.is_function_union_compat(arg_type, inst_constraint)
                            {
                                return CallResult::ArgumentTypeMismatch {
                                    index: i,
                                    expected: inst_constraint,
                                    actual: arg_type,
                                };
                            }
                        }
                    }
                }
            }

            // arg_type <: target_type
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                arg_type,
                target_type,
                crate::types::InferencePriority::NakedTypeVariable,
            );
        }

        // Process rest tuple in Round 1 (it's non-contextual)
        if let Some((_start, target_type, tuple_type)) = rest_tuple_inference {
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                tuple_type,
                target_type,
                crate::types::InferencePriority::NakedTypeVariable,
            );
        }

        // === Fixing: Resolve variables with enough information ===
        // This "fixes" type variables that have candidates from Round 1,
        // preventing Round 2 from overriding them with lower-priority constraints.
        if infer_ctx.fix_current_variables().is_err() {
            // Fixing failed - this might indicate a constraint conflict
            // Continue with partial fixing, final resolution will detect errors
        }

        // === Round 2: Process contextual arguments ===
        // These are arguments like lambdas that need contextual typing.
        // Now that non-contextual arguments have been processed, we can provide
        // proper contextual types to lambdas based on fixed type variables.
        if has_context_sensitive_args {
            for (i, &arg_type) in arg_types.iter().enumerate() {
                if rest_tuple_start.is_some_and(|start| i >= start) {
                    continue;
                }
                let Some(target_type) =
                    self.param_type_for_arg_index(&instantiated_params, i, arg_types.len())
                else {
                    break;
                };

                // Only process contextually sensitive arguments in Round 2
                if !self.is_contextually_sensitive(arg_type) {
                    continue;
                }

                // Check if target_type contains placeholders BEFORE any re-instantiation
                placeholder_visited.clear();
                let target_has_placeholders =
                    self.type_contains_placeholder(target_type, &var_map, &mut placeholder_visited);

                if !target_has_placeholders {
                    // No placeholders in target - direct assignability check
                    if !self.checker.is_assignable_to(arg_type, target_type)
                        && !self.is_function_union_compat(arg_type, target_type)
                    {
                        return CallResult::ArgumentTypeMismatch {
                            index: i,
                            expected: target_type,
                            actual: arg_type,
                        };
                    }
                } else {
                    // Target has placeholders - collect constraints using the original target_type
                    // This preserves the connection to inference variables (e.g., U in (x: T) => U)
                    // IMPORTANT: Use target_type directly, not contextual_target, to maintain
                    // the placeholder connection for unresolved type parameters
                    self.constrain_types(
                        &mut infer_ctx,
                        &var_map,
                        arg_type,
                        target_type,
                        crate::types::InferencePriority::ReturnType,
                    );

                    // Special case: If target_type is a function with rest param type parameter,
                    // and arg_type is a function, infer the tuple type from function parameters.
                    // Example: test<A>((x: string) => {}) where A extends any[]
                    // Should infer A = [string]
                    if let Some(TypeData::Function(target_fn_id)) =
                        self.interner.lookup(target_type)
                    {
                        let target_fn = self.interner.function_shape(target_fn_id);
                        if let Some(t_last) = target_fn.params.last()
                            && t_last.rest
                            && var_map.contains_key(&t_last.type_id)
                            && let Some(TypeData::Function(source_fn_id)) =
                                self.interner.lookup(arg_type)
                        {
                            let source_fn = self.interner.function_shape(source_fn_id);
                            // Create tuple from source function's parameters
                            use crate::type_queries::unpack_tuple_rest_parameter;
                            let params_unpacked: Vec<ParamInfo> = source_fn
                                .params
                                .iter()
                                .flat_map(|p| unpack_tuple_rest_parameter(self.interner, p))
                                .collect();

                            let tuple_elements: Vec<TupleElement> = params_unpacked
                                .iter()
                                .map(|p| TupleElement {
                                    type_id: p.type_id,
                                    name: p.name,
                                    optional: p.optional,
                                    rest: p.rest,
                                })
                                .collect();
                            let param_tuple = self.interner.tuple(tuple_elements);

                            // Infer: A = [string, number]
                            if let Some(&var) = var_map.get(&t_last.type_id) {
                                infer_ctx.add_candidate(
                                    var,
                                    param_tuple,
                                    crate::types::InferencePriority::NakedTypeVariable,
                                );
                            }
                        }
                    }
                }
            }
        }

        // 4. Resolve inference variables
        // CRITICAL: Strengthen inter-parameter constraints before resolution
        // This ensures SCC-based cycle unification happens (commit c3ede45a9)
        if infer_ctx.strengthen_constraints().is_err() {
            // Cycle unification failed - this indicates a circularity that cannot be resolved
            // Fall back to resolving without unification (may result in less precise types)
        }

        let mut final_subst = TypeSubstitution::new();
        let mut infer_subst_cache: Option<TypeSubstitution> = None;
        for (tp, &var) in func.type_params.iter().zip(type_param_vars.iter()) {
            let constraints = infer_ctx.get_constraints(var);
            let has_constraints = matches!(&constraints, Some(c) if !c.is_empty());

            trace!(
                type_param_name = ?self.interner.resolve_atom(tp.name),
                var = ?var,
                has_constraints = has_constraints,
                constraints = ?constraints,
                has_default = tp.default.is_some(),
                has_constraint = tp.constraint.is_some(),
                constraint = ?tp.constraint,
                "Resolving type parameter"
            );

            let ty = if has_constraints {
                match infer_ctx.resolve_with_constraints_by(var, |source, target| {
                    self.checker.is_assignable_to(source, target)
                }) {
                    Ok(ty) => {
                        trace!(
                            resolved_type = ?ty,
                            "Type parameter resolved successfully from constraints"
                        );
                        ty
                    }
                    Err(e) => {
                        trace!(
                            error = ?e,
                            "Constraint resolution failed, using fallback"
                        );

                        // When the bounds violation comes from callback return type
                        // inference (Round 2, ReturnType priority), tsc uses the inferred
                        // type and reports TS2322 on the return expression rather than
                        // falling back to the constraint and reporting TS2345 on the
                        // whole callback argument.
                        let use_inferred = matches!(&e, InferenceError::BoundsViolation { .. })
                            && infer_ctx.all_candidates_are_return_type(var);

                        let fallback = if use_inferred {
                            // Use the inferred type (lower bound from BoundsViolation)
                            if let InferenceError::BoundsViolation { lower, .. } = &e {
                                *lower
                            } else {
                                panic!(
                                    "invariant violation: expected bounds violation when using inferred fallback"
                                )
                            }
                        } else if let Some(default) = tp.default {
                            instantiate_type(self.interner, default, &final_subst)
                        } else if let Some(constraint) = tp.constraint {
                            instantiate_type(self.interner, constraint, &final_subst)
                        } else {
                            TypeId::ERROR
                        };
                        trace!(
                            fallback_type = ?fallback,
                            use_inferred = use_inferred,
                            "Using fallback type"
                        );
                        fallback
                    }
                }
            } else if let Some(default) = tp.default {
                let ty = instantiate_type(self.interner, default, &final_subst);
                trace!(resolved_type = ?ty, "Using default type");
                ty
            } else if let Some(constraint) = tp.constraint {
                let ty = instantiate_type(self.interner, constraint, &final_subst);
                trace!(resolved_type = ?ty, "Using constraint as fallback (no constraints collected)");
                ty
            } else {
                trace!("Using UNKNOWN (unconstrained type parameter)");
                // TypeScript infers 'unknown' for unconstrained type parameters without defaults
                TypeId::UNKNOWN
            };

            // Generic source contextual instantiation can produce temporary placeholders
            // (e.g. `__infer_src_*`) while collecting constraints for callback arguments.
            // Those placeholders must never leak into final instantiated signatures.
            let ty = if has_context_sensitive_args {
                let infer_subst = if let Some(ref cached) = infer_subst_cache {
                    cached
                } else {
                    infer_subst_cache = Some(infer_ctx.get_current_substitution());
                    infer_subst_cache
                        .as_ref()
                        .expect("inference substitution cache just initialized")
                };
                self.normalize_inferred_placeholder_type(ty, infer_subst)
            } else {
                ty
            };

            final_subst.insert(tp.name, ty);
        }

        // Recursively resolve placeholders in final_subst.
        // If an inferred type contains transient placeholders from source functions (e.g. __infer_src_U),
        // we must resolve them using the full inference context substitution.
        // Example: B -> Array(__infer_src_U) where __infer_src_U -> T. We want B -> Array(T).
        {
            let full_subst = infer_ctx.get_current_substitution();
            let mut resolved_subst = TypeSubstitution::new();
            for (name, ty) in final_subst.map().iter() {
                // Iteratively apply substitution to resolve transitive placeholders.
                let mut current = *ty;
                for _ in 0..8 {
                    let next = instantiate_type(self.interner, current, &full_subst);
                    if next == current {
                        break;
                    }
                    current = next;
                }
                resolved_subst.insert(*name, current);
            }
            // Update final_subst with fully resolved types
            for (name, ty) in resolved_subst.map().iter() {
                final_subst.insert(*name, *ty);
            }
        }

        // Constraint checking is deferred until ALL type parameters are resolved.
        // This handles cases like `<T extends U, U>` where T's constraint references
        // U, which may not be in final_subst until later iterations.
        for (tp, &var) in func.type_params.iter().zip(type_param_vars.iter()) {
            if let Some(constraint) = tp.constraint {
                let ty = final_subst.get(tp.name).unwrap_or(TypeId::ERROR);
                let constraint_ty = instantiate_type(self.interner, constraint, &final_subst);
                // Strip freshness before constraint check: inferred types should not
                // trigger excess property checking against type parameter constraints.
                let ty_for_check = crate::freshness::widen_freshness(self.interner, ty);
                if !self.checker.is_assignable_to(ty_for_check, constraint_ty) {
                    // Try to recover using un-widened literal candidates when widening
                    // caused the violation (e.g., "b" widened to string violates keyof O).
                    let un_widened = infer_ctx.get_literal_candidates(var);
                    let recovered = if !un_widened.is_empty() {
                        let candidate_type = if un_widened.len() == 1 {
                            un_widened[0]
                        } else {
                            self.interner.union(un_widened)
                        };
                        if self.checker.is_assignable_to(candidate_type, constraint_ty) {
                            Some(candidate_type)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    if let Some(recovered_ty) = recovered {
                        final_subst.insert(tp.name, recovered_ty);
                    } else {
                        // Fall back to constraint type so argument checking emits TS2345
                        final_subst.insert(tp.name, constraint_ty);
                    }
                }
            }
        }

        let instantiated_params: Vec<ParamInfo> = func
            .params
            .iter()
            .map(|p| {
                let instantiated = instantiate_type(self.interner, p.type_id, &final_subst);
                ParamInfo {
                    name: p.name,
                    type_id: instantiated,
                    optional: p.optional,
                    rest: p.rest,
                }
            })
            .collect();
        let (min_args, max_args) = self.arg_count_bounds(&instantiated_params);
        if arg_types.len() < min_args {
            return CallResult::ArgumentCountMismatch {
                expected_min: min_args,
                expected_max: max_args,
                actual: arg_types.len(),
            };
        }
        if let Some(max) = max_args
            && arg_types.len() > max
        {
            return CallResult::ArgumentCountMismatch {
                expected_min: min_args,
                expected_max: Some(max),
                actual: arg_types.len(),
            };
        }
        // Final check: verify arguments against instantiated parameters.
        // When callbacks are contextually typed with the callee's inference placeholders
        // (__infer_0, etc.), those placeholders leak into the arg types. Replace them
        // with the inferred values before the assignability check. Using placeholder
        // names avoids name collisions with same-named type parameters from outer scopes.
        let placeholder_subst = {
            let mut s = TypeSubstitution::new();
            for (i, tp) in func.type_params.iter().enumerate() {
                if let Some(inferred) = final_subst.get(tp.name) {
                    use std::fmt::Write;
                    placeholder_buf.clear();
                    write!(placeholder_buf, "__infer_{}", type_param_vars[i].0).unwrap();
                    let placeholder_atom = self.interner.intern_string(&placeholder_buf);
                    s.insert(placeholder_atom, inferred);
                }
            }
            s
        };
        let final_args: Vec<TypeId> = if placeholder_subst.is_empty() {
            arg_types.to_vec()
        } else {
            arg_types
                .iter()
                .map(|&arg| instantiate_type(self.interner, arg, &placeholder_subst))
                .collect()
        };
        tracing::debug!(
            "Final argument check with {} instantiated params",
            instantiated_params.len()
        );
        for (i, (param, &arg_type)) in instantiated_params
            .iter()
            .zip(final_args.iter())
            .enumerate()
        {
            tracing::debug!("  Param {}: {:?}", i, self.interner.lookup(param.type_id));
            tracing::debug!("  Arg   {}: {:?}", i, self.interner.lookup(arg_type));
        }
        if let Some(result) =
            self.check_argument_types_with(&instantiated_params, &final_args, true, func.is_method)
        {
            tracing::debug!("Final check failed: {:?}", result);
            return result;
        }
        tracing::debug!("Final check succeeded");

        let return_type = instantiate_type(self.interner, func.return_type, &final_subst);

        // Instantiate the type predicate if present, so the checker can use it
        // for flow narrowing with the correct (inferred) type arguments.
        if let Some(ref predicate) = func.type_predicate {
            let instantiated_predicate = TypePredicate {
                asserts: predicate.asserts,
                target: predicate.target.clone(),
                type_id: predicate
                    .type_id
                    .map(|tid| instantiate_type(self.interner, tid, &final_subst)),
                parameter_index: predicate.parameter_index,
            };
            let instantiated_params_for_pred: Vec<ParamInfo> = func
                .params
                .iter()
                .map(|p| ParamInfo {
                    name: p.name,
                    type_id: instantiate_type(self.interner, p.type_id, &final_subst),
                    optional: p.optional,
                    rest: p.rest,
                })
                .collect();
            self.last_instantiated_predicate =
                Some((instantiated_predicate, instantiated_params_for_pred));
        }

        CallResult::Success(return_type)
    }

    /// Fast path for identity-style generic calls:
    /// `<T extends C>(x: T) => T` with a single non-rest argument.
    ///
    /// This shape is common in constraint-heavy code and does not require full
    /// multi-pass inference machinery. We can infer `T` directly from the argument,
    /// validate the constraint once, and return the argument type.
    fn resolve_trivial_single_type_param_call(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> Option<CallResult> {
        if func.type_params.len() != 1 || func.params.len() != 1 || arg_types.len() != 1 {
            return None;
        }
        if func.params[0].rest || func.params[0].optional {
            return None;
        }
        if func.this_type.is_some() || func.type_predicate.is_some() {
            return None;
        }

        let tp = &func.type_params[0];
        let param_ty = func.params[0].type_id;
        let return_ty = func.return_type;

        let is_tp = |ty: TypeId| {
            matches!(
                self.interner.lookup(ty),
                Some(TypeData::TypeParameter(info)) if info.name == tp.name
            )
        };
        if !is_tp(param_ty) || !is_tp(return_ty) {
            return None;
        }

        let arg_ty = arg_types[0];
        let inferred_ty = if tp.is_const {
            arg_ty
        } else {
            crate::widening::widen_type(self.interner.as_type_database(), arg_ty)
        };
        if let Some(constraint) = tp.constraint
            && !self.checker.is_assignable_to(inferred_ty, constraint)
            && !self.is_function_union_compat(inferred_ty, constraint)
        {
            return Some(CallResult::TypeParameterConstraintViolation {
                inferred_type: inferred_ty,
                constraint_type: constraint,
                return_type: inferred_ty,
            });
        }

        Some(CallResult::Success(inferred_ty))
    }

    /// Collapse transient inference placeholders (like `__infer_src_*`) to stable types.
    ///
    /// The generic call pipeline uses temporary type parameter placeholders for
    /// contextually-instantiated callback arguments. If one of those placeholders
    /// survives as an inferred result, we normalize it through the current
    /// substitution map and fall back to `unknown` if it remains unresolved.
    ///
    /// Uses iterative `instantiate_type` to resolve placeholders within compound
    /// types (e.g., `Array(__infer_src_0)` → `Array(__infer_0)` → `Array(number)`).
    fn normalize_inferred_placeholder_type(
        &self,
        ty: TypeId,
        infer_subst: &TypeSubstitution,
    ) -> TypeId {
        if infer_subst.is_empty() {
            return ty;
        }

        // Iteratively apply substitution to resolve transitive placeholders.
        // Each pass may resolve one level (e.g., __infer_src_0 → __infer_0[] → number[]).
        let mut current = ty;
        for _ in 0..8 {
            let next = instantiate_type(self.interner, current, infer_subst);
            if next == current {
                break;
            }
            current = next;
        }

        current
    }

    /// Computes contextual types for function parameters after Round 1 inference.
    ///
    /// This is used by the Checker to implement two-pass argument checking:
    /// 1. Checker checks non-contextual arguments (arrays, primitives)
    /// 2. Checker calls this method to run Round 1 inference on those arguments
    /// 3. This method returns the current type substitution (with fixed variables)
    /// 4. Checker uses the substitution to construct contextual types for lambdas
    /// 5. Checker checks lambdas with those contextual types (Round 2)
    ///
    /// # Arguments
    /// * `func` - The function shape being called
    /// * `arg_types` - The types of all arguments (both contextual and non-contextual)
    ///
    /// # Returns
    /// A `TypeSubstitution` mapping type parameter placeholder names to their
    /// inferred types after Round 1 inference. The Checker can use this to
    /// instantiate parameter types for contextual arguments.
    pub fn compute_contextual_types(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> TypeSubstitution {
        use crate::types::InferencePriority;

        // Save state to prevent pollution if evaluator is reused
        let previous_defaulted = std::mem::take(&mut self.defaulted_placeholders);

        let mut infer_ctx = InferenceContext::new(self.interner.as_type_database());
        let mut substitution = TypeSubstitution::new();
        let mut var_map: FxHashMap<TypeId, crate::infer::InferenceVar> = FxHashMap::default();
        let mut type_param_vars = Vec::with_capacity(func.type_params.len());

        self.constraint_pairs.borrow_mut().clear();
        *self.constraint_recursion_depth.borrow_mut() = 0;

        // Reusable visited set for type_contains_placeholder checks (avoids per-iteration alloc)
        let mut placeholder_visited = FxHashSet::default();
        // Reusable buffer for placeholder names (avoids per-iteration String allocation)
        let mut placeholder_buf = String::with_capacity(24);

        // 1. Create inference variables and placeholders for each type parameter
        for tp in &func.type_params {
            let var = infer_ctx.fresh_var();
            type_param_vars.push(var);

            use std::fmt::Write;
            placeholder_buf.clear();
            write!(placeholder_buf, "__infer_{}", var.0).unwrap();
            let placeholder_atom = self.interner.intern_string(&placeholder_buf);
            infer_ctx.register_type_param(placeholder_atom, var, tp.is_const);
            let placeholder_key = TypeData::TypeParameter(TypeParamInfo {
                is_const: tp.is_const,
                name: placeholder_atom,
                constraint: tp.constraint,
                default: None,
            });
            let placeholder_id = self.interner.intern(placeholder_key);

            substitution.insert(tp.name, placeholder_id);
            var_map.insert(placeholder_id, var);

            // Track defaulted placeholders to prevent union inference in constrain_types
            if tp.default.is_some() {
                self.defaulted_placeholders.insert(placeholder_id);
            }

            // Add type parameter constraint as upper bound (if concrete)
            if let Some(constraint) = tp.constraint {
                let inst_constraint = instantiate_type(self.interner, constraint, &substitution);
                placeholder_visited.clear();
                if !self.type_contains_placeholder(
                    inst_constraint,
                    &var_map,
                    &mut placeholder_visited,
                ) {
                    infer_ctx.add_upper_bound(var, inst_constraint);
                }
            }
        }

        // 2. Instantiate parameters with placeholders
        let instantiated_params: Vec<ParamInfo> = func
            .params
            .iter()
            .map(|p| ParamInfo {
                name: p.name,
                type_id: instantiate_type(self.interner, p.type_id, &substitution),
                optional: p.optional,
                rest: p.rest,
            })
            .collect();

        // 2.5. Seed contextual constraints from return type
        // Skip `any` and `unknown` — they don't contribute useful inference constraints
        if let Some(ctx_type) = self.contextual_type
            && ctx_type != TypeId::ANY
            && ctx_type != TypeId::UNKNOWN
        {
            let return_type_with_placeholders =
                instantiate_type(self.interner, func.return_type, &substitution);
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                return_type_with_placeholders,
                ctx_type,
                InferencePriority::ReturnType,
            );
        }

        // 3. Round 1: Process non-contextual arguments only
        let rest_tuple_inference =
            self.rest_tuple_inference_target(&instantiated_params, arg_types, &var_map);
        let rest_tuple_start = rest_tuple_inference.as_ref().map(|(start, _, _)| *start);

        for (i, &arg_type) in arg_types.iter().enumerate() {
            if rest_tuple_start.is_some_and(|start| i >= start) {
                continue;
            }
            let Some(target_type) =
                self.param_type_for_arg_index(&instantiated_params, i, arg_types.len())
            else {
                break;
            };

            // Skip contextually sensitive arguments (Checker will handle them in Round 2)
            if self.is_contextually_sensitive(arg_type) {
                continue;
            }

            // Add constraint for non-contextual arguments
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                arg_type,
                target_type,
                InferencePriority::NakedTypeVariable,
            );
        }

        // Process rest tuple in Round 1
        if let Some((_start, target_type, tuple_type)) = rest_tuple_inference {
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                tuple_type,
                target_type,
                InferencePriority::NakedTypeVariable,
            );
        }

        // 4. Fix variables with enough information from Round 1
        let _ = infer_ctx.fix_current_variables();

        // Restore state to prevent pollution if evaluator is reused
        self.defaulted_placeholders = previous_defaulted;

        // 5. Remap substitution to use original type parameter names.
        // get_current_substitution() returns keys like "__infer_0", but the Checker
        // needs keys matching the original type parameter names (e.g., "T", "U")
        // so that instantiate_type can find and replace TypeParameter nodes.
        let infer_subst = infer_ctx.get_current_substitution();
        let mut result_subst = TypeSubstitution::new();

        // Pass 1: Collect all resolved (non-UNKNOWN) type parameters
        let mut unresolved_indices = Vec::new();
        for (i, tp) in func.type_params.iter().enumerate() {
            use std::fmt::Write;
            placeholder_buf.clear();
            write!(placeholder_buf, "__infer_{}", type_param_vars[i].0).unwrap();
            let placeholder_atom = self.interner.intern_string(&placeholder_buf);
            if let Some(resolved) = infer_subst.get(placeholder_atom) {
                if resolved != TypeId::UNKNOWN {
                    result_subst.insert(tp.name, resolved);
                } else {
                    unresolved_indices.push(i);
                }
            }
        }

        // Pass 2: For unresolved type params, try using the default or constraint
        // instantiated with already-resolved params as a contextual type.
        // Priority: default > constraint > placeholder (the default is what the type IS
        // when no argument is provided; the constraint is just an upper bound).
        // As a last resort, use the inference placeholder (__infer_N) so that callbacks
        // get unique placeholder types instead of the callee's raw type parameters,
        // which avoids name collisions with outer scope type parameters of the same name.
        for i in unresolved_indices {
            let tp = &func.type_params[i];
            // Try default first — this determines the contextual type when no inference
            // happened (e.g. `<T = TypegenDisabled>` should use TypegenDisabled, not the
            // constraint `TypegenEnabled | TypegenDisabled`).
            if let Some(default) = tp.default {
                let inst_default = instantiate_type(self.interner, default, &result_subst);
                if !crate::visitor::contains_type_parameters(
                    self.interner.as_type_database(),
                    inst_default,
                ) {
                    result_subst.insert(tp.name, inst_default);
                    continue;
                }
            }
            // Fall back to constraint if default didn't resolve.
            // This enables contextual typing for patterns like:
            //   test<TContext, TFn extends (ctx: TContext) => void>(context: TContext, fn: TFn)
            // where TContext is inferred in Round 1 but TFn needs its constraint.
            if let Some(constraint) = tp.constraint {
                let inst_constraint = instantiate_type(self.interner, constraint, &result_subst);
                if !crate::visitor::contains_type_parameters(
                    self.interner.as_type_database(),
                    inst_constraint,
                ) {
                    result_subst.insert(tp.name, inst_constraint);
                    continue;
                }
            }
            // Last resort: use the inference placeholder so callbacks get unique
            // placeholder types instead of the callee's raw type parameter.
            // This ensures that `foo((x) => 1, (x) => '')` produces arg types with
            // `__infer_0` instead of `T`, avoiding name collisions with outer `T`.
            {
                use std::fmt::Write;
                placeholder_buf.clear();
                write!(placeholder_buf, "__infer_{}", type_param_vars[i].0).unwrap();
                let placeholder_atom = self.interner.intern_string(&placeholder_buf);
                let placeholder_key = TypeData::TypeParameter(TypeParamInfo {
                    is_const: tp.is_const,
                    name: placeholder_atom,
                    constraint: tp.constraint,
                    default: None,
                });
                let placeholder_id = self.interner.intern(placeholder_key);
                result_subst.insert(tp.name, placeholder_id);
            }
        }

        result_subst
    }
}
