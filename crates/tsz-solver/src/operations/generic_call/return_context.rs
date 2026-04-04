//! Return context substitution methods for generic call inference.

use crate::inference::infer::InferenceContext;
use crate::inference::infer::InferenceVar;
use crate::instantiation::instantiate::TypeSubstitution;
use crate::operations::{AssignabilityChecker, CallEvaluator, CallResult};
use crate::types::{FunctionShape, TypeData, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub(super) fn hoist_resolved_type_params_into_return_type(
        &self,
        func: &FunctionShape,
        final_subst: &TypeSubstitution,
        return_type: TypeId,
    ) -> TypeId {
        let Some(TypeData::Function(shape_id)) = self.interner.lookup(return_type) else {
            return return_type;
        };

        let mut shape = self.interner.function_shape(shape_id).as_ref().clone();
        if !shape.type_params.is_empty() {
            return return_type;
        }

        let mut hoisted = Vec::new();
        let mut seen = FxHashSet::default();
        for tp in &func.type_params {
            let Some(resolved) = final_subst.get(tp.name) else {
                continue;
            };
            let Some(TypeData::TypeParameter(info)) = self.interner.lookup(resolved) else {
                continue;
            };
            if seen.insert(info.name)
                && crate::contains_type_parameter_named(
                    self.interner.as_type_database(),
                    return_type,
                    info.name,
                )
            {
                hoisted.push(info);
            }
        }

        if hoisted.is_empty() {
            return return_type;
        }

        shape.type_params = hoisted;
        self.interner.function(shape)
    }

    pub(super) fn normalize_function_shape_params_for_context(
        &self,
        shape: &FunctionShape,
    ) -> FunctionShape {
        use crate::type_queries::unpack_tuple_rest_parameter;

        let mut normalized = shape.clone();
        normalized.params = shape
            .params
            .iter()
            .flat_map(|param| unpack_tuple_rest_parameter(self.interner, param))
            .collect();
        normalized
    }

    fn get_overloaded_source_signature_for_arity(
        db: &dyn crate::TypeDatabase,
        type_id: TypeId,
        arg_count: usize,
    ) -> Option<FunctionShape> {
        let (signatures, is_constructor) = crate::type_queries::get_call_signatures(db, type_id)
            .filter(|signatures| !signatures.is_empty())
            .map(|signatures| (signatures, false))
            .or_else(|| {
                crate::type_queries::get_construct_signatures(db, type_id)
                    .filter(|signatures| !signatures.is_empty())
                    .map(|signatures| (signatures, true))
            })?;
        let signature_accepts_arg_count = |params: &[crate::types::ParamInfo], count: usize| {
            let required_count = params.iter().filter(|p| !p.optional).count();
            let has_rest = params.iter().any(|p| p.rest);
            if has_rest {
                count >= required_count
            } else {
                count >= required_count && count <= params.len()
            }
        };
        let sig = signatures
            .iter()
            .rev()
            .find(|sig| signature_accepts_arg_count(&sig.params, arg_count))
            .or_else(|| signatures.last())?;
        Some(FunctionShape {
            type_params: sig.type_params.clone(),
            params: sig.params.clone(),
            this_type: sig.this_type,
            return_type: sig.return_type,
            type_predicate: sig.type_predicate,
            is_constructor,
            is_method: sig.is_method,
        })
    }

    pub(super) fn get_source_signature_for_target(
        db: &dyn crate::TypeDatabase,
        source_type: TypeId,
        target_type: TypeId,
    ) -> Option<(FunctionShape, FunctionShape)> {
        let target_fn = Self::get_contextual_signature(db, target_type)?;
        let source_fn = Self::get_overloaded_source_signature_for_arity(
            db,
            source_type,
            target_fn.params.len(),
        )
        .or_else(|| Self::get_contextual_signature(db, source_type))?;
        Some((source_fn, target_fn))
    }

    pub(super) fn should_use_contextual_return_substitution(
        &mut self,
        inferred: TypeId,
        contextual: TypeId,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
    ) -> bool {
        if matches!(inferred, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR) {
            return true;
        }

        // Only check for inference placeholders from the CURRENT generic call,
        // not outer-scope type parameters. Outer-scope type parameters (e.g., `U`
        // from an enclosing `function test<U>(...)`) are concrete in this context
        // and should not trigger the contextual return substitution override.
        let mut visited = FxHashSet::default();
        if self.type_contains_placeholder(inferred, var_map, &mut visited)
            || crate::type_queries::contains_infer_types_db(
                self.interner.as_type_database(),
                inferred,
            )
        {
            return true;
        }

        // If the inferred result only reached a broad fallback (typically the
        // declared constraint/default) and the contextual return substitution is
        // strictly narrower, prefer the contextual result. This keeps round-2
        // contextual typing from being discarded for deferred callback arguments.
        if self.checker.is_assignable_to(contextual, inferred)
            && !self.checker.is_assignable_to(inferred, contextual)
        {
            return true;
        }

        false
    }

    pub(super) fn contains_tuple_like_parameter_target(
        db: &dyn crate::TypeDatabase,
        type_id: TypeId,
    ) -> bool {
        if crate::type_queries::get_tuple_elements(db, type_id).is_some() {
            return true;
        }

        if let Some(members) = crate::type_queries::get_union_members(db, type_id) {
            return members
                .iter()
                .copied()
                .any(|member| Self::contains_tuple_like_parameter_target(db, member));
        }

        if let Some(members) = crate::type_queries::get_intersection_members(db, type_id) {
            return members
                .iter()
                .copied()
                .any(|member| Self::contains_tuple_like_parameter_target(db, member));
        }

        false
    }

    pub(super) fn can_apply_contextual_return_substitution(
        &mut self,
        infer_ctx: &mut InferenceContext<'_>,
        var: InferenceVar,
        inferred: TypeId,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
    ) -> bool {
        let has_non_return_candidates =
            infer_ctx.var_has_candidates(var) && !infer_ctx.all_candidates_are_return_type(var);

        if !has_non_return_candidates {
            return true;
        }

        if matches!(inferred, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR) {
            return true;
        }

        // Only check for inference placeholders from the CURRENT generic call,
        // not outer-scope type parameters.
        let mut visited = FxHashSet::default();
        self.type_contains_placeholder(inferred, var_map, &mut visited)
            || crate::type_queries::contains_infer_types_db(
                self.interner.as_type_database(),
                inferred,
            )
    }

    fn collect_return_context_substitution(
        &mut self,
        source: TypeId,
        target: TypeId,
        tracked_type_params: &FxHashSet<tsz_common::Atom>,
        substitution: &mut TypeSubstitution,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
    ) {
        if !visited.insert((source, target)) {
            return;
        }

        if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(source)
            && tracked_type_params.contains(&tp.name)
            && target != TypeId::UNKNOWN
            && target != TypeId::ERROR
            && substitution.get(tp.name).is_none()
            // Don't insert if target contains untracked type parameters from
            // nested generic signatures (e.g., Promise.catch's TResult parameter
            // when matching through .then()). These would contaminate inference.
            && !self.target_contains_untracked_type_params(target, tracked_type_params)
            // Don't insert if target contains OTHER tracked type parameters.
            // This prevents incorrect mappings when both TResult1 and TResult2
            // from a source union would be mapped to the same target that
            // references both of them.
            && !self.type_references_other_tracked_params(target, tp.name, tracked_type_params)
        {
            substitution.insert(tp.name, target);
            return;
        }

        // Source union decomposition: when the source return type is a union
        // of simple type parameters (like TResult1 | TResult2), decompose it
        // and match each member against the target. This is essential for
        // matching Application type args (e.g., Promise<TResult1 | TResult2>
        // vs Promise<DooDad>).
        // Guard: only decompose when ALL non-nullish members are tracked type
        // parameters. Complex unions (containing conditionals, applications,
        // etc.) should not be decomposed as the individual members lack the
        // context needed for correct matching.
        if let Some(source_members) =
            crate::type_queries::get_union_members(self.interner.as_type_database(), source)
        {
            let non_nullish: Vec<TypeId> = source_members
                .into_iter()
                .filter(|member| *member != TypeId::NULL && *member != TypeId::UNDEFINED)
                .collect();
            let all_tracked_type_params = !non_nullish.is_empty()
                && non_nullish.iter().all(|&member| {
                    if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(member) {
                        tracked_type_params.contains(&tp.name)
                    } else {
                        false
                    }
                });
            if all_tracked_type_params {
                for &member in &non_nullish {
                    self.collect_return_context_substitution(
                        member,
                        target,
                        tracked_type_params,
                        substitution,
                        visited,
                    );
                }
                if !substitution.is_empty() {
                    return;
                }
            }
        }

        if let Some(target_members) =
            crate::type_queries::get_union_members(self.interner.as_type_database(), target)
        {
            let before_len = substitution.len();
            for member in target_members
                .into_iter()
                .filter(|member| *member != TypeId::NULL && *member != TypeId::UNDEFINED)
            {
                self.collect_return_context_substitution(
                    source,
                    member,
                    tracked_type_params,
                    substitution,
                    visited,
                );
                if substitution.len() > before_len {
                    return;
                }
            }
        }

        if let Some(inner) = match self.interner.lookup(target) {
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => Some(inner),
            _ => None,
        } {
            self.collect_return_context_substitution(
                source,
                inner,
                tracked_type_params,
                substitution,
                visited,
            );
            if !substitution.is_empty() {
                return;
            }
        }

        if let Some(inner) = match self.interner.lookup(source) {
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => Some(inner),
            _ => None,
        } {
            self.collect_return_context_substitution(
                inner,
                target,
                tracked_type_params,
                substitution,
                visited,
            );
            if !substitution.is_empty() {
                return;
            }
        }

        let source_eval = self.interner.evaluate_type(source);
        let target_eval = self.interner.evaluate_type(target);
        let function_info = match (
            Self::get_contextual_signature(self.interner.as_type_database(), source),
            Self::get_contextual_signature(self.interner.as_type_database(), target),
        ) {
            (Some(source_fn), Some(target_fn)) => Some((source_fn, target_fn)),
            _ => match (
                Self::get_contextual_signature(self.interner.as_type_database(), source_eval),
                Self::get_contextual_signature(self.interner.as_type_database(), target_eval),
            ) {
                (Some(source_fn), Some(target_fn)) => Some((source_fn, target_fn)),
                _ => None,
            },
        };

        if let Some((source_fn, target_fn)) = function_info
            && source_fn.params.len() <= target_fn.params.len()
        {
            // When the target function is generic (e.g., `<A>(x: A) => Box<A>`),
            // directly insert mappings for source type parameters that appear in
            // parameter or return positions, bypassing the untracked-type-param
            // and references-other-tracked guards. These guards prevent
            // contamination from nested generic signatures, but contextual type
            // parameters like `A` from the variable's type annotation are
            // legitimate targets. Without this, inference variables (__infer_*)
            // leak into final types because e.g. `U -> Box<A>` gets blocked.
            if !target_fn.type_params.is_empty() {
                for (source_param, target_param) in
                    source_fn.params.iter().zip(target_fn.params.iter())
                {
                    if let Some(TypeData::TypeParameter(tp)) =
                        self.interner.lookup(source_param.type_id)
                        && tracked_type_params.contains(&tp.name)
                        && substitution.get(tp.name).is_none()
                        && target_param.type_id != TypeId::UNKNOWN
                        && target_param.type_id != TypeId::ERROR
                    {
                        substitution.insert(tp.name, target_param.type_id);
                    }
                }
                if let Some(TypeData::TypeParameter(tp)) =
                    self.interner.lookup(source_fn.return_type)
                    && tracked_type_params.contains(&tp.name)
                    && substitution.get(tp.name).is_none()
                    && target_fn.return_type != TypeId::UNKNOWN
                    && target_fn.return_type != TypeId::ERROR
                {
                    substitution.insert(tp.name, target_fn.return_type);
                }
                // Recurse for non-TypeParameter positions (nested structures).
                // Use an ungated helper that doesn't apply the
                // target-contains-untracked and references-other-tracked guards.
                // When the target function is generic, its type params (e.g. `A`
                // from `<A>(a: A[]) => Box<A>[]`) are legitimate targets, not
                // contaminants from nested generic signatures.
                for (source_param, target_param) in
                    source_fn.params.iter().zip(target_fn.params.iter())
                {
                    if !matches!(
                        self.interner.lookup(source_param.type_id),
                        Some(TypeData::TypeParameter(_))
                    ) {
                        self.collect_return_context_for_generic_target(
                            source_param.type_id,
                            target_param.type_id,
                            tracked_type_params,
                            substitution,
                        );
                    }
                }
                if !matches!(
                    self.interner.lookup(source_fn.return_type),
                    Some(TypeData::TypeParameter(_))
                ) {
                    self.collect_return_context_for_generic_target(
                        source_fn.return_type,
                        target_fn.return_type,
                        tracked_type_params,
                        substitution,
                    );
                }
            } else {
                for (source_param, target_param) in
                    source_fn.params.iter().zip(target_fn.params.iter())
                {
                    self.collect_return_context_substitution(
                        source_param.type_id,
                        target_param.type_id,
                        tracked_type_params,
                        substitution,
                        visited,
                    );
                }
                self.collect_return_context_substitution(
                    source_fn.return_type,
                    target_fn.return_type,
                    tracked_type_params,
                    substitution,
                    visited,
                );
            }
            return;
        }

        if let (Some(TypeData::Tuple(source_list_id)), Some(TypeData::Tuple(target_list_id))) =
            (self.interner.lookup(source), self.interner.lookup(target))
        {
            let source_elems = self.interner.tuple_list(source_list_id);
            let target_elems = self.interner.tuple_list(target_list_id);
            for (source_elem, target_elem) in source_elems.iter().zip(target_elems.iter()) {
                self.collect_return_context_substitution(
                    source_elem.type_id,
                    target_elem.type_id,
                    tracked_type_params,
                    substitution,
                    visited,
                );
            }
            return;
        }

        if let (Some(source_elem), Some(target_elem)) = (
            crate::type_queries::get_array_element_type(self.interner.as_type_database(), source),
            crate::type_queries::get_array_element_type(self.interner.as_type_database(), target),
        ) {
            self.collect_return_context_substitution(
                source_elem,
                target_elem,
                tracked_type_params,
                substitution,
                visited,
            );
            return;
        }

        if let Some(source_elem) =
            crate::type_queries::get_array_element_type(self.interner.as_type_database(), source)
            && let Some((_target_base, target_args)) =
                crate::type_queries::get_application_info(self.interner.as_type_database(), target)
            && target_args.len() == 1
        {
            self.collect_return_context_substitution(
                source_elem,
                target_args[0],
                tracked_type_params,
                substitution,
                visited,
            );
            return;
        }

        if let Some(source_elem) =
            crate::type_queries::get_array_element_type(self.interner.as_type_database(), source)
            && let Some(iterator_info) =
                crate::operations::get_iterator_info(self.interner, target, false)
        {
            self.collect_return_context_substitution(
                source_elem,
                iterator_info.yield_type,
                tracked_type_params,
                substitution,
                visited,
            );
            return;
        }

        let source_eval = self.interner.evaluate_type(source);
        let target_eval = self.interner.evaluate_type(target);
        let app_info = match (
            crate::type_queries::get_application_info(self.interner.as_type_database(), source),
            crate::type_queries::get_application_info(self.interner.as_type_database(), target),
        ) {
            (Some(source_app), Some(target_app)) => Some((source_app, target_app)),
            _ => match (
                crate::type_queries::get_application_info(
                    self.interner.as_type_database(),
                    source_eval,
                ),
                crate::type_queries::get_application_info(
                    self.interner.as_type_database(),
                    target_eval,
                ),
            ) {
                (Some(source_app), Some(target_app)) => Some((source_app, target_app)),
                _ => None,
            },
        };

        if let Some(((source_base, source_args), (target_base, target_args))) = app_info
            && source_args.len() == target_args.len()
        {
            if source_base == target_base {
                for (source_arg, target_arg) in source_args.iter().zip(target_args.iter()) {
                    self.collect_return_context_substitution(
                        *source_arg,
                        *target_arg,
                        tracked_type_params,
                        substitution,
                        visited,
                    );
                }
                return;
            }
            // When bases differ (e.g., AssignAction<TActor> vs ActionFunction<ConcreteType>),
            // match type arguments positionally if any source arg is a tracked type parameter.
            // This handles branded-property patterns where different interfaces share
            // structural positions for their type parameters (e.g., _out_TActor?: TActor).
            let has_tracked_source_arg = source_args.iter().any(|&arg| {
                if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(arg) {
                    tracked_type_params.contains(&tp.name)
                } else {
                    false
                }
            });
            if has_tracked_source_arg {
                for (source_arg, target_arg) in source_args.iter().zip(target_args.iter()) {
                    self.collect_return_context_substitution(
                        *source_arg,
                        *target_arg,
                        tracked_type_params,
                        substitution,
                        visited,
                    );
                }
                if !substitution.is_empty() {
                    return;
                }
            }
        }

        // Fallback: when source is an Application wrapping a single tracked type
        // parameter (e.g., Awaited<T>) and no structural match was found above,
        // try inferring the type parameter directly. This handles return context
        // inference for Promise.all where the return type contains Awaited<T> and
        // the contextual type is a concrete non-thenable type.
        // Guard: verify by evaluating Application(Base, [target]) and checking
        // it equals target — this ensures the alias is "transparent" (like
        // Awaited<X> = X for non-thenables) and not a structural wrapper (like
        // Task<X> which wraps X in a function type).
        if let Some((source_base, source_args)) =
            crate::type_queries::get_application_info(self.interner.as_type_database(), source)
                .or_else(|| {
                    crate::type_queries::get_application_info(
                        self.interner.as_type_database(),
                        source_eval,
                    )
                })
            && source_args.len() == 1
            && let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(source_args[0])
            && tracked_type_params.contains(&tp.name)
            && substitution.get(tp.name).is_none()
            && !self.target_contains_untracked_type_params(target, tracked_type_params)
        {
            // Verify: Application(Base, [target]) should evaluate to target
            // for the substitution to be correct.
            let test_app = self.interner.application(source_base, vec![target]);
            let evaluated = self.interner.evaluate_type(test_app);
            if evaluated == target {
                substitution.insert(tp.name, target);
            }
        }
    }

    /// Structural matching helper for the generic-target-function case.
    /// Unlike `collect_return_context_substitution`, this does NOT apply the
    /// `target_contains_untracked_type_params` or `type_references_other_tracked_params`
    /// guards. Those guards exist to prevent contamination from nested generic
    /// signatures (e.g., `Promise.catch`'s TResult), but when the target is the
    /// contextual type's own generic function, its type params (like `A` in
    /// `<A>(a: A[]) => Box<A>[]`) are legitimate substitution targets.
    fn collect_return_context_for_generic_target(
        &self,
        source: TypeId,
        target: TypeId,
        tracked_type_params: &FxHashSet<tsz_common::Atom>,
        substitution: &mut TypeSubstitution,
    ) {
        // Direct TypeParameter leaf — insert without guards
        if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(source)
            && tracked_type_params.contains(&tp.name)
            && target != TypeId::UNKNOWN
            && target != TypeId::ERROR
            && substitution.get(tp.name).is_none()
        {
            substitution.insert(tp.name, target);
            return;
        }

        // Array matching
        if let (Some(source_elem), Some(target_elem)) = (
            crate::type_queries::get_array_element_type(self.interner.as_type_database(), source),
            crate::type_queries::get_array_element_type(self.interner.as_type_database(), target),
        ) {
            self.collect_return_context_for_generic_target(
                source_elem,
                target_elem,
                tracked_type_params,
                substitution,
            );
            return;
        }

        // Tuple matching
        if let (Some(TypeData::Tuple(source_list_id)), Some(TypeData::Tuple(target_list_id))) =
            (self.interner.lookup(source), self.interner.lookup(target))
        {
            let source_elems = self.interner.tuple_list(source_list_id);
            let target_elems = self.interner.tuple_list(target_list_id);
            for (source_elem, target_elem) in source_elems.iter().zip(target_elems.iter()) {
                self.collect_return_context_for_generic_target(
                    source_elem.type_id,
                    target_elem.type_id,
                    tracked_type_params,
                    substitution,
                );
            }
            return;
        }

        // Application matching (same base, same arg count)
        if let (Some((source_base, source_args)), Some((target_base, target_args))) = (
            crate::type_queries::get_application_info(self.interner.as_type_database(), source),
            crate::type_queries::get_application_info(self.interner.as_type_database(), target),
        ) {
            if source_base == target_base && source_args.len() == target_args.len() {
                for (source_arg, target_arg) in source_args.iter().zip(target_args.iter()) {
                    self.collect_return_context_for_generic_target(
                        *source_arg,
                        *target_arg,
                        tracked_type_params,
                        substitution,
                    );
                }
            }
        }
    }

    /// Check if a type contains or IS a literal type (directly or in unions).
    pub(super) fn type_contains_literals(&self, type_id: TypeId) -> bool {
        match self.interner.lookup(type_id) {
            Some(TypeData::Literal(_)) => true,
            Some(TypeData::Union(members_id)) => {
                let members = self.interner.type_list(members_id);
                members.iter().any(|&m| self.type_contains_literals(m))
            }
            _ => false,
        }
    }

    /// Check if a type references tracked type parameters OTHER than `exclude_name`.
    fn type_references_other_tracked_params(
        &self,
        type_id: TypeId,
        exclude_name: tsz_common::Atom,
        tracked: &FxHashSet<tsz_common::Atom>,
    ) -> bool {
        if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(type_id) {
            return tp.name != exclude_name && tracked.contains(&tp.name);
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Union(members_id) | TypeData::Intersection(members_id)) => {
                let members = self.interner.type_list(members_id);
                members
                    .iter()
                    .any(|&m| self.type_references_other_tracked_params(m, exclude_name, tracked))
            }
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                app.args.iter().any(|&arg| {
                    self.type_references_other_tracked_params(arg, exclude_name, tracked)
                })
            }
            _ => false,
        }
    }

    /// Check if a type contains `TypeParameter` references that are NOT in the
    /// tracked set. These are "foreign" type params from nested generic signatures
    /// (e.g., `Promise.catch`'s `TResult` when matching through `.then()`).
    fn target_contains_untracked_type_params(
        &self,
        type_id: TypeId,
        tracked: &FxHashSet<tsz_common::Atom>,
    ) -> bool {
        if let Some(TypeData::TypeParameter(tp)) = self.interner.lookup(type_id) {
            return !tracked.contains(&tp.name);
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Union(members_id) | TypeData::Intersection(members_id)) => {
                let members = self.interner.type_list(members_id);
                members
                    .iter()
                    .any(|&m| self.target_contains_untracked_type_params(m, tracked))
            }
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                app.args
                    .iter()
                    .any(|&arg| self.target_contains_untracked_type_params(arg, tracked))
            }
            _ => false,
        }
    }

    pub(super) fn compute_return_context_substitution(
        &mut self,
        func: &FunctionShape,
        contextual_type: Option<TypeId>,
    ) -> TypeSubstitution {
        let Some(contextual_type) = contextual_type else {
            return TypeSubstitution::new();
        };

        let tracked_type_params: FxHashSet<_> = func.type_params.iter().map(|tp| tp.name).collect();
        if tracked_type_params.is_empty() {
            return TypeSubstitution::new();
        }

        let mut substitution = TypeSubstitution::new();
        let mut visited = FxHashSet::default();
        self.collect_return_context_substitution(
            func.return_type,
            contextual_type,
            &tracked_type_params,
            &mut substitution,
            &mut visited,
        );
        substitution
    }

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
}
