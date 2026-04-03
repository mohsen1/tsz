//! Trivial call resolution, placeholder normalization, and contextual type computation.

use crate::inference::infer::{InferenceContext, InferenceVar};
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::{AssignabilityChecker, CallEvaluator, CallResult};
use crate::types::{FunctionShape, ParamInfo, TypeData, TypeId, TypeParamInfo};
use rustc_hash::{FxHashMap, FxHashSet};

use super::{constraint_is_primitive_type, unique_placeholder_name};

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// Fast path for identity-style generic calls:
    /// `<T extends C>(x: T) => T` with a single non-rest argument.
    ///
    /// This shape is common in constraint-heavy code and does not require full
    /// multi-pass inference machinery. We can infer `T` directly from the argument,
    /// validate the constraint once, and return the argument type.
    pub(super) fn resolve_trivial_single_type_param_call(
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

        // Bail out for self-referential constraints like `T extends Test<keyof T>`.
        // The fast path cannot properly instantiate the constraint with the inferred
        // type (it checks the raw constraint), and it uses `widen_type` which
        // deep-widens object properties. The normal inference path handles this
        // correctly by instantiating the constraint with `final_subst`.
        if let Some(constraint) = tp.constraint
            && crate::visitor::contains_type_parameter_named(
                self.interner.as_type_database(),
                constraint,
                tp.name,
            )
        {
            return None;
        }

        let arg_ty = arg_types[0];
        let inferred_ty = if tp.is_const {
            arg_ty
        } else {
            // When the declared constraint is a primitive type (string, number,
            // boolean, bigint) or a union thereof, preserve literal types without
            // widening. This matches tsc's getInferredType which checks
            // isLiteralType(constraint) and skips getWidenedLiteralType.
            // Example: `<T extends string>(x: T): T` called with `"hello"`
            // should infer T = "hello", not T = string.
            let constraint_is_primitive = tp
                .constraint
                .is_some_and(|c| constraint_is_primitive_type(self.interner, c));
            if constraint_is_primitive {
                arg_ty
            } else {
                let widened = crate::operations::widening::widen_type(
                    self.interner.as_type_database(),
                    arg_ty,
                );
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
                } else if let Some(ctx_type) = self.contextual_type
                    && ctx_type != TypeId::ANY
                    && ctx_type != TypeId::UNKNOWN
                    && widened != arg_ty
                    && !self.checker.is_assignable_to(widened, ctx_type)
                    && self.checker.is_assignable_to(arg_ty, ctx_type)
                {
                    // When a contextual return type exists (e.g., from a variable
                    // declaration like `let v: DooDad = identity('ELSE')`), and
                    // widening the argument type breaks assignability to the
                    // contextual type, preserve the literal. This matches tsc's
                    // behavior where contextual return types inform inference and
                    // prevent unnecessary widening of literals.
                    arg_ty
                } else {
                    widened
                }
            }
        };
        if let Some(constraint) = tp.constraint
            && !self.checker.is_assignable_to(inferred_ty, constraint)
            && !self.is_function_union_compat(inferred_ty, constraint)
        {
            // In the trivial single-type-param fast path, the parameter IS the
            // type parameter itself, so a constraint violation means the argument
            // doesn't match the effective parameter type (the constraint).
            // tsc reports TS2345 ("Argument of type X is not assignable to
            // parameter of type Y") here, not TS2322.
            return Some(CallResult::ArgumentTypeMismatch {
                index: 0,
                expected: constraint,
                actual: inferred_ty,
                fallback_return: inferred_ty,
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
    pub(super) fn normalize_inferred_placeholder_type(
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

        let mut source_placeholder_subst = TypeSubstitution::new();
        for ty in crate::visitor::collect_all_types(self.interner.as_type_database(), current) {
            if let Some(TypeData::TypeParameter(info)) = self.interner.lookup(ty)
                && self
                    .interner
                    .resolve_atom(info.name)
                    .as_str()
                    .starts_with("__infer_src_")
            {
                source_placeholder_subst.insert(info.name, TypeId::UNKNOWN);
            }
        }
        if !source_placeholder_subst.is_empty() {
            current = instantiate_type(self.interner, current, &source_placeholder_subst);
        }

        self.prune_placeholder_union_members(current)
    }

    fn prune_placeholder_union_members(&self, ty: TypeId) -> TypeId {
        let Some(TypeData::Union(member_list_id)) = self.interner.lookup(ty) else {
            return ty;
        };

        let members = self.interner.type_list(member_list_id);
        let retained: Vec<_> = members
            .iter()
            .copied()
            .filter(|member| {
                !crate::type_queries::contains_infer_types_db(
                    self.interner.as_type_database(),
                    *member,
                )
            })
            .collect();

        if retained.is_empty() || retained.len() == members.len() {
            return ty;
        }

        if retained.len() == 1 {
            retained[0]
        } else {
            self.interner.union_preserve_members(retained)
        }
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
        let has_context_sensitive_args = arg_types
            .iter()
            .copied()
            .any(|arg| self.is_contextually_sensitive(arg));

        // Save state to prevent pollution if evaluator is reused
        let previous_defaulted = std::mem::take(&mut self.defaulted_placeholders);

        let mut infer_ctx = InferenceContext::with_resolver(
            self.interner.as_type_database(),
            self.interner.as_type_resolver(),
        );
        let mut substitution = TypeSubstitution::new();
        let mut var_map: FxHashMap<TypeId, crate::inference::infer::InferenceVar> =
            FxHashMap::default();
        let mut type_param_vars = Vec::with_capacity(func.type_params.len());
        let mut type_param_placeholder_atoms: Vec<tsz_common::Atom> =
            Vec::with_capacity(func.type_params.len());

        self.constraint_pairs.borrow_mut().clear();
        self.constraint_fixed_union_members.borrow_mut().clear();
        self.constraint_recursion_depth.set(0);
        self.constraint_step_count.set(0);

        let mut placeholder_probe_map: FxHashMap<TypeId, InferenceVar> = FxHashMap::default();
        let mut placeholder_visited = FxHashSet::default();
        // Reusable buffer for placeholder names (avoids per-iteration String allocation)
        let mut placeholder_buf = String::with_capacity(24);

        // 1. Create inference variables and placeholders for each type parameter
        for tp in &func.type_params {
            let var = infer_ctx.fresh_var();
            type_param_vars.push(var);

            unique_placeholder_name(&mut placeholder_buf);
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
            type_param_placeholder_atoms.push(placeholder_atom);

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
        let mut instantiated_params: Vec<ParamInfo> = func
            .params
            .iter()
            .map(|p| ParamInfo {
                name: p.name,
                type_id: instantiate_type(self.interner, p.type_id, &substitution),
                optional: p.optional,
                rest: p.rest,
            })
            .collect();
        let mut round1_direct_seed_vars = FxHashSet::default();
        for (i, &arg_type) in arg_types.iter().enumerate() {
            let Some(target_type) =
                self.param_type_for_arg_index(&instantiated_params, i, arg_types.len())
            else {
                break;
            };
            if self
                .contextual_round1_arg_types(arg_type, target_type)
                .is_some()
            {
                round1_direct_seed_vars.extend(self.collect_placeholder_vars_in_type(
                    target_type,
                    &var_map,
                    &mut placeholder_probe_map,
                    &mut placeholder_visited,
                ));
            }
        }

        // 2.5. Seed contextual constraints from return type
        // Skip `any` and `unknown` — they don't contribute useful inference constraints
        if !has_context_sensitive_args
            && let Some(ctx_type) = self.contextual_type
            && ctx_type != TypeId::ANY
            && ctx_type != TypeId::UNKNOWN
        {
            let return_type_with_placeholders =
                instantiate_type(self.interner, func.return_type, &substitution);
            let return_seed_vars = self.collect_placeholder_vars_in_type(
                return_type_with_placeholders,
                &var_map,
                &mut placeholder_probe_map,
                &mut placeholder_visited,
            );
            // Same logic as primary resolve path: skip only when ALL return vars
            // are covered by round-1 inference.
            let all_return_vars_covered = !return_seed_vars.is_empty()
                && return_seed_vars
                    .iter()
                    .all(|var| round1_direct_seed_vars.contains(var));
            if !all_return_vars_covered {
                self.constrain_types(
                    &mut infer_ctx,
                    &var_map,
                    return_type_with_placeholders,
                    ctx_type,
                    InferencePriority::ReturnType,
                );

                self.constrain_return_context_structure(
                    &mut infer_ctx,
                    &var_map,
                    return_type_with_placeholders,
                    ctx_type,
                    InferencePriority::ReturnType,
                );
            }
        }

        let structural_return_subst = if has_context_sensitive_args {
            TypeSubstitution::new()
        } else {
            // When the source function's return type is a bare type parameter
            // that also appears in its parameter list (like `f<T>(x: T): T`),
            // and the contextual type is a function (from
            // instantiate_generic_function_argument_against_target), extract
            // the function's RETURN TYPE for return context substitution.
            // Without this, `f<T>(x: T): T` passed as `(x: number) => number`
            // would substitute T → (x: number) => number (the full function type)
            // instead of T → number (the return type), causing false TS2322.
            let return_type_is_param_shared_with_params = func.type_params.iter().any(|tp| {
                let ret_is_this_param = matches!(
                    self.interner.lookup(func.return_type),
                    Some(TypeData::TypeParameter(ref info)) if info.name == tp.name
                );
                let param_uses_this_param = func.params.iter().any(|p| {
                    crate::visitor::collect_referenced_types(
                        self.interner.as_type_database(),
                        p.type_id,
                    )
                    .into_iter()
                    .any(|ty| {
                        crate::type_param_info(self.interner.as_type_database(), ty)
                            .is_some_and(|info| info.name == tp.name)
                    })
                });
                ret_is_this_param && param_uses_this_param
            });

            let ctx_for_return = if return_type_is_param_shared_with_params {
                self.contextual_type.map(|ctx| {
                    // Only extract return type from CONCRETE function types
                    // (no type parameters). When the contextual is generic
                    // (e.g. (x: U) => T from outer inference), keep the full
                    // function type so inner inference sees the structure.
                    if crate::visitor::contains_type_parameters(
                        self.interner.as_type_database(),
                        ctx,
                    ) {
                        ctx
                    } else if let Some(fn_shape) = crate::type_queries::get_function_shape(
                        self.interner.as_type_database(),
                        ctx,
                    ) {
                        fn_shape.return_type
                    } else {
                        ctx
                    }
                })
            } else {
                self.contextual_type
            };
            self.compute_return_context_substitution(func, ctx_for_return)
        };
        if !structural_return_subst.is_empty() {
            for (&name, &ty) in structural_return_subst.map().iter() {
                substitution.insert(name, ty);
            }
            instantiated_params = func
                .params
                .iter()
                .map(|p| ParamInfo {
                    name: p.name,
                    type_id: instantiate_type(self.interner, p.type_id, &substitution),
                    optional: p.optional,
                    rest: p.rest,
                })
                .collect();
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
                self.constrain_sensitive_function_return_types(
                    &mut infer_ctx,
                    &var_map,
                    arg_type,
                    target_type,
                    InferencePriority::NakedTypeVariable,
                );
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

            let source_is_function = self.type_evaluates_to_function(contextual_arg_type);
            let target_is_function = self.type_evaluates_to_function(contextual_target_type);
            if source_is_function || target_is_function {
                self.constrain_return_context_structure(
                    &mut infer_ctx,
                    &var_map,
                    contextual_arg_type,
                    contextual_target_type,
                    InferencePriority::NakedTypeVariable,
                );
            }

            if let (
                Some(TypeData::Application(arg_app_id)),
                Some(TypeData::Application(target_app_id)),
            ) = (
                self.interner.lookup(contextual_arg_type),
                self.interner.lookup(contextual_target_type),
            ) {
                let arg_app = self.interner.type_application(arg_app_id);
                let target_app = self.interner.type_application(target_app_id);
                if arg_app.base == target_app.base
                    && arg_app.args.len() == target_app.args.len()
                    && self.should_directly_constrain_same_base_application(
                        contextual_arg_type,
                        contextual_target_type,
                    )
                {
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
        let _ = infer_ctx.fix_current_variables_with(Some(|source, target| {
            self.checker.is_assignable_to(source, target)
        }));

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
            let placeholder_atom = type_param_placeholder_atoms[i];
            // Skip the preferred_lower_bound optimization in compute_contextual_types.
            // Unlike resolve_generic_call_inner (which gates this on direct_param_vars
            // for parameters where the type IS the type parameter, like f<T>(x: T)),
            // compute_contextual_types lacks that tracking. Applying it unconditionally
            // can remove genuine candidates that happen to match the constraint type,
            // leading to false positives when the constraint is also a valid candidate
            // from object property inference.
            let preferred_lower_bound: Option<TypeId> = None;
            let resolved = preferred_lower_bound.or_else(|| {
                match infer_ctx.resolve_with_constraints_by(type_param_vars[i], |source, target| {
                    self.checker.is_assignable_to_strict(source, target)
                }) {
                    Ok(resolved) => Some(resolved),
                    Err(_) => self
                        .single_concrete_upper_bound(&mut infer_ctx, type_param_vars[i])
                        .or_else(|| infer_subst.get(placeholder_atom)),
                }
            });
            if let Some(resolved) = resolved {
                let resolved = self.normalize_inferred_placeholder_type(resolved, &infer_subst);
                let resolved = if !has_context_sensitive_args
                    && let Some(contextual_ty) = structural_return_subst.get(tp.name)
                {
                    if self.can_apply_contextual_return_substitution(
                        &mut infer_ctx,
                        type_param_vars[i],
                        resolved,
                        &var_map,
                    ) && self.should_use_contextual_return_substitution(
                        resolved,
                        contextual_ty,
                        &var_map,
                    ) {
                        contextual_ty
                    } else {
                        resolved
                    }
                } else {
                    resolved
                };
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
            // unique placeholder names instead of `T`, avoiding name collisions.
            {
                let placeholder_atom = type_param_placeholder_atoms[i];
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
