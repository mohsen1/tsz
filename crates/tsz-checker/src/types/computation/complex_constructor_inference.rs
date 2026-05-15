use crate::query_boundaries::state::type_resolution as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn seed_new_literal_constraint_type_args(
        &mut self,
        substitution: &mut tsz_solver::TypeSubstitution,
        shape: &tsz_solver::FunctionShape,
        args: &[tsz_parser::NodeIndex],
    ) -> bool {
        let mut seeded = false;
        for (i, param) in shape.params.iter().enumerate() {
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
            let Some(&arg_idx) = args.get(i) else {
                continue;
            };
            let Some(literal_arg_type) = self.literal_type_from_initializer(arg_idx) else {
                continue;
            };
            let widened_literal = crate::query_boundaries::common::widen_literal_type(
                self.ctx.types,
                literal_arg_type,
            );
            if widened_literal == literal_arg_type {
                continue;
            }
            let instantiated_constraint = crate::query_boundaries::common::instantiate_type(
                self.ctx.types,
                constraint,
                substitution,
            );
            let evaluated_constraint = self.evaluate_type_with_env(instantiated_constraint);
            let current = substitution.get(type_param.name);
            let can_replace_current = current.is_none_or(|ty| {
                ty == TypeId::ANY
                    || ty == TypeId::UNKNOWN
                    || ty == widened_literal
                    || ty == evaluated_constraint
                    || crate::query_boundaries::common::contains_infer_types(self.ctx.types, ty)
                    || crate::query_boundaries::common::type_param_info(self.ctx.types, ty)
                        .is_some()
            });
            if can_replace_current && widened_literal == evaluated_constraint {
                substitution.insert(type_param.name, literal_arg_type);
                seeded = true;
            }
        }
        seeded
    }

    pub(super) fn new_type_args_are_applyable(
        &mut self,
        shape: &tsz_solver::FunctionShape,
        type_args: &[TypeId],
        substitution: &tsz_solver::TypeSubstitution,
    ) -> bool {
        let mut has_concrete_arg = false;

        for (type_param, &type_arg) in shape.type_params.iter().zip(type_args.iter()) {
            if type_arg == TypeId::UNKNOWN || type_arg == TypeId::ANY || type_arg == TypeId::ERROR {
                continue;
            }
            if crate::query_boundaries::common::contains_infer_types(self.ctx.types, type_arg)
                || crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    type_arg,
                )
            {
                continue;
            }
            if let Some(constraint) = type_param.constraint {
                let instantiated_constraint = crate::query_boundaries::common::instantiate_type(
                    self.ctx.types,
                    constraint,
                    substitution,
                );
                let evaluated_constraint = self.evaluate_type_with_env(instantiated_constraint);
                if evaluated_constraint != TypeId::ANY
                    && evaluated_constraint != TypeId::UNKNOWN
                    && evaluated_constraint != TypeId::ERROR
                    && !self.is_assignable_to_with_env(type_arg, evaluated_constraint)
                {
                    return false;
                }
            }
            has_concrete_arg = true;
        }

        has_concrete_arg
    }

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

        let Some((contextual_base, contextual_args)) =
            query::get_application_info(self.ctx.types, contextual_type).or_else(|| {
                self.ctx
                    .types
                    .get_display_alias(contextual_type)
                    .and_then(|alias| query::get_application_info(self.ctx.types, alias))
            })
        else {
            return false;
        };

        !contextual_args.is_empty()
            && self.types_share_application_base_identity(return_type, contextual_base)
            && contextual_args
                .iter()
                .any(|&arg| arg != TypeId::ANY && arg != TypeId::UNKNOWN && arg != TypeId::ERROR)
    }

    fn types_share_application_base_identity(&self, left: TypeId, right: TypeId) -> bool {
        if left == right {
            return true;
        }
        if self.ctx.types.get_display_alias(left) == Some(right)
            || self.ctx.types.get_display_alias(right) == Some(left)
        {
            return true;
        }
        crate::query_boundaries::common::lazy_def_id(self.ctx.types, left)
            .zip(crate::query_boundaries::common::lazy_def_id(
                self.ctx.types,
                right,
            ))
            .is_some_and(|(left_def, right_def)| left_def == right_def)
    }
}
