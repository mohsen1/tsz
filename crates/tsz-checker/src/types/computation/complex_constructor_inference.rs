use crate::query_boundaries::state::type_resolution as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn default_current_infer_placeholders_to_unknown(&self, type_id: TypeId) -> TypeId {
        let mut substitution = tsz_solver::TypeSubstitution::new();
        for ty in crate::query_boundaries::common::collect_all_types(self.ctx.types, type_id) {
            let Some(info) = crate::query_boundaries::common::type_param_info(self.ctx.types, ty)
            else {
                continue;
            };
            let name = self.ctx.types.resolve_atom_ref(info.name);
            if name.starts_with("__infer_") && !name.starts_with("__infer_src_") {
                substitution.insert(info.name, TypeId::UNKNOWN);
            }
        }
        if substitution.is_empty() {
            type_id
        } else {
            crate::query_boundaries::common::instantiate_type(
                self.ctx.types,
                type_id,
                &substitution,
            )
        }
    }

    pub(super) fn report_direct_constructor_type_param_constraint_mismatches(
        &mut self,
        shape: &tsz_solver::FunctionShape,
        args: &[tsz_parser::NodeIndex],
        arg_types: &[TypeId],
    ) {
        for (i, param) in shape.params.iter().enumerate() {
            let Some(&arg_idx) = args.get(i) else {
                continue;
            };
            let Some(&actual) = arg_types.get(i) else {
                continue;
            };
            let Some(param_info) =
                crate::query_boundaries::common::type_param_info(self.ctx.types, param.type_id)
            else {
                continue;
            };
            let Some(type_param) = shape
                .type_params
                .iter()
                .find(|type_param| type_param.name == param_info.name)
            else {
                continue;
            };
            let Some(constraint) = type_param.constraint else {
                continue;
            };
            let constraint = self.evaluate_type_with_env(constraint);
            let actual_for_check =
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, actual);
            if !self.is_assignable_to(actual_for_check, constraint) {
                let _ =
                    self.check_argument_assignable_or_report(actual_for_check, constraint, arg_idx);
            }
        }
    }

    pub(super) fn generic_constructor_nested_constraint_failure_return(
        &mut self,
        shape: &tsz_solver::FunctionShape,
        arg_types: &[TypeId],
    ) -> Option<TypeId> {
        let mut failed = false;
        for (i, param) in shape.params.iter().enumerate() {
            if crate::query_boundaries::common::type_param_info(self.ctx.types, param.type_id)
                .is_some()
            {
                continue;
            }
            let Some(&actual) = arg_types.get(i) else {
                continue;
            };
            for param_part in
                crate::query_boundaries::common::collect_all_types(self.ctx.types, param.type_id)
            {
                let Some(param_info) =
                    crate::query_boundaries::common::type_param_info(self.ctx.types, param_part)
                else {
                    continue;
                };
                let Some(type_param) = shape
                    .type_params
                    .iter()
                    .find(|type_param| type_param.name == param_info.name)
                else {
                    continue;
                };
                let Some(constraint) = type_param.constraint else {
                    continue;
                };
                let constraint = self.evaluate_type_with_env(constraint);
                if self
                    .primitive_parts(actual)
                    .into_iter()
                    .any(|part| !self.is_assignable_to(part, constraint))
                {
                    failed = true;
                }
            }
        }
        if !failed {
            return None;
        }

        let mut substitution = tsz_solver::TypeSubstitution::new();
        for type_param in &shape.type_params {
            let replacement = type_param
                .constraint
                .map(|constraint| self.evaluate_type_with_env(constraint))
                .unwrap_or(TypeId::UNKNOWN);
            substitution.insert(type_param.name, replacement);
        }
        Some(crate::query_boundaries::common::instantiate_type(
            self.ctx.types,
            shape.return_type,
            &substitution,
        ))
    }

    fn primitive_parts(&self, type_id: TypeId) -> Vec<TypeId> {
        crate::query_boundaries::common::collect_all_types(self.ctx.types, type_id)
            .into_iter()
            .map(|part| crate::query_boundaries::common::widen_literal_type(self.ctx.types, part))
            .filter(|&part| {
                matches!(
                    part,
                    TypeId::STRING
                        | TypeId::NUMBER
                        | TypeId::BOOLEAN
                        | TypeId::BIGINT
                        | TypeId::SYMBOL
                )
            })
            .collect()
    }

    pub(super) fn constructor_inferred_type_args_satisfy_constraints(
        &mut self,
        type_params: &[tsz_solver::TypeParamInfo],
        type_args: &[TypeId],
    ) -> bool {
        if type_params.len() != type_args.len() {
            return false;
        }

        let mut substitution = crate::query_boundaries::common::TypeSubstitution::new();
        for (tp, &type_arg) in type_params.iter().zip(type_args.iter()) {
            substitution.insert(tp.name, type_arg);
        }

        for (tp, &type_arg) in type_params.iter().zip(type_args.iter()) {
            if crate::query_boundaries::common::contains_infer_types(self.ctx.types, type_arg)
                || crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    type_arg,
                )
            {
                return false;
            }

            if type_arg == TypeId::ANY || type_arg == TypeId::UNKNOWN || type_arg == TypeId::ERROR {
                continue;
            }

            let Some(constraint) = tp.constraint else {
                continue;
            };
            let instantiated_constraint = crate::query_boundaries::common::instantiate_type(
                self.ctx.types,
                constraint,
                &substitution,
            );
            let evaluated_constraint = self.evaluate_type_with_env(instantiated_constraint);
            if !self.is_assignable_to_with_env(type_arg, evaluated_constraint) {
                return false;
            }
        }

        true
    }

    pub(super) fn seed_substitution_from_partial_function_returns(
        &mut self,
        substitution: &mut tsz_solver::TypeSubstitution,
        source_partial: TypeId,
        target_param: TypeId,
        type_params: &[tsz_solver::TypeParamInfo],
    ) {
        let Some(source_shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, source_partial)
        else {
            return;
        };
        let source_properties = source_shape.properties.clone();

        for source_prop in source_properties {
            let prop_name = self.ctx.types.resolve_atom(source_prop.name).to_owned();
            let Some(target_prop_type) = self
                .contextual_object_literal_property_type(target_param, &prop_name)
                .or_else(|| {
                    let evaluated = self.evaluate_type_with_env(target_param);
                    self.contextual_object_literal_property_type(evaluated, &prop_name)
                })
            else {
                continue;
            };
            let Some(source_fn) = crate::query_boundaries::common::function_shape_for_type(
                self.ctx.types,
                source_prop.type_id,
            ) else {
                continue;
            };
            for target_fn in self.function_shapes_from_type(target_prop_type) {
                self.seed_substitution_from_return_type_pair(
                    substitution,
                    source_fn.return_type,
                    target_fn.return_type,
                    type_params,
                );
                for returned_target_fn in self.function_shapes_from_type(target_fn.return_type) {
                    self.seed_substitution_from_return_type_pair(
                        substitution,
                        source_fn.return_type,
                        returned_target_fn.return_type,
                        type_params,
                    );
                }
            }
            self.seed_single_type_param_from_source_return_application(
                substitution,
                source_fn.return_type,
                target_prop_type,
                type_params,
            );
        }
    }

    pub(super) fn constructor_mismatch_recovery_matches_contextual_return(
        &self,
        constructor_type: TypeId,
        contextual_type: TypeId,
    ) -> bool {
        let Some(return_type) = crate::query_boundaries::common::construct_return_type_for_type(
            self.ctx.types,
            constructor_type,
        ) else {
            return false;
        };

        if return_type == contextual_type {
            return true;
        }

        let return_app = query::get_application_info(self.ctx.types, return_type).or_else(|| {
            self.ctx
                .types
                .get_display_alias(return_type)
                .and_then(|alias| query::get_application_info(self.ctx.types, alias))
        });
        let contextual_app =
            query::get_application_info(self.ctx.types, contextual_type).or_else(|| {
                self.ctx
                    .types
                    .get_display_alias(contextual_type)
                    .and_then(|alias| query::get_application_info(self.ctx.types, alias))
            });
        if let (Some((return_base, _)), Some((contextual_base, contextual_args))) =
            (return_app, contextual_app)
            && return_base == contextual_base
            && !contextual_args.is_empty()
            && contextual_args
                .iter()
                .any(|&arg| arg != TypeId::ANY && arg != TypeId::UNKNOWN && arg != TypeId::ERROR)
        {
            return true;
        }

        let return_name = self.format_type(return_type);
        let contextual_name = self.format_type(contextual_type);
        contextual_name
            .strip_prefix(return_name.as_str())
            .is_some_and(|suffix| suffix.starts_with('<'))
    }

    pub(super) fn function_shapes_from_type(&self, ty: TypeId) -> Vec<tsz_solver::FunctionShape> {
        let mut result = Vec::new();
        if let Some(members) = crate::query_boundaries::common::union_members(self.ctx.types, ty) {
            for member in members {
                result.extend(self.function_shapes_from_type(member));
            }
            return result;
        }
        if let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, ty)
        {
            result.push((*shape).clone());
        }
        if let Some(signatures) =
            crate::query_boundaries::common::call_signatures_for_type(self.ctx.types, ty)
        {
            result.extend(signatures.into_iter().map(|sig| tsz_solver::FunctionShape {
                params: sig.params,
                return_type: sig.return_type,
                this_type: sig.this_type,
                type_params: sig.type_params,
                type_predicate: sig.type_predicate,
                is_constructor: false,
                is_method: sig.is_method,
            }));
        }
        result
    }

    fn seed_substitution_from_return_type_pair(
        &self,
        substitution: &mut tsz_solver::TypeSubstitution,
        source_return: TypeId,
        target_return: TypeId,
        type_params: &[tsz_solver::TypeParamInfo],
    ) {
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, target_return)
        {
            for member in members {
                self.seed_substitution_from_return_type_pair(
                    substitution,
                    source_return,
                    member,
                    type_params,
                );
            }
            return;
        }

        let source_app = query::get_application_info(self.ctx.types, source_return).or_else(|| {
            self.ctx
                .types
                .get_display_alias(source_return)
                .and_then(|alias| query::get_application_info(self.ctx.types, alias))
        });
        let target_app = query::get_application_info(self.ctx.types, target_return).or_else(|| {
            self.ctx
                .types
                .get_display_alias(target_return)
                .and_then(|alias| query::get_application_info(self.ctx.types, alias))
        });
        let (Some((source_base, source_args)), Some((target_base, target_args))) =
            (source_app, target_app)
        else {
            return;
        };
        if source_base != target_base || source_args.len() != target_args.len() {
            return;
        }

        for (source_arg, target_arg) in source_args.iter().zip(target_args.iter()) {
            let Some(info) =
                crate::query_boundaries::common::type_param_info(self.ctx.types, *target_arg)
            else {
                continue;
            };
            if !type_params.iter().any(|tp| tp.name == info.name) {
                continue;
            }
            let current = substitution.get(info.name);
            let unresolved = current.is_none_or(|ty| {
                ty == TypeId::ANY
                    || ty == TypeId::UNKNOWN
                    || crate::query_boundaries::common::type_param_info(self.ctx.types, ty)
                        .is_some()
            });
            if unresolved {
                substitution.insert(info.name, *source_arg);
            }
        }
    }

    fn seed_single_type_param_from_source_return_application(
        &self,
        substitution: &mut tsz_solver::TypeSubstitution,
        source_return: TypeId,
        target_type: TypeId,
        type_params: &[tsz_solver::TypeParamInfo],
    ) {
        let source_app = query::get_application_info(self.ctx.types, source_return).or_else(|| {
            self.ctx
                .types
                .get_display_alias(source_return)
                .and_then(|alias| query::get_application_info(self.ctx.types, alias))
        });
        let Some((_source_base, source_args)) = source_app else {
            return;
        };
        let [source_arg] = source_args.as_slice() else {
            return;
        };

        let mut target_param_names = Vec::new();
        for ty in crate::query_boundaries::common::collect_all_types(self.ctx.types, target_type) {
            let Some(info) = crate::query_boundaries::common::type_param_info(self.ctx.types, ty)
            else {
                continue;
            };
            if type_params.iter().any(|tp| tp.name == info.name)
                && !target_param_names.contains(&info.name)
            {
                target_param_names.push(info.name);
            }
        }
        let [target_name] = target_param_names.as_slice() else {
            return;
        };

        let current = substitution.get(*target_name);
        let unresolved = current.is_none_or(|ty| {
            ty == TypeId::ANY
                || ty == TypeId::UNKNOWN
                || crate::query_boundaries::common::type_param_info(self.ctx.types, ty).is_some()
        });
        if unresolved {
            substitution.insert(*target_name, *source_arg);
        }
    }
}
