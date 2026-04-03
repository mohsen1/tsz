//! Inference helper methods for generic call resolution.

use crate::inference::infer::{InferenceContext, InferenceVar};
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{
    FunctionShape, ParamInfo, TypeData, TypeId, TypePredicate,
};
use rustc_hash::{FxHashMap, FxHashSet};

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub(super) fn resolve_direct_parameter_inference_type(
        &self,
        lower_bounds: &[TypeId],
        inferred: TypeId,
    ) -> TypeId {
        if lower_bounds.len() <= 1 {
            return inferred;
        }

        let member_list_id = match self.interner.lookup(inferred) {
            Some(TypeData::Union(id)) => id,
            _ => return inferred,
        };

        // If this is already a single-member union, keep it as-is.
        if self.interner.type_list(member_list_id).len() <= 1 {
            return inferred;
        }

        if let Some(preferred_tuple_candidate) =
            self.preferred_specific_tuple_inference_candidate(lower_bounds)
        {
            return preferred_tuple_candidate;
        }

        // Direct arguments should stay narrow when there are heterogeneous candidates.
        // Otherwise TypeScript-style checks can get masked by a broad union result.
        if lower_bounds
            .iter()
            .all(|ty| self.is_mergeable_direct_inference_candidate(*ty))
        {
            // Guard: if lower bounds contain literals with different primitive bases
            // (e.g., "" and 3 → string vs number), fall back to the first candidate.
            // tsc keeps the first candidate in those cases so later argument checks
            // can report a proper TS2345 mismatch.
            if !self.has_conflicting_literal_bases(lower_bounds) {
                return inferred;
            }
        }

        // Fall back to the first lower-bound candidate so later argument checks
        // drive assignability failures on the mismatch site.
        lower_bounds[0]
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
        let mut concrete_bounds = lower_bounds
            .iter()
            .copied()
            .filter(|ty| {
                !matches!(*ty, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
                    && !crate::visitor::contains_type_parameters(
                        self.interner.as_type_database(),
                        *ty,
                    )
                    && !crate::type_queries::contains_infer_types_db(
                        self.interner.as_type_database(),
                        *ty,
                    )
            })
            .collect::<Vec<_>>();
        concrete_bounds.dedup();
        if concrete_bounds.len() == 1
            && (crate::type_queries::contains_infer_types_db(
                self.interner.as_type_database(),
                inferred,
            ) || matches!(inferred, TypeId::ANY | TypeId::UNKNOWN))
        {
            return concrete_bounds[0];
        }

        if lower_bounds.len() <= 1 {
            return inferred;
        }

        let inferred_union_members = match self.interner.lookup(inferred) {
            Some(TypeData::Union(member_list_id)) => self.interner.type_list(member_list_id),
            _ => return inferred,
        };
        if inferred_union_members.len() <= 1 {
            return inferred;
        }

        let all_structural = lower_bounds
            .iter()
            .all(|ty| self.is_structural_return_inference_candidate(*ty));
        if all_structural {
            return lower_bounds[0];
        }

        inferred
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
                source_fn = self.instantiate_function_shape_from_argument_types(
                    &source_fn,
                    &target_param_types,
                );
            }
            constrained_structurally = true;
            for (source_param, target_param) in source_fn.params.iter().zip(target_fn.params.iter())
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
        }

        constrained_structurally
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

    pub(super) fn direct_inference_tracking_target(&self, ty: TypeId) -> Option<TypeId> {
        match self.interner.lookup(ty) {
            Some(TypeData::Union(members)) => {
                let non_nullish: Vec<TypeId> = self
                    .interner
                    .type_list(members)
                    .iter()
                    .copied()
                    .filter(|member| !member.is_nullable())
                    .collect();
                if non_nullish.len() == 1 {
                    self.direct_inference_tracking_target(non_nullish[0])
                } else {
                    None
                }
            }
            Some(TypeData::Intersection(_)) => None,
            _ => Some(ty),
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
        tracked_type_params: &FxHashSet<tsz_common::Atom>,
    ) -> bool {
        let params_contain_tracked_type_param = |params: &[ParamInfo]| {
            params.iter().any(|param| {
                crate::visitor::collect_all_types(self.interner.as_type_database(), param.type_id)
                    .into_iter()
                    .any(|candidate| {
                        crate::type_param_info(self.interner.as_type_database(), candidate)
                            .is_some_and(|info| tracked_type_params.contains(&info.name))
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
        type_param_name: tsz_common::Atom,
    ) -> bool {
        let tracked_type_params = FxHashSet::from_iter([type_param_name]);

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

                arg_is_generic_function_like
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
            Self::get_contextual_signature(self.interner.as_type_database(), arg_type),
            Self::get_contextual_signature(self.interner.as_type_database(), target_type),
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
            if !has_generic_call_sig && !has_generic_construct_sig {
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
                            .is_some_and(|info| info.name == tp.name)
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
        if source_type_params_fully_determined_by_params && any_target_param_is_type_param {
            let source_type_params_are_naked = source_fn.type_params.iter().all(|tp| {
                source_fn.params.iter().any(|param| {
                    matches!(
                        self.interner.lookup(param.type_id),
                        Some(TypeData::TypeParameter(info)) if info.name == tp.name
                    )
                })
            });
            if !source_type_params_are_naked {
                let has_generic_contextual_type = self.contextual_type.is_some_and(|ctx| {
                    crate::type_queries::get_function_shape(self.interner.as_type_database(), ctx)
                        .is_some_and(|shape| !shape.type_params.is_empty())
                });
                if has_generic_contextual_type {
                    // Case 2: let constrain_types handle it with fresh variables
                    return source_ty;
                }
                // Case 3: erase to constraints/unknown
                let mut erasure_sub = TypeSubstitution::new();
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
                    return_type: instantiate_type(
                        self.interner,
                        source_fn.return_type,
                        &erasure_sub,
                    ),
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
            // Case 1: naked type params — fall through to instantiation
        }

        let prev_contextual_type = self.contextual_type;
        // Suppress contextual type when source type params are fully determined by params.
        // This prevents return type from incorrectly constraining T when T already comes
        // from param positions (e.g., `identity<T>(v:T)=>T` vs `Iterator<S, boolean>`).
        self.contextual_type = if source_type_params_fully_determined_by_params {
            None
        } else {
            Some(target_ty)
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

    pub(super) fn single_concrete_upper_bound(
        &self,
        infer_ctx: &mut InferenceContext<'_>,
        var: InferenceVar,
    ) -> Option<TypeId> {
        let constraints = infer_ctx.get_constraints(var)?;
        let mut concrete_upper_bounds = constraints
            .upper_bounds
            .iter()
            .copied()
            .filter(|upper| {
                !matches!(*upper, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
                    && !crate::visitor::contains_type_parameters(
                        self.interner.as_type_database(),
                        *upper,
                    )
                    && !crate::type_queries::contains_infer_types_db(
                        self.interner.as_type_database(),
                        *upper,
                    )
            })
            .collect::<Vec<_>>();
        concrete_upper_bounds.dedup();
        if concrete_upper_bounds.len() == 1 {
            concrete_upper_bounds.pop()
        } else {
            None
        }
    }

    fn is_mergeable_direct_inference_candidate(&self, ty: TypeId) -> bool {
        let evaluated_ty = self.interner.evaluate_type(ty);
        // Primitives (null, undefined, string, number, boolean, void, never, etc.)
        // are always safe to merge into a union — they don't indicate structural
        // ambiguity. Without this, `equal(B, D | undefined)` would discard the
        // union and use only the first candidate, causing false TS2345 errors.
        if ty.is_nullish() || ty.is_any_or_unknown() || ty == TypeId::NEVER || ty == TypeId::VOID {
            return true;
        }
        // Primitive base types are safe to merge — they're just as unambiguous as
        // null/undefined. Literal types (string/number/boolean/bigint literals)
        // are also safe since they widen to their base primitive during resolution.
        if matches!(
            ty,
            TypeId::STRING
                | TypeId::NUMBER
                | TypeId::BOOLEAN
                | TypeId::BIGINT
                | TypeId::SYMBOL
                | TypeId::OBJECT
                | TypeId::BOOLEAN_TRUE
                | TypeId::BOOLEAN_FALSE
        ) {
            return true;
        }
        // Nominal private brands should never be merged into a union during
        // direct argument inference. TypeScript fixes `T` to the first such
        // candidate and reports the later mismatch (`C` vs `D`) instead of
        // inferring `C | D`.
        if crate::type_queries::get_private_brand_name(self.interner.as_type_database(), ty)
            .is_some()
            || crate::type_queries::get_private_field_name(self.interner.as_type_database(), ty)
                .is_some()
            || crate::type_queries::get_private_brand_name(
                self.interner.as_type_database(),
                evaluated_ty,
            )
            .is_some()
            || crate::type_queries::get_private_field_name(
                self.interner.as_type_database(),
                evaluated_ty,
            )
            .is_some()
        {
            return false;
        }
        match self.interner.lookup(ty) {
            Some(
                TypeData::Literal(_)
                | TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Function(_)
                | TypeData::Callable(_)
                | TypeData::Intersection(_)
                | TypeData::Enum(..)
                | TypeData::Lazy(_)
                | TypeData::Application(_)
                | TypeData::Conditional(_)
                | TypeData::IndexAccess(..)
                | TypeData::TemplateLiteral(_)
                | TypeData::ReadonlyType(_)
                | TypeData::KeyOf(_),
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

    fn is_structural_return_inference_candidate(&self, ty: TypeId) -> bool {
        match self.interner.lookup(ty) {
            Some(
                TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Function(_)
                | TypeData::Callable(_)
                | TypeData::Intersection(_),
            ) => true,
            Some(TypeData::Union(members)) => {
                let members = self.interner.type_list(members);
                !members.is_empty()
                    && members
                        .iter()
                        .all(|member| self.is_structural_return_inference_candidate(*member))
            }
            _ => false,
        }
    }

    /// Returns `true` when the lower bounds contain literal types from different
    /// primitive families (e.g., a string literal and a number literal). This indicates
    /// heterogeneous candidates that tsc would NOT merge into a union.
    fn has_conflicting_literal_bases(&self, lower_bounds: &[TypeId]) -> bool {
        // Direct-parameter inference should keep the leftmost candidate when
        // fresh candidates disagree on primitive base. That preserves TypeScript's
        // first-wins behavior for cases like `bar<T>(x: T, y: T); bar(1, "")`,
        // where `T` should settle on `number` and the second argument should
        // still produce TS2345 instead of broadening the call to `number | string`.
        let mut seen_base: Option<TypeId> = None;
        for &ty in lower_bounds {
            let base = self.primitive_base_of(ty);
            if let Some(b) = base {
                match seen_base {
                    None => seen_base = Some(b),
                    Some(prev) if prev != b => return true,
                    _ => {}
                }
            }
        }
        false
    }

    /// Returns the primitive base TypeId for a type if it's a literal or primitive,
    /// or `None` for non-primitive types (objects, arrays, etc.).
    fn primitive_base_of(&self, ty: TypeId) -> Option<TypeId> {
        // Check well-known primitive TypeIds first
        if matches!(
            ty,
            TypeId::STRING | TypeId::NUMBER | TypeId::BOOLEAN | TypeId::BIGINT | TypeId::SYMBOL
        ) {
            return Some(ty);
        }
        if matches!(ty, TypeId::BOOLEAN_TRUE | TypeId::BOOLEAN_FALSE) {
            return Some(TypeId::BOOLEAN);
        }
        match self.interner.lookup(ty) {
            Some(TypeData::Literal(lit)) => Some(lit.primitive_type_id()),
            _ => None,
        }
    }

}
