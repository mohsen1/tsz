use crate::query_boundaries::common::{self, LiteralTypeKind};
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
            let Some(param_info) = common::type_param_info(self.ctx.types, param.type_id) else {
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
            let widened_literal = common::widen_literal_type(self.ctx.types, literal_arg_type);
            if widened_literal == literal_arg_type {
                continue;
            }
            let current = substitution.get(type_param.name);
            let unresolved = current.is_none_or(|ty| {
                ty == TypeId::ANY
                    || ty == TypeId::UNKNOWN
                    || ty == widened_literal
                    || common::contains_infer_types(self.ctx.types, ty)
                    || common::type_param_info(self.ctx.types, ty).is_some()
            });
            if !unresolved {
                continue;
            }
            let instantiated_constraint =
                common::instantiate_type(self.ctx.types, constraint, substitution);
            let evaluated_constraint = self.evaluate_type_with_env(instantiated_constraint);
            if widened_literal == evaluated_constraint {
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
            if common::contains_infer_types(self.ctx.types, type_arg)
                || common::contains_type_parameters(self.ctx.types, type_arg)
            {
                continue;
            }
            if let Some(constraint) = type_param.constraint {
                let instantiated_constraint =
                    common::instantiate_type(self.ctx.types, constraint, substitution);
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

    pub(super) fn generic_new_literal_preservation_mask(
        &mut self,
        shape: &tsz_solver::FunctionShape,
        arg_count: usize,
    ) -> Vec<bool> {
        (0..arg_count)
            .map(|i| {
                Self::generic_new_param_type_for_arg(shape, i)
                    .is_some_and(|param_type| self.generic_new_param_preserves_literal(param_type))
            })
            .collect()
    }

    fn generic_new_param_type_for_arg(
        shape: &tsz_solver::FunctionShape,
        i: usize,
    ) -> Option<TypeId> {
        shape.params.get(i).map(|p| p.type_id).or_else(|| {
            let last = shape.params.last()?;
            last.rest.then_some(last.type_id)
        })
    }

    fn generic_new_param_preserves_literal(&mut self, param_type: TypeId) -> bool {
        let Some(info) = common::type_param_info(self.ctx.types, param_type) else {
            return false;
        };
        let Some(constraint) = info.constraint else {
            return false;
        };

        Self::generic_new_constraint_preserves_literals(self.ctx.types, constraint) || {
            let evaluated = self.evaluate_type_with_env(constraint);
            evaluated != constraint
                && Self::generic_new_constraint_preserves_literals(self.ctx.types, evaluated)
        }
    }

    fn generic_new_constraint_preserves_literals(
        db: &dyn tsz_solver::QueryDatabase,
        ty: TypeId,
    ) -> bool {
        if matches!(
            ty,
            TypeId::STRING | TypeId::NUMBER | TypeId::BOOLEAN | TypeId::BIGINT
        ) {
            return true;
        }
        if matches!(
            common::classify_literal_type(db, ty),
            LiteralTypeKind::String(_)
                | LiteralTypeKind::Number(_)
                | LiteralTypeKind::BigInt(_)
                | LiteralTypeKind::Boolean(_)
        ) {
            return true;
        }
        common::union_members(db, ty).is_some_and(|members| {
            members
                .iter()
                .copied()
                .any(|member| Self::generic_new_constraint_preserves_literals(db, member))
        })
    }

    pub(super) fn seed_substitution_from_partial_function_returns(
        &mut self,
        substitution: &mut tsz_solver::TypeSubstitution,
        source_partial: TypeId,
        target_param: TypeId,
        type_params: &[tsz_solver::TypeParamInfo],
    ) {
        let Some(source_shape) = common::object_shape_for_type(self.ctx.types, source_partial)
        else {
            return;
        };

        for source_prop in source_shape.properties.clone() {
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
            let Some(source_fn) =
                common::function_shape_for_type(self.ctx.types, source_prop.type_id)
            else {
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

    fn function_shapes_from_type(&self, ty: TypeId) -> Vec<tsz_solver::FunctionShape> {
        let mut result = Vec::new();
        if let Some(members) = common::union_members(self.ctx.types, ty) {
            for member in members {
                result.extend(self.function_shapes_from_type(member));
            }
            return result;
        }
        if let Some(shape) = common::function_shape_for_type(self.ctx.types, ty) {
            result.push((*shape).clone());
        }
        if let Some(signatures) = common::call_signatures_for_type(self.ctx.types, ty) {
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
        if let Some(members) = common::union_members(self.ctx.types, target_return) {
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
            let Some(info) = common::type_param_info(self.ctx.types, *target_arg) else {
                continue;
            };
            if !type_params.iter().any(|tp| tp.name == info.name) {
                continue;
            }
            let current = substitution.get(info.name);
            let unresolved = current.is_none_or(|ty| {
                ty == TypeId::ANY
                    || ty == TypeId::UNKNOWN
                    || common::type_param_info(self.ctx.types, ty).is_some()
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
        for ty in common::collect_all_types(self.ctx.types, target_type) {
            let Some(info) = common::type_param_info(self.ctx.types, ty) else {
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
                || common::type_param_info(self.ctx.types, ty).is_some()
        });
        if unresolved {
            substitution.insert(*target_name, *source_arg);
        }
    }
}
