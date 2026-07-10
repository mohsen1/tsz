use super::assignability_diagnostic_common as common;
use crate::state::CheckerState;
use common::{TypeSubstitution, instantiate_type, type_param_info};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn should_suppress_partial_self_argument_mismatch(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some(inner) = self.partial_self_argument_inner_type(target) else {
            return false;
        };

        self.type_matches_partial_self_inner(source, inner)
    }

    fn partial_self_argument_inner_type(&mut self, target: TypeId) -> Option<TypeId> {
        let (base, args) = self.application_info_or_display_alias(target).or_else(|| {
            let evaluated = self.evaluate_type_for_assignability(target);
            self.application_info_or_display_alias(evaluated)
        })?;
        self.partial_like_application_inner_arg(base, &args)
    }

    fn partial_like_application_inner_arg(&self, base: TypeId, args: &[TypeId]) -> Option<TypeId> {
        if args.len() == 1 && self.application_base_is_lib_partial(base) {
            return args.first().copied();
        }

        let def_id = common::lazy_def_id(self.ctx.types, base)
            .or_else(|| self.ctx.definition_store.find_def_for_type(base))?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind != tsz_solver::def::DefKind::TypeAlias || def.type_params.len() != args.len() {
            return None;
        }
        let inner = self.optional_homomorphic_mapped_inner_type(def.body?)?;
        let param = type_param_info(self.ctx.types, inner)?;
        let arg_idx = def
            .type_params
            .iter()
            .position(|type_param| type_param.name == param.name)?;
        args.get(arg_idx).copied()
    }

    fn optional_homomorphic_mapped_inner_type(&self, type_id: TypeId) -> Option<TypeId> {
        let mapped = common::mapped_type_info(self.ctx.types, type_id)?;
        if mapped.optional_modifier == Some(tsz_solver::MappedModifier::Remove)
            || mapped.optional_modifier.is_none()
        {
            return None;
        }

        let inner = common::keyof_inner_type(self.ctx.types, mapped.constraint)?;
        let (template_object, _) = common::index_access_types(self.ctx.types, mapped.template)?;
        (template_object == inner).then_some(inner)
    }

    fn application_base_is_lib_partial(&self, base: TypeId) -> bool {
        let Some(partial_def) = self.ctx.actual_lib_def_id_for_bare_name("Partial") else {
            return false;
        };
        common::lazy_def_id(self.ctx.types, base)
            .or_else(|| self.ctx.definition_store.find_def_for_type(base))
            == Some(partial_def)
    }

    fn type_matches_partial_self_inner(&mut self, source: TypeId, inner: TypeId) -> bool {
        if source == inner {
            return true;
        }
        self.ctx.types.get_display_alias(source) == Some(inner)
            || self.partial_inner_alias_instantiates_to_source(inner, source)
    }

    fn partial_inner_alias_instantiates_to_source(
        &mut self,
        inner: TypeId,
        source: TypeId,
    ) -> bool {
        let Some((base, args)) = self.application_info_or_display_alias(inner) else {
            return false;
        };
        let Some(def_id) = common::lazy_def_id(self.ctx.types, base)
            .or_else(|| self.ctx.definition_store.find_def_for_type(base))
        else {
            return false;
        };
        let Some(def) = self.ctx.definition_store.get(def_id) else {
            return false;
        };
        if def.kind != tsz_solver::def::DefKind::TypeAlias || def.type_params.len() != args.len() {
            return false;
        }
        let Some(body) = def.body else {
            return false;
        };

        let substitution = TypeSubstitution::from_args(self.ctx.types, &def.type_params, &args);
        let instantiated = instantiate_type(self.ctx.types, body, &substitution);
        source == instantiated || self.ctx.types.get_display_alias(source) == Some(instantiated)
    }

    pub(crate) fn should_suppress_self_referential_generic_function_arg_mismatch(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some(source_sig) = common::callable_shape_for_type_extended(self.ctx.types, source)
            .and_then(|shape| {
                (shape.call_signatures.len() == 1).then(|| shape.call_signatures[0].clone())
            })
        else {
            return false;
        };
        if !source_sig.type_params.iter().any(|tp| {
            tp.constraint.is_some_and(|constraint| {
                common::contains_type_parameter_named(self.ctx.types, constraint, tp.name)
            })
        }) {
            return false;
        }

        let Some(target_sig) = common::callable_shape_for_type_extended(self.ctx.types, target)
            .and_then(|shape| {
                (shape.call_signatures.len() == 1).then(|| shape.call_signatures[0].clone())
            })
        else {
            return false;
        };
        if target_sig.return_type != TypeId::UNKNOWN {
            return false;
        }
        let Some(rest_param) = target_sig.params.last().filter(|param| param.rest) else {
            return false;
        };
        if rest_param.type_id == TypeId::UNKNOWN {
            return true;
        }
        common::tuple_elements(self.ctx.types, rest_param.type_id).is_some_and(|elements| {
            !elements.is_empty()
                && elements
                    .iter()
                    .all(|element| element.type_id == TypeId::UNKNOWN)
        })
    }

    pub(crate) fn should_suppress_self_referential_mapped_constraint_arg_mismatch(
        &mut self,
        source: TypeId,
        target: TypeId,
        arg_idx: NodeIndex,
    ) -> bool {
        if self
            .ctx
            .arena
            .get(arg_idx)
            .is_none_or(|node| node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
        {
            return false;
        }
        if !common::contains_type_parameters(self.ctx.types, target)
            || !self.type_contains_generic_mapped_constraint(target, &mut Default::default())
        {
            return false;
        }

        let mut substitution = TypeSubstitution::new();
        for referenced in common::collect_referenced_types(self.ctx.types, target) {
            let Some(info) = type_param_info(self.ctx.types, referenced) else {
                continue;
            };
            let Some(constraint) = info.constraint else {
                if common::contains_type_parameter_named(self.ctx.types, target, info.name) {
                    substitution.insert(info.name, source);
                }
                continue;
            };
            if common::contains_type_parameter_named(self.ctx.types, constraint, info.name)
                || common::contains_type_parameter_named(self.ctx.types, target, info.name)
            {
                substitution.insert(info.name, source);
            }
        }
        if substitution.is_empty() {
            return false;
        }

        let instantiated = instantiate_type(self.ctx.types, target, &substitution);
        let env_evaluated = self.evaluate_type_with_env(instantiated);
        let evaluated = self.evaluate_type_for_assignability(env_evaluated);
        let contextual = self.evaluate_contextual_type(instantiated);
        evaluated != target
            && evaluated != TypeId::UNKNOWN
            && evaluated != TypeId::ERROR
            && (self
                .generic_argument_suppression_relation_outcome_with_env(source, evaluated)
                .related
                || self
                    .generic_argument_suppression_relation_outcome_with_env(source, contextual)
                    .related
                || self.self_referential_mapped_intersection_accepts_object_literal(
                    source, evaluated, arg_idx,
                ))
    }

    fn self_referential_mapped_intersection_accepts_object_literal(
        &mut self,
        source: TypeId,
        target: TypeId,
        arg_idx: NodeIndex,
    ) -> bool {
        let Some(members) = common::intersection_members(self.ctx.types, target) else {
            return false;
        };

        let mut skipped_generic_mapped = false;
        let mut allowed_keys = rustc_hash::FxHashSet::default();
        for member in members {
            if self.type_contains_generic_mapped_constraint(member, &mut Default::default())
                || common::mapped_type_info(self.ctx.types, member).is_some()
            {
                skipped_generic_mapped = true;
                continue;
            }

            let member = self.evaluate_type_with_env(member);
            let Some(shape) = common::object_shape_for_type(self.ctx.types, member) else {
                if !self
                    .generic_argument_suppression_relation_outcome_with_env(source, member)
                    .related
                {
                    return false;
                }
                continue;
            };

            allowed_keys.extend(shape.properties.iter().map(|prop| prop.name));
            if shape.string_index.is_some() || shape.number_index.is_some() {
                return self
                    .generic_argument_suppression_relation_outcome_with_env(source, member)
                    .related;
            }
            if !self
                .generic_argument_suppression_relation_outcome_with_env(source, member)
                .related
            {
                return false;
            }
        }

        skipped_generic_mapped
            && self
                .object_literal_property_names(arg_idx)
                .is_some_and(|names| names.into_iter().all(|name| allowed_keys.contains(&name)))
    }

    fn object_literal_property_names(&self, arg_idx: NodeIndex) -> Option<Vec<tsz_common::Atom>> {
        let node = self.ctx.arena.get(arg_idx)?;
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let object = self.ctx.arena.get_literal_expr(node)?;
        let mut names = Vec::new();
        for &element_idx in &object.elements.nodes {
            let Some(element) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            let name = if let Some(prop) = self.ctx.arena.get_property_assignment(element) {
                self.get_property_name(prop.name)
            } else if element.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT {
                self.ctx
                    .arena
                    .get_shorthand_property(element)
                    .and_then(|prop| self.ctx.arena.get_identifier_text(prop.name))
                    .map(str::to_string)
            } else if let Some(method) = self.ctx.arena.get_method_decl(element) {
                self.property_name_for_error(method.name)
            } else {
                None
            };
            let name = name?;
            names.push(self.ctx.types.intern_string(&name));
        }
        Some(names)
    }

    fn type_contains_generic_mapped_constraint(
        &self,
        type_id: TypeId,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) -> bool {
        if !visited.insert(type_id) {
            return false;
        }
        if common::is_generic_mapped_type(self.ctx.types, type_id) {
            return true;
        }
        if let Some(mapped) = common::mapped_type_info(self.ctx.types, type_id) {
            return self.type_contains_generic_mapped_constraint(mapped.constraint, visited)
                || mapped.name_type.is_some_and(|name_type| {
                    self.type_contains_generic_mapped_constraint(name_type, visited)
                });
        }
        if let Some((_, args)) = common::application_info(self.ctx.types, type_id)
            && args
                .iter()
                .any(|&arg| self.type_contains_generic_mapped_constraint(arg, visited))
        {
            return true;
        }
        if let Some(members) = common::union_members(self.ctx.types, type_id)
            && members
                .iter()
                .any(|&member| self.type_contains_generic_mapped_constraint(member, visited))
        {
            return true;
        }
        if let Some(members) = common::intersection_members(self.ctx.types, type_id)
            && members
                .iter()
                .any(|&member| self.type_contains_generic_mapped_constraint(member, visited))
        {
            return true;
        }
        if let Some((object_type, index_type)) = common::index_access_types(self.ctx.types, type_id)
        {
            return self.type_contains_generic_mapped_constraint(object_type, visited)
                || self.type_contains_generic_mapped_constraint(index_type, visited);
        }
        if let Some(info) = type_param_info(self.ctx.types, type_id)
            && let Some(constraint) = info.constraint
        {
            return self.type_contains_generic_mapped_constraint(constraint, visited);
        }
        false
    }
}
