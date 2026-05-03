//! Helpers for TS2344 cases involving utility mapped type constraints.

use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn type_node_is_generic_ref_with_scoped_type_param_arg(
        &self,
        arg_idx: NodeIndex,
    ) -> bool {
        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return false;
        };
        let Some(type_ref) = self.ctx.arena.get_type_ref(arg_node) else {
            return false;
        };
        let Some(type_args) = &type_ref.type_arguments else {
            return false;
        };
        type_args
            .nodes
            .iter()
            .copied()
            .any(|node_idx| self.type_node_contains_scoped_type_parameter(node_idx))
    }

    fn type_node_contains_scoped_type_parameter(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if let Some(identifier) = self.ctx.arena.get_identifier(node)
            && self
                .ctx
                .type_parameter_scope
                .contains_key(&identifier.escaped_text)
        {
            return true;
        }
        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child_idx| self.type_node_contains_scoped_type_parameter(child_idx))
    }

    pub(super) fn required_mapped_constraint_source_is_required_and_arg_satisfies(
        &mut self,
        type_arg: TypeId,
        constraint: TypeId,
        substitutions: &[(tsz_common::Atom, TypeId)],
    ) -> bool {
        let Some(source) = self.required_mapped_constraint_source(constraint) else {
            return false;
        };
        let source = self.substitute_required_mapped_source(source, substitutions);

        let source = self.resolve_lazy_type(source);
        self.ensure_relation_input_ready(source);
        let source = self.evaluate_type_with_resolution(source);
        let tsz_solver::objects::PropertyCollectionResult::Properties { properties, .. } =
            tsz_solver::objects::collect_properties(source, self.ctx.types, &self.ctx)
        else {
            return false;
        };
        if properties.is_empty() || properties.iter().any(|prop| prop.optional) {
            return false;
        }

        let type_arg_resolved = self.resolve_lazy_type(type_arg);
        self.ensure_relation_input_ready(type_arg_resolved);
        let type_arg_evaluated = self.evaluate_type_with_resolution(type_arg_resolved);
        type_arg_evaluated == source
            || self.is_assignable_to(type_arg_evaluated, source)
            || self.type_satisfies_required_source_properties(type_arg_resolved, &properties)
            || (type_arg_evaluated != type_arg_resolved
                && self.type_satisfies_required_source_properties(type_arg_evaluated, &properties))
            || self.type_literal_alias_satisfies_required_source(type_arg_resolved, source)
    }

    fn required_mapped_constraint_source(&self, constraint: TypeId) -> Option<TypeId> {
        let db = self.ctx.types.as_type_database();
        if let Some(mapped) = crate::query_boundaries::common::mapped_type_info(db, constraint)
            && mapped.optional_modifier == Some(tsz_solver::MappedModifier::Remove)
        {
            return crate::query_boundaries::common::homomorphic_mapped_source(db, constraint);
        }

        let (Some(base_def), args) = query::application_base_def_and_args(db, constraint)? else {
            return None;
        };
        if args.len() != 1 {
            return None;
        }
        let sym_id = self.ctx.def_to_symbol_id(base_def)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        (symbol.escaped_name == "Required").then_some(args[0])
    }

    fn substitute_required_mapped_source(
        &self,
        source: TypeId,
        substitutions: &[(tsz_common::Atom, TypeId)],
    ) -> TypeId {
        let db = self.ctx.types.as_type_database();
        let Some(name) = query::type_parameter_name(db, source) else {
            return source;
        };
        substitutions
            .iter()
            .find_map(|&(param_name, arg)| (param_name == name).then_some(arg))
            .unwrap_or(source)
    }

    fn type_satisfies_required_source_properties(
        &mut self,
        type_arg: TypeId,
        source_properties: &[tsz_solver::PropertyInfo],
    ) -> bool {
        let tsz_solver::objects::PropertyCollectionResult::Properties { properties, .. } =
            tsz_solver::objects::collect_properties(type_arg, self.ctx.types, &self.ctx)
        else {
            return false;
        };
        for source_prop in source_properties {
            let Some(arg_prop) = properties.iter().find(|prop| prop.name == source_prop.name)
            else {
                return false;
            };
            if arg_prop.optional {
                return false;
            }
            if arg_prop.type_id != source_prop.type_id {
                let arg_type = self.evaluate_type_for_assignability(arg_prop.type_id);
                let source_type = self.evaluate_type_for_assignability(source_prop.type_id);
                if !self.is_assignable_to(arg_type, source_type) {
                    return false;
                }
            }
        }
        true
    }

    fn type_literal_alias_satisfies_required_source(
        &mut self,
        type_arg: TypeId,
        source: TypeId,
    ) -> bool {
        let Some(source_props) = self.type_literal_alias_property_nodes(source) else {
            return false;
        };
        if source_props.is_empty() || source_props.iter().any(|(_, _, optional)| *optional) {
            return false;
        }
        let Some(arg_props) = self.type_literal_alias_property_nodes(type_arg) else {
            return false;
        };

        for (source_name, source_type_node, _) in source_props {
            let Some((_, arg_type_node, arg_optional)) = arg_props
                .iter()
                .find(|(arg_name, _, _)| arg_name == &source_name)
                .cloned()
            else {
                return false;
            };
            if arg_optional {
                return false;
            }

            let arg_type = self.get_type_from_type_node(arg_type_node);
            let source_type = self.get_type_from_type_node(source_type_node);
            if arg_type != source_type && !self.is_assignable_to(arg_type, source_type) {
                return false;
            }
        }

        true
    }

    fn type_literal_alias_property_nodes(
        &self,
        type_id: TypeId,
    ) -> Option<Vec<(String, NodeIndex, bool)>> {
        let sym_id = self.ctx.resolve_type_to_symbol_id(type_id)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        for &decl_idx in &symbol.declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(alias) = self.ctx.arena.get_type_alias(decl_node) else {
                continue;
            };
            let Some(type_node) = self.ctx.arena.get(alias.type_node) else {
                continue;
            };
            let Some(type_lit) = self.ctx.arena.get_type_literal(type_node) else {
                continue;
            };

            let mut props = Vec::new();
            for &member_idx in &type_lit.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let Some(signature) = self.ctx.arena.get_signature(member_node) else {
                    continue;
                };
                if signature.type_annotation == NodeIndex::NONE {
                    continue;
                }
                let Some(name) = self.ctx.arena.identifier_text_owned(signature.name) else {
                    continue;
                };
                props.push((name, signature.type_annotation, signature.question_token));
            }
            return Some(props);
        }
        None
    }
}
