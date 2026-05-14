use crate::query_boundaries::state::type_resolution as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
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
