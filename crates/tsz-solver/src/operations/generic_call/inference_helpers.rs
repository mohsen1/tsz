//! Inference helper methods for generic call resolution.

use crate::inference::infer::{InferenceContext, InferenceVar};
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{
    FunctionShape, ObjectFlags, ParamInfo, TypeData, TypeId, TypeParamInfo, TypePredicate,
};
use rustc_hash::{FxHashMap, FxHashSet};

/// A lower-bound inference candidate is "concrete evidence" when it carries a
/// real type the relation can judge — not `any`/`unknown`/error, and free of
/// unresolved type parameters or `infer` placeholders. Shared by return-position
/// candidate selection and the contextual-return substitution evidence guard.
pub(super) fn is_concrete_inference_bound(
    db: &dyn crate::construction::TypeDatabase,
    ty: TypeId,
) -> bool {
    !ty.is_any_unknown_or_error()
        && !crate::visitor::contains_type_parameters(db, ty)
        && !crate::type_queries::contains_infer_types_db(db, ty)
}

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub(super) fn eval_type_param_default(
        &mut self,
        default: TypeId,
        subst: &TypeSubstitution,
        actual_this_type: Option<TypeId>,
    ) -> TypeId {
        let instantiated =
            super::instantiate_call_type(self.interner, default, subst, actual_this_type);
        self.checker.evaluate_type(instantiated)
    }

    pub(super) fn resolve_direct_parameter_inference_type(
        &mut self,
        lower_bounds: &[TypeId],
        inferred: TypeId,
        has_usable_contra_candidates: bool,
        has_array_element_candidates: bool,
        leftmost_dropped_by_priority: bool,
    ) -> TypeId {
        if lower_bounds.len() <= 1 {
            return inferred;
        }

        let concrete_lower_bounds: Vec<TypeId> = lower_bounds
            .iter()
            .copied()
            .filter(|ty| !ty.is_any_unknown_or_error())
            .collect();

        if let Some(preferred_tuple_candidate) =
            self.preferred_specific_tuple_inference_candidate(lower_bounds)
        {
            return preferred_tuple_candidate;
        }

        let inferred_is_union = matches!(self.interner.lookup(inferred), Some(TypeData::Union(_)));

        let all_mergeable = lower_bounds
            .iter()
            .all(|ty| self.is_mergeable_direct_inference_candidate(*ty));

        // Direct arguments should stay narrow when there are heterogeneous candidates.
        // Otherwise TypeScript-style checks can get masked by a broad union result.
        if all_mergeable {
            // Preserve tsc's nullable-envelope inference for direct rest parameters.
            // `foo<T>(...s: T[])` called as `foo(false, undefined, null, "x")`
            // infers `T = boolean | null | undefined`; the later string should
            // fail against that type rather than forcing T back to the first
            // boolean candidate and reporting the earlier `undefined` mismatch.
            if self.should_preserve_nullable_direct_inference_result(lower_bounds, inferred) {
                return crate::operations::widening::widen_literal_type(self.interner, inferred);
            }
            // Guard: if lower bounds contain literals with different primitive bases
            // (e.g., "" and 3 → string vs number), fall back to the first candidate.
            // tsc keeps the first candidate in those cases so later argument checks
            // can report a proper TS2345 mismatch.
            let has_concrete_literal_conflict =
                self.has_conflicting_literal_bases(&concrete_lower_bounds);
            // tsc keeps the LEFTMOST candidate on a base conflict
            // (`getCommonSupertype`'s `reduceLeft`) and widens it
            // (`getWidenedLiteralType`): `f1({ r: () => E1.X }, E2.X)` fixes
            // `T = E1` and reports TS2345 on the `E2.X` argument, and
            // `f1({ r: () => 0 }, "s")` fixes `T = number` likewise. tsz's
            // priority-filtered `inferred` can instead carry a LATER
            // argument's candidate (a source-function-return candidate is
            // recorded at `ReturnType` priority while a naked argument is
            // `NakedTypeVariable`), which inverts the reported mismatch onto
            // the first argument. Re-anchor on the first concrete bound's
            // widened base when the priority winner carries a different base.
            // Gated on `leftmost_dropped_by_priority`: with a same-priority
            // candidate list the resolver's own combination (BCT tournament,
            // the all-object-property first-property fallback) already
            // reproduces tsc's pick, and the raw constraint-set bound order
            // is not tsc's candidate order there.
            if has_concrete_literal_conflict
                && leftmost_dropped_by_priority
                && let Some(&first) = concrete_lower_bounds.first()
                && let Some(first_base) = self.primitive_base_of(first)
                && self.primitive_base_of(inferred) != Some(first_base)
            {
                return first_base;
            }
            if !has_concrete_literal_conflict {
                // If direct inference collapsed to a single non-union candidate while
                // we also have contravariant evidence, preserve the combined direct
                // argument information. This prevents over-narrowing from first-wins in
                // co/contra scenarios such as callback predicates over union arrays.
                //
                // `concrete_lower_bounds` (computed above) already drops the
                // ANY/UNKNOWN/ERROR bounds, which are not meaningful inference
                // evidence — they usually leak in from an unresolved callback
                // parameter and would widen every concrete candidate back to ANY,
                // silencing downstream diagnostics like TS2488/TS2769.
                if has_usable_contra_candidates && !inferred_is_union {
                    // A widened direct argument, e.g. `[1, 2, 3]` -> `number`,
                    // should continue to own T. Folding a conflicting callback
                    // return into the direct lower-bound union would mask the
                    // later callback-return assignability error.
                    if has_array_element_candidates
                        && !inferred.is_any_unknown_or_error()
                        && concrete_lower_bounds
                            .iter()
                            .any(|&bound| self.checker.is_assignable_to(bound, inferred))
                    {
                        return inferred;
                    }
                    if !concrete_lower_bounds.is_empty() {
                        let union =
                            crate::utils::union_or_single(self.interner, concrete_lower_bounds);
                        // A widened direct argument keeps ownership of the type
                        // parameter: `inferred` is the resolved best-common supertype
                        // with tsc's literal widening applied (a fresh literal `0` seed
                        // for `init: U` resolves to `number`). When the raw bounds union
                        // to a type strictly narrower than `inferred` (un-widened `0` vs
                        // `number`), returning it would reintroduce the literal and the
                        // callback's widened body no longer matches — a false TS2769 on
                        // `arr.reduce((acc, x) => acc + x.f, 0)`. Genuinely wider or
                        // heterogeneous unions are not assignable to `inferred`, so they
                        // still flow through unchanged.
                        if union != inferred
                            && !inferred.is_any_unknown_or_error()
                            && self.checker.is_assignable_to(union, inferred)
                            && !self.checker.is_assignable_to(inferred, union)
                        {
                            return inferred;
                        }
                        return union;
                    }
                }
                return inferred;
            }
        }

        if !inferred_is_union {
            return inferred;
        }

        if lower_bounds
            .iter()
            .any(|ty| matches!(*ty, TypeId::ANY | TypeId::ERROR))
        {
            return TypeId::ANY;
        }

        // Fall back to the first lower-bound candidate so later argument checks
        // drive assignability failures on the mismatch site.
        lower_bounds
            .iter()
            .copied()
            .find(|ty| !ty.is_any_unknown_or_error())
            .unwrap_or(lower_bounds[0])
    }

    pub(super) fn should_prefer_single_contra_candidate_for_direct_inference(
        &mut self,
        lower_bounds: &[TypeId],
        inferred: TypeId,
        contra: TypeId,
    ) -> bool {
        if lower_bounds.len() <= 1 {
            return false;
        }

        if !matches!(self.interner.lookup(inferred), Some(TypeData::Union(_))) {
            return false;
        }

        let mut saw_fresh_literal_candidate = false;
        let mut saw_concrete_lower_bound = false;

        for &bound in lower_bounds {
            if matches!(bound, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR) {
                continue;
            }

            saw_concrete_lower_bound = true;

            if self.checker.is_assignable_to(bound, contra) {
                continue;
            }

            if self.is_fresh_direct_object_or_array_literal_candidate(bound) {
                saw_fresh_literal_candidate = true;
                continue;
            }

            return false;
        }

        saw_concrete_lower_bound && saw_fresh_literal_candidate
    }

    pub(super) fn select_single_contra_candidate_direct_inference_type(
        &mut self,
        lower_bounds: &[TypeId],
        contra: TypeId,
    ) -> TypeId {
        lower_bounds
            .iter()
            .copied()
            .find(|bound| {
                !matches!(*bound, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
                    && !self.is_fresh_direct_object_or_array_literal_candidate(*bound)
                    && self.checker.is_assignable_to(*bound, contra)
            })
            .unwrap_or(contra)
    }

    fn is_fresh_direct_object_or_array_literal_candidate(&self, ty: TypeId) -> bool {
        if ty.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(ty) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => self
                .interner
                .object_shape(shape_id)
                .flags
                .contains(ObjectFlags::FRESH_LITERAL),
            Some(TypeData::Tuple(_)) => true,
            _ => false,
        }
    }

    fn preferred_specific_tuple_inference_candidate(
        &self,
        lower_bounds: &[TypeId],
    ) -> Option<TypeId> {
        if lower_bounds.len() <= 1
            || !lower_bounds.iter().all(|&ty| {
                crate::type_queries::get_tuple_elements(self.interner.as_type_database(), ty)
                    .is_some()
            })
        {
            return None;
        }

        let mut specific_iter = lower_bounds
            .iter()
            .copied()
            .filter(|&ty| !self.tuple_contains_any_or_unknown(ty));

        if let Some(first) = specific_iter.next()
            && specific_iter.next().is_none()
        {
            // Exactly one specific bound
            return Some(self.sanitize_tuple_inference_candidate(first));
        }

        None
    }

    fn tuple_contains_any_or_unknown(&self, ty: TypeId) -> bool {
        crate::visitor::collect_all_types(self.interner.as_type_database(), ty)
            .into_iter()
            .any(TypeId::is_any_or_unknown)
    }

    fn sanitize_tuple_inference_candidate(&self, ty: TypeId) -> TypeId {
        let mut substitution = TypeSubstitution::new();
        for nested in crate::visitor::collect_all_types(self.interner.as_type_database(), ty) {
            let Some(TypeData::TypeParameter(info)) = self.interner.lookup(nested) else {
                continue;
            };
            let replacement = info.constraint.or(info.default).unwrap_or(TypeId::UNKNOWN);
            substitution.insert(info.name, replacement);
        }

        if substitution.is_empty() {
            ty
        } else {
            instantiate_type(self.interner, ty, &substitution)
        }
    }

    pub(super) fn resolve_return_position_inference_type(
        &self,
        lower_bounds: &[TypeId],
        inferred: TypeId,
    ) -> TypeId {
        let pruned_bounds = self.prune_wrapped_return_type_param_candidates(lower_bounds);
        if pruned_bounds.len() != lower_bounds.len()
            && let Some(candidate) = self.single_bare_return_type_param_candidate(&pruned_bounds)
        {
            return candidate;
        }
        let effective_lower_bounds = if pruned_bounds.is_empty() {
            lower_bounds
        } else {
            pruned_bounds.as_slice()
        };

        let mut concrete_bounds = effective_lower_bounds
            .iter()
            .copied()
            .filter(|ty| is_concrete_inference_bound(self.interner.as_type_database(), *ty))
            .collect::<Vec<_>>();
        concrete_bounds.dedup();
        // When the lone surviving "concrete" bound is `never` *and* the
        // candidate set also contained an `unknown`/`any` candidate (so
        // BCT chose that wider type as the result), promoting `never`
        // back into the inferred return type contradicts BCT and forces
        // a downstream argument check (e.g. a generic identity callback
        // whose return type still references the type variable) to
        // reject a perfectly valid argument. Skip the promotion in that
        // mixed case so the BCT result (`unknown`/`any`) stands.
        //
        // Single-never lower-bound sets (e.g. `T = never` from an
        // unconstrained `f1<T>([])` call) are intentionally left intact:
        // those legitimately mean "no information beyond never" and tsc
        // also infers `never` there.
        //
        // Conformance: `subtypeRelationForNever.ts`.
        let drop_never_promotion = concrete_bounds.len() == 1
            && concrete_bounds[0] == TypeId::NEVER
            && effective_lower_bounds
                .iter()
                .any(|&b| matches!(b, TypeId::ANY | TypeId::UNKNOWN));
        if !drop_never_promotion
            && concrete_bounds.len() == 1
            && (crate::type_queries::contains_infer_types_db(
                self.interner.as_type_database(),
                inferred,
            ) || matches!(inferred, TypeId::ANY | TypeId::UNKNOWN))
        {
            return concrete_bounds[0];
        }

        if effective_lower_bounds.len() <= 1 {
            return inferred;
        }

        let inferred_union_members = match self.interner.lookup(inferred) {
            Some(TypeData::Union(member_list_id)) => self.interner.type_list(member_list_id),
            _ => return inferred,
        };
        if inferred_union_members.len() <= 1 {
            return inferred;
        }

        let all_structural = effective_lower_bounds
            .iter()
            .all(|ty| self.is_structural_return_inference_candidate(*ty));
        if all_structural {
            return effective_lower_bounds[0];
        }

        inferred
    }

    fn prune_wrapped_return_type_param_candidates(&self, lower_bounds: &[TypeId]) -> Vec<TypeId> {
        let Some(duplicated_name) = self.duplicated_bare_return_type_param_name(lower_bounds)
        else {
            return lower_bounds.to_vec();
        };

        lower_bounds
            .iter()
            .copied()
            .filter(|&bound| {
                self.bare_return_type_param_name(bound) == Some(duplicated_name)
                    || !self.is_structural_return_inference_candidate(bound)
            })
            .collect()
    }

    fn duplicated_bare_return_type_param_name(
        &self,
        lower_bounds: &[TypeId],
    ) -> Option<tsz_common::Atom> {
        let mut seen = FxHashSet::default();
        let mut duplicated = None;
        for &bound in lower_bounds {
            let Some(name) = self.bare_return_type_param_name(bound) else {
                continue;
            };
            if !seen.insert(name) {
                if duplicated.is_some_and(|existing| existing != name) {
                    return None;
                }
                duplicated = Some(name);
            }
        }
        duplicated
    }

    fn bare_return_type_param_name(&self, ty: TypeId) -> Option<tsz_common::Atom> {
        match self.interner.lookup(ty) {
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => Some(info.name),
            _ => None,
        }
    }

    fn single_bare_return_type_param_candidate(&self, lower_bounds: &[TypeId]) -> Option<TypeId> {
        let mut first = None;
        let mut first_name = None;
        for &bound in lower_bounds {
            let name = self.bare_return_type_param_name(bound)?;
            if let Some(existing) = first_name {
                if existing != name {
                    return None;
                }
            } else {
                first_name = Some(name);
                first = Some(bound);
            }
        }
        first
    }

    pub(super) fn constrain_return_context_structure(
        &mut self,
        infer_ctx: &mut InferenceContext<'_>,
        var_map: &FxHashMap<TypeId, InferenceVar>,
        source_ty: TypeId,
        target_ty: TypeId,
        priority: crate::types::InferencePriority,
    ) -> bool {
        let mut constrained_structurally = false;
        let raw_apps = match (
            self.interner.lookup(source_ty),
            self.interner.lookup(target_ty),
        ) {
            (Some(TypeData::Application(s_app_id)), Some(TypeData::Application(t_app_id))) => {
                Some((s_app_id, t_app_id))
            }
            _ => None,
        };
        let evaluated_source_ty = self.interner.evaluate_type(source_ty);
        let evaluated_target_ty = self.interner.evaluate_type(target_ty);
        let evaluated_apps = match (
            self.interner.lookup(evaluated_source_ty),
            self.interner.lookup(evaluated_target_ty),
        ) {
            (Some(TypeData::Application(s_app_id)), Some(TypeData::Application(t_app_id))) => {
                Some((s_app_id, t_app_id))
            }
            _ => None,
        };
        if let Some((s_app_id, t_app_id)) = raw_apps.or(evaluated_apps) {
            let s_app = self.interner.type_application(s_app_id);
            let t_app = self.interner.type_application(t_app_id);
            if s_app.base == t_app.base
                && s_app.args.len() == t_app.args.len()
                && self.should_directly_constrain_same_base_application(source_ty, target_ty)
            {
                constrained_structurally = true;
                for (s_arg, t_arg) in s_app.args.iter().zip(t_app.args.iter()) {
                    self.constrain_types(infer_ctx, var_map, *s_arg, *t_arg, priority);
                }
            }
        }

        // Contextual (target) type is a type-alias application whose body is a
        // union — e.g. a declared return type
        // `ParseReturnType<T> = Sync<T> | Async<T>`. tsc relates the
        // placeholder-bearing signature return against the *reduced apparent
        // type* of the contextual type, so an arm that shares the source's
        // generic base (here the `Promise<...>` arm) must constrain the
        // source's type arguments. `expand_type_alias_application` substitutes
        // an alias body one level without deep-reducing its members, keeping
        // that arm an Application so its type argument (which carries the
        // tracked return placeholder) stays visible; a deep `evaluate_type`
        // would collapse the arm to a bare object shape and lose the argument.
        // Arms may themselves be alias wrappers of the source's generic (e.g.
        // `AsyncParseReturnType<T> = Promise<Sync<T>>`), so expand each arm a
        // bounded number of levels until its base matches the source. Without
        // this the tracked return placeholder (`TResult1`) never receives a
        // candidate and collapses to `never`, spuriously rejecting a
        // non-thenable callback body against `PromiseLike<never>`.
        if !constrained_structurally
            && let Some(TypeData::Application(s_app_id)) = self.interner.lookup(source_ty)
            && matches!(
                self.interner.lookup(target_ty),
                Some(TypeData::Application(_))
            )
            && let Some(expanded_target) = self.checker.expand_type_alias_application(target_ty)
            && let Some(arms) = crate::type_queries::get_union_members(
                self.interner.as_type_database(),
                expanded_target,
            )
        {
            let s_app = self.interner.type_application(s_app_id);
            let s_base = s_app.base;
            let s_args: Vec<TypeId> = s_app.args.to_vec();
            for arm in arms.iter() {
                if let Some(arm_args) = self.same_base_application_args(*arm, s_base, s_args.len())
                {
                    constrained_structurally = true;
                    for (s_arg, arm_arg) in s_args.iter().zip(arm_args.iter()) {
                        // Covariant return position: the contextual arm's type
                        // argument is a candidate (lower bound) for the return
                        // placeholder, so drive it as the source and the
                        // placeholder-bearing return arg as the target. A union of
                        // return placeholders (`TResult1 | TResult2`) is decomposed
                        // on the target side, seeding each with the arm argument.
                        self.constrain_types(infer_ctx, var_map, *arm_arg, *s_arg, priority);
                    }
                }
            }
        }

        let raw_functions = Self::get_source_signature_for_target(
            self.interner.as_type_database(),
            source_ty,
            target_ty,
        );
        let evaluated_functions = Self::get_source_signature_for_target(
            self.interner.as_type_database(),
            evaluated_source_ty,
            evaluated_target_ty,
        );
        if let Some((mut source_fn, target_fn)) = raw_functions.or(evaluated_functions)
            && source_fn.params.len() == target_fn.params.len()
        {
            if !source_fn.type_params.is_empty() {
                let target_param_types: Vec<_> =
                    target_fn.params.iter().map(|p| p.type_id).collect();
                // Skip pinning the generic source against a target parameter that
                // still carries an outer inference placeholder (the higher-order
                // shared-middle shape `X_src[]`). Instantiating the source param
                // against such a wrapper re-runs a nested inference that drops the
                // foreign placeholder to `unknown`, seeding a poisoned
                // `(unknown[])[]` candidate that competes with the placeholder-
                // preserving one produced by the main generic-source constraint
                // walk. With a placeholder-bearing target the structural-return
                // constraint adds no information the generic-source branch has not
                // already collected, so leave the source generic and fall through.
                let target_pins_source = !target_param_types.iter().any(|&pt| {
                    crate::type_queries::contains_infer_types_db(
                        self.interner.as_type_database(),
                        pt,
                    )
                });
                if target_pins_source {
                    source_fn = self.instantiate_function_shape_from_argument_types(
                        &source_fn,
                        &target_param_types,
                    );
                }
            }
            constrained_structurally = true;
            if !self.constrain_return_context_params_with_rest(
                infer_ctx,
                var_map,
                &source_fn.params,
                &target_fn.params,
                priority,
            ) {
                for (source_param, target_param) in
                    source_fn.params.iter().zip(target_fn.params.iter())
                {
                    // Function parameters are contravariant in assignability, so the
                    // contextual target parameter constrains the returned function's
                    // source parameter.
                    let nested_structural = self.constrain_return_context_structure(
                        infer_ctx,
                        var_map,
                        target_param.type_id,
                        source_param.type_id,
                        priority,
                    );
                    if !nested_structural {
                        self.constrain_types(
                            infer_ctx,
                            var_map,
                            target_param.type_id,
                            source_param.type_id,
                            priority,
                        );
                    }
                }
            }
            let nested_structural = self.constrain_return_context_structure(
                infer_ctx,
                var_map,
                source_fn.return_type,
                target_fn.return_type,
                priority,
            );
            if !nested_structural {
                self.constrain_types(
                    infer_ctx,
                    var_map,
                    source_fn.return_type,
                    target_fn.return_type,
                    priority,
                );
            }

            self.propagate_contextual_return_upper_bounds(
                infer_ctx,
                var_map,
                source_fn.return_type,
                target_fn.return_type,
            );
        }

        constrained_structurally
    }

    /// Resolve `ty` to the type arguments of an application whose base is
    /// `base` and whose arity is `arity`, expanding wrapping type aliases a
    /// bounded number of levels. Used by the contextual-return union matcher to
    /// recognise a union arm that reuses the source's generic base directly
    /// (`Promise<Sync<T>>`) or through an alias (`Async<T> = Promise<Sync<T>>`).
    fn same_base_application_args(
        &mut self,
        ty: TypeId,
        base: TypeId,
        arity: usize,
    ) -> Option<Vec<TypeId>> {
        const MAX_ARM_ALIAS_EXPANSIONS: usize = 4;
        let mut candidate = ty;
        for _ in 0..MAX_ARM_ALIAS_EXPANSIONS {
            let app = match self.interner.lookup(candidate) {
                Some(TypeData::Application(app_id)) => {
                    let app = self.interner.type_application(app_id);
                    Some((app.base, app.args.to_vec()))
                }
                _ => None,
            };
            if let Some((cand_base, cand_args)) = app
                && cand_base == base
                && cand_args.len() == arity
            {
                return Some(cand_args);
            }
            match self.checker.expand_type_alias_application(candidate) {
                Some(next) if next != candidate => candidate = next,
                _ => return None,
            }
        }
        None
    }

    pub(super) fn collect_placeholder_vars_in_type(
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
        for nested in crate::visitor::collect_all_types(self.interner.as_type_database(), ty) {
            if let Some(&var) = var_map.get(&nested) {
                result.insert(var);
            }
        }
        let evaluated_ty = self.interner.evaluate_type(ty);
        if evaluated_ty != ty {
            for nested in
                crate::visitor::collect_all_types(self.interner.as_type_database(), evaluated_ty)
            {
                if let Some(&var) = var_map.get(&nested) {
                    result.insert(var);
                }
            }
        }
        if result.is_empty() {
            for (&placeholder_id, &var) in var_map.iter() {
                probe_map.clear();
                probe_map.insert(placeholder_id, var);
                visited.clear();
                if self.type_contains_placeholder(ty, probe_map, visited) {
                    result.insert(var);
                }
            }
        }

        result
    }

    pub(super) fn collect_noinfer_placeholder_vars_in_type(
        &mut self,
        ty: TypeId,
        var_map: &FxHashMap<TypeId, InferenceVar>,
        result: &mut FxHashSet<InferenceVar>,
        probe_map: &mut FxHashMap<TypeId, InferenceVar>,
        visited: &mut FxHashSet<TypeId>,
    ) {
        if !visited.insert(ty) {
            return;
        }

        let mut roots = vec![ty];
        if let Some(expanded) = self.checker.expand_type_alias_application(ty)
            && expanded != ty
            && visited.insert(expanded)
        {
            roots.push(expanded);
        }

        for root in roots {
            for nested in crate::visitor::collect_all_types(self.interner.as_type_database(), root)
            {
                if let Some(TypeData::NoInfer(inner)) = self.interner.lookup(nested) {
                    let mut inner_visited = FxHashSet::default();
                    result.extend(self.collect_placeholder_vars_in_type(
                        inner,
                        var_map,
                        probe_map,
                        &mut inner_visited,
                    ));
                }
            }
        }
    }

    pub(super) fn direct_inference_tracking_target(&self, ty: TypeId) -> Option<TypeId> {
        match self.interner.lookup(ty) {
            Some(TypeData::Union(members)) => {
                let member_list = self.interner.type_list(members);
                let mut non_nullish = member_list
                    .iter()
                    .copied()
                    .filter(|member| !member.is_nullable());
                let member = non_nullish.next()?;
                if non_nullish.next().is_none() {
                    self.direct_inference_tracking_target(member)
                } else {
                    None
                }
            }
            Some(TypeData::Intersection(_)) => None,
            _ => Some(ty),
        }
    }

    pub(super) fn collect_direct_placeholder_vars_in_type(
        &self,
        ty: TypeId,
        var_map: &FxHashMap<TypeId, InferenceVar>,
        visited: &mut FxHashSet<TypeId>,
    ) -> FxHashSet<InferenceVar> {
        let mut result = FxHashSet::default();
        self.collect_direct_placeholder_vars_in_type_inner(ty, var_map, visited, &mut result);
        result
    }

    fn collect_direct_placeholder_vars_in_type_inner(
        &self,
        ty: TypeId,
        var_map: &FxHashMap<TypeId, InferenceVar>,
        visited: &mut FxHashSet<TypeId>,
        result: &mut FxHashSet<InferenceVar>,
    ) {
        if ty.is_intrinsic() || !visited.insert(ty) {
            return;
        }
        if let Some(&var) = var_map.get(&ty) {
            result.insert(var);
            return;
        }

        let Some(key) = self.interner.lookup(ty) else {
            return;
        };
        match key {
            TypeData::ReadonlyType(inner)
            | TypeData::NoInfer(inner)
            | TypeData::Array(inner)
            | TypeData::KeyOf(inner) => {
                self.collect_direct_placeholder_vars_in_type_inner(inner, var_map, visited, result);
            }
            TypeData::Substitution {
                base_type,
                constraint,
            } => {
                self.collect_direct_placeholder_vars_in_type_inner(
                    base_type, var_map, visited, result,
                );
                self.collect_direct_placeholder_vars_in_type_inner(
                    constraint, var_map, visited, result,
                );
            }
            TypeData::Tuple(elements_id) => {
                let elements = self.interner.tuple_list(elements_id);
                for element in elements.iter().filter(|element| !element.rest) {
                    self.collect_direct_placeholder_vars_in_type_inner(
                        element.type_id,
                        var_map,
                        visited,
                        result,
                    );
                }
            }
            TypeData::Union(members_id) | TypeData::Intersection(members_id) => {
                for &member in self.interner.type_list(members_id).iter() {
                    self.collect_direct_placeholder_vars_in_type_inner(
                        member, var_map, visited, result,
                    );
                }
            }
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    self.collect_direct_placeholder_vars_in_type_inner(
                        prop.type_id,
                        var_map,
                        visited,
                        result,
                    );
                }
                if let Some(index) = shape.string_index.as_ref() {
                    self.collect_direct_placeholder_vars_in_type_inner(
                        index.value_type,
                        var_map,
                        visited,
                        result,
                    );
                }
                if let Some(index) = shape.number_index.as_ref() {
                    self.collect_direct_placeholder_vars_in_type_inner(
                        index.value_type,
                        var_map,
                        visited,
                        result,
                    );
                }
            }
            TypeData::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.collect_direct_placeholder_vars_in_type_inner(
                    app.base, var_map, visited, result,
                );
                for &arg in &app.args {
                    self.collect_direct_placeholder_vars_in_type_inner(
                        arg, var_map, visited, result,
                    );
                }
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.get_mapped(mapped_id);
                self.collect_direct_placeholder_vars_in_type_inner(
                    mapped.constraint,
                    var_map,
                    visited,
                    result,
                );
                if let Some(name_type) = mapped.name_type {
                    self.collect_direct_placeholder_vars_in_type_inner(
                        name_type, var_map, visited, result,
                    );
                }
                self.collect_direct_placeholder_vars_in_type_inner(
                    mapped.template,
                    var_map,
                    visited,
                    result,
                );
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.get_conditional(cond_id);
                for nested in [
                    cond.check_type,
                    cond.extends_type,
                    cond.true_type,
                    cond.false_type,
                ] {
                    self.collect_direct_placeholder_vars_in_type_inner(
                        nested, var_map, visited, result,
                    );
                }
            }
            TypeData::IndexAccess(object, index) => {
                self.collect_direct_placeholder_vars_in_type_inner(
                    object, var_map, visited, result,
                );
                self.collect_direct_placeholder_vars_in_type_inner(index, var_map, visited, result);
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                for param in &shape.params {
                    self.collect_direct_placeholder_vars_in_type_inner(
                        param.type_id,
                        var_map,
                        visited,
                        result,
                    );
                }
                if let Some(this_type) = shape.this_type {
                    self.collect_direct_placeholder_vars_in_type_inner(
                        this_type, var_map, visited, result,
                    );
                }
                self.collect_direct_placeholder_vars_in_type_inner(
                    shape.return_type,
                    var_map,
                    visited,
                    result,
                );
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                for sig in shape
                    .call_signatures
                    .iter()
                    .chain(shape.construct_signatures.iter())
                {
                    for param in &sig.params {
                        self.collect_direct_placeholder_vars_in_type_inner(
                            param.type_id,
                            var_map,
                            visited,
                            result,
                        );
                    }
                    if let Some(this_type) = sig.this_type {
                        self.collect_direct_placeholder_vars_in_type_inner(
                            this_type, var_map, visited, result,
                        );
                    }
                    self.collect_direct_placeholder_vars_in_type_inner(
                        sig.return_type,
                        var_map,
                        visited,
                        result,
                    );
                }
                for prop in &shape.properties {
                    self.collect_direct_placeholder_vars_in_type_inner(
                        prop.type_id,
                        var_map,
                        visited,
                        result,
                    );
                }
            }
            TypeData::StringIntrinsic { type_arg, .. } => {
                self.collect_direct_placeholder_vars_in_type_inner(
                    type_arg, var_map, visited, result,
                );
            }
            TypeData::TemplateLiteral(spans_id) => {
                for span in self.interner.template_list(spans_id).iter() {
                    if let crate::types::TemplateSpan::Type(nested) = span {
                        self.collect_direct_placeholder_vars_in_type_inner(
                            *nested, var_map, visited, result,
                        );
                    }
                }
            }
            TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::UnresolvedTypeName(_)
            | TypeData::Enum(_, _)
            | TypeData::Error => {}
        }
    }

    pub(super) fn function_like_placeholder_appears_in_parameter_position(
        &self,
        ty: TypeId,
        var_map: &FxHashMap<TypeId, InferenceVar>,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        let params_contain_placeholder = |params: &[ParamInfo], visited: &mut FxHashSet<TypeId>| {
            params.iter().any(|param| {
                visited.clear();
                self.type_contains_placeholder(param.type_id, var_map, visited)
            })
        };

        match self.interner.lookup(ty) {
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                params_contain_placeholder(&shape.params, visited)
            }
            Some(TypeData::Callable(shape_id)) => {
                let shape = self.interner.callable_shape(shape_id);
                shape
                    .call_signatures
                    .iter()
                    .any(|sig| params_contain_placeholder(&sig.params, visited))
                    || shape
                        .construct_signatures
                        .iter()
                        .any(|sig| params_contain_placeholder(&sig.params, visited))
            }
            Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => self
                .interner
                .type_list(list_id)
                .iter()
                .copied()
                .any(|member| {
                    self.function_like_placeholder_appears_in_parameter_position(
                        member, var_map, visited,
                    )
                }),
            Some(
                TypeData::Application(_)
                | TypeData::Lazy(_)
                | TypeData::Mapped(_)
                | TypeData::Conditional(_)
                | TypeData::IndexAccess(_, _),
            ) => {
                let evaluated = self.interner.evaluate_type(ty);
                evaluated != ty
                    && self.function_like_placeholder_appears_in_parameter_position(
                        evaluated, var_map, visited,
                    )
            }
            _ => false,
        }
    }

    pub(super) fn function_like_type_param_appears_in_parameter_position(
        &self,
        ty: TypeId,
        tracked_type_params: &[TypeParamInfo],
    ) -> bool {
        let params_contain_tracked_type_param = |params: &[ParamInfo]| {
            params.iter().any(|param| {
                crate::visitor::collect_all_types(self.interner.as_type_database(), param.type_id)
                    .into_iter()
                    .any(|candidate| {
                        crate::type_param_info(self.interner.as_type_database(), candidate)
                            .is_some_and(|info| {
                                tracked_type_params
                                    .iter()
                                    .any(|type_param| type_param.is_same_binder(info))
                            })
                    })
            })
        };

        match self.interner.lookup(ty) {
            Some(TypeData::Function(shape_id)) => {
                let shape = self.interner.function_shape(shape_id);
                params_contain_tracked_type_param(&shape.params)
            }
            Some(TypeData::Callable(shape_id)) => {
                let shape = self.interner.callable_shape(shape_id);
                shape
                    .call_signatures
                    .iter()
                    .any(|sig| params_contain_tracked_type_param(&sig.params))
                    || shape
                        .construct_signatures
                        .iter()
                        .any(|sig| params_contain_tracked_type_param(&sig.params))
            }
            Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => self
                .interner
                .type_list(list_id)
                .iter()
                .copied()
                .any(|member| {
                    self.function_like_type_param_appears_in_parameter_position(
                        member,
                        tracked_type_params,
                    )
                }),
            Some(
                TypeData::Application(_)
                | TypeData::Lazy(_)
                | TypeData::Mapped(_)
                | TypeData::Conditional(_)
                | TypeData::IndexAccess(_, _),
            ) => {
                let evaluated = self.interner.evaluate_type(ty);
                evaluated != ty
                    && self.function_like_type_param_appears_in_parameter_position(
                        evaluated,
                        tracked_type_params,
                    )
            }
            _ => false,
        }
    }

    pub(super) fn later_generic_function_like_arg_depends_on_type_param(
        &self,
        func: &FunctionShape,
        arg_types: &[TypeId],
        start_index: usize,
        type_param: TypeParamInfo,
    ) -> bool {
        let tracked_type_params = [type_param];

        func.params
            .iter()
            .enumerate()
            .skip(start_index + 1)
            .any(|(index, param)| {
                let Some(&arg_type) = arg_types.get(index) else {
                    return false;
                };

                let arg_is_generic_function_like = match self.interner.lookup(arg_type) {
                    Some(TypeData::Function(shape_id)) => !self
                        .interner
                        .function_shape(shape_id)
                        .type_params
                        .is_empty(),
                    Some(TypeData::Callable(shape_id)) => {
                        let shape = self.interner.callable_shape(shape_id);
                        shape
                            .call_signatures
                            .iter()
                            .any(|sig| !sig.type_params.is_empty())
                            || shape
                                .construct_signatures
                                .iter()
                                .any(|sig| !sig.type_params.is_empty())
                    }
                    _ => false,
                };

                // Only defer the current arg when the later generic function arg
                // is contextually sensitive (e.g., a lambda with untyped params).
                // Non-contextually-sensitive generic function references (like
                // `identity`) don't benefit from deferral — they get instantiated
                // in Round 1 via instantiate_generic_function_argument_against_target.
                // Deferring the current arg in that case prevents its type from
                // being inferred, causing T to resolve to `unknown`.
                arg_is_generic_function_like
                    && self.is_contextually_sensitive(arg_type)
                    && self.function_like_type_param_appears_in_parameter_position(
                        param.type_id,
                        &tracked_type_params,
                    )
            })
    }

    fn should_skip_contextual_arg_in_round1(&self, arg_type: TypeId) -> bool {
        if !self.is_contextually_sensitive(arg_type) {
            return false;
        }

        match self.interner.lookup(arg_type) {
            Some(TypeData::Object(shape_id)) | Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                if shape.all_properties_context_sensitive() {
                    return true;
                }
                !shape
                    .properties
                    .iter()
                    .any(|prop| !self.is_contextually_sensitive(prop.type_id))
            }
            _ => true,
        }
    }

    fn partial_round1_object_pair(
        &mut self,
        source_ty: TypeId,
        target_ty: TypeId,
    ) -> Option<(TypeId, TypeId)> {
        let source_ty = self.checker.evaluate_type(source_ty);
        let target_ty = self.checker.evaluate_type(target_ty);

        let (Some(source_obj), Some(target_obj)) =
            (
                match self.interner.lookup(source_ty) {
                    Some(TypeData::Object(shape_id))
                    | Some(TypeData::ObjectWithIndex(shape_id)) => Some(shape_id),
                    _ => None,
                },
                match self.interner.lookup(target_ty) {
                    Some(TypeData::Object(shape_id))
                    | Some(TypeData::ObjectWithIndex(shape_id)) => Some(shape_id),
                    _ => None,
                },
            )
        else {
            return None;
        };

        let source_shape = self.interner.object_shape(source_obj);
        let target_shape = self.interner.object_shape(target_obj);
        if source_shape.all_properties_context_sensitive() {
            return None;
        }

        let mut target_props_by_name: FxHashMap<_, _> = FxHashMap::default();
        for prop in &target_shape.properties {
            target_props_by_name.insert(prop.name, prop);
        }

        let mut source_properties = Vec::new();
        let mut target_properties = Vec::new();
        for prop in &source_shape.properties {
            if self.is_contextually_sensitive(prop.type_id) {
                continue;
            }

            if let Some(target_prop) = target_props_by_name.get(&prop.name) {
                source_properties.push(prop.clone());
                target_properties.push((**target_prop).clone());
            }
        }

        if source_properties.is_empty() {
            return None;
        }

        if source_properties.len() == source_shape.properties.len()
            && target_properties.len() == target_shape.properties.len()
        {
            return Some((source_ty, target_ty));
        }

        let mut source_shape = (*source_shape).clone();
        source_shape.properties = source_properties;

        let mut target_shape = (*target_shape).clone();
        target_shape.properties = target_properties;

        Some((
            self.interner.object_with_index(source_shape),
            self.interner.object_with_index(target_shape),
        ))
    }

    pub(super) fn contextual_round1_arg_types(
        &mut self,
        arg_type: TypeId,
        target_type: TypeId,
    ) -> Option<(TypeId, TypeId)> {
        if let (Some(mut source_fn), Some(mut target_fn)) = (
            Self::get_contextual_signature_cached(self.interner, arg_type),
            Self::get_contextual_signature_cached(self.interner, target_type),
        ) && source_fn.params.len() == target_fn.params.len()
            && let Some((source_return, target_return)) =
                self.partial_round1_object_pair(source_fn.return_type, target_fn.return_type)
        {
            source_fn.return_type = source_return;
            target_fn.return_type = target_return;
            return Some((
                self.interner.function(source_fn),
                self.interner.function(target_fn),
            ));
        }

        // Generic function references (e.g., `<E>(ma: Either<E, number>) => boolean`)
        // with fully-annotated parameters must be erased before inference. Without
        // this, constrain_types creates fresh inference variables for the source
        // function's type params that can cross-contaminate the outer call's inference
        // context. Erasing the source's type params to their constraints (or `unknown`)
        // matches tsc's getErasedSignature behavior during inference.
        //
        // This check must run BEFORE the is_contextually_sensitive early return
        // because generic functions with fully-typed params are NOT contextually
        // sensitive (tsc's isContextSensitive is AST-level), so the early return
        // would pass them through un-erased.
        if let Some(TypeData::Function(shape_id)) = self.interner.lookup(arg_type) {
            let shape = self.interner.function_shape(shape_id);
            if !shape.type_params.is_empty()
                && !self.function_signature_is_contextually_sensitive(&shape.params)
            {
                let instantiated = self
                    .instantiate_generic_function_argument_against_target(arg_type, target_type);
                if instantiated != arg_type {
                    return Some((instantiated, target_type));
                }
            }
        }

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

    pub(super) fn constrain_sensitive_function_return_types(
        &mut self,
        infer_ctx: &mut InferenceContext<'_>,
        var_map: &FxHashMap<TypeId, InferenceVar>,
        source_ty: TypeId,
        target_ty: TypeId,
        priority: crate::types::InferencePriority,
    ) -> bool {
        let raw_functions = Self::get_source_signature_for_target(
            self.interner.as_type_database(),
            source_ty,
            target_ty,
        );
        let evaluated_source_ty = self.interner.evaluate_type(source_ty);
        let evaluated_target_ty = self.interner.evaluate_type(target_ty);
        let evaluated_functions = Self::get_source_signature_for_target(
            self.interner.as_type_database(),
            evaluated_source_ty,
            evaluated_target_ty,
        );

        let Some((mut source_fn, target_fn)) = raw_functions.or(evaluated_functions) else {
            return false;
        };

        if !source_fn.type_params.is_empty() && source_fn.params.len() == target_fn.params.len() {
            let target_param_types: Vec<_> = target_fn.params.iter().map(|p| p.type_id).collect();
            source_fn = self
                .instantiate_function_shape_from_argument_types(&source_fn, &target_param_types);
        }

        if self.is_contextually_sensitive(source_fn.return_type) {
            return false;
        }

        let nested_structural = self.constrain_return_context_structure(
            infer_ctx,
            var_map,
            source_fn.return_type,
            target_fn.return_type,
            priority,
        );
        if !nested_structural {
            self.constrain_types(
                infer_ctx,
                var_map,
                source_fn.return_type,
                target_fn.return_type,
                priority,
            );
        }

        // A type-predicate argument (`(raw: any) => raw is bigint`) whose `any`
        // parameter makes it contextually sensitive is routed here instead of
        // through `infer_signatures`. Its predicate target carries the same
        // inference information as a return type — a `raw is I` parameter must
        // learn `I = bigint` — so mirror the predicate branch of
        // `infer_signatures`. Without this the type parameter the predicate
        // pins stays unresolved (`unknown`) during Round-1 contextual typing, so
        // a sibling context-sensitive callback whose parameter references it is
        // typed against `unknown` (M12: superjson `makeCodec`).
        if let (Some(source_pred), Some(target_pred)) =
            (&source_fn.type_predicate, &target_fn.type_predicate)
        {
            let targets_match = match (source_pred.parameter_index, target_pred.parameter_index) {
                (Some(s_idx), Some(t_idx)) => s_idx == t_idx,
                _ => source_pred.target == target_pred.target,
            };
            if targets_match
                && source_pred.asserts == target_pred.asserts
                && let (Some(source_ty), Some(target_ty)) =
                    (source_pred.type_id, target_pred.type_id)
            {
                self.constrain_types(infer_ctx, var_map, source_ty, target_ty, priority);
            }
        }
        true
    }

    fn instantiate_function_shape_from_argument_types(
        &mut self,
        func: &FunctionShape,
        arg_types: &[TypeId],
    ) -> FunctionShape {
        let substitution = self.compute_contextual_types(func, arg_types);
        FunctionShape {
            params: func
                .params
                .iter()
                .map(|param| ParamInfo {
                    name: param.name,
                    type_id: instantiate_type(self.interner, param.type_id, &substitution),
                    optional: param.optional,
                    rest: param.rest,
                })
                .collect(),
            return_type: instantiate_type(self.interner, func.return_type, &substitution),
            this_type: func
                .this_type
                .map(|this_type| instantiate_type(self.interner, this_type, &substitution)),
            type_params: vec![],
            type_predicate: func.type_predicate.as_ref().map(|predicate| TypePredicate {
                asserts: predicate.asserts,
                target: predicate.target,
                type_id: predicate
                    .type_id
                    .map(|tid| instantiate_type(self.interner, tid, &substitution)),
                parameter_index: predicate.parameter_index,
            }),
            is_constructor: func.is_constructor,
            is_method: func.is_method,
        }
    }

    pub(crate) fn instantiate_generic_function_argument_against_target(
        &mut self,
        source_ty: TypeId,
        target_ty: TypeId,
    ) -> TypeId {
        // Class constructor Callable types (e.g., `Promise`) must not be
        // decomposed into a Function type, because that loses static members and
        // the construct-signature wrapper. However, ordinary declared generic
        // functions and generic constructor callbacks represented as Callable
        // types do need contextual instantiation against the target callback
        // signature. Distinguish those cases by checking for a single generic
        // call or construct signature.
        if let Some(TypeData::Callable(shape_id)) = self.interner.lookup(source_ty) {
            let shape = self.interner.callable_shape(shape_id);
            let has_generic_call_sig = shape
                .call_signatures
                .iter()
                .any(|sig| !sig.type_params.is_empty());
            let has_generic_construct_sig = shape.call_signatures.is_empty()
                && shape.construct_signatures.len() == 1
                && !shape.construct_signatures[0].type_params.is_empty();
            let has_overloaded_call_sigs = shape.call_signatures.len() > 1;
            if !has_generic_call_sig && !has_generic_construct_sig && !has_overloaded_call_sigs {
                return source_ty;
            }
            // When the Callable has construct signatures AND properties (static
            // members), it represents a class constructor type (e.g., `typeof
            // MyClass`). Decomposing it into a Function type would lose static
            // members and the construct signature, causing false TS2345/TS2769
            // errors when the class is passed as an argument to a `typeof MyClass`
            // parameter in generic overload resolution.
            if !shape.construct_signatures.is_empty() && !shape.properties.is_empty() {
                return source_ty;
            }
        }
        let evaluated_source_ty = self.interner.evaluate_type(source_ty);
        let evaluated_target_ty = self.interner.evaluate_type(target_ty);
        let function_info = Self::get_source_signature_for_target(
            self.interner.as_type_database(),
            source_ty,
            target_ty,
        )
        .or_else(|| {
            Self::get_source_signature_for_target(
                self.interner.as_type_database(),
                evaluated_source_ty,
                evaluated_target_ty,
            )
        })
        .or_else(|| {
            // When the target is an Application with a Lazy base (interface-defined
            // callback like `Callback<T, R>`), the solver's evaluate_type can't resolve
            // the Lazy DefId. Use the checker's evaluate_type which has access to the
            // type environment for DefId → Callable resolution.
            let checker_target = self.checker.evaluate_type(target_ty);
            if checker_target != target_ty && checker_target != evaluated_target_ty {
                Self::get_source_signature_for_target(
                    self.interner.as_type_database(),
                    source_ty,
                    checker_target,
                )
            } else {
                None
            }
        });

        let Some((source_fn, target_fn)) = function_info else {
            return source_ty;
        };
        let source_fn = self.normalize_function_shape_params_for_context(&source_fn);
        let target_fn = self.normalize_function_shape_params_for_context(&target_fn);
        if !source_fn.type_params.is_empty() && !target_fn.type_params.is_empty() {
            return source_ty;
        }

        // Keep generic callbacks intact for `(...args: I)` targets so the
        // constraint walker can infer `I` as a tuple from the source parameters.
        // Contextual instantiation would ask for an element type of unresolved
        // `I` and collapse to its array constraint.
        let target_rest_is_outer_inference_placeholder =
            target_fn.params.last().is_some_and(|param| {
                if !param.rest {
                    return false;
                }
                matches!(
                    self.interner.lookup(param.type_id),
                    Some(TypeData::TypeParameter(info)) if info.is_infer_placeholder()
                )
            });
        if !source_fn.type_params.is_empty() && target_rest_is_outer_inference_placeholder {
            return source_ty;
        }

        if source_fn.type_params.is_empty() {
            let source_has_calls = crate::type_queries::get_call_signatures(
                self.interner.as_type_database(),
                source_ty,
            )
            .is_some_and(|sigs| !sigs.is_empty());
            let source_has_constructs = crate::type_queries::get_construct_signatures(
                self.interner.as_type_database(),
                source_ty,
            )
            .is_some_and(|sigs| !sigs.is_empty());
            let target_has_calls = crate::type_queries::get_call_signatures(
                self.interner.as_type_database(),
                target_ty,
            )
            .is_some_and(|sigs| !sigs.is_empty());
            let target_has_constructs = crate::type_queries::get_construct_signatures(
                self.interner.as_type_database(),
                target_ty,
            )
            .is_some_and(|sigs| !sigs.is_empty());
            if !source_has_calls
                && source_has_constructs
                && !target_has_calls
                && target_has_constructs
            {
                return source_ty;
            }
            return self.interner.function(source_fn);
        }

        let mut target_param_types = Vec::with_capacity(source_fn.params.len());
        for index in 0..source_fn.params.len() {
            let Some(param_type) =
                self.param_type_for_arg_index(&target_fn.params, index, source_fn.params.len())
            else {
                return source_ty;
            };
            target_param_types.push(param_type);
        }

        if target_param_types.is_empty() {
            return source_ty;
        }
        if target_param_types.iter().any(|&param_type| {
            Self::contains_tuple_like_parameter_target(self.interner.as_type_database(), param_type)
        }) {
            return source_ty;
        }

        let source_type_params_fully_determined_by_params =
            source_fn.type_params.iter().all(|tp| {
                source_fn.params.iter().any(|param| {
                    crate::visitor::collect_referenced_types(
                        self.interner.as_type_database(),
                        param.type_id,
                    )
                    .into_iter()
                    .any(|ty| {
                        crate::type_param_info(self.interner.as_type_database(), ty)
                            .is_some_and(|info| tp.is_same_binder(info))
                    })
                })
            });

        // Handle generic function arguments when target params are inference
        // placeholders from an outer generic call. Three cases:
        //
        // 1. Naked type params (e.g., `list<T>(a: T)`): Skip erasure, let
        //    instantiation proceed. The params match 1:1 against target placeholders.
        //
        // 2. Non-naked type params (e.g., `unbox<W>(x: Box<W>)`) WITH a generic
        //    contextual type: Return source_ty unchanged so `constrain_types_impl`'s
        //    generic function branch creates fresh inference variables in the shared
        //    context, enabling proper higher-order inference (e.g., compose(unbox, unlist)).
        //
        // 3. Non-naked type params WITHOUT a generic contextual type: Erase source
        //    type params to constraints/unknown (old behavior). Without a generic
        //    contextual type, the fresh inference variables would leak unresolved.
        let any_target_param_is_type_param = target_param_types.iter().any(|&param_type| {
            matches!(
                self.interner.lookup(param_type),
                Some(TypeData::TypeParameter(_))
            )
        });
        let any_target_param_contains_infer_placeholder =
            target_param_types.iter().any(|&param_type| {
                crate::type_queries::contains_infer_types_db(
                    self.interner.as_type_database(),
                    param_type,
                )
            });
        let target_params_need_hofi =
            any_target_param_is_type_param || any_target_param_contains_infer_placeholder;

        // Conflicting-candidate substitution applies only when target params
        // are concrete (post-inference) types. When *any* target param is
        // still an inference placeholder from an outer generic call (e.g.,
        // `apply<A,B,C>(fn: (a: A, b: B) => C, ...)` invoking `g<T>(x:T,y:T)`),
        // the existing Case 1/2/3 placeholder-aware logic below is the
        // correct path: two distinct unconstrained TypeParameters mapped to
        // the same source param look "conflicting" by `is_assignable_to`,
        // which would short-circuit erasure and produce a partially-
        // instantiated function.
        if !target_params_need_hofi
            && let Some(substitution) =
                self.conflicting_contextual_param_candidate_substitution(&source_fn, &target_fn)
        {
            return self.interner.function(FunctionShape {
                params: source_fn
                    .params
                    .iter()
                    .map(|param| ParamInfo {
                        name: param.name,
                        type_id: instantiate_type(self.interner, param.type_id, &substitution),
                        optional: param.optional,
                        rest: param.rest,
                    })
                    .collect(),
                return_type: instantiate_type(self.interner, source_fn.return_type, &substitution),
                this_type: source_fn
                    .this_type
                    .map(|this_type| instantiate_type(self.interner, this_type, &substitution)),
                type_params: vec![],
                type_predicate: source_fn
                    .type_predicate
                    .as_ref()
                    .map(|predicate| TypePredicate {
                        asserts: predicate.asserts,
                        target: predicate.target,
                        type_id: predicate
                            .type_id
                            .map(|tid| instantiate_type(self.interner, tid, &substitution)),
                        parameter_index: predicate.parameter_index,
                    }),
                is_constructor: source_fn.is_constructor,
                is_method: source_fn.is_method,
            });
        }

        let source_type_params_are_naked = source_fn.type_params.iter().all(|tp| {
            source_fn.params.iter().any(|param| {
                matches!(
                    self.interner.lookup(param.type_id),
                    Some(TypeData::TypeParameter(info)) if tp.is_same_binder(info)
                )
            })
        });
        let source_type_params_have_constraints = source_fn
            .type_params
            .iter()
            .any(|tp| tp.constraint.is_some());
        if source_type_params_are_naked
            && source_type_params_have_constraints
            && target_params_need_hofi
        {
            return source_ty;
        }
        if source_type_params_fully_determined_by_params
            && target_params_need_hofi
            && !source_type_params_are_naked
        {
            let has_generic_contextual_type = self.contextual_type.is_some_and(|ctx| {
                crate::type_queries::get_function_shape(self.interner.as_type_database(), ctx)
                    .is_some_and(|shape| {
                        !shape.type_params.is_empty()
                            && shape.params.iter().any(|param| {
                                crate::type_queries::get_function_shape(
                                    self.interner.as_type_database(),
                                    param.type_id,
                                )
                                .is_some_and(|inner| !inner.type_params.is_empty())
                                    || crate::type_queries::get_call_signatures(
                                        self.interner.as_type_database(),
                                        param.type_id,
                                    )
                                    .is_some_and(|sigs| {
                                        sigs.iter().any(|sig| !sig.type_params.is_empty())
                                    })
                            })
                    })
            });
            let target_is_pure_placeholder = target_fn.type_params.is_empty()
                && target_param_types.iter().all(|&pt| {
                    matches!(self.interner.lookup(pt), Some(TypeData::TypeParameter(_)))
                })
                && matches!(
                    self.interner.lookup(target_fn.return_type),
                    Some(TypeData::TypeParameter(_))
                );
            if has_generic_contextual_type
                || target_is_pure_placeholder
                || any_target_param_contains_infer_placeholder
            {
                // Case 2: let constrain_types handle it with fresh variables
                return source_ty;
            }
            let preserve_callable_alias = source_fn.type_params.len() == 1
                && source_fn.params.len() == 1
                && matches!(
                    self.interner.lookup(source_fn.params[0].type_id),
                    Some(TypeData::TypeParameter(param_tp))
                        if source_fn.type_params[0].is_same_binder(param_tp)
                )
                && matches!(
                    self.interner.lookup(source_fn.return_type),
                    Some(TypeData::TypeParameter(ret_tp))
                        if source_fn.type_params[0].is_same_binder(ret_tp)
                );
            if preserve_callable_alias {
                return source_ty;
            }
            // Case 3: erase to constraints/unknown
            let mut erasure_sub = TypeSubstitution::new();
            erasure_sub.protect_type_parameters(&source_fn.type_params);
            for tp in &source_fn.type_params {
                erasure_sub.insert(tp.name, tp.constraint.unwrap_or(TypeId::UNKNOWN));
            }
            let erased = FunctionShape {
                params: source_fn
                    .params
                    .iter()
                    .map(|p| ParamInfo {
                        name: p.name,
                        type_id: instantiate_type(self.interner, p.type_id, &erasure_sub),
                        optional: p.optional,
                        rest: p.rest,
                    })
                    .collect(),
                return_type: instantiate_type(self.interner, source_fn.return_type, &erasure_sub),
                this_type: source_fn
                    .this_type
                    .map(|t| instantiate_type(self.interner, t, &erasure_sub)),
                type_params: vec![],
                type_predicate: source_fn.type_predicate.as_ref().map(|pred| TypePredicate {
                    asserts: pred.asserts,
                    target: pred.target,
                    type_id: pred
                        .type_id
                        .map(|tid| instantiate_type(self.interner, tid, &erasure_sub)),
                    parameter_index: pred.parameter_index,
                }),
                is_constructor: source_fn.is_constructor,
                is_method: source_fn.is_method,
            };
            return self.interner.function(erased);
        }

        // Case 1b: naked source type params against a pure higher-order
        // inference target. TypeScript 3.4 higher-order function type inference
        // propagates the free type parameters of a generic function argument
        // through the wrapper instead of collapsing them. When the contextual
        // target is built entirely from outer inference placeholders (the
        // `pipe`/`compose`/`makeGetter` family), instantiating the naked source
        // param against that placeholder unifies the source type parameter with
        // an outer placeholder that no concrete argument can pin, so it later
        // resolves to `unknown` and the return-flow link is lost.
        //
        // Routing such arguments through the constraint walker's generic source
        // branch (`return source_ty`) instead creates fresh `__infer_src_*`
        // placeholders for the source type parameters. Those survive into the
        // call result and are re-generalized by
        // `hoist_source_placeholders_into_return_type`, reproducing tsc's
        // `<T>(a: T) => { value: T[] }` shape. When an outer argument *does*
        // pin the parameter, the source placeholder simply unifies with the
        // concrete evidence and resolves normally, so this path stays correct
        // for the determined case as well.
        if source_type_params_are_naked
            && target_params_need_hofi
            && target_fn.type_params.is_empty()
        {
            // Every contextual position of this argument (its parameters and its
            // return) must be *built entirely from* outer inference placeholders
            // for the pure higher-order shape to hold. A bare placeholder
            // (`__infer_1`) is the round-1 case; a placeholder wrapped in
            // structure (`__infer_src_3#X[]` after the shared middle type was
            // fixed in round 1) is the round-2 case. Both must route through the
            // generic-source branch so the source param chains into the result;
            // instantiating the naked source param against the structure instead
            // re-runs a nested inference that drops the foreign outer placeholder
            // to `unknown`, severing the higher-order return-flow link (the
            // `pipe(list, wrap)` middle `B = X[]` collapsing to `(unknown[])[]`).
            //
            // Each position must be placeholder-only (built solely from outer
            // inference placeholders carried by inert `Array`/tuple wrappers); a
            // single position carrying concrete pinning evidence short-circuits
            // to `false`. The placeholders accepted are:
            //
            // * a call-local outer placeholder (`__infer_N`) belonging to THIS
            //   resolution — the fail-closed check against
            //   `current_call_inference_placeholders` guards against nested/stale
            //   state (the original bare-placeholder behavior); and
            // * a higher-order *source* placeholder (`__infer_src_*`) — minted
            //   fresh by the generic-source constraint branch within this
            //   resolution to carry one generic argument's free type parameter
            //   into the next argument's target (the `pipe` shared middle type
            //   `B = X[]` becoming the second argument's `X_src[]` target after
            //   round 1 fixes it). Instantiating against it instead of routing
            //   back through the constraint walker drops `X_src` to `unknown`.
            let safe_to_regeneralize = {
                let mut at_least_one = false;
                let qualifies = target_param_types
                    .iter()
                    .chain(std::iter::once(&target_fn.return_type))
                    .all(|&pt| {
                        self.position_is_regeneralizable_higher_order_target(pt, &mut at_least_one)
                    });
                qualifies && at_least_one
            };
            if safe_to_regeneralize {
                return source_ty;
            }
        }

        // Case 1: naked type params — fall through to instantiation

        let prev_contextual_type = self.contextual_type;
        // Suppress contextual type when source type params are fully determined by params.
        // This prevents return type from incorrectly constraining T when T already comes
        // from param positions (e.g., `identity<T>(v:T)=>T` vs `Iterator<S, boolean>`).
        //
        // When source type params are NOT fully determined by params, use the target
        // function's RETURN TYPE as the contextual type — not the whole target function.
        // compute_contextual_types (step 2.5) constrains the source function's return
        // type against the contextual type. If the contextual type is the whole target
        // function, return-only type params get incorrectly matched against the target's
        // parameter types instead of its return type. For example:
        //   pair: <T, S>(x: T) => (y: S) => { x: T; y: S }
        //   target: (x: T_zw) => (y: S_zw) => U_zw
        // Without this fix, pair's return `(y: S) => ...` would be matched against
        // the whole target `(x: T_zw) => ...`, causing S to be inferred from T_zw
        // instead of S_zw.
        self.contextual_type = if source_type_params_fully_determined_by_params {
            None
        } else {
            Some(target_fn.return_type)
        };
        let instantiated =
            self.instantiate_function_shape_from_argument_types(&source_fn, &target_param_types);
        self.contextual_type = prev_contextual_type;
        let result = self.interner.function(instantiated);

        // If the instantiation produced a function with unresolved inference
        // placeholders (e.g., because the target parameter was a Union that
        // couldn't be structurally matched against the source's Application
        // type), fall back to erasure.  This prevents leaking `__infer_*`
        // placeholders into argument types and diagnostic messages.
        //
        // Skip this fallback when the target params are inference placeholders
        // from an outer generic call. In that case, the result is expected to
        // contain those placeholders — they represent proper higher-order
        // generic relationships (e.g., compose(list, box)) and will be resolved
        // by the outer inference context.
        if source_type_params_fully_determined_by_params
            && !any_target_param_is_type_param
            && crate::type_queries::contains_infer_types_db(
                self.interner.as_type_database(),
                result,
            )
        {
            let mut erasure_sub = TypeSubstitution::new();
            erasure_sub.protect_type_parameters(&source_fn.type_params);
            for tp in &source_fn.type_params {
                erasure_sub.insert(tp.name, tp.constraint.unwrap_or(TypeId::UNKNOWN));
            }
            let erased = FunctionShape {
                params: source_fn
                    .params
                    .iter()
                    .map(|p| ParamInfo {
                        name: p.name,
                        type_id: instantiate_type(self.interner, p.type_id, &erasure_sub),
                        optional: p.optional,
                        rest: p.rest,
                    })
                    .collect(),
                return_type: instantiate_type(self.interner, source_fn.return_type, &erasure_sub),
                this_type: source_fn
                    .this_type
                    .map(|t| instantiate_type(self.interner, t, &erasure_sub)),
                type_params: vec![],
                type_predicate: source_fn.type_predicate.as_ref().map(|pred| TypePredicate {
                    asserts: pred.asserts,
                    target: pred.target,
                    type_id: pred
                        .type_id
                        .map(|tid| instantiate_type(self.interner, tid, &erasure_sub)),
                    parameter_index: pred.parameter_index,
                }),
                is_constructor: source_fn.is_constructor,
                is_method: source_fn.is_method,
            };
            return self.interner.function(erased);
        }

        result
    }
}
