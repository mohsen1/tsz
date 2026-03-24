//! Inference resolution, variance analysis, and constraint strengthening.
//!
//! This module contains the resolution phase of type inference:
//! - Constraint-based resolution (upper/lower bounds)
//! - Candidate filtering and widening
//! - Variance analysis for type parameters
//! - Circular constraint unification (SCC/Tarjan)
//! - Constraint strengthening and propagation
//! - Variable fixing and substitution building

use crate::inference::infer::{
    InferenceCandidate, InferenceContext, InferenceError, InferenceInfo, InferenceVar,
    MAX_CONSTRAINT_ITERATIONS, MAX_TYPE_RECURSION_DEPTH,
};
use crate::instantiation::instantiate::TypeSubstitution;
use crate::operations::widening;
use crate::types::{InferencePriority, ObjectFlags, TemplateSpan, TypeData, TypeId};
use rustc_hash::FxHashSet;
use tsz_common::interner::Atom;

#[allow(dead_code)] // Reserved for variance analysis in inference resolution
struct VarianceState<'a> {
    target_param: Atom,
    covariant: &'a mut u32,
    contravariant: &'a mut u32,
}

impl<'a> InferenceContext<'a> {
    fn resolve_from_contra_candidates(
        &self,
        contra_candidates: &[crate::inference::infer::InferenceCandidate],
    ) -> TypeId {
        let mut contra_types: Vec<TypeId> = Vec::new();
        for candidate in contra_candidates {
            if !contra_types.contains(&candidate.type_id) {
                contra_types.push(candidate.type_id);
            }
        }

        // Filter out `any` when there are more specific candidates.
        // `any` in contravariant positions (e.g., from `boolean | any = any` in
        // interface method signatures) doesn't carry useful inference information
        // and should not override concrete candidates like `boolean`.
        // This matches tsc's behavior where `any` is treated as uninformative
        // during inference resolution.
        if contra_types.len() > 1 {
            let has_non_any = contra_types.iter().any(|&t| t != TypeId::ANY);
            if has_non_any {
                contra_types.retain(|&t| t != TypeId::ANY);
            }
        }

        if contra_types.len() <= 1 {
            return contra_types.first().copied().unwrap_or(TypeId::UNKNOWN);
        }

        // Filter out `any` from the tournament when there are non-any candidates.
        // In tsc, `any` inferences from structural decomposition (e.g., matching
        // a method with `(t: any)` against `(t: T | U)`) don't override more
        // specific inferences from other call signatures. Since `any` is a subtype
        // of everything, it always wins the tournament incorrectly.
        let non_any: Vec<TypeId> = contra_types
            .iter()
            .copied()
            .filter(|&t| t != TypeId::ANY)
            .collect();
        let effective_types = if non_any.is_empty() {
            &contra_types
        } else {
            &non_any
        };

        if effective_types.len() <= 1 {
            return effective_types.first().copied().unwrap_or(TypeId::UNKNOWN);
        }

        // Tournament: find the most-specific candidate in O(N).
        // The most-specific type must be a subtype of all others.
        // Walk the list, keeping the current "winner" — replace it whenever
        // a new candidate is a subtype of it.
        let mut winner = effective_types[0];
        for &candidate in &effective_types[1..] {
            if self.is_subtype(candidate, winner) {
                winner = candidate;
            }
        }

        // Verify the winner is actually a subtype of ALL candidates (O(N)).
        // If verification fails, there's no single most-specific type.
        let verified = effective_types
            .iter()
            .all(|&other| winner == other || self.is_subtype(winner, other));

        if verified {
            winner
        } else {
            self.interner.intersection(effective_types.to_vec())
        }
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

        let (root, result, upper_bounds, upper_bounds_only) =
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

        let (root, result, upper_bounds, upper_bounds_only) =
            self.compute_constraint_result(var, Some(&mut is_subtype));

        // Skip upper bound validation for `any` — it satisfies all constraints in tsc.
        if !upper_bounds_only && result != TypeId::ANY {
            let filtered_upper_bounds = Self::filter_relevant_upper_bounds(&upper_bounds);
            if let Some(upper) =
                self.first_failed_upper_bound(result, &filtered_upper_bounds, is_subtype)
            {
                return Err(InferenceError::BoundsViolation {
                    var,
                    lower: result,
                    upper,
                });
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
            .filter(|&upper| !matches!(upper, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR))
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

    fn compute_constraint_result<F>(
        &mut self,
        var: InferenceVar,
        mut external_is_subtype: Option<F>,
    ) -> (InferenceVar, TypeId, Vec<TypeId>, bool)
    where
        F: FnMut(TypeId, TypeId) -> bool,
    {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);
        let target_names = self.type_param_names_for_root(root);
        let mut upper_bounds = Vec::new();
        let mut seen_upper_bounds = FxHashSet::default();
        let mut candidates = info.candidates;
        let contra_candidates = info.contra_candidates;
        for bound in info.upper_bounds {
            if self.occurs_in(root, bound) {
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
                .any(|&upper| !matches!(upper, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR));
            // Check if there are concrete (non-top) candidates before filtering.
            // When `any` is the only meaningful candidate, keep it even with
            // informative upper bounds. This matches tsc where passing `any` to
            // `f<T extends X>(v: T)` infers T=any, not T=X.
            let has_concrete_candidate = candidates
                .iter()
                .any(|c| !matches!(c.type_id, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR));
            candidates.retain(|candidate| match candidate.type_id {
                TypeId::UNKNOWN | TypeId::ERROR => false,
                TypeId::ANY => !has_informative_upper_bound || !has_concrete_candidate,
                _ => true,
            });
        }

        // Check if this is a const type parameter to preserve literal types
        let is_const = self.is_var_const(root);

        // Filter out TypeParameter contra-candidates: these are typically
        // leaked original type parameters from contextual typing of overloaded
        // generic calls. They should not drive inference over concrete covariant
        // candidates. This matches the intent of has_concrete_contra_candidates.
        let concrete_contra_candidates: Vec<_> = contra_candidates
            .iter()
            .filter(|c| {
                !matches!(
                    self.interner.lookup(c.type_id),
                    Some(TypeData::TypeParameter(_))
                )
            })
            .cloned()
            .collect();

        let upper_bounds_only = candidates.is_empty()
            && concrete_contra_candidates.is_empty()
            && !upper_bounds.is_empty();

        let declared_constraint = self.declared_constraints.get(&root).copied();

        let result = if !candidates.is_empty() {
            // Covariant candidates exist: use union/BCT (matches tsc's getInferredType)
            let covariant_result = self.resolve_from_candidates(
                &candidates,
                is_const,
                &upper_bounds,
                declared_constraint,
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
                let covariant_is_uninformative = matches!(
                    covariant_result,
                    TypeId::NEVER | TypeId::UNKNOWN | TypeId::ANY
                );
                let covariant_assignable_to_contra = !covariant_is_uninformative
                    && concrete_contra_candidates.iter().any(|c| {
                        // Use the external (full checker) assignability test when
                        // available, falling back to the simplified BCT is_subtype.
                        // The BCT checker cannot resolve Lazy (interface/class) types
                        // through their extends chains in all cases, so the full
                        // checker is needed to correctly determine e.g. B <: A when
                        // B extends A.
                        if let Some(ref mut ext) = external_is_subtype {
                            ext(covariant_result, c.type_id)
                        } else {
                            self.is_subtype(covariant_result, c.type_id)
                        }
                    });
                if covariant_assignable_to_contra {
                    covariant_result
                } else {
                    self.resolve_from_contra_candidates(&concrete_contra_candidates)
                }
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

        (root, result, upper_bounds, upper_bounds_only)
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

    fn resolve_from_candidates(
        &self,
        candidates: &[InferenceCandidate],
        is_const: bool,
        upper_bounds: &[TypeId],
        declared_constraint: Option<TypeId>,
    ) -> TypeId {
        let filtered = self.filter_candidates_by_priority(candidates);
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
        let all_from_object_properties = filtered_no_never
            .iter()
            .all(|candidate| candidate.from_object_property);
        // TypeScript preserves literal types when:
        // 1. The type parameter is `const`, OR
        // 2. The declared constraint implies literals (e.g., T extends "a" | "b"), OR
        // 3. The declared constraint IS a primitive (e.g., T extends string/number/boolean/bigint)
        // Note: we use the declared constraint (from the `extends` clause), NOT upper_bounds
        // which also includes contextual type bounds. This prevents false preservation when
        // e.g., `<T>(value: T): Box<T>` is contextually typed as `Box<boolean>`.
        let preserve_literals = is_const
            || self.constraint_implies_literals(upper_bounds)
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
        let candidate_types: Vec<TypeId> = if is_const {
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
        // When candidates include nullish types (undefined/null) and no candidates
        // are from object properties, use union semantics. This matches tsc's
        // getCommonSupertype behavior for nullable inference patterns like
        // equal<T>(a: T, b: T) called as equal(B, undefined | D) → T = B | undefined | D.
        let has_nullish_candidates = candidate_types.iter().any(|&ty| {
            if ty == TypeId::UNDEFINED || ty == TypeId::NULL || ty == TypeId::VOID {
                return true;
            }
            // Also check inside union candidates: `undefined | D` contains undefined
            if let Some(TypeData::Union(members)) = self.interner.lookup(ty) {
                let member_list = self.interner.type_list(members);
                member_list
                    .iter()
                    .any(|&m| m == TypeId::UNDEFINED || m == TypeId::NULL || m == TypeId::VOID)
            } else {
                false
            }
        });
        let no_object_property_candidates = !filtered_no_never
            .iter()
            .any(|candidate| candidate.from_object_property);
        let nullable_union_inference = has_nullish_candidates && no_object_property_candidates;
        let resolved = if priority_implies_combination
            || all_from_index_signatures
            || nullable_union_inference
        {
            // Union: used for return type inference, low-priority contexts,
            // index signature inference, and nullable parameter inference
            self.best_common_type(&candidate_types)
        } else {
            // Common supertype: used for NakedTypeVariable and other direct inference.
            // tsc widens literal candidates BEFORE getCommonSupertype (via baseCandidates =
            // sameMap(candidates, getWidenedLiteralType)). This ensures the tournament
            // operates on widened types (number, string) not literals (3, "").
            let has_non_fresh = filtered_no_never.iter().any(|c| !c.is_fresh_literal);
            #[allow(clippy::redundant_clone)]
            let widened_candidates: Vec<TypeId> = if !preserve_literals
                && !is_const
                && !has_non_fresh
            {
                candidate_types
                    .iter()
                    .map(|&ty| crate::operations::widening::widen_literal_type(self.interner, ty))
                    .collect()
            } else {
                candidate_types.clone()
            };
            self.get_common_supertype_for_inference(
                &widened_candidates,
                // When all candidates are fresh literals that were widened, use
                // first-wins (leftmost) semantics matching tsc's getSingleCommonSupertype.
                // This is critical for cases like `new Map([["", true], ["", 0]])`
                // where V gets fresh candidates `true` and `0`, widened to `boolean`
                // and `number`. tsc resolves V=boolean (first wins), not boolean|number.
                !preserve_literals && !is_const && !has_non_fresh,
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
                // Create a union of all valid candidate types (subtype-reduced).
                self.best_common_type(&valid_candidates)
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
        let has_non_fresh = filtered_no_never.iter().any(|c| !c.is_fresh_literal);
        let resolved = if !preserve_literals && !is_const && !has_non_fresh {
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
        // Deep-widen: only for non-contextual inference (NakedTypeVariable,
        // HomomorphicMappedType, etc). Skip for ReturnType/LowPriority since
        // those come from contextual typing where literal types should be
        // preserved (tsc uses RequiresWidening flag for this; we approximate
        // by checking the inference priority).
        let highest_priority = filtered_no_never.first().map(|c| c.priority);
        let is_contextual_inference = matches!(
            highest_priority,
            Some(InferencePriority::ReturnType | InferencePriority::LowPriority)
        );
        let resolved = if !preserve_literals && !is_contextual_inference {
            match self.interner.lookup(resolved) {
                Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                    // Only deep-widen fresh object literals (from object literal
                    // expressions). Non-fresh objects (from type annotations/aliases)
                    // should preserve their literal property types, matching tsc's
                    // RequiresWidening check in getWidenedType().
                    let shape = self.interner.object_shape(shape_id);
                    if shape.flags.contains(ObjectFlags::FRESH_LITERAL) {
                        widening::widen_type_for_inference(self.interner, resolved)
                    } else {
                        resolved
                    }
                }
                // Arrays and tuples: widen element types unconditionally.
                // `[true]` inferred as `Array<true>` should widen to `Array<boolean>`.
                // Unlike objects, arrays don't have a freshness concept that should
                // prevent widening.
                Some(TypeData::Array(_) | TypeData::Tuple(_)) => {
                    widening::widen_type_for_inference(self.interner, resolved)
                }
                _ => resolved,
            }
        } else {
            resolved
        };
        if all_from_object_properties
            && !has_index_signature_candidates
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
    fn constraint_contains_type_param_with_primitive_constraint(
        &self,
        type_id: TypeId,
        depth: u32,
    ) -> bool {
        if depth > 4 {
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
            _ => false,
        }
    }

    fn type_implies_literals(&self, type_id: TypeId) -> bool {
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
    fn filter_candidates_by_priority(
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

    // =========================================================================
    // Conditional Type Inference
    // =========================================================================

    /// Infer type parameters from a conditional type.
    /// When a type parameter appears in a conditional type, we can sometimes
    /// infer its value from the check and extends clauses.
    #[allow(dead_code)] // Reserved for conditional type inference
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
    // Variance Inference
    // =========================================================================

    /// Compute the variance of a type parameter within a type.
    /// Returns (`covariant_count`, `contravariant_count`, `invariant_count`, `bivariant_count`)
    #[allow(dead_code)] // Reserved for variance analysis in inference
    pub fn compute_variance(&self, ty: TypeId, target_param: Atom) -> (u32, u32, u32, u32) {
        let mut covariant = 0u32;
        let mut contravariant = 0u32;
        let invariant = 0u32;
        let bivariant = 0u32;
        let mut state = VarianceState {
            target_param,
            covariant: &mut covariant,
            contravariant: &mut contravariant,
        };

        self.compute_variance_helper(ty, true, &mut state);

        (covariant, contravariant, invariant, bivariant)
    }

    fn compute_variance_helper(
        &self,
        ty: TypeId,
        polarity: bool, // true = covariant, false = contravariant
        state: &mut VarianceState<'_>,
    ) {
        match self.interner.lookup(ty) {
            Some(TypeData::TypeParameter(info)) if info.name == state.target_param => {
                if polarity {
                    *state.covariant += 1;
                } else {
                    *state.contravariant += 1;
                }
            }
            Some(TypeData::Array(elem)) => {
                self.compute_variance_helper(elem, polarity, state);
            }
            Some(TypeData::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                for elem in elements.iter() {
                    self.compute_variance_helper(elem.type_id, polarity, state);
                }
            }
            Some(TypeData::Union(members) | TypeData::Intersection(members)) => {
                let members = self.interner.type_list(members);
                for &member in members.iter() {
                    self.compute_variance_helper(member, polarity, state);
                }
            }
            Some(TypeData::Object(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    // Properties are covariant in their type (read position)
                    self.compute_variance_helper(prop.type_id, polarity, state);
                    // Properties are contravariant in their write type (write position)
                    if prop.write_type != prop.type_id && !prop.readonly {
                        self.compute_variance_helper(prop.write_type, !polarity, state);
                    }
                }
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    self.compute_variance_helper(prop.type_id, polarity, state);
                    if prop.write_type != prop.type_id && !prop.readonly {
                        self.compute_variance_helper(prop.write_type, !polarity, state);
                    }
                }
                if let Some(index) = shape.string_index.as_ref() {
                    self.compute_variance_helper(index.value_type, polarity, state);
                }
                if let Some(index) = shape.number_index.as_ref() {
                    self.compute_variance_helper(index.value_type, polarity, state);
                }
            }
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                // Variance depends on the generic type definition
                // For now, assume covariant for all type arguments
                for &arg in &app.args {
                    self.compute_variance_helper(arg, polarity, state);
                }
            }
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                // Parameters are contravariant
                for param in &shape.params {
                    self.compute_variance_helper(param.type_id, !polarity, state);
                }
                // Return type is covariant
                self.compute_variance_helper(shape.return_type, polarity, state);
            }
            Some(TypeData::Conditional(cond_id)) => {
                let cond = self.interner.get_conditional(cond_id);
                // Conditional types are invariant in their type parameters
                self.compute_variance_helper(cond.check_type, false, state);
                self.compute_variance_helper(cond.extends_type, false, state);
                // But can be either in the result
                self.compute_variance_helper(cond.true_type, polarity, state);
                self.compute_variance_helper(cond.false_type, polarity, state);
            }
            _ => {}
        }
    }

    /// Get the variance of a type parameter as a string.
    #[allow(dead_code)] // Reserved for variance analysis in inference
    pub fn get_variance(&self, ty: TypeId, target_param: Atom) -> &'static str {
        let (covariant, contravariant, invariant, bivariant) =
            self.compute_variance(ty, target_param);

        if invariant > 0 {
            "invariant"
        } else if bivariant > 0 {
            "bivariant"
        } else if covariant > 0 && contravariant > 0 {
            "invariant" // Both covariant and contravariant means invariant
        } else if covariant > 0 {
            "covariant"
        } else if contravariant > 0 {
            "contravariant"
        } else {
            "unused"
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

    /// Detect and unify type parameters that form circular constraints.
    /// For example, if T extends U and U extends T, they should be unified
    /// into a single equivalence class for inference purposes.
    fn unify_circular_constraints(&mut self) -> Result<(), InferenceError> {
        use rustc_hash::{FxHashMap, FxHashSet};

        let type_params: Vec<_> = self.type_params.clone();

        // Build adjacency list: var -> set of vars it extends (upper bounds)
        let mut graph: FxHashMap<InferenceVar, FxHashSet<InferenceVar>> = FxHashMap::default();
        let mut var_for_param: FxHashMap<Atom, InferenceVar> = FxHashMap::default();

        for (name, var, _) in &type_params {
            let root = self.table.find(*var);
            var_for_param.insert(*name, root);
            graph.entry(root).or_default();
        }

        // Populate edges based on upper_bounds
        for (_name, var, _) in &type_params {
            let root = self.table.find(*var);
            let info = self.table.probe_value(root);

            for &upper in &info.upper_bounds {
                // Only follow naked type parameter upper bounds (not List<T>, etc.)
                if let Some(TypeData::TypeParameter(param_info)) = self.interner.lookup(upper)
                    && let Some(&upper_var) = var_for_param.get(&param_info.name)
                {
                    let upper_root = self.table.find(upper_var);
                    // Add edge: root extends upper_root
                    graph.entry(root).or_default().insert(upper_root);
                }
            }
        }

        // Find SCCs using Tarjan's algorithm
        let mut index_counter = 0;
        let mut indices: FxHashMap<InferenceVar, usize> = FxHashMap::default();
        let mut lowlink: FxHashMap<InferenceVar, usize> = FxHashMap::default();
        let mut stack: Vec<InferenceVar> = Vec::new();
        let mut on_stack: FxHashSet<InferenceVar> = FxHashSet::default();
        let mut sccs: Vec<Vec<InferenceVar>> = Vec::new();

        struct TarjanState<'a> {
            graph: &'a FxHashMap<InferenceVar, FxHashSet<InferenceVar>>,
            index_counter: &'a mut usize,
            indices: &'a mut FxHashMap<InferenceVar, usize>,
            lowlink: &'a mut FxHashMap<InferenceVar, usize>,
            stack: &'a mut Vec<InferenceVar>,
            on_stack: &'a mut FxHashSet<InferenceVar>,
            sccs: &'a mut Vec<Vec<InferenceVar>>,
        }

        fn strongconnect(var: InferenceVar, state: &mut TarjanState) {
            state.indices.insert(var, *state.index_counter);
            state.lowlink.insert(var, *state.index_counter);
            *state.index_counter += 1;
            state.stack.push(var);
            state.on_stack.insert(var);

            if let Some(neighbors) = state.graph.get(&var) {
                for &neighbor in neighbors {
                    if !state.indices.contains_key(&neighbor) {
                        strongconnect(neighbor, state);
                        let neighbor_low = *state.lowlink.get(&neighbor).unwrap_or(&0);
                        let var_low = state
                            .lowlink
                            .get_mut(&var)
                            .expect("var was inserted into lowlink above");
                        *var_low = (*var_low).min(neighbor_low);
                    } else if state.on_stack.contains(&neighbor) {
                        let neighbor_idx = *state.indices.get(&neighbor).unwrap_or(&0);
                        let var_low = state
                            .lowlink
                            .get_mut(&var)
                            .expect("var was inserted into lowlink above");
                        *var_low = (*var_low).min(neighbor_idx);
                    }
                }
            }

            if *state.lowlink.get(&var).unwrap_or(&0) == *state.indices.get(&var).unwrap_or(&0) {
                let mut scc = Vec::new();
                loop {
                    let w = state
                        .stack
                        .pop()
                        .expect("Tarjan SCC invariant: stack non-empty while processing component");
                    state.on_stack.remove(&w);
                    scc.push(w);
                    if w == var {
                        break;
                    }
                }
                state.sccs.push(scc);
            }
        }

        // Run Tarjan's on all nodes
        for &var in graph.keys() {
            if !indices.contains_key(&var) {
                let mut state = TarjanState {
                    graph: &graph,
                    index_counter: &mut index_counter,
                    indices: &mut indices,
                    lowlink: &mut lowlink,
                    stack: &mut stack,
                    on_stack: &mut on_stack,
                    sccs: &mut sccs,
                };
                strongconnect(var, &mut state);
            }
        }

        // Unify variables within each SCC (if SCC has >1 member)
        for scc in sccs {
            if scc.len() > 1 {
                // Unify all variables in this SCC
                let first = scc[0];
                for &other in &scc[1..] {
                    self.unify_vars(first, other)?;
                }
            }
        }

        Ok(())
    }

    /// Strengthen constraints by analyzing relationships between type parameters.
    /// For example, if T <: U and we know T = string, then U must be at least string.
    pub fn strengthen_constraints(&mut self) -> Result<(), InferenceError> {
        // Detect and unify circular constraints (SCCs)
        // This ensures that type parameters in cycles (T extends U, U extends T)
        // are treated as a single equivalence class for inference.
        self.unify_circular_constraints()?;

        let type_params: Vec<_> = self.type_params.clone();
        let mut changed = true;
        let mut iterations = 0;

        // Fixed-point propagation
        // Iterate to fixed point - continue until no new candidates are added
        while changed && iterations < MAX_CONSTRAINT_ITERATIONS {
            changed = false;
            iterations += 1;

            for (name, var, _) in &type_params {
                let root = self.table.find(*var);

                // We need to clone info to avoid borrow checker issues while mutating
                // This is expensive but necessary for correctness in this design
                let info = self.table.probe_value(root).clone();

                // Propagate candidates UP the extends chain
                // If T extends U (T <: U), then candidates of T are also candidates of U
                for &upper in &info.upper_bounds {
                    if self.propagate_candidates_to_upper(root, upper, *name)? {
                        changed = true;
                    }
                }
            }
        }
        Ok(())
    }

    /// Propagates candidates from a subtype (var) to its supertype (upper).
    /// If `var extends upper` (var <: upper), then candidates of `var` are also candidates of `upper`.
    fn propagate_candidates_to_upper(
        &mut self,
        var_root: InferenceVar,
        upper: TypeId,
        exclude_param: Atom,
    ) -> Result<bool, InferenceError> {
        // Check if 'upper' is a type parameter we are inferring
        if let Some(TypeData::TypeParameter(info)) = self.interner.lookup(upper)
            && info.name != exclude_param
            && let Some(upper_var) = self.find_type_param(info.name)
        {
            let upper_root = self.table.find(upper_var);

            // Don't propagate to self
            if var_root == upper_root {
                return Ok(false);
            }

            // Get candidates from the subtype (var)
            let var_candidates = self.table.probe_value(var_root).candidates;

            // Add them to the supertype (upper)
            let mut changed = false;
            for candidate in var_candidates {
                // Use Circular priority to indicate this came from propagation
                if self.add_candidate_if_new(
                    upper_root,
                    candidate.type_id,
                    InferencePriority::Circular,
                ) {
                    changed = true;
                }
            }
            return Ok(changed);
        }
        Ok(false)
    }

    /// Helper to track if we actually added something (for fixed-point loop)
    fn add_candidate_if_new(
        &mut self,
        var: InferenceVar,
        ty: TypeId,
        priority: InferencePriority,
    ) -> bool {
        let root = self.table.find(var);
        let info = self.table.probe_value(root);

        // Check if type already exists in candidates
        if info.candidates.iter().any(|c| c.type_id == ty) {
            return false;
        }

        self.add_candidate(var, ty, priority);
        true
    }

    /// Fix (resolve) inference variables that have candidates from Round 1.
    ///
    /// This is called after processing non-contextual arguments to "fix" type
    /// variables that have enough information, before processing contextual
    /// arguments (like lambdas) in Round 2.
    ///
    /// The fixing process:
    /// 1. Finds variables with candidates but no resolved type yet
    /// 2. Computes their best current type from candidates
    /// 3. Sets the `resolved` field to prevent Round 2 from overriding
    ///
    /// Variables without candidates are NOT fixed (they might get info from Round 2).
    ///
    /// The optional `external_is_subtype` closure provides access to the full
    /// checker-level assignability check, which is needed for Lazy (interface/class)
    /// types that the simplified BCT checker cannot resolve through extends chains.
    pub fn fix_current_variables_with<F>(
        &mut self,
        mut external_is_subtype: Option<F>,
    ) -> Result<(), InferenceError>
    where
        F: FnMut(TypeId, TypeId) -> bool,
    {
        let type_params: Vec<_> = self.type_params.clone();

        for (_name, var, _is_const) in &type_params {
            let root = self.table.find(*var);
            let info = self.table.probe_value(root);

            // Skip if already resolved
            if info.resolved.is_some() {
                continue;
            }

            // Skip if no candidates yet (might get info from Round 2)
            if info.candidates.is_empty() && info.contra_candidates.is_empty() {
                continue;
            }

            // Compute the current best type from existing candidates.
            // Mirror the unknown/error candidate filtering from compute_constraint_result:
            // when informative upper bounds exist, discard unknown/error covariant candidates
            // so that contra-candidates (from contravariant positions like function params)
            // can drive inference instead.
            let is_const = self.is_var_const(root);
            let dc = self.declared_constraints.get(&root).copied();
            let mut candidates = info.candidates.clone();
            if !info.upper_bounds.is_empty() {
                let has_informative_upper_bound = info
                    .upper_bounds
                    .iter()
                    .any(|&upper| !matches!(upper, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR));
                let has_concrete_candidate = candidates
                    .iter()
                    .any(|c| !matches!(c.type_id, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR));
                candidates.retain(|candidate| match candidate.type_id {
                    TypeId::UNKNOWN | TypeId::ERROR => false,
                    TypeId::ANY => !has_informative_upper_bound || !has_concrete_candidate,
                    _ => true,
                });
            }
            // Filter out TypeParameter contra-candidates: these are typically
            // leaked original type parameters from contextual typing of overloaded
            // generic calls. They should not drive inference over concrete covariant
            // candidates. This matches the intent of has_concrete_contra_candidates.
            let concrete_contra_candidates: Vec<_> = info
                .contra_candidates
                .iter()
                .filter(|c| {
                    !matches!(
                        self.interner.lookup(c.type_id),
                        Some(TypeData::TypeParameter(_))
                    )
                })
                .cloned()
                .collect();
            let result = if !candidates.is_empty() {
                let covariant_result =
                    self.resolve_from_candidates(&candidates, is_const, &info.upper_bounds, dc);
                // (TypeParameter filtering already done above)
                if !concrete_contra_candidates.is_empty() {
                    // Match tsc's getInferredType: prefer covariant ONLY IF it's
                    // assignable to some contra-candidate. Otherwise use contra result.
                    let covariant_is_uninformative = matches!(
                        covariant_result,
                        TypeId::NEVER | TypeId::UNKNOWN | TypeId::ANY
                    );
                    let covariant_assignable_to_contra = !covariant_is_uninformative
                        && concrete_contra_candidates.iter().any(|c| {
                            if let Some(ref mut ext) = external_is_subtype {
                                ext(covariant_result, c.type_id)
                            } else {
                                self.is_subtype(covariant_result, c.type_id)
                            }
                        });
                    if covariant_assignable_to_contra {
                        covariant_result
                    } else {
                        self.resolve_from_contra_candidates(&concrete_contra_candidates)
                    }
                } else {
                    covariant_result
                }
            } else if !concrete_contra_candidates.is_empty() {
                self.resolve_from_contra_candidates(&concrete_contra_candidates)
            } else {
                // All covariant candidates were filtered; fall back to upper bounds
                if info.upper_bounds.len() == 1 {
                    info.upper_bounds[0]
                } else if !info.upper_bounds.is_empty() {
                    self.interner.intersection(info.upper_bounds.clone())
                } else {
                    TypeId::UNKNOWN
                }
            };

            // Check for occurs (recursive type)
            if self.occurs_in(root, result) {
                // Don't fix variables with occurs - let them be resolved later
                continue;
            }

            // Fix this variable by setting resolved field
            // This prevents Round 2 from overriding with lower-priority constraints
            self.table.union_value(
                root,
                InferenceInfo {
                    resolved: Some(result),
                    // Keep candidates and upper_bounds for later validation
                    candidates: info.candidates,
                    contra_candidates: info.contra_candidates,
                    upper_bounds: info.upper_bounds,
                },
            );
        }

        Ok(())
    }

    /// Fix (resolve) inference variables without an external checker.
    /// Convenience wrapper for `fix_current_variables_with(None)`.
    pub fn fix_current_variables(&mut self) -> Result<(), InferenceError> {
        self.fix_current_variables_with(None::<fn(TypeId, TypeId) -> bool>)
    }

    /// Get the current best substitution for all type parameters.
    ///
    /// This returns a `TypeSubstitution` mapping each type parameter to its
    /// current best type (either resolved or the best candidate so far).
    /// Used in Round 2 to provide contextual types to lambda arguments.
    pub fn get_current_substitution(&mut self) -> TypeSubstitution {
        let mut subst = TypeSubstitution::new();
        let type_params: Vec<_> = self.type_params.clone();

        for (name, var, _) in &type_params {
            let ty = match self.probe(*var) {
                Some(resolved) => {
                    tracing::trace!(
                        ?name,
                        ?var,
                        ?resolved,
                        "get_current_substitution: already resolved"
                    );
                    resolved
                }
                None => {
                    // Not resolved yet, try to get best candidate
                    let root = self.table.find(*var);
                    let info = self.table.probe_value(root);
                    tracing::trace!(
                        ?name, ?var,
                        candidates_count = info.candidates.len(),
                        contra_candidates_count = info.contra_candidates.len(),
                        upper_bounds_count = info.upper_bounds.len(),
                        upper_bounds = ?info.upper_bounds,
                        "get_current_substitution: not resolved"
                    );

                    if !info.candidates.is_empty() {
                        let is_const = self.is_var_const(root);
                        let dc = self.declared_constraints.get(&root).copied();
                        let covariant_result = self.resolve_from_candidates(
                            &info.candidates,
                            is_const,
                            &info.upper_bounds,
                            dc,
                        );
                        if !info.contra_candidates.is_empty() {
                            let covariant_is_uninformative = matches!(
                                covariant_result,
                                TypeId::NEVER | TypeId::UNKNOWN | TypeId::ANY
                            );
                            let covariant_ok = !covariant_is_uninformative
                                && info
                                    .contra_candidates
                                    .iter()
                                    .any(|c| self.is_subtype(covariant_result, c.type_id));
                            if covariant_ok {
                                covariant_result
                            } else {
                                self.resolve_from_contra_candidates(&info.contra_candidates)
                            }
                        } else {
                            covariant_result
                        }
                    } else if !info.contra_candidates.is_empty() {
                        self.resolve_from_contra_candidates(&info.contra_candidates)
                    } else if !info.upper_bounds.is_empty() {
                        // No candidates yet, but we have a constraint (upper bound).
                        // Use the constraint as contextual fallback so that mapped types
                        // like `{ [K in keyof P]: P[K] }` resolve using the constraint
                        // type. This matches tsc's behavior for contextual typing of
                        // generic call arguments when all arguments are context-sensitive.
                        if info.upper_bounds.len() == 1 {
                            info.upper_bounds[0]
                        } else {
                            self.interner.intersection(info.upper_bounds.to_vec())
                        }
                    } else {
                        // No info yet, use unknown as placeholder
                        TypeId::UNKNOWN
                    }
                }
            };

            subst.insert(*name, ty);
        }

        subst
    }
}
