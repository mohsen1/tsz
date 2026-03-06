//! Generic function call inference.
//!
//! Contains the core generic call resolution logic, including:
//! - Multi-pass type argument inference (Round 1 + Round 2)
//! - Contextual type computation for lambda arguments
//! - Trivial single-type-param fast path
//! - Placeholder normalization

use crate::inference::infer::{InferenceContext, InferenceError, InferenceVar};
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::widening;
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
        let mut var_map: FxHashMap<TypeId, crate::inference::infer::InferenceVar> =
            FxHashMap::default();
        let mut type_param_vars = Vec::with_capacity(func.type_params.len());

        self.constraint_pairs.borrow_mut().clear();
        self.constraint_fixed_union_members.borrow_mut().clear();
        *self.constraint_recursion_depth.borrow_mut() = 0;
        *self.constraint_step_count.borrow_mut() = 0;

        // Reusable visited set for type_contains_placeholder checks (avoids per-iteration alloc)
        let mut placeholder_visited = FxHashSet::default();
        // Track placeholders that are used directly as argument targets.
        // For those parameters we keep inference constrained so final argument checks
        // can report concrete mismatches instead of silently widening to unions.
        let mut direct_param_vars = FxHashSet::default();
        let mut placeholder_probe_map: FxHashMap<TypeId, InferenceVar> = FxHashMap::default();
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
                    infer_ctx.set_declared_constraint(var, inst_constraint);
                }
            }

            if tp.default.is_some() {
                self.defaulted_placeholders.insert(placeholder_id);
            }
        }

        // 1.5. Pre-compute which placeholders should have their argument's object
        // properties widened. In tsc, object literal property widening happens at the
        // expression level (checkObjectLiteral) based on contextual type. When the
        // contextual type is a bare type parameter whose constraint doesn't contain
        // literal types, properties like `false` are widened to `boolean`.
        //
        // We suppress widening in two cases:
        // (a) The constraint contains literal types (discriminated union protection)
        // (b) The type parameter is referenced in another type param's constraint,
        //     because widening would cause a mismatch between the widened candidate
        //     and the un-widened contextual type used for callback parameters.
        let widenable_placeholders: FxHashSet<TypeId> = var_map
            .keys()
            .filter(|&&placeholder_id| {
                if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(placeholder_id) {
                    // (a) Skip if constraint implies literal types
                    if let Some(constraint) = tp.constraint {
                        let inst = instantiate_type(self.interner, constraint, &substitution);
                        if type_implies_literals_deep(self.interner, inst) {
                            return false;
                        }
                    }
                    // (b) Skip if this placeholder is referenced in another type param's
                    // constraint (e.g., TContext in TMethods extends Record<..., (ctx: TContext) => ...>)
                    let is_referenced_in_other_constraints =
                        func.type_params.iter().any(|other_tp| {
                            if other_tp.name == tp.name {
                                return false; // Skip self
                            }
                            if let Some(constraint) = other_tp.constraint {
                                let inst =
                                    instantiate_type(self.interner, constraint, &substitution);
                                type_references_placeholder(self.interner, inst, placeholder_id)
                            } else {
                                false
                            }
                        });
                    !is_referenced_in_other_constraints
                } else {
                    false
                }
            })
            .copied()
            .collect();

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

            // For same-base Application types (e.g., Promise<__infer_0> vs Promise<Obj>),
            // also constrain type arguments directly. The general constraint engine
            // evaluates Applications to structural forms (Objects), but for interfaces
            // like Promise, structural decomposition through methods like `then` can't
            // reach type args because those methods have their own generic signatures.
            // This targeted pairwise extraction only runs during return type seeding,
            // so it doesn't affect parameter-based inference (preserving variance).
            if let (Some(TypeData::Application(s_app_id)), Some(TypeData::Application(t_app_id))) = (
                self.interner.lookup(return_type_with_placeholders),
                self.interner.lookup(ctx_type),
            ) {
                let s_app = self.interner.type_application(s_app_id);
                let t_app = self.interner.type_application(t_app_id);
                if s_app.base == t_app.base && s_app.args.len() == t_app.args.len() {
                    for (s_arg, t_arg) in s_app.args.iter().zip(t_app.args.iter()) {
                        self.constrain_types(
                            &mut infer_ctx,
                            &var_map,
                            *s_arg,
                            *t_arg,
                            crate::types::InferencePriority::ReturnType,
                        );
                    }
                }
            }
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

            // Keep round-2 contextual arguments for full checking, but only process
            // non-contextual arguments (and non-contextual parts of mixed objects) in
            // round 1.
            let Some((contextual_arg_type, contextual_target_type)) =
                self.contextual_round1_arg_types(arg_type, target_type)
            else {
                has_context_sensitive_args = true;
                continue;
            };
            if self.is_contextually_sensitive(arg_type) {
                has_context_sensitive_args = true;
            }

            // Direct placeholders (inference variables) are validated by final
            // constraint resolution below. Skipping eager checks here avoids
            // duplicate expensive assignability work on hot generic-call paths.
            let is_rest_param_arg = instantiated_params.last().is_some_and(|param| param.rest)
                && i >= instantiated_params.len().saturating_sub(1);

            if !var_map.contains_key(&target_type) {
                placeholder_visited.clear();
                if !self.type_contains_placeholder(target_type, &var_map, &mut placeholder_visited)
                {
                    // No placeholder in target_type - check assignability directly
                    if !self
                        .checker
                        .is_assignable_to(contextual_arg_type, contextual_target_type)
                        && !self
                            .is_function_union_compat(contextual_arg_type, contextual_target_type)
                    {
                        return CallResult::ArgumentTypeMismatch {
                            index: i,
                            expected: contextual_target_type,
                            actual: contextual_arg_type,
                            fallback_return: TypeId::ERROR,
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
                            if !self
                                .checker
                                .is_assignable_to(contextual_arg_type, inst_constraint)
                                && !self
                                    .is_function_union_compat(contextual_arg_type, inst_constraint)
                            {
                                return CallResult::ArgumentTypeMismatch {
                                    index: i,
                                    expected: inst_constraint,
                                    actual: contextual_arg_type,
                                    fallback_return: TypeId::ERROR,
                                };
                            }
                        }
                    }
                }
            } else if !is_rest_param_arg {
                direct_param_vars.extend(self.collect_placeholder_vars_in_type(
                    target_type,
                    &var_map,
                    &mut placeholder_probe_map,
                    &mut placeholder_visited,
                ));
            }

            // When the target is a bare type parameter placeholder whose constraint
            // doesn't imply literal types, widen the argument's object literal properties.
            // This matches tsc's behavior: `{ c: false }` passed to parameter `T` becomes
            // `{ c: boolean }` for inference, preventing false TS2322/TS2345 errors.
            let source_for_inference = if widenable_placeholders.contains(&contextual_target_type) {
                widening::widen_object_literal_properties(self.interner, contextual_arg_type)
            } else {
                contextual_arg_type
            };

            // arg_type <: target_type
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                source_for_inference,
                contextual_target_type,
                crate::types::InferencePriority::NakedTypeVariable,
            );
        }

        // Process rest tuple in Round 1 (it's non-contextual).
        // Skip when the rest param's type variable also appears in other parameter
        // types (e.g., `call<TS>(handler: (...args: TS) => void, ...args: TS)`).
        // In that case the other parameter provides a more authoritative constraint
        // (e.g., from the handler's callback params), and the rest args should be
        // validated against the inferred type, not used to infer it.
        if let Some((_start, target_type, tuple_type)) = rest_tuple_inference {
            let target_var_map: FxHashMap<TypeId, crate::inference::infer::InferenceVar> =
                FxHashMap::from_iter([(target_type, crate::inference::infer::InferenceVar(0))]);
            let appears_in_other_params = instantiated_params
                [..instantiated_params.len().saturating_sub(1)]
                .iter()
                .any(|p| {
                    placeholder_visited.clear();
                    self.type_contains_placeholder(
                        p.type_id,
                        &target_var_map,
                        &mut placeholder_visited,
                    )
                });
            if !appears_in_other_params {
                self.constrain_types(
                    &mut infer_ctx,
                    &var_map,
                    tuple_type,
                    target_type,
                    crate::types::InferencePriority::NakedTypeVariable,
                );
            }
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
                let is_rest_param_arg = instantiated_params.last().is_some_and(|param| param.rest)
                    && i >= instantiated_params.len().saturating_sub(1);

                if target_has_placeholders && !is_rest_param_arg {
                    direct_param_vars.extend(self.collect_placeholder_vars_in_type(
                        target_type,
                        &var_map,
                        &mut placeholder_probe_map,
                        &mut placeholder_visited,
                    ));
                }

                if !target_has_placeholders {
                    // No placeholders in target - direct assignability check
                    if !self.checker.is_assignable_to(arg_type, target_type)
                        && !self.is_function_union_compat(arg_type, target_type)
                    {
                        return CallResult::ArgumentTypeMismatch {
                            index: i,
                            expected: target_type,
                            actual: arg_type,
                            fallback_return: TypeId::ERROR,
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
            let lower_bounds = constraints
                .as_ref()
                .map(|c| c.lower_bounds.clone())
                .unwrap_or_default();

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
                let mut resolved_direct = None;

                if direct_param_vars.contains(&var)
                    && let Some(constraint_ty) = tp.constraint
                    && let Some(constraints) = constraints.as_ref()
                    && constraints.lower_bounds.contains(&constraint_ty)
                {
                    let mut non_constraint_bounds = Vec::new();
                    for bound in &constraints.lower_bounds {
                        if *bound != constraint_ty && !non_constraint_bounds.contains(bound) {
                            non_constraint_bounds.push(*bound);
                        }
                    }

                    if !non_constraint_bounds.is_empty() {
                        let candidate = self.resolve_direct_parameter_inference_type(
                            &non_constraint_bounds,
                            infer_ctx.best_common_type(&non_constraint_bounds),
                        );
                        let upper_bounds_ok = constraints.upper_bounds.iter().all(|upper| {
                            !matches!(upper, &TypeId::ANY | &TypeId::UNKNOWN | &TypeId::ERROR)
                                && infer_ctx.is_subtype(candidate, *upper)
                                || matches!(upper, &TypeId::ANY | &TypeId::UNKNOWN | &TypeId::ERROR)
                        });

                        if upper_bounds_ok {
                            resolved_direct = Some(candidate);
                        }
                    }
                }

                let ty = if let Some(resolved) = resolved_direct {
                    let root = infer_ctx.table.find(var);
                    let mut info = infer_ctx.table.probe_value(root);
                    info.resolved = Some(resolved);
                    infer_ctx.table.union_value(root, info);
                    resolved
                } else {
                    match infer_ctx.resolve_with_constraints_by(var, |source, target| {
                        self.checker.is_assignable_to(source, target)
                    }) {
                        Ok(ty) => {
                            let ty = if direct_param_vars.contains(&var) {
                                self.resolve_direct_parameter_inference_type(&lower_bounds, ty)
                            } else {
                                ty
                            };
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
                            let fallback = if direct_param_vars.contains(&var) {
                                self.resolve_direct_parameter_inference_type(
                                    &lower_bounds,
                                    fallback,
                                )
                            } else {
                                fallback
                            };
                            trace!(
                                fallback_type = ?fallback,
                                use_inferred = use_inferred,
                                "Using fallback type"
                            );
                            fallback
                        }
                    }
                };

                // Generic source contextual instantiation can produce temporary placeholders
                // (e.g. `__infer_src_*`) while collecting constraints for callback arguments.
                // Those placeholders must never leak into final instantiated signatures.
                if has_context_sensitive_args {
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
                let constraint_ty_raw = instantiate_type(self.interner, constraint, &final_subst);
                // Evaluate the instantiated constraint so concrete conditionals like
                // `null extends string ? any : never` resolve to their branch (`never`)
                // instead of remaining as unevaluated Conditional types.
                let constraint_ty = self.interner.evaluate_type(constraint_ty_raw);
                // Strip freshness before constraint check: inferred types should not
                // trigger excess property checking against type parameter constraints.
                let ty_for_check = crate::relations::freshness::widen_freshness(self.interner, ty);
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
        let return_type = instantiate_type(self.interner, func.return_type, &final_subst);
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
        // Store instantiated params for post-inference excess property checking.
        // The checker needs these to perform EPC on the concrete (post-inference)
        // parameter types rather than the raw types that still contain type parameters.
        // Store BEFORE the final check so they're available even if the check fails
        // (the checker uses these to perform EPC on ArgumentTypeMismatch too).
        self.last_instantiated_params = Some(instantiated_params.clone());

        if let Some(result) =
            self.check_argument_types_with(&instantiated_params, &final_args, true, func.is_method)
        {
            tracing::debug!("Final check failed: {:?}", result);
            return match result {
                CallResult::ArgumentTypeMismatch {
                    index,
                    expected,
                    actual,
                    ..
                } => CallResult::ArgumentTypeMismatch {
                    index,
                    expected,
                    actual,
                    fallback_return: return_type,
                },
                _ => result,
            };
        }
        tracing::debug!("Final check succeeded");

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

    fn resolve_direct_parameter_inference_type(
        &self,
        lower_bounds: &[TypeId],
        inferred: TypeId,
    ) -> TypeId {
        if lower_bounds.len() <= 1 {
            return inferred;
        }

        let inferred_union_members = match self.interner.lookup(inferred) {
            Some(TypeData::Union(member_list_id)) => {
                self.interner.type_list(member_list_id).to_vec()
            }
            _ => return inferred,
        };

        // If this is already a single-member union, keep it as-is.
        if inferred_union_members.len() <= 1 {
            return inferred;
        }

        // Direct arguments should stay narrow when there are heterogeneous candidates.
        // Otherwise TypeScript-style checks can get masked by a broad union result.
        if lower_bounds
            .iter()
            .all(|ty| self.is_mergeable_direct_inference_candidate(*ty))
        {
            return inferred;
        }

        // Fall back to the first lower-bound candidate so later argument checks
        // drive assignability failures on the mismatch site.
        lower_bounds[0]
    }

    fn collect_placeholder_vars_in_type(
        &self,
        ty: TypeId,
        var_map: &FxHashMap<TypeId, InferenceVar>,
        probe_map: &mut FxHashMap<TypeId, InferenceVar>,
        visited: &mut FxHashSet<TypeId>,
    ) -> FxHashSet<InferenceVar> {
        if var_map.is_empty() {
            return FxHashSet::default();
        }

        let mut result = FxHashSet::default();
        for (&placeholder_id, &var) in var_map.iter() {
            probe_map.clear();
            probe_map.insert(placeholder_id, var);
            visited.clear();
            if self.type_contains_placeholder(ty, probe_map, visited) {
                result.insert(var);
            }
        }

        result
    }

    fn should_skip_contextual_arg_in_round1(&self, arg_type: TypeId) -> bool {
        if !self.is_contextually_sensitive(arg_type) {
            return false;
        }

        match self.interner.lookup(arg_type) {
            Some(TypeData::Object(shape_id)) | Some(TypeData::ObjectWithIndex(shape_id)) => {
                if !crate::relations::freshness::is_fresh_object_type(self.interner, arg_type) {
                    return false;
                }
                let shape = self.interner.object_shape(shape_id);
                !shape
                    .properties
                    .iter()
                    .any(|prop| !self.is_contextually_sensitive(prop.type_id))
            }
            Some(TypeData::Application(_)) => false,
            _ => true,
        }
    }

    fn contextual_round1_arg_types(
        &self,
        arg_type: TypeId,
        target_type: TypeId,
    ) -> Option<(TypeId, TypeId)> {
        if !self.is_contextually_sensitive(arg_type) {
            return Some((arg_type, target_type));
        }

        if self.should_skip_contextual_arg_in_round1(arg_type) {
            return None;
        }

        let (Some(arg_obj), Some(target_obj)) =
            (
                match self.interner.lookup(arg_type) {
                    Some(TypeData::Object(shape_id))
                    | Some(TypeData::ObjectWithIndex(shape_id)) => Some(shape_id),
                    _ => None,
                },
                match self.interner.lookup(target_type) {
                    Some(TypeData::Object(shape_id))
                    | Some(TypeData::ObjectWithIndex(shape_id)) => Some(shape_id),
                    _ => None,
                },
            )
        else {
            return Some((arg_type, target_type));
        };

        let arg_shape = self.interner.object_shape(arg_obj);
        let target_shape = self.interner.object_shape(target_obj);

        let mut target_props_by_name: FxHashMap<_, _> = FxHashMap::default();
        for prop in &target_shape.properties {
            target_props_by_name.insert(prop.name, prop);
        }

        let mut arg_properties = Vec::new();
        let mut target_properties = Vec::new();
        for prop in &arg_shape.properties {
            if self.is_contextually_sensitive(prop.type_id) {
                continue;
            }

            if let Some(target_prop) = target_props_by_name.get(&prop.name) {
                arg_properties.push(prop.clone());
                target_properties.push((**target_prop).clone());
            }
        }

        if arg_properties.is_empty() {
            return None;
        }

        if arg_properties.len() == arg_shape.properties.len()
            && target_properties.len() == target_shape.properties.len()
        {
            return Some((arg_type, target_type));
        }

        let mut arg_shape = (*arg_shape).clone();
        arg_shape.properties = arg_properties;

        let mut target_shape = (*target_shape).clone();
        target_shape.properties = target_properties;

        Some((
            self.interner.object_with_index(arg_shape),
            self.interner.object_with_index(target_shape),
        ))
    }

    fn is_mergeable_direct_inference_candidate(&self, ty: TypeId) -> bool {
        // Primitives (null, undefined, string, number, boolean, void, never, etc.)
        // are always safe to merge into a union — they don't indicate structural
        // ambiguity. Without this, `equal(B, D | undefined)` would discard the
        // union and use only the first candidate, causing false TS2345 errors.
        if ty.is_nullish() || ty.is_any_or_unknown() || ty == TypeId::NEVER || ty == TypeId::VOID {
            return true;
        }
        match self.interner.lookup(ty) {
            Some(
                TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Intrinsic(_)
                | TypeData::Literal(_)
                | TypeData::Function(_)
                | TypeData::Callable(_),
            ) => true,
            Some(TypeData::Union(members)) => {
                let members = self.interner.type_list(members);
                !members.is_empty()
                    && members
                        .iter()
                        .all(|member| self.is_mergeable_direct_inference_candidate(*member))
            }
            _ => false,
        }
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
        let preserve_literal_arg = matches!(
            self.interner.lookup(arg_ty),
            Some(TypeData::Literal(_) | TypeData::TemplateLiteral(_) | TypeData::UniqueSymbol(_))
        );
        let inferred_ty = if tp.is_const || preserve_literal_arg {
            arg_ty
        } else {
            let widened =
                crate::operations::widening::widen_type(self.interner.as_type_database(), arg_ty);
            // When a constraint exists and the widened type violates it, fall back
            // to the unwidened (literal) type. This matches tsc's behavior: for
            // `<T extends 'a' | 'b'>(x: T)` called with `'a'`, T infers as `"a"`
            // (not `string`), because widening to `string` would violate the
            // constraint. Similarly for tuple constraints like
            // `<T extends [string, string, 'a' | 'b']>(x: T)`.
            if let Some(constraint) = tp.constraint {
                if !self.checker.is_assignable_to(widened, constraint)
                    && self.checker.is_assignable_to(arg_ty, constraint)
                {
                    arg_ty
                } else {
                    widened
                }
            } else {
                widened
            }
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
        let mut var_map: FxHashMap<TypeId, crate::inference::infer::InferenceVar> =
            FxHashMap::default();
        let mut type_param_vars = Vec::with_capacity(func.type_params.len());

        self.constraint_pairs.borrow_mut().clear();
        self.constraint_fixed_union_members.borrow_mut().clear();
        *self.constraint_recursion_depth.borrow_mut() = 0;
        *self.constraint_step_count.borrow_mut() = 0;

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

            // NOTE: We intentionally do NOT add the type parameter constraint as an
            // upper bound here. The constraint is already part of the TypeParameter
            // declaration and is used as a fallback in Pass 2 (when no inference
            // candidates or upper bounds exist). Adding it as an upper bound during
            // inference initialization would pollute the contextual substitution:
            // when the return type context provides a specific type (e.g.,
            // `(a: number) => void`), creating an intersection with the constraint
            // (e.g., `(...args: any[]) => any`) produces a merged Callable whose
            // conflicting parameter types cause `get_parameter_type` to return None,
            // triggering false TS7006 errors.
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

            // Same-base Application pairwise extraction (see resolve_generic_call_inner).
            if let (Some(TypeData::Application(s_app_id)), Some(TypeData::Application(t_app_id))) = (
                self.interner.lookup(return_type_with_placeholders),
                self.interner.lookup(ctx_type),
            ) {
                let s_app = self.interner.type_application(s_app_id);
                let t_app = self.interner.type_application(t_app_id);
                if s_app.base == t_app.base && s_app.args.len() == t_app.args.len() {
                    for (s_arg, t_arg) in s_app.args.iter().zip(t_app.args.iter()) {
                        self.constrain_types(
                            &mut infer_ctx,
                            &var_map,
                            *s_arg,
                            *t_arg,
                            InferencePriority::ReturnType,
                        );
                    }
                }
            }
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

            let Some((contextual_arg_type, contextual_target_type)) =
                self.contextual_round1_arg_types(arg_type, target_type)
            else {
                continue;
            };

            // Add constraint for non-contextual arguments
            self.constrain_types(
                &mut infer_ctx,
                &var_map,
                contextual_arg_type,
                contextual_target_type,
                InferencePriority::NakedTypeVariable,
            );

            // Same-base Application pairwise extraction for Round 1 arguments.
            // Example: A<(value: number) => void> against A<__infer_0> should infer
            // __infer_0 from the inner type argument, not rely only on structural
            // assignability of the outer application wrapper.
            if let (
                Some(TypeData::Application(arg_app_id)),
                Some(TypeData::Application(target_app_id)),
            ) = (
                self.interner.lookup(contextual_arg_type),
                self.interner.lookup(contextual_target_type),
            ) {
                let arg_app = self.interner.type_application(arg_app_id);
                let target_app = self.interner.type_application(target_app_id);
                if arg_app.base == target_app.base && arg_app.args.len() == target_app.args.len() {
                    for (arg_inner, target_inner) in arg_app.args.iter().zip(target_app.args.iter())
                    {
                        self.constrain_types(
                            &mut infer_ctx,
                            &var_map,
                            *arg_inner,
                            *target_inner,
                            InferencePriority::NakedTypeVariable,
                        );
                    }
                }
            }
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
        let _ = infer_ctx.strengthen_constraints();

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
            let preferred_lower_bound =
                infer_ctx
                    .get_constraints(type_param_vars[i])
                    .and_then(|constraints| {
                        let constraint_ty = tp.constraint?;
                        let mut non_constraint_bounds = Vec::new();
                        for bound in &constraints.lower_bounds {
                            if *bound != constraint_ty && !non_constraint_bounds.contains(bound) {
                                non_constraint_bounds.push(*bound);
                            }
                        }
                        if non_constraint_bounds.is_empty() {
                            return None;
                        }
                        let candidate = self.resolve_direct_parameter_inference_type(
                            &non_constraint_bounds,
                            infer_ctx.best_common_type(&non_constraint_bounds),
                        );
                        let upper_bounds_ok = constraints.upper_bounds.iter().all(|upper| {
                            !matches!(upper, &TypeId::ANY | &TypeId::UNKNOWN | &TypeId::ERROR)
                                && infer_ctx.is_subtype(candidate, *upper)
                                || matches!(upper, &TypeId::ANY | &TypeId::UNKNOWN | &TypeId::ERROR)
                        });
                        upper_bounds_ok.then_some(candidate)
                    });
            let resolved = preferred_lower_bound.or_else(|| {
                match infer_ctx.resolve_with_constraints_by(type_param_vars[i], |source, target| {
                    self.checker.is_assignable_to(source, target)
                }) {
                    Ok(resolved) => Some(resolved),
                    Err(_) => infer_subst.get(placeholder_atom),
                }
            });
            if let Some(resolved) = resolved {
                if resolved != TypeId::UNKNOWN {
                    result_subst.insert(tp.name, resolved);
                } else {
                    unresolved_indices.push(i);
                }
            } else {
                unresolved_indices.push(i);
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

        trace!(
            contextual_result_subst = ?result_subst
                .map()
                .iter()
                .map(|(name, ty)| (
                    self.interner.resolve_atom(*name).to_string(),
                    *ty,
                    self.interner.lookup(*ty),
                ))
                .collect::<Vec<_>>(),
            "compute_contextual_types: final substitution"
        );

        result_subst
    }
}

/// Check if a type contains literal types — recursing into unions, intersections,
/// and object properties. Used to detect discriminated union constraints like
/// `{ kind: "a" } | { kind: "b" }` where the literal property types should
/// prevent widening of the corresponding argument properties.
fn type_implies_literals_deep(db: &dyn crate::TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::Literal(_)) => true,
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| type_implies_literals_deep(db, m))
        }
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| type_implies_literals_deep(db, m))
        }
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            shape
                .properties
                .iter()
                .any(|prop| type_implies_literals_deep(db, prop.type_id))
        }
        _ => false,
    }
}

/// Check if a type structurally contains a reference to a specific placeholder TypeId.
/// Used to detect when a type parameter (e.g., `TContext`) is referenced inside another
/// type parameter's constraint (e.g., `TMethods` extends Record<string, (ctx: `TContext`) => unknown>).
fn type_references_placeholder(
    db: &dyn crate::TypeDatabase,
    type_id: TypeId,
    placeholder: TypeId,
) -> bool {
    if type_id == placeholder {
        return true;
    }
    match db.lookup(type_id) {
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members
                .iter()
                .any(|&m| type_references_placeholder(db, m, placeholder))
        }
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            shape.properties.iter().any(|prop| {
                type_references_placeholder(db, prop.type_id, placeholder)
                    || type_references_placeholder(db, prop.write_type, placeholder)
            })
        }
        Some(TypeData::Array(elem)) => type_references_placeholder(db, elem, placeholder),
        Some(TypeData::Tuple(list_id)) => {
            let elems = db.tuple_list(list_id);
            elems
                .iter()
                .any(|e| type_references_placeholder(db, e.type_id, placeholder))
        }
        Some(TypeData::Function(fn_id)) => {
            let func = db.function_shape(fn_id);
            func.params
                .iter()
                .any(|p| type_references_placeholder(db, p.type_id, placeholder))
                || type_references_placeholder(db, func.return_type, placeholder)
        }
        Some(TypeData::Application(app_id)) => {
            let app = db.type_application(app_id);
            type_references_placeholder(db, app.base, placeholder)
                || app
                    .args
                    .iter()
                    .any(|&a| type_references_placeholder(db, a, placeholder))
        }
        Some(TypeData::Conditional(cond_id)) => {
            let cond = db.conditional_type(cond_id);
            type_references_placeholder(db, cond.check_type, placeholder)
                || type_references_placeholder(db, cond.extends_type, placeholder)
                || type_references_placeholder(db, cond.true_type, placeholder)
                || type_references_placeholder(db, cond.false_type, placeholder)
        }
        Some(TypeData::IndexAccess(obj, idx)) => {
            type_references_placeholder(db, obj, placeholder)
                || type_references_placeholder(db, idx, placeholder)
        }
        Some(TypeData::KeyOf(inner)) => type_references_placeholder(db, inner, placeholder),
        Some(TypeData::Mapped(mapped_id)) => {
            let mapped = db.mapped_type(mapped_id);
            type_references_placeholder(db, mapped.template, placeholder)
                || type_references_placeholder(db, mapped.constraint, placeholder)
                || mapped
                    .name_type
                    .is_some_and(|n| type_references_placeholder(db, n, placeholder))
        }
        _ => false,
    }
}
