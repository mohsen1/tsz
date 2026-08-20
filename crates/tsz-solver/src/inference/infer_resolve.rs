//! Inference resolution and constraint strengthening.
//!
//! This module contains the resolution phase of type inference:
//! - Constraint-based resolution (upper/lower bounds)
//! - Candidate filtering and widening
//! - Circular constraint unification (SCC/Tarjan)
//! - Constraint strengthening and propagation
//! - Variable fixing and substitution building

use crate::inference::infer::{
    InferenceCandidate, InferenceContext, InferenceError, InferenceInfo, InferenceVar,
    MAX_TYPE_RECURSION_DEPTH,
};
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::widening;
use crate::types::{InferencePriority, ObjectFlags, TemplateSpan, TypeData, TypeId};
use crate::visitor::is_literal_type;
use rustc_hash::FxHashSet;
use tsz_common::interner::Atom;

/// Per-variable resolution flags threaded into candidate resolution together.
/// The first two are the literal-widening halves of tsc
/// `getCovariantInference`'s `widenLiteralTypes` gate.
#[derive(Clone, Copy)]
pub(super) struct LiteralWideningPolicy {
    /// The contextual-pin / conditional-parameter mark
    /// (`top_level_in_return_type_unfixed`): fresh literal candidates are not
    /// widened for this variable.
    pub(super) skip_literal_widening: bool,
    /// The runtime `isFixed` half: the variable sits at the top level of the
    /// signature's return type and was never fixed for contextual typing
    /// (`root_preserves_return_position_literals`).
    pub(super) preserve_return_position_literals: bool,
    /// The variable is typed by a callback parameter (`(x: T) => …`), so the
    /// return-type "first wins" pin is disabled and disjoint callback-return
    /// candidates take the combination path (`vars_typed_by_callback_parameter`,
    /// #17761).
    pub(super) disable_return_type_first_wins: bool,
}

impl<'a> InferenceContext<'a> {
    pub(super) fn discard_self_referential_candidates(
        &mut self,
        root: InferenceVar,
        candidates: &[InferenceCandidate],
    ) -> Vec<InferenceCandidate> {
        candidates
            .iter()
            .copied()
            .filter(|candidate| !self.occurs_in(root, candidate.type_id))
            .collect()
    }

    /// Returns `true` when `type_id` is a bare named `TypeParameter` with no
    /// `extends` constraint — a name that carries no structural information
    /// that a covariant inference result could violate.
    fn is_unconstrained_type_parameter(&self, type_id: TypeId) -> bool {
        crate::type_queries::named_type_param_info(self.interner, type_id)
            .is_some_and(|info| info.constraint.is_none())
    }

    /// Widen the covariant candidate and decide between the covariant and contra result.
    ///
    /// Called when both covariant and contra-variant candidates exist.
    /// tsc calls `getWidenedType(covariantInference)` in `getInferredType` before
    /// testing assignability to contra-candidates, so fresh object literals like
    /// `{a:1,b:2}` are widened to `{a:number,b:number}` before the check.
    /// Without widening, excess property checking rejects them against `{a:number}`.
    pub(super) fn resolve_covariant_against_contra(
        &self,
        covariant_result: TypeId,
        covariant_has_readonly_source: bool,
        concrete_contra_candidates: &[InferenceCandidate],
        from_array_element: bool,
        declared_constraint: Option<TypeId>,
        spread_rest_mode: Option<crate::inference::spread_rest_literals::SpreadRestLiteralMode>,
        mut external_is_subtype: Option<&mut dyn FnMut(TypeId, TypeId) -> bool>,
    ) -> TypeId {
        let covariant_widened = if is_literal_type(self.interner, covariant_result) {
            covariant_result
        } else if let Some(mode) = spread_rest_mode {
            // Tuples packed from trailing rest arguments widen per element
            // against the rest type parameter's declared constraint (tsc's
            // `getSpreadArgumentType`); the blanket deep widening below would
            // discard literal elements a literal-flavored constraint keeps.
            crate::inference::spread_rest_literals::widen_spread_rest_tuple(
                self.interner,
                covariant_result,
                declared_constraint,
                mode,
            )
        } else {
            widening::widen_type_for_inference(self.interner, covariant_result)
        };
        let covariant_is_uninformative = matches!(
            covariant_widened,
            TypeId::NEVER | TypeId::UNKNOWN | TypeId::ANY
        );
        let covariant_assignable_to_contra = !covariant_is_uninformative
            && concrete_contra_candidates.iter().any(|c| {
                if let Some(ref mut ext) = external_is_subtype {
                    ext(covariant_widened, c.type_id)
                } else {
                    self.is_subtype(covariant_widened, c.type_id)
                }
            });
        if !covariant_assignable_to_contra && !covariant_is_uninformative {
            let contra_result = self.resolve_from_contra_candidates(concrete_contra_candidates);
            if contra_result == TypeId::NEVER {
                return covariant_result;
            }
            if covariant_has_readonly_source {
                let contra_assignable_to_covariant = if let Some(ref mut ext) = external_is_subtype
                {
                    ext(contra_result, covariant_widened)
                } else {
                    self.is_subtype(contra_result, covariant_widened)
                };
                if contra_assignable_to_covariant {
                    return covariant_widened;
                }
            }
        }
        self.choose_covariant_or_contra(
            covariant_widened,
            concrete_contra_candidates,
            covariant_assignable_to_contra,
            covariant_is_uninformative,
            from_array_element,
        )
    }

    /// Apply the shared "prefer covariant" decision used by both
    /// `compute_constraint_result` and `fix_current_variables_with`.
    ///
    /// The covariant inference wins when it is assignable to some
    /// contra-candidate (tsc's normal `getInferredType` rule), or when every
    /// contra-candidate is a bare unconstrained type parameter and the covariant
    /// inference came from array-element matching (`T[]`). That is the stale leak
    /// shape from union-contextual overload argument typing, while higher-order
    /// function return-context inferences can carry real outer generic evidence.
    fn choose_covariant_or_contra(
        &self,
        covariant_result: TypeId,
        concrete_contra: &[InferenceCandidate],
        covariant_assignable_to_contra: bool,
        covariant_is_uninformative: bool,
        allow_stale_unconstrained_contra_override: bool,
    ) -> TypeId {
        if covariant_assignable_to_contra
            || (allow_stale_unconstrained_contra_override
                && !covariant_is_uninformative
                && concrete_contra
                    .iter()
                    .all(|c| self.is_unconstrained_type_parameter(c.type_id)))
        {
            covariant_result
        } else {
            self.resolve_from_contra_candidates(concrete_contra)
        }
    }

    pub(super) fn resolve_from_contra_candidates(
        &self,
        contra_candidates: &[crate::inference::infer::InferenceCandidate],
    ) -> TypeId {
        // tsc clears both candidate lists when a better inference priority is
        // encountered and then records only candidates at that priority. TSZ
        // retains the full history, so apply the equivalent filter here before
        // deduplication or combination.
        let prioritized_candidates = self.filter_candidates_by_priority(contra_candidates);
        let mut contra_types: Vec<InferenceCandidate> = Vec::new();
        for candidate in &prioritized_candidates {
            if !contra_types
                .iter()
                .any(|existing| existing.type_id == candidate.type_id)
            {
                contra_types.push(*candidate);
            }
        }

        // Filter out `any` when there are more specific candidates.
        // `any` in contravariant positions (e.g., from `boolean | any = any` in
        // interface method signatures) doesn't carry useful inference information
        // and should not override concrete candidates like `boolean`.
        // This matches tsc's behavior where `any` is treated as uninformative
        // during inference resolution.
        if contra_types.len() > 1 {
            let has_non_any = contra_types
                .iter()
                .any(|candidate| candidate.type_id != TypeId::ANY);
            if has_non_any {
                contra_types.retain(|candidate| candidate.type_id != TypeId::ANY);
            }
        }

        if contra_types.len() <= 1 {
            return contra_types
                .first()
                .map(|candidate| candidate.type_id)
                .unwrap_or(TypeId::UNKNOWN);
        }

        // Filter out `any` from the tournament when there are non-any candidates.
        // In tsc, `any` inferences from structural decomposition (e.g., matching
        // a method with `(t: any)` against `(t: T | U)`) don't override more
        // specific inferences from other call signatures. Since `any` is a subtype
        // of everything, it always wins the tournament incorrectly.
        let non_any: Vec<TypeId> = contra_types
            .iter()
            .map(|candidate| candidate.type_id)
            .filter(|&ty| ty != TypeId::ANY)
            .collect();
        let effective_types: Vec<TypeId> = if non_any.is_empty() {
            contra_types
                .iter()
                .map(|candidate| candidate.type_id)
                .collect()
        } else {
            non_any
        };

        if effective_types.len() <= 1 {
            return effective_types.first().copied().unwrap_or(TypeId::UNKNOWN);
        }

        let best_priority = contra_types.first().map(|candidate| candidate.priority);
        let priority_implies_combination = best_priority.is_some_and(|priority| {
            matches!(
                priority,
                InferencePriority::ReturnType
                    | InferencePriority::LowPriority
                    | InferencePriority::MappedType
                    | InferencePriority::LiteralKeyof
            )
        });
        if priority_implies_combination {
            return self.interner.intersection(effective_types);
        }

        // Mirror tsc's `getCommonSubtype`: walk candidates from left to right
        // and replace the current winner only when the new candidate is its
        // subtype. Unrelated candidates therefore keep the first inference.
        let mut winner = effective_types[0];
        for &candidate in &effective_types[1..] {
            if self.is_subtype(candidate, winner) {
                winner = candidate;
            }
        }
        winner
    }

    // =========================================================================
    // Bounds Checking and Resolution
    // =========================================================================

    /// Resolve an inference variable using its collected constraints.
    ///
    /// Algorithm:
    /// 1. If already unified to a concrete type, return that
    /// 2. Otherwise, compute the best common type from lower bounds
    /// 3. Validate against upper bounds
    /// 4. If no lower bounds, use the constraint (upper bound) or default
    pub fn resolve_with_constraints(
        &mut self,
        var: InferenceVar,
    ) -> Result<TypeId, InferenceError> {
        // Check if already resolved
        if let Some(ty) = self.probe(var) {
            return Ok(ty);
        }

        let (root, result, upper_bounds, upper_bounds_only, self_referential_bounds) =
            self.compute_constraint_result(var, None::<fn(TypeId, TypeId) -> bool>);

        // Validate against upper bounds.
        // Skip validation when result is `any` — tsc treats `any` as satisfying
        // all constraints, so it always passes upper bound checks.
        if !upper_bounds_only && result != TypeId::ANY {
            let filtered_upper_bounds = Self::filter_relevant_upper_bounds(&upper_bounds);
            if let Some(upper) =
                self.first_failed_upper_bound(result, &filtered_upper_bounds, |a, b| {
                    self.is_subtype(a, b)
                })
            {
                return Err(InferenceError::BoundsViolation {
                    var,
                    lower: result,
                    upper,
                });
            }
        }

        // Validate against self-referential bounds (e.g., `T extends I2<T>`).
        // These were skipped in compute_constraint_result because the variable
        // occurs in its own constraint. Now that we have a resolved value, substitute
        // it into the constraint and check if the resolved value satisfies it.
        // This matches tsc's getInferredType which checks:
        //   context.compareTypes(inferredType, instantiateType(constraint, nonFixingMapper))
        // For self-referential constraints, the nonFixingMapper resolves the variable
        // to its inferred value, making the constraint concrete and checkable.
        if !self_referential_bounds.is_empty() && result != TypeId::ANY {
            let names = self.type_param_names_for_root(root);
            if let Some(&param_name) = names.first() {
                let sub = TypeSubstitution::single(param_name, result);
                for &bound in &self_referential_bounds {
                    let instantiated_bound = instantiate_type(self.interner, bound, &sub);
                    if !self.is_subtype(result, instantiated_bound) {
                        return Err(InferenceError::BoundsViolation {
                            var,
                            lower: result,
                            upper: instantiated_bound,
                        });
                    }
                }
            }
        }

        if self.occurs_in(root, result) {
            return Err(InferenceError::OccursCheck {
                var: root,
                ty: result,
            });
        }

        // Store the result
        self.table.union_value(
            root,
            InferenceInfo {
                resolved: Some(result),
                ..InferenceInfo::default()
            },
        );

        Ok(result)
    }

    /// Resolve an inference variable using its collected constraints and a custom
    /// assignability check for upper-bound validation.
    pub fn resolve_with_constraints_by<F>(
        &mut self,
        var: InferenceVar,
        mut is_subtype: F,
    ) -> Result<TypeId, InferenceError>
    where
        F: FnMut(TypeId, TypeId) -> bool,
    {
        // Check if already resolved
        if let Some(ty) = self.probe(var) {
            return Ok(ty);
        }

        let (root, result, upper_bounds, upper_bounds_only, self_referential_bounds) =
            self.compute_constraint_result(var, Some(&mut is_subtype));

        // Skip upper bound validation for `any` — it satisfies all constraints in tsc.
        if !upper_bounds_only && result != TypeId::ANY {
            let filtered_upper_bounds = Self::filter_relevant_upper_bounds(&upper_bounds);
            if let Some(upper) =
                self.first_failed_upper_bound(result, &filtered_upper_bounds, |a, b| {
                    is_subtype(a, b)
                })
            {
                return Err(InferenceError::BoundsViolation {
                    var,
                    lower: result,
                    upper,
                });
            }
        }

        // Validate self-referential bounds (same as in resolve_with_constraints).
        if !self_referential_bounds.is_empty() && result != TypeId::ANY {
            let names = self.type_param_names_for_root(root);
            if let Some(&param_name) = names.first() {
                let sub = TypeSubstitution::single(param_name, result);
                for &bound in &self_referential_bounds {
                    let instantiated_bound = instantiate_type(self.interner, bound, &sub);
                    if !is_subtype(result, instantiated_bound) {
                        return Err(InferenceError::BoundsViolation {
                            var,
                            lower: result,
                            upper: instantiated_bound,
                        });
                    }
                }
            }
        }

        if self.occurs_in(root, result) {
            return Err(InferenceError::OccursCheck {
                var: root,
                ty: result,
            });
        }

        self.table.union_value(
            root,
            InferenceInfo {
                resolved: Some(result),
                ..InferenceInfo::default()
            },
        );

        Ok(result)
    }

    fn filter_relevant_upper_bounds(upper_bounds: &[TypeId]) -> Vec<TypeId> {
        upper_bounds
            .iter()
            .copied()
            .filter(|&upper| !upper.is_any_unknown_or_error())
            .collect()
    }

    fn first_failed_upper_bound<F>(
        &self,
        result: TypeId,
        filtered_upper_bounds: &[TypeId],
        mut is_subtype: F,
    ) -> Option<TypeId>
    where
        F: FnMut(TypeId, TypeId) -> bool,
    {
        match filtered_upper_bounds {
            [] => None,
            [single] => (!is_subtype(result, *single)).then_some(*single),
            many => {
                // Building and checking a very large synthetic intersection can be
                // more expensive than directly validating bounds one-by-one.
                // Keep the intersection shortcut for small/medium bound sets only.
                if many.len() <= Self::UPPER_BOUND_INTERSECTION_FAST_PATH_LIMIT {
                    let intersection = self.interner.intersection(many.to_vec());
                    if is_subtype(result, intersection) {
                        return None;
                    }
                }
                // For very large upper-bound sets, a single intersection check can
                // still be profitable in the common success path (all bounds satisfy).
                // Fall back to per-bound checks if that coarse check fails.
                if many.len() >= Self::UPPER_BOUND_INTERSECTION_LARGE_SET_THRESHOLD
                    && self.should_try_large_upper_bound_intersection(result, many)
                {
                    let intersection = self.interner.intersection(many.to_vec());
                    if is_subtype(result, intersection) {
                        return None;
                    }
                }
                many.iter()
                    .copied()
                    .find(|&upper| !is_subtype(result, upper))
            }
        }
    }

    fn should_try_large_upper_bound_intersection(&self, result: TypeId, bounds: &[TypeId]) -> bool {
        self.is_object_like_upper_bound(result)
            && bounds
                .iter()
                .copied()
                .all(|bound| self.is_object_like_upper_bound(bound))
    }

    fn is_object_like_upper_bound(&self, ty: TypeId) -> bool {
        if ty.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(ty) {
            Some(
                TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Lazy(_)
                | TypeData::Intersection(_),
            ) => true,
            Some(TypeData::TypeParameter(info)) => info
                .constraint
                .is_some_and(|constraint| self.is_object_like_upper_bound(constraint)),
            _ => false,
        }
    }

    pub(super) fn compute_constraint_result<F>(
        &mut self,
        var: InferenceVar,
        mut external_is_subtype: Option<F>,
    ) -> (InferenceVar, TypeId, Vec<TypeId>, bool, Vec<TypeId>)
    where
        F: FnMut(TypeId, TypeId) -> bool,
    {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        let target_names = self.type_param_names_for_root(root);
        let mut upper_bounds = Vec::new();
        let mut self_referential_bounds = Vec::new();
        let mut seen_upper_bounds = FxHashSet::default();
        let mut candidates = self.discard_self_referential_candidates(root, &info.candidates);
        let contra_candidates =
            self.discard_self_referential_candidates(root, &info.contra_candidates);
        for bound in info.upper_bounds {
            if self.occurs_in(root, bound) {
                self_referential_bounds.push(bound);
                continue;
            }
            if !target_names.is_empty() && self.upper_bound_cycles_param(bound, &target_names) {
                self.expand_cyclic_upper_bound(
                    root,
                    bound,
                    &target_names,
                    &mut candidates,
                    &mut upper_bounds,
                );
                continue;
            }
            if seen_upper_bounds.insert(bound) {
                upper_bounds.push(bound);
            }
        }

        if !upper_bounds.is_empty() {
            // Keep `any` candidates when bounds are only top types (unknown/any/error).
            // Otherwise unconstrained generic parameters can collapse from `any` to `unknown`
            // (e.g. Promise/iterable inference with implicit `extends unknown`).
            let has_informative_upper_bound = upper_bounds
                .iter()
                .any(|&upper| !upper.is_any_unknown_or_error());
            // Check if there are concrete (non-top) candidates before filtering.
            // When `any` is the only meaningful candidate, keep it even with
            // informative upper bounds. This matches tsc where passing `any` to
            // `f<T extends X>(v: T)` infers T=any, not T=X.
            let has_concrete_candidate = candidates
                .iter()
                .any(|c| !c.type_id.is_any_unknown_or_error());
            candidates.retain(|candidate| match candidate.type_id {
                TypeId::UNKNOWN | TypeId::ERROR => false,
                TypeId::ANY => !has_informative_upper_bound || !has_concrete_candidate,
                _ => true,
            });
        }

        if self.vars_with_substituted_candidates.contains(&root)
            && candidates.iter().any(|candidate| {
                !crate::type_queries::data::contains_current_infer_placeholder_db(
                    self.interner,
                    candidate.type_id,
                )
            })
        {
            // Candidate substitution can leave the pre-substitution placeholder
            // candidate in the union table. Drop only call-local placeholders when
            // concrete candidates exist, then let the normal resolver handle
            // priority, upper bounds, contra candidates, and occurs checks.
            candidates.retain(|candidate| {
                !crate::type_queries::data::contains_current_infer_placeholder_db(
                    self.interner,
                    candidate.type_id,
                )
            });
        }

        // Check if this is a const type parameter to preserve literal types
        let is_const = self.is_var_const(root);

        let mut concrete_contra_candidates: Vec<_> = contra_candidates
            .iter()
            .filter(|c| self.is_concrete_contra_candidate(c.type_id))
            .cloned()
            .collect();

        // Discard contra-candidates whose priority is strictly worse than the
        // best covariant priority. Mirrors tsc's `inferTypes` (checker.ts
        // ~line 26895), which clears any existing candidates and ignores new
        // ones whose priority is worse than `inference.priority`. Without this,
        // a low-priority `LiteralKeyof` contra-candidate (synthesised from
        // `keyof T = "z"`) can override a high-priority `NakedTypeVariable`
        // covariant candidate (from `obj: T = a`).
        if !candidates.is_empty()
            && !concrete_contra_candidates.is_empty()
            && let Some(best_cov_priority) = candidates.iter().map(|c| c.priority).min()
        {
            concrete_contra_candidates.retain(|c| c.priority <= best_cov_priority);
        }

        let upper_bounds_only = candidates.is_empty()
            && concrete_contra_candidates.is_empty()
            && !upper_bounds.is_empty();

        let declared_constraint = self.declared_constraints.get(&root).copied();
        let declared_constraint_preserves_literals =
            self.literal_preserving_declared_constraints.contains(&root);
        let skip_literal_widening = self.top_level_in_return_type_unfixed.contains(&root);
        let spread_rest_mode = self.spread_rest_var_modes.get(&root).copied();

        let result = if !candidates.is_empty() {
            // Covariant candidates exist: use union/BCT (matches tsc's getInferredType)
            let covariant_result = self.resolve_from_candidates(
                &candidates,
                is_const,
                &upper_bounds,
                declared_constraint,
                declared_constraint_preserves_literals,
                LiteralWideningPolicy {
                    skip_literal_widening,
                    preserve_return_position_literals: self
                        .root_preserves_return_position_literals(root),
                    disable_return_type_first_wins: self
                        .vars_typed_by_callback_parameter
                        .contains(&root),
                },
                spread_rest_mode,
            );
            if !concrete_contra_candidates.is_empty() {
                // Match tsc's getInferredType: when both co- and contra-variant
                // inferences exist, prefer the covariant result ONLY IF:
                //   1. It is not never or any
                //   2. It is assignable to some contra-candidate
                // Otherwise, fall back to the contravariant result.
                //
                // This ensures that inference from function parameter positions
                // (contravariant) takes precedence when the covariant candidate
                // (from direct argument inference) conflicts with the structural
                // constraints. For example:
                //   declare function create<P>(factory: (props: P) => void, props: P): void;
                //   create(f, { value: "C" });
                // P should be inferred from the function parameter (contra: Props),
                // not from the object literal (co: { value: "C" }), because the
                // object literal type is not assignable to Props.
                self.resolve_covariant_against_contra(
                    covariant_result,
                    candidates.iter().any(|c| c.from_readonly_source),
                    &concrete_contra_candidates,
                    candidates.iter().any(|c| c.from_array_element),
                    declared_constraint,
                    spread_rest_mode,
                    external_is_subtype
                        .as_mut()
                        .map(|e| e as &mut dyn FnMut(TypeId, TypeId) -> bool),
                )
            } else {
                covariant_result
            }
        } else if !concrete_contra_candidates.is_empty() {
            // Only contravariant candidates: use intersection (matches tsc behavior).
            // In tsc, when only contraCandidates exist, getIntersectionType is used.
            self.resolve_from_contra_candidates(&concrete_contra_candidates)
        } else if !upper_bounds.is_empty() {
            // RESTORED: Fall back to upper bounds (constraints) when no candidates exist.
            // This matches TypeScript: un-inferred generics default to their constraint.
            // We use intersection in case there are multiple upper bounds (T extends A, T extends B).
            if upper_bounds.len() == 1 {
                upper_bounds[0]
            } else {
                self.interner.intersection(upper_bounds.clone())
            }
        } else {
            // Only return UNKNOWN if there are NO candidates AND NO upper bounds
            TypeId::UNKNOWN
        };

        (
            root,
            result,
            upper_bounds,
            upper_bounds_only,
            self_referential_bounds,
        )
    }

    /// Resolve all type parameters using constraints.
    pub fn resolve_all_with_constraints(&mut self) -> Result<Vec<(Atom, TypeId)>, InferenceError> {
        // CRITICAL: Strengthen inter-parameter constraints before resolution
        // This ensures that constraints flow between dependent type parameters
        // Example: If T extends U, and T is constrained to string, then U is also
        // constrained to accept string (string must be assignable to U)
        self.strengthen_constraints()?;

        let type_params: Vec<_> = self.type_params.clone();
        let mut results = Vec::new();

        for (name, var, _) in type_params {
            let ty = self.resolve_with_constraints(var)?;
            results.push((name, ty));
        }

        Ok(results)
    }

    pub(super) fn resolve_from_candidates(
        &self,
        candidates: &[InferenceCandidate],
        is_const: bool,
        upper_bounds: &[TypeId],
        declared_constraint: Option<TypeId>,
        declared_constraint_preserves_literals: bool,
        widening_policy: LiteralWideningPolicy,
        spread_rest_mode: Option<crate::inference::spread_rest_literals::SpreadRestLiteralMode>,
    ) -> TypeId {
        let LiteralWideningPolicy {
            skip_literal_widening,
            preserve_return_position_literals,
            disable_return_type_first_wins,
        } = widening_policy;
        let filtered = self.filter_candidates_by_priority(candidates);
        tracing::trace!(
            candidates = ?candidates
                .iter()
                .map(|c| (c.type_id, c.priority, c.is_fresh_literal, c.from_object_property, c.from_top_level_naked))
                .collect::<Vec<_>>(),
            filtered = filtered.len(),
            "resolve_from_candidates"
        );
        if filtered.is_empty() {
            return TypeId::UNKNOWN;
        }
        let filtered_no_never: Vec<_> = filtered
            .iter()
            .filter(|c| c.type_id != TypeId::NEVER)
            .cloned()
            .collect();
        if filtered_no_never.is_empty() {
            return TypeId::NEVER;
        }
        // tsc's `getSupertypeOrUnion` unions covariant candidates that are
        // literals of a single base type (`literalTypesWithSameBaseType` ->
        // `getUnionType`) instead of collapsing to one priority winner or
        // widening them to their base. tsz records a directly-passed callback's
        // return inference at `ReturnType` priority and a naked argument at
        // `NakedTypeVariable`, so without this the priority filter drops the
        // lower-priority candidate (`h<U>(fn: () => U, init: U)` called
        // `h(() => 5, 0)` resolves `U = 0` rather than `0 | 5`) and the
        // `ReturnType` combination branch widens two callback candidates
        // (`k<T>(a: () => T, b: () => T)` called `k(() => 1, () => 2)` resolves
        // `T = number` rather than `1 | 2`).
        //
        // Fire only when the call pins these literals (`skip_literal_widening`,
        // e.g. a literal contextual type on a naked return-position parameter,
        // #17710) and every argument-derived candidate across all priorities is a
        // literal of one base type. This runs BEFORE the pin-agreement narrowing
        // below (#17778): that narrowing widens a *disagreeing* pinned literal to
        // its base to avoid a spurious second diagnostic, but when the disagreeing
        // literals share a base tsc combines them instead, which this returns.
        // The pure-naked same-base case (`f<T>(a: T, b: T)` -> `1 | 2`) already
        // reaches the same union through `get_common_supertype_for_inference`;
        // this extends it across the priority levels tsz separates.
        //
        // Contextual-return-hint candidates (`from_contextual_return_hint`) are
        // excluded: tsc seeds the call's own literal contextual type as a
        // covariant candidate but drops it once genuine argument candidates
        // arrive, so `const r: 5 = h(() => 7, 0)` unions the callback/argument
        // literals `0 | 7` rather than folding in the contextual `5`. When the
        // contextual value also comes from an argument, that argument's own
        // (unflagged) candidate keeps the value (`h(() => 5, 0)` -> `0 | 5`).
        if skip_literal_widening {
            let arg_literal_candidates: Vec<TypeId> = candidates
                .iter()
                .filter(|candidate| {
                    candidate.type_id != TypeId::NEVER && !candidate.from_contextual_return_hint
                })
                .map(|candidate| candidate.type_id)
                .collect();
            if arg_literal_candidates.len() > 1
                && arg_literal_candidates
                    .iter()
                    .all(|&ty| is_literal_type(self.interner, ty))
            {
                let base = self.get_base_type(arg_literal_candidates[0]);
                if base.is_some()
                    && arg_literal_candidates
                        .iter()
                        .all(|&ty| self.get_base_type(ty) == base)
                {
                    let mut distinct: Vec<TypeId> =
                        Vec::with_capacity(arg_literal_candidates.len());
                    for &ty in &arg_literal_candidates {
                        if !distinct.contains(&ty) {
                            distinct.push(ty);
                        }
                    }
                    if distinct.len() > 1 {
                        return self.interner.union_from_slice(&distinct);
                    }
                }
            }
        }
        // The union above already returned `tsc`'s combined type for the
        // same-base disagreeing case (#17773); the residual is a base mismatch,
        // where honouring either pin source keeps one literal and re-checks the
        // other at its own inference site — a second diagnostic `tsc` never emits.
        // Both pin sources therefore compose under this agreement condition
        // (computed once over the raw candidate set, since priority filtering has
        // already discarded the losing literal from `filtered_no_never`).
        let fresh_literal_candidates_agree = {
            let mut fresh_literals = candidates
                .iter()
                .filter(|candidate| candidate.is_fresh_literal)
                .map(|candidate| candidate.type_id);
            match fresh_literals.next() {
                None => true,
                Some(first) => fresh_literals.all(|type_id| type_id == first),
            }
        };
        let all_from_object_properties = filtered_no_never
            .iter()
            .all(|candidate| candidate.from_object_property);
        // tsc `getCovariantInference`: `widenLiteralTypes = inference.topLevel &&
        // (inference.isFixed || !isTypeParameterAtTopLevelInReturnType(signature,
        // tp))`. `preserve_return_position_literals` carries the parenthesized
        // half (top level in the return type, never fixed for contextual
        // typing); the `inference.topLevel` half holds when every counted
        // candidate was itself inferred at the top level of its argument
        // position (a structural/nested-position candidate re-enables
        // widening, matching tsc clearing `topLevel`).
        //
        // Both pin sources — the contextual mark and the runtime
        // return-position preserve — compose under the agreement condition:
        // neither may pin while the fresh literal candidates disagree, or the
        // losing literal (already discarded from `filtered_no_never` by
        // priority filtering, hence the read over raw `candidates`) is
        // re-checked against the pinned winner and produces a second
        // diagnostic tsc never emits (#17773/#17778).
        let skip_literal_widening = (skip_literal_widening
            || (preserve_return_position_literals
                && filtered_no_never.iter().all(|c| c.at_top_level_of_walk)))
            && fresh_literal_candidates_agree;
        // TypeScript preserves literal types when:
        // 1. The type parameter is `const`, OR
        // 2. The declared constraint implies literals (e.g., T extends "a" | "b"), OR
        // 3. The declared constraint IS a primitive (e.g., T extends string/number/boolean/bigint)
        // Note: we use the declared constraint (from the `extends` clause), NOT upper_bounds
        // which also includes contextual type bounds. This prevents false preservation when
        // e.g., `<T>(value: T): Box<T>` is contextually typed as `Box<boolean>`.
        let preserve_literals = is_const
            || self.constraint_implies_literals(upper_bounds)
            || declared_constraint_preserves_literals
            || declared_constraint.is_some_and(|c| self.type_implies_literals(c))
            || declared_constraint.is_some_and(|c| self.declared_constraint_is_primitive(c))
            || declared_constraint.is_some_and(|c| {
                self.constraint_contains_type_param_with_primitive_constraint(c, 0)
            });
        // Match tsc's inference resolution order.
        //
        // tsc's getCovariantInference checks `priority & PriorityImpliesCombination`:
        //   - If combination priority (ReturnType, MappedTypeConstraint, LiteralKeyof):
        //     use getUnionType (creates a union of all candidates)
        //   - Otherwise (NakedTypeVariable, HomomorphicMappedType, etc.):
        //     use getCommonSupertype which does NOT create unions for incompatible types.
        //     Instead, it picks the first non-superseded candidate (via reduceLeft).
        //
        // This distinction is critical: for `foo<T>(n: {x: T, y: T}, m: T)` called as
        // `foo({x: 3, y: ""}, 4)`, tsc infers T = number (first candidate wins),
        // NOT T = number | string (union). The string property then gets TS2322.
        let const_applies_readonly_assertion = is_const
            && !declared_constraint.is_some_and(|constraint| {
                crate::type_queries::constraint_allows_mutable_array_like(self.interner, constraint)
            });
        let candidate_types: Vec<TypeId> = if const_applies_readonly_assertion {
            filtered_no_never
                .iter()
                .map(|c| widening::apply_const_assertion(self.interner, c.type_id))
                .collect()
        } else {
            filtered_no_never.iter().map(|c| c.type_id).collect()
        };
        // Match tsc's PriorityImpliesCombination = ReturnType | MappedTypeConstraint | LiteralKeyof.
        // When candidates come from these priority levels, they are combined via union
        // (getUnionType) rather than common supertype. This is critical for mapped types:
        // `makeRecord<T, K extends string>(obj: { [P in K]: T })` called with
        // `{ a: Box<number>, b: Box<string> }` should infer T = Box<number> | Box<string>,
        // not just Box<number>.
        let priority_implies_combination = filtered_no_never
            .first()
            .map(|c| {
                matches!(
                    c.priority,
                    InferencePriority::ReturnType
                        | InferencePriority::LowPriority
                        | InferencePriority::MappedType
                        | InferencePriority::LiteralKeyof
                )
            })
            .unwrap_or(false);
        // When ALL candidates come from index signature inference (e.g.,
        // {a: string, b: number} → {[key: string]: T}), use union semantics.
        // The index signature T represents the union of all property value types.
        // tsc handles this via getCommonSupertype's fallback to getUnionType when
        // no single supertype exists, but only for this pattern — for direct
        // parameter inference (e.g., f<T>(x: T, y: T) called as f(1, "")),
        // tsc picks the first non-superseded candidate.
        let all_from_index_signatures = filtered_no_never
            .iter()
            .all(|candidate| candidate.from_index_signature);
        let has_index_signature_candidates = filtered_no_never
            .iter()
            .any(|candidate| candidate.from_index_signature);
        let has_type_annotation_candidate = filtered_no_never
            .iter()
            .any(|candidate| candidate.source_is_type_annotation);
        // ReturnType candidates are the one member of the combination set that
        // does NOT always union in tsc: two directly-passed callback arguments
        // that both contribute a ReturnType-priority candidate for the same type
        // parameter are NOT combined — `declare function k(a: () => T, b: () =>
        // T): T; k(() => "s", () => 1)` fixes `T = string` from the first
        // callback and reports `TS2322` on the second (#17553), the "first wins"
        // rule tsc's `getCommonSupertype` already applies to disjoint bare
        // primitives elsewhere in this file (naked/array-element candidates).
        // Scoped tightly to avoid disturbing the index-signature re-union gate
        // below (which also keys on `priority_implies_combination`): only fires
        // with 2+ ReturnType candidates that are all disjoint bare primitives
        // and none index-signature-sourced.
        let return_type_disjoint_primitives_first_wins = !disable_return_type_first_wins
            && filtered_no_never.len() > 1
            && !has_index_signature_candidates
            && filtered_no_never
                .first()
                .is_some_and(|c| c.priority == InferencePriority::ReturnType)
            && filtered_no_never.iter().all(|c| {
                matches!(
                    c.type_id,
                    TypeId::STRING
                        | TypeId::NUMBER
                        | TypeId::BOOLEAN
                        | TypeId::BIGINT
                        | TypeId::SYMBOL
                )
            });
        let resolved = if return_type_disjoint_primitives_first_wins {
            candidate_types[0]
        } else if priority_implies_combination || all_from_index_signatures {
            // Mirror tsc's `getCovariantInference` for the
            // `PriorityImpliesCombination` branch: build the subtype-reduced
            // union of the candidates rather than the common supertype.
            //
            // We must NOT use `best_common_type` when every candidate is a
            // non-fresh literal type, because its first step
            // (`find_common_base_type`) collapses literals to their primitive
            // base (`1 | 2` -> `number`), discarding the precise type the
            // user asked for via `as const`. tsc's later `getWidenedType`
            // step is the right place for that collapse — and it is gated on
            // candidate freshness (issue #9714).
            //
            // For structural / class-hierarchy candidates we keep
            // `best_common_type`: its tournament + common-base-class steps
            // produce the supertype that the existing test baselines (and
            // tsz's downstream solver paths) already depend on.  Switching
            // those wholesale to `getUnionType(Subtype)` is correct in
            // principle but produces a different inferred type for many
            // tests (e.g. `[Dog, Cat]` extending `Animal` becomes
            // `Dog | Cat` instead of `Animal`) — that is a separate broad
            // realignment best handled in its own PR.
            let all_non_fresh_literals = !filtered_no_never.is_empty()
                && filtered_no_never
                    .iter()
                    .all(|c| !c.is_fresh_literal && is_literal_type(self.interner, c.type_id));
            // When the literal-widening gate says fresh literals survive
            // (`skip_literal_widening`), `best_common_type`'s
            // `find_common_base_type` step must not collapse them either:
            // union the all-literal candidate set instead, mirroring the
            // widening branch's gate on the combination path (#17710).
            let gated_fresh_literals = skip_literal_widening
                && !filtered_no_never.is_empty()
                && filtered_no_never
                    .iter()
                    .all(|c| is_literal_type(self.interner, c.type_id));
            if all_non_fresh_literals || gated_fresh_literals {
                self.interner.union_from_slice(&candidate_types)
            } else {
                self.best_common_type(&candidate_types)
            }
        } else {
            // Common supertype: used for NakedTypeVariable and other direct inference.
            // tsc widens literal candidates BEFORE getCommonSupertype (via baseCandidates =
            // sameMap(candidates, getWidenedLiteralType)). This ensures the tournament
            // operates on widened types (number, string) not literals (3, "").
            let has_fresh_array_element_candidate = filtered_no_never
                .iter()
                .any(|c| c.from_array_element && c.is_fresh_literal);
            let has_non_fresh = filtered_no_never.iter().any(|c| {
                !(c.is_fresh_literal
                    || has_fresh_array_element_candidate && c.type_id.is_any_unknown_or_error())
            });
            // Mirror tsc's `widenLiteralTypes` gate in `getCovariantInference`:
            // when the type parameter is at top level in the return type AND has
            // not yet been fixed, fresh literal candidates are NOT widened during
            // the contextual-type substitution. This preserves literals like `U=1`
            // for the Round 2 deferred-callback contextual type, so that
            // `(a: T) => U` becomes `(a: number) => 1` (matching tsc) rather than
            // `(a: number) => number`.
            let should_widen =
                !preserve_literals && !is_const && !has_non_fresh && !skip_literal_widening;
            let widened_candidates: Vec<TypeId> = candidate_types
                .iter()
                .map(|&ty| {
                    if should_widen {
                        let widened =
                            crate::operations::widening::widen_literal_type(self.interner, ty);
                        if widened == ty {
                            // tsc's `getWidenedLiteralType` also widens an enum
                            // member literal to its parent enum type under the
                            // same freshness gate (`E1.X` -> `E1`), which
                            // `widen_literal_type` does not model because it
                            // has no resolver for the member -> parent link.
                            self.widen_enum_member_to_parent(ty).unwrap_or(ty)
                        } else {
                            widened
                        }
                    } else {
                        ty
                    }
                })
                .collect();
            let has_non_fresh_object_candidate = widened_candidates
                .iter()
                .any(|&ty| self.is_non_fresh_object_candidate(ty));
            let has_fresh_object_candidate = widened_candidates
                .iter()
                .any(|&ty| self.is_fresh_object_literal_candidate(ty));
            let widened_candidates = if has_non_fresh_object_candidate && has_fresh_object_candidate
            {
                widened_candidates
                    .into_iter()
                    .filter(|&ty| !self.is_fresh_object_literal_candidate(ty))
                    .collect()
            } else {
                widened_candidates
            };
            // Match tsc's unionObjectAndArrayLiteralCandidates: before running the
            // common-supertype tournament, union all object and array literal
            // candidates into a single union candidate. This ensures that for
            // `f<const T>(obj: {x: T, y: T})` called with `{x: {a: 1}, y: {a: 2}}`,
            // T is inferred as `{a: 1} | {a: 2}` (union) rather than `{a: 1}`
            // (first-wins from the tournament).
            let widened_candidates =
                self.union_object_and_array_literal_candidates(&widened_candidates);
            // When a candidate was inferred from an array-literal element position
            // (a `T[]` parameter), tsc's `getCommonSupertype` fixes `T` to the
            // leftmost candidate rather than unioning incompatible primitives.
            // Surface that fact so the supertype fallback matches tsc and a
            // conflicting naked argument is reported (issue #9667).
            let has_array_element_candidate =
                filtered_no_never.iter().any(|c| c.from_array_element);
            // Every candidate came from matching a top-level argument directly
            // against a bare type parameter (`f<T>(a: T, b: T)`), so tsz's
            // candidate order is the source argument order that tsc's
            // `getCommonSupertype` `reduceLeft` keys on, making the disjoint
            // bare-primitive leftmost-wins fallback safe (issue #17484).
            // Candidates collected inside a structural walk (object property,
            // tuple/array/rest element) have `from_top_level_naked = false`, so
            // they keep tsc's order-independent union — tsz's order there does
            // not match tsc's.
            let all_from_top_level_naked = !filtered_no_never.is_empty()
                && filtered_no_never.iter().all(|c| c.from_top_level_naked);
            // Distinguish the *all-from-array-element* case (e.g. both `V`
            // candidates of `new Map([["", true], ["", 0]])`, one per tuple leg)
            // from the mixed array+naked case (#9667). tsc id-sorts the former
            // (lowest intrinsic wins, order-independent) but keeps the leftmost
            // array candidate for the latter, so only the all-from-array case
            // takes the ranked-winner path (#17364).
            let all_from_array_element = !filtered_no_never.is_empty()
                && filtered_no_never.iter().all(|c| c.from_array_element);
            self.get_common_supertype_for_inference(
                &widened_candidates,
                has_array_element_candidate,
                all_from_top_level_naked,
                all_from_array_element,
                all_from_object_properties,
            )
        };
        // When candidates come from index signature inference (e.g., inferring T from
        // source properties against target `{ [x: string]: T }`), tsc creates a union
        // of all candidate types. The tournament in get_common_supertype_for_inference
        // may have picked a single winner, but for index signatures we need the union.
        let resolved = if has_index_signature_candidates && !priority_implies_combination {
            // Filter out error types that arise from failed constraint paths
            // (e.g., readonly mismatches). These should not pollute the union.
            let valid_candidates: Vec<TypeId> = candidate_types
                .iter()
                .copied()
                .filter(|&c| c != TypeId::ERROR && c != TypeId::NEVER)
                .collect();
            let all_same = valid_candidates.iter().all(|&c| c == resolved);
            if all_same || valid_candidates.is_empty() {
                resolved
            } else {
                // Apply the same all-non-fresh-literals gate as the
                // combination branch above: union with subtype reduction when
                // the candidates are user-pinned literals (so `1 | 2` does
                // not collapse to `number`), otherwise fall back to the
                // common-supertype tournament via `best_common_type`.
                let all_non_fresh_literals = filtered_no_never
                    .iter()
                    .filter(|c| valid_candidates.contains(&c.type_id))
                    .all(|c| !c.is_fresh_literal && is_literal_type(self.interner, c.type_id))
                    && !filtered_no_never.is_empty();
                if all_non_fresh_literals {
                    self.interner.union(valid_candidates)
                } else {
                    self.best_common_type(&valid_candidates)
                }
            }
        } else {
            resolved
        };
        // Widen the resolved type if literals should not be preserved.
        // After best_common_type, subtype reduction has already eliminated redundant
        // fresh literals (e.g., `1` is absorbed by `0 | 1`).
        //
        // We only widen when ALL candidates are fresh literals. This matches tsc's
        // getWidenedLiteralType which only widens fresh literal types. When a
        // non-fresh candidate (e.g., from a type annotation) survives BCT, its
        // literal types should be preserved.
        let has_fresh_array_element_candidate = filtered_no_never
            .iter()
            .any(|c| c.from_array_element && c.is_fresh_literal);
        let has_non_fresh = filtered_no_never.iter().any(|c| {
            !(c.is_fresh_literal
                || has_fresh_array_element_candidate && c.type_id.is_any_unknown_or_error())
        });
        let resolved =
            if !preserve_literals && !is_const && !has_non_fresh && !skip_literal_widening {
                self.widen_resolved_inference(resolved)
            } else {
                resolved
            };
        // Deep-widen the resolved type when it is an object literal containing
        // fresh literals. TSC calls getWidenedType() in getInferredType() for this.
        // E.g., { c: false } → { c: boolean }.
        // Only apply to Object types — simple literals and unions are already handled
        // by widen_candidate_types above; tuples/arrays are excluded to avoid
        // over-widening in contexts like `new Map([["", true]])`.
        // Deep-widen object literal candidates for non-contextual priorities
        // (NakedTypeVariable, HomomorphicMappedType, etc). Skip for ReturnType
        // and LowPriority which come from contextual typing where literal types
        // should be preserved (tsc uses RequiresWidening for this; we approximate
        // by checking the inference priority).
        // HomomorphicMappedType is non-contextual: `{ [K in keyof T]: ... }` candidates
        // should be deep-widened, e.g. `{ c: false }` → `{ c: boolean }`.
        let highest_priority = filtered_no_never.first().map(|c| c.priority);
        let is_contextual_inference = matches!(
            highest_priority,
            Some(InferencePriority::ReturnType | InferencePriority::LowPriority)
        );
        let resolved = if !preserve_literals && !is_contextual_inference && !resolved.is_intrinsic()
        {
            match self.interner.lookup(resolved) {
                Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                    // Only deep-widen fresh object literals (from object literal
                    // expressions). Non-fresh objects (from type annotations/aliases)
                    // should preserve their literal property types, matching tsc's
                    // RequiresWidening check in getWidenedType().
                    let shape = self.interner.object_shape(shape_id);
                    if shape.flags.contains(ObjectFlags::FRESH_LITERAL)
                        && !has_type_annotation_candidate
                    {
                        widening::widen_type_for_inference(self.interner, resolved)
                    } else {
                        resolved
                    }
                }
                // Arrays and tuples: only widen when the candidate is fresh
                // (not from a type assertion). Mirrors tsc's RequiresWidening
                // semantics: `as T` produces non-fresh types that must not widen.
                Some(TypeData::Array(_) | TypeData::Tuple(_)) => {
                    if has_type_annotation_candidate {
                        resolved
                    } else if let Some(mode) = spread_rest_mode {
                        // A tuple packed from trailing rest arguments widens per
                        // element against the rest type parameter's declared
                        // constraint (tsc's `getSpreadArgumentType`), so
                        // `f<T extends string[]>(...args: T)` keeps `["a", "b"]`
                        // while `T extends any[]` still widens to
                        // `[string, string]`.
                        crate::inference::spread_rest_literals::widen_spread_rest_tuple(
                            self.interner,
                            resolved,
                            declared_constraint,
                            mode,
                        )
                    } else {
                        widening::widen_type_for_inference(self.interner, resolved)
                    }
                }
                _ => resolved,
            }
        } else {
            resolved
        };
        // First-property-wins fallback: when BCT/getCommonSupertype produced a
        // union from object-property candidates that don't share a subtype
        // relationship (e.g., `{x: 3, y: ""}` → `number | string`), tsc actually
        // picks the first candidate (number). We replicate that here.
        //
        // EXCEPTION: when any candidate is a nullable type (undefined/null/void),
        // the union is the *correct* result of tsc's getCommonSupertype, which
        // strips nullables, runs the tournament on what remains, and then
        // re-attaches the nullable members via getNullableType. For example,
        // `foo<T>(f1: { x: T; y: T })` called with `{ x: undefined, y: "def" }`
        // strips `undefined`, sees a single non-nullable candidate `string`
        // (after widening), and returns `string | undefined`. Applying first-
        // wins here would collapse that back to `undefined`, which is wrong:
        // tsc emits `Type 'string | undefined' is not assignable to type 'number'`,
        // not `Type 'undefined' is not assignable to type 'number'`.
        let any_candidate_is_nullable = filtered_no_never.iter().any(|c| c.type_id.is_nullable());
        if all_from_object_properties
            && !has_index_signature_candidates
            && !is_const
            && !any_candidate_is_nullable
            && let Some(TypeData::Union(member_list_id)) = self.interner.lookup(resolved)
        {
            let member_count = self.interner.type_list(member_list_id).len();
            if member_count > 1 {
                let mut first_property_name = None;
                let mut has_multiple_property_names = false;
                for candidate in &filtered_no_never {
                    if let Some(name) = candidate.object_property_name {
                        if let Some(prev_name) = first_property_name {
                            if prev_name != name {
                                has_multiple_property_names = true;
                                break;
                            }
                        } else {
                            first_property_name = Some(name);
                        }
                    } else {
                        has_multiple_property_names = false;
                        break;
                    }
                }

                if !has_multiple_property_names {
                    return resolved;
                }

                if let Some(fallback_idx) = filtered_no_never
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, candidate)| {
                        candidate.object_property_name.map(|name| {
                            (
                                self.interner.resolve_atom_ref(name),
                                candidate.object_property_index.unwrap_or(u32::MAX),
                                idx,
                            )
                        })
                    })
                    .min_by(|(name_l, index_l, _), (name_r, index_r, _)| {
                        name_l.cmp(name_r).then_with(|| index_l.cmp(index_r))
                    })
                    .map(|(_, _, fallback_idx)| fallback_idx)
                {
                    return candidate_types[fallback_idx];
                }
                return candidate_types[0];
            }
        }
        resolved
    }

    /// Check if any upper bounds contain or imply literal types.
    fn constraint_implies_literals(&self, upper_bounds: &[TypeId]) -> bool {
        upper_bounds
            .iter()
            .any(|&bound| self.type_implies_literals(bound))
    }

    /// Check if a type contains literal types (directly, in unions/intersections,
    /// or in object properties). This is critical for discriminated union constraints
    /// like `{ kind: "a" } | { kind: "b" }` — the literal "a"/"b" in object
    /// properties must be detected so `preserve_literals` prevents widening.
    /// Check if a declared constraint is a primitive type (string/number/boolean/bigint)
    /// or a union containing one. These constraints permit literal type preservation.
    fn declared_constraint_is_primitive(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::STRING
            || type_id == TypeId::NUMBER
            || type_id == TypeId::BOOLEAN
            || type_id == TypeId::BIGINT
        {
            return true;
        }
        if type_id.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Union(list_id)) => {
                let members = self.interner.type_list(list_id);
                members
                    .iter()
                    .any(|&m| self.declared_constraint_is_primitive(m))
            }
            // `keyof T` constraints produce string literal unions at runtime,
            // so literals should be preserved (not widened to `string`).
            // This matches the behavior in constraint_is_primitive_type.
            Some(TypeData::KeyOf(_)) => true,
            // Intersections like `keyof T & string` — check if any member
            // implies literal preservation.
            Some(TypeData::Intersection(list_id)) => {
                let members = self.interner.type_list(list_id);
                members
                    .iter()
                    .any(|&m| self.declared_constraint_is_primitive(m))
            }
            _ => false,
        }
    }

    /// Check whether a declared constraint contains a `TypeParameter` whose own
    /// declared constraint is a primitive type (string/number/boolean/bigint/symbol).
    ///
    /// This handles `Object.freeze` overload 1:
    ///   `freeze<T extends { [idx: string]: U | null | undefined | object }, U extends string | bigint | number | boolean | symbol>(o: T): Readonly<T>`
    /// T's declared constraint is an index-signature object, not itself primitive,
    /// but *contains* U which IS primitive-constrained. When this holds, literal
    /// string values in the object's properties must not be widened during inference.
    ///
    /// It also handles variadic-tuple constraints such as
    ///   `arrayToEnum<T extends string, U extends [T, ...T[]]>(items: U): { [k in U[number]]: k }`
    /// where `U`'s declared constraint is the tuple `[T, ...T[]]` and `T` carries
    /// the primitive constraint. tsc preserves the literal element types inferred
    /// for `U`; without recursing through the tuple/array element types we widen
    /// them to `string`, which collapses `{ [k in U[number]]: k }` to a string
    /// index signature (the zod `ZodIssueCode` family).
    fn constraint_contains_type_param_with_primitive_constraint(
        &self,
        type_id: TypeId,
        depth: u32,
    ) -> bool {
        if depth > 4 {
            return false;
        }
        if type_id.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::TypeParameter(info)) => info
                .constraint
                .is_some_and(|c| self.declared_constraint_is_primitive(c)),
            Some(TypeData::Union(list_id)) => {
                let members = self.interner.type_list(list_id).to_vec();
                members.iter().any(|&m| {
                    self.constraint_contains_type_param_with_primitive_constraint(m, depth + 1)
                })
            }
            Some(TypeData::Intersection(list_id)) => {
                let members = self.interner.type_list(list_id).to_vec();
                members.iter().any(|&m| {
                    self.constraint_contains_type_param_with_primitive_constraint(m, depth + 1)
                })
            }
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                if shape.properties.iter().any(|prop| {
                    self.constraint_contains_type_param_with_primitive_constraint(
                        prop.type_id,
                        depth + 1,
                    )
                }) {
                    return true;
                }
                if let Some(idx) = shape.string_index.as_ref()
                    && self.constraint_contains_type_param_with_primitive_constraint(
                        idx.value_type,
                        depth + 1,
                    )
                {
                    return true;
                }
                if let Some(idx) = shape.number_index.as_ref()
                    && self.constraint_contains_type_param_with_primitive_constraint(
                        idx.value_type,
                        depth + 1,
                    )
                {
                    return true;
                }
                false
            }
            // A tuple constraint such as `U extends [T, ...T[]]` (where
            // `T extends string`) carries the primitive-constrained type
            // parameter in its element/rest types. tsc preserves literal
            // inferences for `U` against such a constraint (the tuple element
            // context implies literals), so recurse through the element types.
            Some(TypeData::Tuple(list_id)) => {
                let elements = self.interner.tuple_list(list_id).to_vec();
                elements.iter().any(|element| {
                    self.constraint_contains_type_param_with_primitive_constraint(
                        element.type_id,
                        depth + 1,
                    )
                })
            }
            // `...T[]` rest elements (and plain array constraints) wrap the
            // primitive-constrained parameter in an array; follow the element.
            Some(TypeData::Array(element)) => {
                self.constraint_contains_type_param_with_primitive_constraint(element, depth + 1)
            }
            _ => false,
        }
    }

    fn type_implies_literals(&self, type_id: TypeId) -> bool {
        // BOOLEAN_TRUE/FALSE are intrinsic IDs that resolve to Literal(Boolean).
        if type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE {
            return true;
        }
        if type_id.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Literal(_)) => true,
            Some(TypeData::Union(list_id)) => {
                let members = self.interner.type_list(list_id);
                members.iter().any(|&m| self.type_implies_literals(m))
            }
            Some(TypeData::Intersection(list_id)) => {
                let members = self.interner.type_list(list_id);
                members.iter().any(|&m| self.type_implies_literals(m))
            }
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_implies_literals(prop.type_id))
            }
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                app.args.iter().any(|&arg| self.type_implies_literals(arg))
            }
            Some(TypeData::Array(elem)) => self.type_implies_literals(elem),
            Some(TypeData::Tuple(list_id)) => {
                let elems = self.interner.tuple_list(list_id);
                elems
                    .iter()
                    .any(|elem| self.type_implies_literals(elem.type_id))
            }
            _ => false,
        }
    }

    /// Filter candidates by priority using `InferencePriority`.
    ///
    /// CRITICAL FIX: In the new enum, LOWER values = HIGHER priority (processed earlier).
    /// - `NakedTypeVariable` (1) is highest priority
    /// - `ReturnType` (32) is lower priority
    ///
    /// Therefore we use `.min()` instead of `.max()` to find the highest priority candidate.
    pub(crate) fn filter_candidates_by_priority(
        &self,
        candidates: &[InferenceCandidate],
    ) -> Vec<InferenceCandidate> {
        let Some(best_priority) = candidates.iter().map(|c| c.priority).min() else {
            return Vec::new();
        };
        candidates
            .iter()
            .filter(|candidate| candidate.priority == best_priority)
            .cloned()
            .collect()
    }

    /// Widen the resolved inference result, matching tsc's `getWidenedLiteralType`.
    ///
    /// Only widens a single literal type (e.g., `1` → `number`). Unions are NOT
    /// widened because:
    /// - If the union came from subtype reduction (e.g., `append(aa, 1)` where
    ///   `aa: Bit[]`), the result is `Bit = 0 | 1` which shouldn't be widened.
    /// - If the union was formed from multiple candidates (e.g., `g2(1, 2)` →
    ///   `1 | 2`), tsc also preserves the literal union.
    fn widen_resolved_inference(&self, type_id: TypeId) -> TypeId {
        // Use the full widen_literal_type which handles both bare literals
        // and unions of literals (e.g., "hello" | 42 → string | number).
        // This matches tsc's getWidenedLiteralType called from getInferredType.
        crate::operations::widening::widen_literal_type(self.interner, type_id)
    }

    /// Whether the covariant candidate list for `var` mixes distinct
    /// `InferencePriority` levels. When it does, priority filtering during
    /// resolution dropped some candidates (e.g. a source-function-return
    /// candidate recorded at `ReturnType` losing to a naked argument at
    /// `NakedTypeVariable`), so the resolved type may not correspond to the
    /// LEFTMOST candidate that tsc's `getCommonSupertype` `reduceLeft` keeps.
    pub fn has_mixed_priority_candidates(&mut self, var: InferenceVar) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        let mut priorities = info.candidates.iter().map(|candidate| candidate.priority);
        let Some(first) = priorities.next() else {
            return false;
        };
        priorities.any(|priority| priority != first)
    }

    /// Parent enum `DefId` for an enum member def, consulting the inference
    /// resolver first and falling back to the shared `DefinitionStore`. The
    /// generic-call inference context's resolver is the `QueryCache`, which
    /// has no binder symbol scope, so its `get_enum_parent_def_id` is always
    /// `None`; the store carries the same program-wide member -> parent edges
    /// the display layer reads.
    pub(super) fn enum_parent_def_for_inference(
        &self,
        def_id: crate::def::DefId,
    ) -> Option<crate::def::DefId> {
        if let Some(resolver) = self.resolver
            && let Some(parent) = resolver.get_enum_parent_def_id(def_id)
        {
            return Some(parent);
        }
        self.query_db?
            .definition_store_for_inference()?
            .get_enum_parent(def_id)
    }

    /// Base enum identity for an enum-branded inference candidate: a member's
    /// parent enum def, or a whole enum's own def. Returns `None` for
    /// non-enum candidates and for a member-shaped def with no parent edge
    /// (a type-position member ref stabilized as its own def is
    /// indistinguishable from a sibling member, so callers must keep the
    /// order-independent union fallback rather than risk splitting one
    /// enum's members).
    pub(super) fn enum_candidate_base_def(&self, ty: TypeId) -> Option<crate::def::DefId> {
        if ty.is_intrinsic() {
            return None;
        }
        let Some(TypeData::Enum(def_id, _)) = self.interner.lookup(ty) else {
            return None;
        };
        if let Some(parent) = self.enum_parent_def_for_inference(def_id) {
            return Some(parent);
        }
        let is_whole_enum_decl = self
            .query_db
            .and_then(|db| db.definition_store_for_inference())
            .and_then(|store| store.get(def_id))
            .is_some_and(|info| !info.enum_members.is_empty());
        tracing::trace!(
            ?def_id,
            has_query_db = self.query_db.is_some(),
            has_store = self
                .query_db
                .and_then(|db| db.definition_store_for_inference())
                .is_some(),
            is_whole_enum_decl,
            "enum_candidate_base_def: no parent edge"
        );
        is_whole_enum_decl.then_some(def_id)
    }

    /// Widen an enum member literal type to its parent enum type
    /// (`E1.X` -> `E1`), mirroring the enum arm of tsc's
    /// `getWidenedLiteralType`. Returns `None` when the type is not an enum
    /// member (including whole-enum types, which have no parent) or when the
    /// member -> parent link cannot be resolved.
    fn widen_enum_member_to_parent(&self, type_id: TypeId) -> Option<TypeId> {
        if type_id.is_intrinsic() {
            return None;
        }
        let Some(TypeData::Enum(def_id, _)) = self.interner.lookup(type_id) else {
            return None;
        };
        let parent_def = self.enum_parent_def_for_inference(def_id)?;
        if let Some(resolver) = self.resolver
            && let Some(parent_ty) = resolver.resolve_lazy(parent_def, self.interner)
        {
            return Some(parent_ty);
        }
        if let Some(body) = self
            .query_db
            .and_then(|db| db.definition_store_for_inference())
            .and_then(|store| store.get_body(parent_def))
        {
            return Some(body);
        }
        Some(self.interner.lazy(parent_def))
    }

    // =========================================================================
    // Conditional Type Inference
    // =========================================================================

    /// Infer type parameters from a conditional type.
    /// When a type parameter appears in a conditional type, we can sometimes
    /// infer its value from the check and extends clauses.
    // Dead in the lib build; entered only from `tests/infer_tests/context_overloads_solv16.rs`.
    // `allow` (not `expect`) is correct: the test build makes it live, which would leave an
    // `expect` unfulfilled.
    #[allow(dead_code)]
    pub fn infer_from_conditional(
        &mut self,
        var: InferenceVar,
        check_type: TypeId,
        extends_type: TypeId,
        true_type: TypeId,
        false_type: TypeId,
    ) {
        // If check_type is an inference variable, try to infer from extends_type
        if let Some(TypeData::TypeParameter(info)) = self.interner.lookup(check_type)
            && let Some(check_var) = self.find_type_param(info.name)
            && check_var == self.table.find(var)
        {
            // check_type is this variable
            // Try to infer from extends_type as an upper bound
            self.add_upper_bound(var, extends_type);
        }

        // Recursively infer from true/false branches
        self.infer_from_type(var, true_type);
        self.infer_from_type(var, false_type);
    }

    /// Infer type parameters from a type by traversing its structure.
    // Reachable only through `infer_from_conditional`, which is itself test-only; dead in the
    // lib build, so it carries its own `allow`.
    #[allow(dead_code)]
    fn infer_from_type(&mut self, var: InferenceVar, ty: TypeId) {
        let root = self.table.find(var);

        // Check if this type contains the inference variable
        if !self.contains_inference_var(ty, root) {
            return;
        }

        match self.interner.lookup(ty) {
            Some(TypeData::TypeParameter(info)) => {
                if let Some(param_var) = self.find_type_param(info.name)
                    && self.table.find(param_var) == root
                {
                    // This type is the inference variable itself
                    // Extract bounds from constraint if present
                    if let Some(constraint) = info.constraint {
                        self.add_upper_bound(var, constraint);
                    }
                }
            }
            Some(TypeData::Array(elem)) => {
                self.infer_from_type(var, elem);
            }
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                for elem in elements.iter() {
                    self.infer_from_type(var, elem.type_id);
                }
            }
            Some(TypeData::Union(members) | TypeData::Intersection(members)) => {
                let members = self.interner.type_list(members);
                for &member in members.iter() {
                    self.infer_from_type(var, member);
                }
            }
            Some(TypeData::Object(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    self.infer_from_type(var, prop.type_id);
                }
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    self.infer_from_type(var, prop.type_id);
                }
                if let Some(index) = shape.string_index.as_ref() {
                    self.infer_from_type(var, index.key_type);
                    self.infer_from_type(var, index.value_type);
                }
                if let Some(index) = shape.number_index.as_ref() {
                    self.infer_from_type(var, index.key_type);
                    self.infer_from_type(var, index.value_type);
                }
            }
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                self.infer_from_type(var, app.base);
                for &arg in &app.args {
                    self.infer_from_type(var, arg);
                }
            }
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                for param in &shape.params {
                    self.infer_from_type(var, param.type_id);
                }
                if let Some(this_type) = shape.this_type {
                    self.infer_from_type(var, this_type);
                }
                self.infer_from_type(var, shape.return_type);
            }
            Some(TypeData::Conditional(cond_id)) => {
                let cond = self.interner.get_conditional(cond_id);
                self.infer_from_conditional(
                    var,
                    cond.check_type,
                    cond.extends_type,
                    cond.true_type,
                    cond.false_type,
                );
            }
            Some(TypeData::TemplateLiteral(spans)) => {
                // Traverse template literal spans to find inference variables
                let spans = self.interner.template_list(spans);
                for span in spans.iter() {
                    if let TemplateSpan::Type(inner) = span {
                        self.infer_from_type(var, *inner);
                    }
                }
            }
            _ => {}
        }
    }

    /// Check if a type contains an inference variable.
    pub(crate) fn contains_inference_var(&mut self, ty: TypeId, var: InferenceVar) -> bool {
        let mut visited = FxHashSet::default();
        self.contains_inference_var_inner(ty, var, &mut visited, 0)
    }

    fn contains_inference_var_inner(
        &mut self,
        ty: TypeId,
        var: InferenceVar,
        visited: &mut FxHashSet<TypeId>,
        depth: usize,
    ) -> bool {
        // Safety limit to prevent infinite recursion on deeply nested or cyclic types
        if depth > MAX_TYPE_RECURSION_DEPTH {
            return false;
        }
        // Intrinsics are leaf types that never contain inference variables.
        if ty.is_intrinsic() {
            return false;
        }
        // Prevent infinite loops on cyclic types
        if !visited.insert(ty) {
            return false;
        }

        let root = self.table.find(var);

        match self.interner.lookup(ty) {
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                if let Some(param_var) = self.find_type_param(info.name) {
                    self.table.find(param_var) == root
                } else {
                    false
                }
            }
            Some(TypeData::Array(elem)) => {
                self.contains_inference_var_inner(elem, var, visited, depth + 1)
            }
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|e| self.contains_inference_var_inner(e.type_id, var, visited, depth + 1))
            }
            Some(TypeData::Union(members) | TypeData::Intersection(members)) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|&m| self.contains_inference_var_inner(m, var, visited, depth + 1))
            }
            Some(TypeData::Object(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|p| self.contains_inference_var_inner(p.type_id, var, visited, depth + 1))
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|p| self.contains_inference_var_inner(p.type_id, var, visited, depth + 1))
                    || shape.string_index.as_ref().is_some_and(|idx| {
                        self.contains_inference_var_inner(idx.key_type, var, visited, depth + 1)
                            || self.contains_inference_var_inner(
                                idx.value_type,
                                var,
                                visited,
                                depth + 1,
                            )
                    })
                    || shape.number_index.as_ref().is_some_and(|idx| {
                        self.contains_inference_var_inner(idx.key_type, var, visited, depth + 1)
                            || self.contains_inference_var_inner(
                                idx.value_type,
                                var,
                                visited,
                                depth + 1,
                            )
                    })
            }
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                self.contains_inference_var_inner(app.base, var, visited, depth + 1)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.contains_inference_var_inner(arg, var, visited, depth + 1))
            }
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                shape
                    .params
                    .iter()
                    .any(|p| self.contains_inference_var_inner(p.type_id, var, visited, depth + 1))
                    || shape.this_type.is_some_and(|t| {
                        self.contains_inference_var_inner(t, var, visited, depth + 1)
                    })
                    || self.contains_inference_var_inner(shape.return_type, var, visited, depth + 1)
            }
            Some(TypeData::Conditional(cond_id)) => {
                let cond = self.interner.get_conditional(cond_id);
                self.contains_inference_var_inner(cond.check_type, var, visited, depth + 1)
                    || self.contains_inference_var_inner(cond.extends_type, var, visited, depth + 1)
                    || self.contains_inference_var_inner(cond.true_type, var, visited, depth + 1)
                    || self.contains_inference_var_inner(cond.false_type, var, visited, depth + 1)
            }
            Some(TypeData::TemplateLiteral(spans)) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => {
                        self.contains_inference_var_inner(*inner, var, visited, depth + 1)
                    }
                })
            }
            _ => false,
        }
    }

    // =========================================================================
    // Enhanced Constraint Resolution
    // =========================================================================

    /// Try to infer a type parameter from its usage context.
    /// This implements bidirectional type inference where the context
    /// (e.g., return type, variable declaration) provides constraints.
    #[allow(dead_code)] // Reserved for bidirectional type inference
    pub fn infer_from_context(
        &mut self,
        var: InferenceVar,
        context_type: TypeId,
    ) -> Result<(), InferenceError> {
        // Add context as an upper bound
        self.add_upper_bound(var, context_type);

        // If the context type contains this inference variable,
        // we need to solve more carefully
        let root = self.table.find(var);
        if self.contains_inference_var(context_type, root) {
            // Context contains the inference variable itself
            // This is a recursive type - we need to handle it specially
            return Err(InferenceError::OccursCheck {
                var: root,
                ty: context_type,
            });
        }

        Ok(())
    }
}
