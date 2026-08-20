use crate::inference::infer::{InferenceError, InferenceVar};
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::generic_call::foreign_param_shapes::{
    is_bare_foreign_type_param, is_substantive_inference_candidate,
};
use crate::operations::generic_call::readonly_direct_inference;
use crate::operations::generic_call::{
    constraint_contains_primitive_constrained_type_param,
    constraint_is_primitive_type_with_resolver, instantiate_call_type, type_implies_literals_deep,
};
use crate::operations::widening;
use crate::operations::{AssignabilityChecker, CallEvaluator, CallResult};
use crate::types::{FunctionShape, ParamInfo, TypeData, TypeId, TypeParamInfo, TypePredicate};
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::trace;

use super::{CallbackPositionVars, FinishGenericCallResolutionArgs};

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    fn transparent_bare_rest_type_parameter_query(
        &self,
        type_id: TypeId,
    ) -> crate::type_queries::RestBinderQuery<Option<TypeParamInfo>> {
        if let Some(resolver) = self.checker.type_resolver() {
            crate::type_queries::transparent_bare_rest_type_parameter_with_resolver_query(
                self.interner,
                &resolver,
                type_id,
            )
        } else {
            crate::type_queries::transparent_bare_rest_type_parameter_with_resolver_query(
                self.interner,
                &self.interner,
                type_id,
            )
        }
    }

    fn rest_type_has_union_surface_query(
        &self,
        type_id: TypeId,
    ) -> crate::type_queries::RestBinderQuery<bool> {
        if let Some(resolver) = self.checker.type_resolver() {
            crate::type_queries::rest_type_has_union_surface_with_resolver_query(
                self.interner,
                &resolver,
                type_id,
            )
        } else {
            crate::type_queries::rest_type_has_union_surface_with_resolver_query(
                self.interner,
                &self.interner,
                type_id,
            )
        }
    }

    fn call_signatures_query(
        &self,
        type_id: TypeId,
    ) -> crate::type_queries::RestBinderQuery<Option<Vec<crate::type_queries::RestCallableSignature>>>
    {
        if let Some(resolver) = self.checker.type_resolver() {
            crate::type_queries::direct_provisional_call_signatures_with_resolver(
                self.interner,
                &resolver,
                type_id,
            )
        } else {
            crate::type_queries::direct_provisional_call_signatures_with_resolver(
                self.interner,
                &self.interner,
                type_id,
            )
        }
    }

    fn direct_provisional_rest_union_site(
        &self,
        raw_type: TypeId,
        instantiated_type: TypeId,
        aggregate_rest_type_params: &[TypeParamInfo],
    ) -> bool {
        if aggregate_rest_type_params.is_empty() {
            return false;
        }
        let instantiated_signatures = match self.call_signatures_query(instantiated_type) {
            crate::type_queries::RestBinderQuery::Complete(Some(signatures)) => signatures,
            crate::type_queries::RestBinderQuery::Complete(None)
            | crate::type_queries::RestBinderQuery::Incomplete => return false,
        };
        let raw_signatures = match self.call_signatures_query(raw_type) {
            crate::type_queries::RestBinderQuery::Complete(Some(signatures)) => signatures,
            crate::type_queries::RestBinderQuery::Complete(None)
            | crate::type_queries::RestBinderQuery::Incomplete => return false,
        };
        if raw_signatures.len() != instantiated_signatures.len() {
            return false;
        }
        let mut found_call_owned_union = false;
        for (raw, instantiated) in raw_signatures.iter().zip(instantiated_signatures.iter()) {
            if raw.is_method != instantiated.is_method
                || raw.params.len() != instantiated.params.len()
            {
                return false;
            }
            let Some(instantiated_rest) = instantiated.params.last().filter(|param| param.rest)
            else {
                continue;
            };
            let instantiated_has_union =
                match self.rest_type_has_union_surface_query(instantiated_rest.type_id) {
                    crate::type_queries::RestBinderQuery::Complete(value) => value,
                    crate::type_queries::RestBinderQuery::Incomplete => return false,
                };
            if !instantiated_has_union {
                continue;
            }

            // The relation flag applies to the whole callable value. Arm it
            // only when every union-rest overload at this site is the aligned
            // instantiation of a raw rest binder owned by this call. A sibling
            // user-written union overload must keep ordinary strict semantics.
            let Some(raw_rest) = raw.params.last().filter(|param| param.rest) else {
                return false;
            };
            let raw_is_call_owned = matches!(
                self.transparent_bare_rest_type_parameter_query(raw_rest.type_id),
                crate::type_queries::RestBinderQuery::Complete(Some(info))
                    if aggregate_rest_type_params
                        .iter()
                        .any(|type_param| type_param.is_same_binder(info))
            );
            if !raw_is_call_owned {
                return false;
            }
            found_call_owned_union = true;
        }
        found_call_owned_union
    }

    /// Register the exact declaration-scoped identities owned by `func`.
    ///
    /// Declaration origins are stable across reconstructed `TypeId`s, including
    /// occurrences below nested generic return signatures, so installation is
    /// O(type params) and needs no graph walk. The common unstamped signature
    /// remains on the legacy name-only path.
    fn protect_call_owned_type_parameters(
        &self,
        func: &FunctionShape,
        substitution: &mut TypeSubstitution,
    ) {
        substitution.protect_type_parameters(&func.type_params);
    }

    pub(super) fn finish_generic_call_resolution(
        &mut self,
        args: FinishGenericCallResolutionArgs<'_>,
    ) -> CallResult {
        let FinishGenericCallResolutionArgs {
            func,
            arg_types,
            actual_this_type,
            infer_ctx,
            substitution,
            type_param_vars,
            type_param_placeholder_atoms,
            var_map,
            direct_param_vars,
            callback_placeholder_subst,
            round1_fixed,
            noinfer_param_vars,
            rest_tuple_target_type,
            aggregate_rest_inference_vars,
            structural_return_subst,
            first_direct_primitive_mismatch,
            saw_deferred_arg,
        } = args;
        // tsc's `inference.isFixed` set for this call. Recomputed here from the
        // (stable) AST masks and declared parameter types so a nested Round-2
        // resolution between the mark site and finalization cannot perturb it.
        // Issue #17710.
        let contextually_fixed = self.contextually_fixed_type_params(func, arg_types);
        let aggregate_rest_type_params = func
            .type_params
            .iter()
            .zip(type_param_vars.iter())
            .filter_map(|(type_param, inference_var)| {
                aggregate_rest_inference_vars
                    .contains(inference_var)
                    .then_some(*type_param)
            })
            .collect::<Vec<_>>();
        let mut infer_ctx = infer_ctx;
        // 4. Resolve inference variables
        // CRITICAL: Strengthen inter-parameter constraints before resolution
        // This ensures SCC-based cycle unification happens (commit c3ede45a9)
        if infer_ctx.strengthen_constraints().is_err() {
            // Cycle unification failed - this indicates a circularity that cannot be resolved
            // Fall back to resolving without unification (may result in less precise types)
        }

        // 4.5. Resolve source inference variables (from generic function arguments)
        // and substitute them into outer variables' candidates.
        //
        // When a generic function like `list<T>` is passed as an argument, the constraint
        // collector creates fresh inference vars (`__infer_src_*`) for its type params.
        // These may leak into outer variables' candidates as raw TypeParam placeholders.
        // We resolve them here and substitute concrete types back, so the outer resolution
        // sees real types (e.g., `T[]`) instead of opaque placeholders (e.g., `__infer_src_3`).
        //
        // Multi-pass: After substituting resolved source vars into outer candidates,
        // resolve outer vars, then re-derive remaining source vars from outer results.
        {
            let outer_var_set: FxHashSet<InferenceVar> = type_param_vars.iter().copied().collect();
            let mut source_subst = TypeSubstitution::new();
            let type_params_snapshot: Vec<_> = infer_ctx.type_params.clone();
            // Pass 1: Resolve source vars with direct candidates (not unknown)
            for (name, var, _) in &type_params_snapshot {
                if !outer_var_set.contains(var)
                    && let Ok(resolved) = infer_ctx.resolve_with_constraints(*var)
                    && resolved != TypeId::UNKNOWN
                {
                    source_subst.insert(*name, resolved);
                }
            }
            if !source_subst.is_empty() {
                infer_ctx.substitute_source_vars_in_targets(
                    type_param_vars,
                    &source_subst,
                    self.interner,
                );
            }
        }

        // Partition the type-parameter inference vars that a context-sensitive
        // callback argument reaches by where they appear in the callback's
        // *expected* signature. A var in a callback **parameter** (contravariant)
        // position cannot be recovered from the callback body during Round 2,
        // whereas a var in the callback **return** (covariant) position can. Vars
        // reached only through callback parameter positions must therefore accept
        // a binding from the contextual return type even though they also occupy a
        // direct-parameter slot; otherwise the binding is dropped and the
        // parameter silently widens to its constraint/`unknown`. This mirrors
        // `tsc`, which fixes such parameters from the return context (e.g.
        // `fromCompare<A>(compare: Ord<A>['compare']): Ord<A>` called with a bare
        // lambda under a contextual `Ord<string>`). Computed here, after argument
        // inference has settled, so the evaluation of indexed-access/alias callback
        // signatures cannot perturb in-flight inference.
        let CallbackPositionVars {
            param_only: callback_param_only_vars,
            return_position: callback_return_position_vars,
        } = self.callback_position_inference_vars(func, callback_placeholder_subst, var_map);
        // #17282: an `any`/`unknown` candidate on any Round-1-fixed callback
        // return-position variable marks the whole call as `any`-tainted. tsc
        // lets such an `any` collapse the type parameters and suppress the
        // callback-return mismatch, and tsz's two-round resolver cannot propagate
        // that collapse across a second callback's parameter. Rather than restore
        // a fix that a propagated `any` would have erased, fall back to the
        // widened (pre-#17282) inference for the entire call — the conservative,
        // no-regression direction (e.g.
        // `typeParameterFixingWithContextSensitiveArguments5`, whose `any`-typed
        // callback body `tsc` accepts).
        let any_tainted_frozen_call = callback_return_position_vars.iter().any(|&var| {
            round1_fixed.contains_key(&var)
                && infer_ctx
                    .get_constraints(var)
                    .is_some_and(|c| c.lower_bounds.iter().any(|b| b.is_any_unknown_or_error()))
        });

        let mut final_subst = TypeSubstitution::new();
        self.protect_call_owned_type_parameters(func, &mut final_subst);
        let mut infer_subst_cache: Option<TypeSubstitution> = None;
        // Track type parameters that fell back to their defaults because inference
        // produced no candidates. For these, we should NOT check argument assignability
        // against the default - the default is a fallback, not a constraint.
        let mut default_fallback_tp_names: FxHashSet<tsz_common::Atom> = FxHashSet::default();
        for (tp, &var) in func.type_params.iter().zip(type_param_vars.iter()) {
            let constraints = infer_ctx.get_constraints(var);
            // Check both ConstraintSet (covariant candidates + upper bounds) and
            // usable contra_candidates. Contra-candidates are NOT in
            // ConstraintSet.lower_bounds to avoid polluting the resolved_direct path,
            // but they still represent valid inference that should trigger resolution.
            // Ignore only synthetic placeholder type parameters; real outer type
            // parameters like `T` must still count as usable evidence.
            let has_constraints = matches!(&constraints, Some(c) if !c.is_empty())
                || infer_ctx.has_usable_contra_candidates(var, self.interner.as_type_database());
            // A defaulted parameter prefers its default over bounds that were merely
            // DECLARED on it (`<T extends A | B = B>` resolves to `B`, not to the
            // constraint) — but only bounds the declaration actually introduced count.
            // Requiring `tp.constraint.is_some()` is what makes the name honest: with no
            // `extends` clause there are no declared upper bounds, so any upper bound
            // present is real inference evidence and must not be discarded.
            //
            // `declare function mk<U = void>(f: (r: U) => void): Array<U>` used as
            // `const p: Array<string> = mk((r) => ...)` seeds U from the contextual RETURN
            // type (normalization.rs:976, `InferencePriority::ReturnType`). For a callback
            // parameter position that candidate lands in UPPER bounds with no lower bound
            // and no usable contra-candidate, so without this conjunct the whole candidate
            // path was skipped and U silently became `void`. Dropping ` = void` made the
            // same call resolve correctly, which is exactly the asymmetry this fixes.
            // (`mk<U = void>(x?: U)` was already correct: a direct parameter position
            // yields a LOWER bound, so the gate never fired for it.)
            let has_only_declared_upper_bounds = tp.default.is_some()
                && tp.constraint.is_some()
                && !infer_ctx.has_usable_contra_candidates(var, self.interner.as_type_database())
                && constraints
                    .as_ref()
                    .is_some_and(|c| c.lower_bounds.is_empty() && !c.upper_bounds.is_empty());
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
            let ty = if has_constraints && !has_only_declared_upper_bounds {
                let mut resolved_direct = None;
                let contra_only = infer_ctx.has_only_contra_candidates(var);
                let has_usable_contra_candidates =
                    infer_ctx.has_usable_contra_candidates(var, self.interner.as_type_database());

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
                        // When all non-constraint bounds are subtypes of the constraint,
                        // the constraint is the correct inference result (it's the best
                        // common supertype). Stripping it would incorrectly narrow T to
                        // a subtype. Example: foo<T extends C>(t: X<T>, t2: X<T>) called
                        // with X<C> and X<D> where D extends C — T should be C, not D.
                        let all_subtypes_of_constraint = non_constraint_bounds
                            .iter()
                            .all(|&bound| self.checker.is_assignable_to(bound, constraint_ty));
                        if !all_subtypes_of_constraint {
                            let leftmost_dropped_by_priority =
                                infer_ctx.has_mixed_priority_candidates(var);
                            let candidate = self.resolve_direct_parameter_inference_type(
                                &non_constraint_bounds,
                                infer_ctx.best_common_type(&non_constraint_bounds),
                                has_usable_contra_candidates,
                                infer_ctx.has_fresh_array_element_candidate(var),
                                leftmost_dropped_by_priority,
                            );
                            let upper_bounds_ok = constraints.upper_bounds.iter().all(|upper| {
                                !matches!(upper, &TypeId::ANY | &TypeId::UNKNOWN | &TypeId::ERROR)
                                    && infer_ctx.is_subtype(candidate, *upper)
                                    || matches!(
                                        upper,
                                        &TypeId::ANY | &TypeId::UNKNOWN | &TypeId::ERROR
                                    )
                            });

                            if upper_bounds_ok {
                                resolved_direct = Some(candidate);
                            }
                        }
                    }
                }

                let has_index_signature_candidates = infer_ctx.has_index_signature_candidates(var);
                let ty = if let Some(resolved) = resolved_direct {
                    let root = infer_ctx.table.find(var);
                    let mut info = infer_ctx.table.probe_value(root);
                    info.resolved = Some(resolved);
                    infer_ctx.table.union_value(root, info);
                    resolved
                } else {
                    match infer_ctx.resolve_with_constraints_by(var, |source, target| {
                        self.checker.is_assignable_to_strict(source, target)
                    }) {
                        Ok(ty) => {
                            let all_return_type = infer_ctx.all_candidates_are_return_type(var);
                            trace!(
                                var = ?var,
                                lower_bounds = ?lower_bounds,
                                direct_param = direct_param_vars.contains(&var),
                                all_return_type = all_return_type,
                                pre_adjusted = ?ty,
                                "Adjusting resolved inference type"
                            );
                            // A union argument whose members are naked outer type
                            // parameters (e.g. `T | T[]`) distributes into direct
                            // inference as bare-foreign-type-param-shaped lower bounds
                            // (`T`, `T[]`). When the inferred BCT is the union of only
                            // such shapes, tsc keeps the distributed union as the
                            // inferred type for a naked-type-parameter target rather
                            // than collapsing to the first candidate; collapsing here
                            // turns the post-inference assignability re-check into a
                            // hard failure (false TS2769). Concrete candidates (e.g.
                            // `string` in `string | T[]`) are NOT bare-foreign shapes,
                            // so heterogeneous unions still collapse and surface the
                            // genuine mismatch.
                            let preserve_distributed_foreign_union = matches!(
                                self.interner.lookup(ty),
                                Some(TypeData::Union(_))
                            ) && lower_bounds.len() > 1
                                && lower_bounds.iter().all(|&bound| {
                                    crate::operations::generic_call::foreign_param_shapes::is_bare_foreign_type_param_shape(
                                        self.interner.as_type_database(),
                                        bound,
                                        &func.type_params,
                                        var_map,
                                    )
                                });
                            let mut ty = if all_return_type {
                                self.resolve_return_position_inference_type(&lower_bounds, ty)
                            } else if preserve_distributed_foreign_union {
                                ty
                            } else if direct_param_vars.contains(&var)
                                && !has_index_signature_candidates
                            {
                                let leftmost_dropped_by_priority =
                                    infer_ctx.has_mixed_priority_candidates(var);
                                self.resolve_direct_parameter_inference_type(
                                    &lower_bounds,
                                    ty,
                                    has_usable_contra_candidates,
                                    infer_ctx.has_fresh_array_element_candidate(var),
                                    leftmost_dropped_by_priority,
                                )
                            } else {
                                ty
                            };
                            if direct_param_vars.contains(&var) && has_usable_contra_candidates {
                                let contra_types = infer_ctx.get_contra_candidate_types(var);
                                let concrete_contra: Vec<_> = contra_types
                                    .into_iter()
                                    .filter(|contra| {
                                        !crate::type_queries::data::is_bare_current_infer_placeholder_db(
                                            self.interner.as_type_database(),
                                            *contra,
                                        )
                                    })
                                    .collect();
                                if concrete_contra.len() == 1 {
                                    let contra = concrete_contra[0];
                                    if self
                                        .should_prefer_single_contra_candidate_for_direct_inference(
                                            &lower_bounds,
                                            ty,
                                            contra,
                                        )
                                    {
                                        ty = self
                                            .select_single_contra_candidate_direct_inference_type(
                                                &lower_bounds,
                                                contra,
                                            );
                                        let root = infer_ctx.table.find(var);
                                        let mut info = infer_ctx.table.probe_value(root);
                                        info.resolved = Some(ty);
                                        infer_ctx.table.union_value(root, info);
                                    }

                                    let mut needs_broader_due_dependent_constraint = false;
                                    if lower_bounds.len() == 1
                                        && self.checker.is_assignable_to(ty, contra)
                                        && !self.checker.is_assignable_to(contra, ty)
                                    {
                                        for (other_tp, &other_var) in
                                            func.type_params.iter().zip(type_param_vars.iter())
                                        {
                                            if other_tp.is_same_binder(*tp) {
                                                continue;
                                            }
                                            let Some(other_constraint) = other_tp.constraint else {
                                                continue;
                                            };
                                            let constraint_references_current =
                                                crate::visitor::contains_type_parameter_binder(
                                                    self.interner.as_type_database(),
                                                    other_constraint,
                                                    *tp,
                                                );
                                            if !constraint_references_current {
                                                continue;
                                            }
                                            let Some(other_constraints) =
                                                infer_ctx.get_constraints(other_var)
                                            else {
                                                continue;
                                            };
                                            for lb in other_constraints.lower_bounds.iter().copied()
                                            {
                                                if lb.is_any_unknown_or_error() {
                                                    continue;
                                                }
                                                if !self.checker.is_assignable_to(lb, ty)
                                                    && self.checker.is_assignable_to(lb, contra)
                                                {
                                                    needs_broader_due_dependent_constraint = true;
                                                    break;
                                                }
                                            }
                                            if needs_broader_due_dependent_constraint {
                                                break;
                                            }
                                        }
                                    }

                                    if needs_broader_due_dependent_constraint {
                                        ty = contra;
                                    }
                                }
                            }
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
                                && infer_ctx.all_candidates_are_return_type(var)
                                && saw_deferred_arg;

                            let fallback = if use_inferred {
                                // Use the inferred type (lower bound from BoundsViolation)
                                if let InferenceError::BoundsViolation { lower, .. } = &e {
                                    *lower
                                } else {
                                    // Missing bounds violation during inferred fallback: continue without constraint error
                                    TypeId::ERROR
                                }
                            } else if let Some(upper) =
                                self.single_concrete_upper_bound(&mut infer_ctx, var)
                            {
                                upper
                            } else if let Some(default) = tp.default {
                                self.eval_type_param_default(
                                    default,
                                    &final_subst,
                                    actual_this_type,
                                )
                            } else if let Some(constraint) = tp.constraint {
                                instantiate_call_type(
                                    self.interner,
                                    constraint,
                                    &final_subst,
                                    actual_this_type,
                                )
                            } else {
                                TypeId::ERROR
                            };
                            let fallback = if direct_param_vars.contains(&var)
                                && !has_index_signature_candidates
                            {
                                let leftmost_dropped_by_priority =
                                    infer_ctx.has_mixed_priority_candidates(var);
                                self.resolve_direct_parameter_inference_type(
                                    &lower_bounds,
                                    fallback,
                                    has_usable_contra_candidates,
                                    infer_ctx.has_fresh_array_element_candidate(var),
                                    leftmost_dropped_by_priority,
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
                if saw_deferred_arg {
                    let infer_subst = if let Some(ref cached) = infer_subst_cache {
                        cached
                    } else {
                        let mut subst = infer_ctx.get_current_substitution();
                        self.remove_unresolved_source_placeholders_from_substitution(&mut subst);
                        infer_subst_cache = Some(subst);
                        infer_subst_cache
                            .as_ref()
                            .expect("inference substitution cache just initialized")
                    };
                    self.normalize_inferred_placeholder_type_preserving_source_placeholders(
                        ty,
                        infer_subst,
                    )
                } else {
                    let instantiated_constraint = tp.constraint.map(|constraint| {
                        instantiate_call_type(
                            self.interner,
                            constraint,
                            substitution,
                            actual_this_type,
                        )
                    });
                    let constraint_preserves_literals =
                        if let Some(instantiated_constraint) = instantiated_constraint {
                            let resolver = self
                                .checker
                                .type_resolver()
                                .unwrap_or_else(|| self.interner.as_type_resolver());
                            type_implies_literals_deep(self.interner, instantiated_constraint)
                                || constraint_is_primitive_type_with_resolver(
                                    self.interner,
                                    resolver,
                                    instantiated_constraint,
                                )
                                || constraint_contains_primitive_constrained_type_param(
                                    self.interner,
                                    resolver,
                                    instantiated_constraint,
                                    0,
                                )
                        } else {
                            false
                        };
                    if !tp.is_const && !contra_only && !constraint_preserves_literals {
                        // Widen fresh inference results from expressions when the type
                        // parameter does NOT have a primitive literal-preserving constraint.
                        // tsc preserves literal types when the constraint is a primitive:
                        //   <T extends string>(a: T) => T  -- T="z" preserved
                        //   <T>(a: T) => T                  -- handled by the trivial fast path
                        if infer_ctx.all_candidates_are_fresh_literals(var) {
                            if noinfer_param_vars.contains(&var) {
                                let mut literal_bounds = lower_bounds
                                    .iter()
                                    .copied()
                                    .filter(|bound| !bound.is_any_unknown_or_error())
                                    .collect::<Vec<_>>();
                                literal_bounds.dedup();
                                if literal_bounds.is_empty() {
                                    ty
                                } else {
                                    let result = crate::utils::union_or_single(
                                        self.interner,
                                        literal_bounds,
                                    );
                                    // tsc's BCT widening: array element inference widens
                                    // fresh literals to their primitive in NoInfer<T>
                                    // positions. Direct scalar arguments are preserved only
                                    // when T appears at the return type's top level (`(): T`).
                                    // Complex return shapes (`(): { v: T }`) use tsc's normal
                                    // widened inference result.
                                    let db = self.interner.as_type_database();
                                    let return_preserves_direct_literal =
                                        crate::visitor::is_type_parameter_at_top_level(
                                            db,
                                            func.return_type,
                                            tp.name,
                                        );
                                    let should_widen = crate::visitor::is_literal_type(db, result)
                                        && (!return_preserves_direct_literal
                                            || infer_ctx.all_candidates_from_array_elements(var)
                                            || infer_ctx.all_candidates_from_object_property(var))
                                        || crate::visitor::is_union_of_fresh_literals(db, result);
                                    if should_widen {
                                        widening::widen_literal_type(db, result)
                                    } else {
                                        result
                                    }
                                }
                            } else {
                                // Mirror tsc's `widenLiteralTypes` gate (checker.ts
                                // `getCovariantInference`): a fresh literal inferred purely
                                // from top-level positions for a type parameter at the top
                                // level of the return type is NOT widened, so `<T>(x: T): T`,
                                // `<T>(x: T, y: T): T`, and `<T>(x: T extends 'a' ? never :
                                // T): T` keep their literal (`'a'`, `1 | 2`, …). A literal
                                // inferred from a nested position (callback return, array
                                // element) or for a parameter not at the return's top level
                                // is widened to its primitive, as before.
                                let db = self.interner.as_type_database();
                                let preserve = self.type_param_preserves_inferred_literal(
                                    func,
                                    tp.name,
                                    contextually_fixed.as_ref(),
                                ) && !infer_ctx
                                    .all_candidates_from_array_elements(var);
                                if preserve {
                                    ty
                                } else {
                                    widening::widen_literal_type(db, ty)
                                }
                            }
                        } else if matches!(
                            self.interner.lookup(ty),
                            Some(crate::types::TypeData::Tuple(_))
                        ) && let Some(literal_mode) = infer_ctx.spread_rest_mode_of(var)
                            && !infer_ctx.has_type_annotation_candidates(var)
                        {
                            // A tuple packed from trailing rest arguments widens
                            // per element against the rest type parameter's
                            // declared constraint (tsc's `getSpreadArgumentType`):
                            // `f<T extends string[]>(...args: T)` keeps
                            // `["a", "b"]` while `T extends any[]` widens to
                            // `[string, string]`.
                            crate::inference::spread_rest_literals::widen_spread_rest_tuple(
                                self.interner.as_type_database(),
                                ty,
                                instantiated_constraint,
                                literal_mode,
                            )
                        } else if self.inference_type_contains_fresh_object_or_array(ty)
                            && !infer_ctx.has_type_annotation_candidates(var)
                        {
                            crate::operations::widening::widen_type_for_inference(
                                self.interner.as_type_database(),
                                ty,
                            )
                        } else {
                            ty
                        }
                    } else {
                        ty
                    }
                }
            } else if let Some(default) = tp.default {
                let ty = self.eval_type_param_default(default, &final_subst, actual_this_type);
                trace!(resolved_type = ?ty, "Using default type");
                // Track that this type parameter fell back to its default.
                // We should NOT check argument assignability against the default
                // since it's a fallback when inference fails, not a constraint.
                default_fallback_tp_names.insert(tp.name);
                ty
            } else if let Some(constraint) = tp.constraint {
                let ty = instantiate_call_type(
                    self.interner,
                    constraint,
                    &final_subst,
                    actual_this_type,
                );
                trace!(resolved_type = ?ty, "Using constraint as fallback (no constraints collected)");
                ty
            } else {
                trace!("Using UNKNOWN (unconstrained type parameter)");
                // TypeScript infers 'unknown' for unconstrained type parameters without defaults
                TypeId::UNKNOWN
            };

            let has_rest_tuple_evidence = rest_tuple_target_type
                .and_then(|target_type| var_map.get(&target_type).copied())
                .is_some_and(|rest_var| rest_var == var);
            let ty = if has_rest_tuple_evidence
                && is_bare_foreign_type_param(
                    self.interner.as_type_database(),
                    ty,
                    &func.type_params,
                    var_map,
                ) {
                let concrete_lower_bounds = lower_bounds
                    .iter()
                    .copied()
                    .filter(|&bound| {
                        is_substantive_inference_candidate(
                            self.interner.as_type_database(),
                            bound,
                            &func.type_params,
                            var_map,
                        )
                    })
                    .collect::<Vec<_>>();
                match concrete_lower_bounds.as_slice() {
                    [] => ty,
                    [single] => *single,
                    bounds => infer_ctx.best_common_type(bounds),
                }
            } else {
                ty
            };
            let ty = if direct_param_vars.contains(&var) {
                readonly_direct_inference::restore_direct_inference(
                    self.interner.as_type_database(),
                    &mut infer_ctx,
                    var,
                    ty,
                )
            } else {
                ty
            };
            let type_param_name = self.interner.resolve_atom(tp.name);
            let ty = if let Some(contextual_ty) = structural_return_subst.get(tp.name) {
                let contextual_can_replace_foreign_source = is_bare_foreign_type_param(
                    self.interner.as_type_database(),
                    ty,
                    &func.type_params,
                    var_map,
                ) && infer_ctx
                    .all_candidates_are_return_type(var);
                // When a type parameter had NO inference candidates at all
                // (has_constraints=false) and defaulted to `unknown`, AND the type
                // parameter was referenced in a non-deferred argument position
                // (direct_param_vars contains it), the contextual return substitution
                // must NOT override it. The `unknown` result means the argument types
                // genuinely provide no inference information for this type parameter
                // (e.g., `NumberMap<Function>` passed to `StringMap<T>` where number
                // index doesn't satisfy string index). Overriding with the contextual
                // type (e.g., `Function` from `var v1: Function[]`) would mask the
                // type mismatch that should produce TS2403 for redeclarations.
                //
                // However, when the type parameter was NOT in a direct parameter
                // position (only in deferred/context-sensitive args), the `unknown`
                // is a placeholder that SHOULD be replaced by the contextual return
                // type to enable proper contextual typing of callbacks.
                let constructor_context_can_fill_unknown =
                    func.is_constructor && structural_return_subst.get(tp.name).is_some();
                let prefer_contextual_constraint_candidate = if direct_param_vars.contains(&var) {
                    if let Some(constraint) = tp.constraint {
                        let constraint_ty_raw = instantiate_call_type(
                            self.interner,
                            constraint,
                            &final_subst,
                            actual_this_type,
                        );
                        let constraint_ty = self.checker.evaluate_type(constraint_ty_raw);
                        let ty_for_check =
                            crate::relations::freshness::widen_freshness(self.interner, ty);
                        let contextual_for_check = crate::relations::freshness::widen_freshness(
                            self.interner,
                            contextual_ty,
                        );
                        let ty_satisfies_constraint =
                            self.checker.is_assignable_to(ty_for_check, constraint_ty);
                        let ty_satisfies_raw = !ty_satisfies_constraint
                            && self.satisfies_raw_instantiated_constraint(
                                ty_for_check,
                                constraint_ty_raw,
                                constraint_ty,
                            );
                        let contextual_satisfies_constraint = self
                            .checker
                            .is_assignable_to(contextual_for_check, constraint_ty);
                        let contextual_satisfies_raw = !contextual_satisfies_constraint
                            && self.satisfies_raw_instantiated_constraint(
                                contextual_for_check,
                                constraint_ty_raw,
                                constraint_ty,
                            );
                        let ty_satisfies_constraint = ty_satisfies_constraint || ty_satisfies_raw;
                        let contextual_satisfies_constraint =
                            contextual_satisfies_constraint || contextual_satisfies_raw;
                        !ty_satisfies_constraint && contextual_satisfies_constraint
                    } else {
                        false
                    }
                } else {
                    false
                };
                // A var that a context-sensitive callback reaches only through a
                // callback *parameter* (contravariant) position is not inferable
                // from the callback body during Round 2, so its `unknown`/fallback
                // here is a placeholder the contextual return type must fill — even
                // though the var also occupies a direct-parameter slot. Treat it
                // like a non-direct var so the contextual return substitution
                // applies (matches `tsc` fixing such parameters from the return
                // context, e.g. `fromCompare<A>(c: Ord<A>['compare']): Ord<A>`).
                let keep_direct_param_inference = direct_param_vars.contains(&var)
                    && !callback_param_only_vars.contains(&var)
                    && !contextual_can_replace_foreign_source
                    && !prefer_contextual_constraint_candidate
                    && ((!has_constraints
                        && ty == TypeId::UNKNOWN
                        && !constructor_context_can_fill_unknown)
                        || (ty != TypeId::UNKNOWN && ty != TypeId::ERROR));
                if keep_direct_param_inference {
                    ty
                } else {
                    let can_apply = self.can_apply_contextual_return_substitution(
                        &mut infer_ctx,
                        var,
                        ty,
                        var_map,
                    );
                    let should_use = contextual_can_replace_foreign_source
                        || self.should_use_contextual_return_substitution(
                            ty,
                            contextual_ty,
                            var_map,
                        );
                    // When the variable was NOT inferred from a direct parameter match
                    // (i.e., it was inferred structurally from e.g. callback return types),
                    // allow the contextual return substitution to override even when
                    // can_apply would normally block it. This handles cases like:
                    //   let xx: 0 | 1 | 2 = invoke(() => 1);
                    // where T gets NakedTypeVariable candidate `number` from the lambda
                    // return type, but the contextual type `0 | 1 | 2` is strictly narrower
                    // and should take priority. Direct parameter vars (e.g., `foo<T>(x: T)`)
                    // are excluded because their inference is authoritative.
                    let indirect_narrowing_override =
                        !direct_param_vars.contains(&var) && should_use && !can_apply;
                    // The contextual return type may only REFINE the inferred type
                    // (e.g. a widened lambda return `number` narrowed to the contextual
                    // `0 | 1 | 2`, whose literal evidence `1` still fits). When a
                    // concrete bottom-up candidate genuinely does not fit the contextual
                    // type, the contextual type is a real mismatch, not a refinement, so
                    // the inferred type must stand and the error surface at the
                    // call/assignment site (#14792). The evidence check is the second
                    // `&&` operand so it only runs once the cheap override gate passes.
                    // Callback-parameter-only bounds are contextual feedback; explicit
                    // annotations are still validated by the final argument check.
                    if ((can_apply && should_use) || indirect_narrowing_override)
                        && (callback_param_only_vars.contains(&var)
                            || !self.contextual_substitution_contradicts_evidence(
                                &lower_bounds,
                                contextual_ty,
                            ))
                    {
                        contextual_ty
                    } else {
                        ty
                    }
                }
            } else {
                ty
            };
            let ty = self.checker.normalize_inferred_type(ty);
            // #17282: a type parameter fixed from the non-context-sensitive
            // (Round-1) arguments is immutable per tsc's `InferenceInfo.isFixed`.
            // A context-sensitive callback body may only *widen* it through a
            // covariant callback **return** position, which tsc discards; restore
            // the Round-1 fix so the body is re-checked against the fixed
            // contextual return type (e.g.
            // `f<T,U>(y:T, y1:U, p:(z:U)=>T, p1:(x:T)=>U)` called with identity
            // lambdas reports TS2741 instead of widening `T`/`U` to `A|B`). See
            // `should_restore_round1_fix` for the exceptions tsc keeps.
            let ty = if let Some(&fixed) = round1_fixed.get(&var)
                && self.should_restore_round1_fix(
                    &mut infer_ctx,
                    var,
                    fixed,
                    ty,
                    &callback_return_position_vars,
                    any_tainted_frozen_call,
                ) {
                self.checker.normalize_inferred_type(fixed)
            } else {
                ty
            };
            trace!(
                type_param_name = %type_param_name.as_str(),
                var = ?var,
                resolved_type = ty.0,
                resolved_type_key = ?self.interner.lookup(ty),
                "Resolved type parameter"
            );
            final_subst.insert(tp.name, ty);
        }

        // Recursively resolve placeholders in final_subst.
        // If an inferred type contains transient placeholders from source functions (e.g. __infer_src_U),
        // we must resolve them using the full inference context substitution.
        // Example: B -> Array(__infer_src_U) where __infer_src_U -> T. We want B -> Array(T).
        // The current call's own placeholders (`__infer_N` for this call's type
        // parameters) are legitimate and preserved; only placeholders leaked from
        // a *nested* generic call must be defaulted. Shared by the `final_subst`
        // cleanup below and the argument-display normalization further down.
        let own_placeholder_atoms: FxHashSet<tsz_common::Atom> =
            type_param_placeholder_atoms.iter().copied().collect();
        {
            let mut full_subst = infer_ctx.get_current_substitution();
            self.remove_unresolved_source_placeholders_from_substitution(&mut full_subst);
            let mut resolved_subst = TypeSubstitution::new();
            for (name, ty) in final_subst.map().iter() {
                let mut placeholder_visited = FxHashSet::default();
                if structural_return_subst.get(*name) == Some(*ty)
                    && !self.type_contains_placeholder(*ty, var_map, &mut placeholder_visited)
                {
                    resolved_subst.insert(*name, *ty);
                    continue;
                }
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
            // Update final_subst with fully resolved types, defaulting any
            // inference placeholder that leaked in from a nested generic call
            // (uninferable curried-callback parameter) to its constraint or
            // `unknown` so no internal `__infer_N` survives into the finalized
            // result type (issue #15461).
            for (name, ty) in resolved_subst.map().iter() {
                let cleaned =
                    self.default_leaked_inference_placeholders(*ty, &own_placeholder_atoms);
                final_subst.insert(*name, cleaned);
            }
        }

        // Constraint checking is deferred until ALL type parameters are resolved.
        // This handles cases like `<T extends U, U>` where T's constraint references
        // U, which may not be in final_subst until later iterations.
        let mut constraint_fallback_tp_names: FxHashSet<tsz_common::Atom> = FxHashSet::default();
        let mut constraint_fallback_display_types: FxHashMap<tsz_common::Atom, TypeId> =
            FxHashMap::default();
        for (tp, &var) in func.type_params.iter().zip(type_param_vars.iter()) {
            if let Some(constraint) = tp.constraint {
                let ty = final_subst.get(tp.name).unwrap_or(TypeId::ERROR);
                if crate::visitors::visitor_predicates::contains_infer_types(
                    self.interner.as_type_database(),
                    constraint,
                ) {
                    final_subst.insert(tp.name, ty);
                    continue;
                }
                let constraint_ty_raw = instantiate_call_type(
                    self.interner,
                    constraint,
                    &final_subst,
                    actual_this_type,
                );
                // Evaluate the instantiated constraint so concrete conditionals like
                // `null extends string ? any : never` resolve to their branch (`never`)
                // instead of remaining as unevaluated Conditional types.
                let constraint_ty = self.checker.evaluate_type(constraint_ty_raw);
                // When the constraint is a deferred `keyof T` where T is a type parameter,
                // skip the constraint validation. TypeScript defers this check to
                // instantiation time because `keyof T` can't be resolved until T is known.
                // Without this, `K extends keyof T` with inferred K = "content" fails
                // even when T extends { content: C }.
                if let Some(keyof_operand) =
                    crate::visitor::keyof_inner_type(self.interner, constraint_ty)
                    && matches!(
                        self.interner.lookup(keyof_operand),
                        Some(crate::TypeData::TypeParameter(_))
                    )
                {
                    final_subst.insert(tp.name, ty);
                    continue;
                }
                // Strip freshness before constraint check: inferred types should not
                // trigger excess property checking against type parameter constraints.
                let ty_for_check = crate::relations::freshness::widen_freshness(self.interner, ty);
                let constraint_satisfied =
                    self.arg_satisfies_type_parameter_constraint(ty_for_check, constraint_ty);
                // Resolver-backed alias expansion is also needed when ordinary
                // evaluation leaves a cross-file application unchanged. Keep
                // it strictly behind the normal relation so passing calls pay
                // none of the bounded materialization cost.
                let raw_constraint_satisfied = !constraint_satisfied
                    && self.satisfies_raw_instantiated_constraint(
                        ty_for_check,
                        constraint_ty_raw,
                        constraint_ty,
                    );
                if !constraint_satisfied
                    && !raw_constraint_satisfied
                    && !self.callable_satisfies_top_rest_any_constraint(ty_for_check, constraint_ty)
                {
                    // When the inferred type is a TypeParameter whose own constraint
                    // is structurally equivalent to the target constraint, accept it.
                    // This handles: K extends keyof S passed to K2 extends keyof S
                    // where the two keyof S expressions use different TypeParameter
                    // TypeIds for S (because Store<S> instantiation created a new S).
                    // When the inferred type is a TypeParameter whose constraint
                    // is structurally the same as the target constraint, accept it.
                    // This handles cross-context type parameter identity:
                    // K extends keyof S passed to K2 extends keyof S where
                    // the two S params have different TypeIds but same name.
                    if let Some(crate::TypeData::TypeParameter(tp_info)) =
                        self.interner.lookup(ty_for_check)
                        && let Some(tp_constraint) = tp_info.constraint
                    {
                        // Direct TypeId match
                        if tp_constraint == constraint_ty {
                            final_subst.insert(tp.name, ty_for_check);
                            continue;
                        }
                        // Both are `keyof <TypeParam>` with the same logical
                        // binder. Scoped origins prevent a captured same-spelled
                        // parameter from satisfying this shortcut.
                        if let (Some(c_inner), Some(t_inner)) = (
                            crate::visitor::keyof_inner_type(self.interner, tp_constraint),
                            crate::visitor::keyof_inner_type(self.interner, constraint_ty),
                        ) && let (
                            Some(crate::TypeData::TypeParameter(c_tp)),
                            Some(crate::TypeData::TypeParameter(t_tp)),
                        ) = (self.interner.lookup(c_inner), self.interner.lookup(t_inner))
                            && c_tp.is_same_binder(t_tp)
                        {
                            final_subst.insert(tp.name, ty_for_check);
                            continue;
                        }
                        // When the inferred type is a TypeParameter from an outer
                        // scope, its constraint is guaranteed to be at least as
                        // specific as the function's type parameter constraint
                        // (the inference already validated upper bounds during
                        // resolution). Accept the TypeParameter to preserve the
                        // more specific type information instead of collapsing
                        // to the constraint. This handles cases like:
                        //   U extends MessageList<T>, MessageList<T> extends Message
                        //   → U satisfies V extends Message
                        // where structural comparison may fail due to `this` types
                        // or unresolved Application types in the constraint chain.
                        final_subst.insert(tp.name, ty_for_check);
                        continue;
                    }
                    // Lazy(DefId) from contextual return inference may fail structural
                    // constraint checks due to evaluation differences in complex
                    // inheritance chains (e.g., DOM). Keep it for non-direct
                    // inference; direct argument inference still has to report
                    // constraint failures on the argument site.
                    if !direct_param_vars.contains(&var)
                        && matches!(self.interner.lookup(ty), Some(TypeData::Lazy(_)))
                    {
                        final_subst.insert(tp.name, ty);
                        continue;
                    }
                    // Try to recover using un-widened literal candidates when widening
                    // caused the violation (e.g., "b" widened to string violates keyof O).
                    let mut un_widened = infer_ctx.get_literal_candidates(var);
                    // Direct array literals enter inference as tuples or arrays
                    // with literal-union elements, then widen during
                    // finalization. If that widening is the only reason a
                    // dependent constraint fails, retry the original container
                    // just as we retry scalar literal evidence.
                    // Some direct array literals are represented as an `Array`
                    // whose element is still the un-widened literal union, so
                    // include changed array bounds too, but only when widening
                    // their element literals produces the finalized element.
                    // This keeps unrelated array lower bounds out of the
                    // recovery tournament.
                    let widened_container_element = crate::type_queries::get_array_element_type(
                        self.interner.as_type_database(),
                        ty_for_check,
                    );
                    if let Some(constraints) = infer_ctx.get_constraints(var) {
                        for lower_bound in constraints.lower_bounds {
                            let lower_bound_widens_to_candidate = widened_container_element
                                .is_some_and(|widened_element| {
                                    crate::type_queries::get_array_element_type(
                                        self.interner.as_type_database(),
                                        lower_bound,
                                    )
                                    .is_some_and(
                                        |lower_element| {
                                            lower_element != widened_element
                                                && crate::operations::widening::widen_literal_type(
                                                    self.interner.as_type_database(),
                                                    lower_element,
                                                ) == widened_element
                                        },
                                    )
                                });
                            if lower_bound_widens_to_candidate
                                && matches!(
                                    self.interner.lookup(lower_bound),
                                    Some(TypeData::Array(_) | TypeData::Tuple(_))
                                )
                                && !un_widened.contains(&lower_bound)
                            {
                                un_widened.push(lower_bound);
                            }
                        }
                    }
                    let candidate_type = if !un_widened.is_empty() {
                        Some(if un_widened.len() == 1 {
                            un_widened[0]
                        } else {
                            self.interner.union_from_slice(&un_widened)
                        })
                    } else {
                        None
                    };
                    let recovered = if let Some(candidate_type) = candidate_type {
                        let candidate_satisfies_constraint =
                            self.checker.is_assignable_to(candidate_type, constraint_ty);
                        let candidate_satisfies_raw = !candidate_satisfies_constraint
                            && self.satisfies_raw_instantiated_constraint(
                                candidate_type,
                                constraint_ty_raw,
                                constraint_ty,
                            );
                        if candidate_satisfies_constraint || candidate_satisfies_raw {
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
                        if let Some(candidate_type) = candidate_type {
                            let previous = final_subst.get(tp.name);
                            final_subst.insert(tp.name, candidate_type);
                            let display_constraint = instantiate_call_type(
                                self.interner,
                                constraint,
                                &final_subst,
                                actual_this_type,
                            );
                            if let Some(previous) = previous {
                                final_subst.insert(tp.name, previous);
                            } else {
                                final_subst.remove(tp.name);
                            }
                            constraint_fallback_display_types.insert(tp.name, display_constraint);
                        }
                        // Fall back to constraint type so argument checking emits TS2345
                        final_subst.insert(tp.name, constraint_ty);
                        constraint_fallback_tp_names.insert(tp.name);
                    }
                }
            }
        }

        // Circular-inference guard for unconstrained type parameters.
        //
        // When an unconstrained T_inner is inferred as a composite type that
        // structurally CONTAINS a foreign outer-scope placeholder (e.g.
        // `T_outer & object`), the final argument assignability check becomes
        // tautological: `T_outer & object <: T_outer & object` trivially passes,
        // so TS2345 is never emitted even though the expression is unsound.
        //
        // tsc detects this and emits TS2345. We match that behaviour by
        // reverting `final_subst[T_inner.name]` back to the call-local
        // placeholder TypeId whenever all four conditions hold:
        //   1. The type parameter has no constraint (unconstrained).
        //   2. Inference produced usable contra-variance candidates (meaning the
        //      outer call constrained the parameter; prevents false positives for
        //      independent generic calls like `identity(value[key])`).
        //   3. At least one covariant candidate is an IndexAccess type (the
        //      structural marker of `T[K]` being passed to `T`). Pure outer-T
        //      forwarding (`T_outer[]` → `T_inner`) never has an IndexAccess
        //      covariant candidate and must not be reverted.
        //   4. The inferred type structurally contains a foreign TypeParameter
        //      (from an outer scope), making the post-substitution check
        //      tautological.
        for (tp, &var) in func.type_params.iter().zip(type_param_vars.iter()) {
            // Condition 1: type parameter is unconstrained.
            if tp.constraint.is_some() {
                continue;
            }
            let Some(inferred_ty) = final_subst.get(tp.name) else {
                continue;
            };
            // Condition 2: the outer call constrained the parameter. Usually
            // this shows up as a usable contra candidate. In a self-recursive
            // call the informative contra candidate is the callee's own type
            // parameter, which `add_contra_candidate` drops as a self-reference;
            // there the covariant `T[K]` candidate referencing the parameter's
            // own declaration is the equivalent circular-inference signal.
            if !infer_ctx.has_usable_contra_candidates(var, self.interner.as_type_database())
                && !infer_ctx.has_own_type_param_index_access_covariant_candidate(var)
            {
                continue;
            }
            // Condition 3: at least one covariant candidate is an IndexAccess type.
            if !infer_ctx.has_index_access_covariant_candidate(var) {
                continue;
            }
            // Condition 4: the inferred type structurally contains a foreign TypeParameter.
            if !self.type_contains_any_foreign_type_param(inferred_ty, var_map) {
                continue;
            }
            // Revert to the call-local placeholder so the argument check is
            // non-tautological and TS2345 can fire.
            if let Some((&pid, _)) = var_map.iter().find(|(_, v)| **v == var) {
                final_subst.insert(tp.name, pid);
            }
        }

        // Check if the rest param's type parameter was explicitly replaced by
        // its constraint during the fallback path above. This only matches when
        // the constraint check FAILED and the code fell through to the fallback
        // (not when the constraint was naturally resolved as the inferred type).
        // Also handles variadic tuple rest params like `readonly [...S, number]`
        // where S is a type parameter from constraint fallback.
        let rest_param_from_constraint_fallback = func.params.last().is_some_and(|p| {
            if !p.rest {
                return false;
            }
            // Direct TypeParameter rest param (e.g., `...args: T`)
            if let Some(crate::TypeData::TypeParameter(tp_info)) = self.interner.lookup(p.type_id)
                && func.type_params.iter().any(|type_param| {
                    constraint_fallback_tp_names.contains(&type_param.name)
                        && type_param.is_same_binder(tp_info)
                })
            {
                return true;
            }
            // Variadic tuple rest param (e.g., `...args: readonly [...S, number]`)
            // where S is a type parameter that fell back to its constraint.
            let unwrapped = self.unwrap_readonly(p.type_id);
            if let Some(crate::TypeData::Tuple(elements)) = self.interner.lookup(unwrapped) {
                let elements = self.interner.tuple_list(elements);
                return elements.iter().any(|elem| {
                    if elem.rest
                        && let Some(crate::TypeData::TypeParameter(tp_info)) =
                            self.interner.lookup(elem.type_id)
                    {
                        return func.type_params.iter().any(|type_param| {
                            constraint_fallback_tp_names.contains(&type_param.name)
                                && type_param.is_same_binder(tp_info)
                        });
                    }
                    false
                });
            }
            false
        });

        if let Some(rest_param) = func.params.last().filter(|param| param.rest) {
            let rest_start = func.params.len().saturating_sub(1);
            if arg_types.len() == rest_start {
                let rest_type = instantiate_call_type(
                    self.interner,
                    rest_param.type_id,
                    &final_subst,
                    actual_this_type,
                );
                let rest_type = self.unwrap_readonly(rest_type);
                let evaluated_rest_type = self.evaluate_rest_param_type(rest_type);
                if self.rest_type_needs_aggregate_argument_check(evaluated_rest_type)
                    && let Some(TypeData::Application(app_id)) = self
                        .interner
                        .lookup(self.unwrap_readonly(rest_param.type_id))
                {
                    let app = self.interner.type_application(app_id);
                    for &arg in app.args.iter() {
                        if let Some(TypeData::TypeParameter(info)) = self.interner.lookup(arg)
                            && final_subst.get(info.name) == Some(TypeId::UNKNOWN)
                        {
                            final_subst.insert(info.name, TypeId::NEVER);
                        }
                    }
                }
            }
        }

        let instantiated_params: Vec<ParamInfo> = func
            .params
            .iter()
            .map(|p| {
                let instantiated =
                    instantiate_call_type(self.interner, p.type_id, &final_subst, actual_this_type);
                ParamInfo {
                    name: p.name,
                    type_id: instantiated,
                    optional: p.optional,
                    rest: p.rest,
                }
            })
            .collect();
        if !rest_param_from_constraint_fallback {
            // Params are already instantiated with the inferred type arguments,
            // so the signature's type parameters are resolved — no erasure.
            let (min_args, max_args) = self.arg_count_bounds(&instantiated_params, &[]);
            if arg_types.len() < min_args {
                if let Some(result) = self.rest_tuple_mismatch_for_too_few_args(
                    &instantiated_params,
                    &[],
                    arg_types,
                    func.return_type,
                ) {
                    return result;
                }
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
        }

        // Validate `this` after substitution; resolved `this: void` opts out.
        if let Some(expected_this) = func.this_type {
            let expected_this =
                instantiate_call_type(self.interner, expected_this, &final_subst, actual_this_type);
            let actual_this = self.actual_this_type.unwrap_or(TypeId::VOID);
            let check_this = expected_this != TypeId::VOID;
            if check_this && !self.checker.is_assignable_to(actual_this, expected_this) {
                return CallResult::ThisTypeMismatch {
                    expected_this,
                    actual_this,
                    emit_not_callable: false,
                };
            }
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
                    let placeholder_atom = type_param_placeholder_atoms[i];
                    s.insert(placeholder_atom, inferred);
                }
            }
            s
        };
        let mut final_arg_subst = infer_ctx.get_current_substitution();
        self.remove_unresolved_source_placeholders_from_substitution(&mut final_arg_subst);
        for (name, ty) in placeholder_subst.map().iter() {
            final_arg_subst.insert(*name, *ty);
        }
        let raw_return_type = instantiate_call_type(
            self.interner,
            func.return_type,
            &final_subst,
            actual_this_type,
        );
        let raw_return_type = self.hoist_source_placeholders_into_return_type(raw_return_type);
        let return_type =
            self.normalize_inferred_placeholder_type(raw_return_type, &final_arg_subst);
        let return_type =
            self.hoist_resolved_type_params_into_return_type(func, &final_subst, return_type);
        // A type parameter reachable only through a nested/curried position that
        // never receives an inference candidate can resolve to a bare
        // self-referential placeholder and ride into another inferred parameter
        // (`U = { x: number; y: __infer_N }`). Default any such leaked placeholder
        // to its constraint-or-`unknown`, matching tsc's `getInferredType`.
        let return_type = self.default_leaked_return_type_placeholders(return_type);
        if self.interner.get_display_alias(return_type).is_none()
            && let Some(app_id) =
                crate::visitor::application_id(self.interner.as_type_database(), raw_return_type)
        {
            let app = self.interner.type_application(app_id);
            let mut changed = false;
            let display_args = app
                .args
                .iter()
                .copied()
                .map(|arg| {
                    let evaluated = if crate::visitor::conditional_type_id(
                        self.interner.as_type_database(),
                        arg,
                    )
                    .is_some()
                        || self.application_expands_to_conditional_alias_for_return_display(arg)
                    {
                        self.checker.evaluate_type(arg)
                    } else {
                        arg
                    };
                    changed |= evaluated != arg;
                    evaluated
                })
                .collect::<Vec<_>>();
            if changed || return_type != raw_return_type {
                let display_app = self.interner.application(app.base, display_args);
                self.interner.store_display_alias(return_type, display_app);
                let evaluated_return = self.checker.evaluate_type(return_type);
                if evaluated_return != return_type
                    && self.interner.get_display_alias(evaluated_return).is_none()
                {
                    self.interner
                        .store_display_alias(evaluated_return, display_app);
                }
            }
        }
        // For generic constructor calls (e.g. `new D()` where `class D<T>`),
        // store a display_alias so the formatter shows `D<unknown>` instead of
        // just `D` or the expanded structural type.
        // Guard: skip Application base types to avoid nested Application causing
        // double type args like `Map<K,V><string, number>` for built-in generics.
        if func.is_constructor
            && !func.type_params.is_empty()
            && self.interner.get_display_alias(return_type).is_none()
            && !matches!(
                self.interner.lookup(func.return_type),
                Some(TypeData::Application(_))
            )
        {
            let resolved_args: Vec<TypeId> = func
                .type_params
                .iter()
                .map(|tp| final_subst.get(tp.name).unwrap_or(TypeId::UNKNOWN))
                .collect();
            let app = self.interner.application(func.return_type, resolved_args);
            self.interner.store_display_alias(return_type, app);
        }
        let mut instantiated_params: Vec<ParamInfo> = {
            let mut finalized = Vec::with_capacity(instantiated_params.len());
            for param in instantiated_params {
                let type_id = self.finalize_instantiated_param_type(
                    param.type_id,
                    &final_arg_subst,
                    &func.type_params,
                );
                finalized.push(ParamInfo {
                    name: param.name,
                    type_id,
                    optional: param.optional,
                    rest: param.rest,
                });
            }
            finalized
        };
        if !final_subst.is_empty() {
            for (i, &arg_type) in arg_types.iter().enumerate() {
                let Some(raw_param_type) =
                    self.param_type_for_arg_index(&func.params, i, arg_types.len())
                else {
                    continue;
                };
                let final_param_type =
                    instantiate_type(self.interner, raw_param_type, &final_subst);
                let mismatch = self.arg_mismatch(arg_type, raw_param_type, final_param_type, func);
                if let Some(expected) = mismatch {
                    return CallResult::ArgumentTypeMismatch {
                        index: i,
                        expected,
                        actual: arg_type,
                        fallback_return: TypeId::ERROR,
                    };
                }
            }
        }
        let final_args: Vec<TypeId> = arg_types
            .iter()
            .enumerate()
            .map(|(i, &arg)| {
                // Preserve spread marker tuples [...T] created by the checker
                // for generic TypeParameter spreads.  These are validated against
                // the full rest param type in check_argument_types_with.
                // Only match markers: a 1-rest-element tuple whose inner type
                // is a TypeParameter (not a regular variadic tuple like [...string[]]).
                if let Some(TypeData::Tuple(elems_id)) = self.interner.lookup(arg) {
                    let elems = self.interner.tuple_list(elems_id);
                    if elems.len() == 1
                        && elems[0].rest
                        && matches!(
                            self.interner.lookup(elems[0].type_id),
                            Some(TypeData::TypeParameter(_))
                        )
                    {
                        return arg;
                    }
                }
                let normalized = if final_arg_subst.is_empty() {
                    arg
                } else {
                    self.normalize_inferred_placeholder_type(arg, &final_arg_subst)
                };
                // Symmetric with the source-placeholder (`__infer_src_*`) defaulting
                // that `normalize_inferred_placeholder_type` already performs: a
                // call-local placeholder leaked from a nested generic call must not
                // ride into a displayed argument type either (issue #15461).
                let normalized =
                    self.default_leaked_inference_placeholders(normalized, &own_placeholder_atoms);
                let Some(param_type) =
                    self.param_type_for_arg_index(&instantiated_params, i, arg_types.len())
                else {
                    return normalized;
                };
                if self.has_conflicting_contextual_signature_instantiation(normalized, param_type) {
                    return normalized;
                }
                self.instantiate_generic_function_argument_against_target(normalized, param_type)
            })
            .collect();
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
        let final_args_len = final_args.len();
        for (i, (param, &arg_type)) in instantiated_params
            .iter_mut()
            .zip(final_args.iter())
            .enumerate()
        {
            let duplicate_constraint = if self.object_constraint_properties_are_any(param.type_id) {
                Some(param.type_id)
            } else {
                self.param_type_for_arg_index(&func.params, i, final_args_len)
                    .and_then(|raw| match self.interner.lookup(raw) {
                        Some(TypeData::TypeParameter(tp)) => tp.constraint,
                        _ => None,
                    })
                    .map(|constraint| {
                        let instantiated = instantiate_call_type(
                            self.interner,
                            constraint,
                            &final_subst,
                            actual_this_type,
                        );
                        self.checker.evaluate_type(instantiated)
                    })
                    .filter(|&constraint| self.object_constraint_properties_are_any(constraint))
            };
            if duplicate_constraint.is_some()
                && let Some(expected) = self.duplicate_single_arg_application_value_shape(arg_type)
            {
                param.type_id = expected;
            }
        }
        // Store instantiated params for post-inference excess property checking.
        // The checker needs these to perform EPC on the concrete (post-inference)
        // parameter types rather than the raw types that still contain type parameters.
        // Store BEFORE the final check so they're available even if the check fails
        // (the checker uses these to perform EPC on ArgumentTypeMismatch too).
        self.apply_callback_optional_rest_slots(func, arg_types, &mut instantiated_params);
        let mut provisional_rest_union_sites = Vec::with_capacity(final_args_len);
        for index in 0..final_args_len {
            let raw_expected = self.param_type_for_arg_index(&func.params, index, final_args_len);
            let instantiated_expected =
                self.param_type_for_arg_index(&instantiated_params, index, final_args_len);
            let provisional =
                raw_expected
                    .zip(instantiated_expected)
                    .is_some_and(|(raw, instantiated)| {
                        self.direct_provisional_rest_union_site(
                            raw,
                            instantiated,
                            &aggregate_rest_type_params,
                        )
                    });
            provisional_rest_union_sites.push(provisional);
        }
        self.last_instantiated_params = Some(instantiated_params.clone());

        if let Some((index, expected, actual)) = first_direct_primitive_mismatch {
            return CallResult::ArgumentTypeMismatch {
                index,
                expected,
                actual,
                fallback_return: return_type,
            };
        }

        if let Some(result) = self.generic_rest_tuple_callback_arity_mismatch(func, &final_args) {
            return result;
        }

        if let Some(result) = self.check_argument_types_with(
            &instantiated_params,
            &final_args,
            true,
            func.is_method,
            Some(&provisional_rest_union_sites),
        ) {
            tracing::debug!("Final check failed: {:?}", result);
            return match result {
                CallResult::ArgumentTypeMismatch {
                    index,
                    expected,
                    actual,
                    ..
                } => {
                    // Check if this parameter's type contains a type parameter that
                    // fell back to its default. If so, skip the error - the default is
                    // a fallback when inference fails, not a constraint.
                    let param_type = self
                        .param_type_for_arg_index(&func.params, index, final_args.len())
                        .unwrap_or(expected);
                    let should_skip = func.type_params.iter().any(|type_param| {
                        default_fallback_tp_names.contains(&type_param.name)
                            && crate::visitor::contains_type_parameter_binder(
                                self.interner.as_type_database(),
                                param_type,
                                *type_param,
                            )
                    });
                    if should_skip {
                        tracing::debug!(
                            "Skipping argument mismatch at index {} - parameter type uses default fallback",
                            index
                        );
                        return CallResult::Success(return_type);
                    }
                    // When the original parameter type is a bare const type parameter
                    // (e.g., `x: T` where T has `const` modifier), skip the argument
                    // mismatch. Const type parameters are inferred directly FROM the
                    // argument type, so the argument is always assignable by construction.
                    // The mismatch arises because the checker computes the arg type with
                    // `in_const_assertion = true` (producing one TypeId) while the solver's
                    // inference engine applies `apply_const_assertion` separately (producing
                    // a different TypeId). Both represent the same readonly/literal type.
                    //
                    // This holds ONLY when the parameter was actually inferred from the
                    // argument. If the type parameter instead fell back to its *constraint*
                    // because the argument supplied no valid lower bound (recorded in
                    // `constraint_fallback_tp_names`), the argument was NOT inferred from —
                    // it is genuinely not assignable to the constraint-instantiated
                    // parameter, so the mismatch is real and must be reported. This lets a
                    // later overload whose constraint the argument *does* satisfy win
                    // instead of locking onto this candidate via a spurious `Success`.
                    let is_bare_const_type_param = func.type_params.iter().any(|tp| {
                        tp.is_const
                            && !constraint_fallback_tp_names.contains(&tp.name)
                            && matches!(
                                self.interner.lookup(param_type),
                                Some(TypeData::TypeParameter(info)) if tp.is_same_binder(info)
                            )
                    });
                    if is_bare_const_type_param {
                        tracing::debug!(
                            "Skipping argument mismatch at index {} - bare const type parameter",
                            index
                        );
                        return CallResult::Success(return_type);
                    }

                    let expected = self
                        .param_type_for_arg_index(&func.params, index, final_args.len())
                        .and_then(|raw| match self.interner.lookup(raw) {
                            Some(TypeData::TypeParameter(tp)) => {
                                constraint_fallback_display_types.get(&tp.name).copied()
                            }
                            _ => None,
                        })
                        .unwrap_or(expected);
                    if crate::contains_this_type(self.interner, expected)
                        && let Some(concrete_this) = self
                            .checker
                            .type_resolver()
                            .and_then(|resolver| resolver.resolve_this_type(self.interner))
                    {
                        let substituted_expected =
                            crate::instantiation::instantiate::substitute_this_type(
                                self.interner,
                                expected,
                                concrete_this,
                            );
                        let substituted_rest_element =
                            crate::contextual::rest_argument_element_type(
                                self.interner,
                                substituted_expected,
                            );
                        if self.checker.is_assignable_to(actual, substituted_expected)
                            || self
                                .checker
                                .is_assignable_to(actual, substituted_rest_element)
                        {
                            return CallResult::Success(return_type);
                        }
                    }

                    CallResult::ArgumentTypeMismatch {
                        index,
                        expected,
                        actual,
                        fallback_return: return_type,
                    }
                }
                _ => result,
            };
        }
        for (i, (&arg_type, raw_param)) in final_args.iter().zip(func.params.iter()).enumerate() {
            if raw_param.rest {
                continue;
            }
            let raw_param_type = raw_param.type_id;
            let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(raw_param_type) else {
                continue;
            };
            let Some(constraint) = tp.constraint else {
                continue;
            };
            if crate::visitors::visitor_predicates::contains_infer_types(
                self.interner.as_type_database(),
                constraint,
            ) {
                continue;
            }
            let constraint =
                instantiate_call_type(self.interner, constraint, &final_subst, actual_this_type);
            if crate::type_queries::contains_type_parameters_db(
                self.interner.as_type_database(),
                constraint,
            ) && let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(constraint)
                && tp.constraint.is_none()
            {
                continue;
            }
            // If the argument is a TypeParameter whose constraint equals the
            // instantiated parameter constraint, T extends C trivially satisfies C.
            // Handles fn<T extends C>(x: T) called with arg typed as T_outer extends C.
            if let Some(TypeData::TypeParameter(arg_tp)) = self.interner.lookup(arg_type)
                && let Some(arg_constraint) = arg_tp.constraint
                && arg_constraint == constraint
            {
                continue;
            }
            if !self.arg_satisfies_type_parameter_constraint(arg_type, constraint)
                && !self.is_function_union_compat(arg_type, constraint)
                && !self.callable_satisfies_top_rest_any_constraint(arg_type, constraint)
            {
                return CallResult::ArgumentTypeMismatch {
                    index: i,
                    expected: constraint,
                    actual: arg_type,
                    fallback_return: return_type,
                };
            }
        }

        tracing::debug!("Final check succeeded");

        // Instantiate the type predicate if present, so the checker can use it
        // for flow narrowing with the correct (inferred) type arguments.
        if let Some(ref predicate) = func.type_predicate {
            let instantiated_predicate = TypePredicate {
                asserts: predicate.asserts,
                target: predicate.target,
                type_id: predicate.type_id.map(|tid| {
                    instantiate_call_type(self.interner, tid, &final_subst, actual_this_type)
                }),
                parameter_index: predicate.parameter_index,
            };
            let instantiated_params_for_pred: Vec<ParamInfo> = func
                .params
                .iter()
                .map(|p| ParamInfo {
                    name: p.name,
                    type_id: instantiate_call_type(
                        self.interner,
                        p.type_id,
                        &final_subst,
                        actual_this_type,
                    ),
                    optional: p.optional,
                    rest: p.rest,
                })
                .collect();
            self.last_instantiated_predicate =
                Some((instantiated_predicate, instantiated_params_for_pred));
        }

        CallResult::Success(return_type)
    }

    /// Whether substituting the contextual return type for a resolved
    /// return-position type parameter would contradict the bottom-up inference
    /// evidence: a concrete lower-bound candidate that is not assignable to the
    /// contextual type marks a genuine mismatch (which tsc keeps in the inferred
    /// type and reports at the call/assignment site), not a refinement the
    /// contextual type may apply. See #14792.
    fn contextual_substitution_contradicts_evidence(
        &mut self,
        lower_bounds: &[TypeId],
        contextual_ty: TypeId,
    ) -> bool {
        let db = self.interner.as_type_database();
        lower_bounds.iter().copied().any(|bound| {
            super::super::inference_helpers::is_concrete_inference_bound(db, bound)
                && !self.checker.is_assignable_to(bound, contextual_ty)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TypeInterner;
    use crate::computation::CompatChecker;
    use crate::instantiation::instantiate::instantiate_type;
    use crate::types::{TupleElement, TypeParamInfo, TypeParamOrigin};

    fn scoped_param(name: tsz_common::Atom, file: tsz_common::Atom, node: u32) -> TypeParamInfo {
        TypeParamInfo {
            name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node },
        }
    }

    fn tuple_members(interner: &TypeInterner, type_id: TypeId) -> Vec<TypeId> {
        let Some(TypeData::Tuple(list_id)) = interner.lookup(type_id) else {
            panic!("expected tuple, got {:?}", interner.lookup(type_id));
        };
        interner
            .tuple_list(list_id)
            .iter()
            .map(|element| element.type_id)
            .collect()
    }

    #[test]
    fn exact_domain_includes_constraint_and_default_only_owned_binders() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("metadata.ts");
        let constraint_name = interner.intern_string("U");
        let default_name = interner.intern_string("W");

        let owned_constraint_info = scoped_param(constraint_name, file, 1);
        let foreign_constraint_info = scoped_param(constraint_name, file, 2);
        let owned_default_info = scoped_param(default_name, file, 3);
        let foreign_default_info = scoped_param(default_name, file, 4);
        let owned_constraint = interner.fresh_type_param(owned_constraint_info);
        let foreign_constraint = interner.fresh_type_param(foreign_constraint_info);
        let owned_default = interner.fresh_type_param(owned_default_info);
        let foreign_default = interner.fresh_type_param(foreign_default_info);

        let constraint = interner.tuple(vec![
            TupleElement::fixed(owned_constraint),
            TupleElement::fixed(foreign_constraint),
        ]);
        let default = interner.tuple(vec![
            TupleElement::fixed(owned_default),
            TupleElement::fixed(foreign_default),
        ]);
        let carrier = TypeParamInfo {
            name: interner.intern_string("Carrier"),
            constraint: Some(constraint),
            default: Some(default),
            is_const: false,
            origin: TypeParamOrigin::User,
        };
        let func = FunctionShape {
            type_params: vec![owned_constraint_info, owned_default_info, carrier],
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        };

        let mut substitution = TypeSubstitution::new();
        substitution.insert(constraint_name, TypeId::NUMBER);
        substitution.insert(default_name, TypeId::STRING);
        let mut checker = CompatChecker::new(&interner);
        let evaluator = CallEvaluator::new(&interner, &mut checker);
        evaluator.protect_call_owned_type_parameters(&func, &mut substitution);

        assert_eq!(
            tuple_members(
                &interner,
                instantiate_type(&interner, constraint, &substitution),
            ),
            vec![TypeId::NUMBER, foreign_constraint],
        );
        assert_eq!(
            tuple_members(
                &interner,
                instantiate_type(&interner, default, &substitution),
            ),
            vec![TypeId::STRING, foreign_default],
        );
    }
}
