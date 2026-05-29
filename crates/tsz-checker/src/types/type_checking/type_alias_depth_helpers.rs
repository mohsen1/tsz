//! Helpers for type-alias recursion depth probes.

use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};

impl<'a> CheckerState<'a> {
    pub(crate) fn type_arg_nodes_all_are_deferred_passthrough_for_depth_check(
        &mut self,
        type_args: &NodeList,
    ) -> bool {
        !type_args.nodes.is_empty()
            && type_args
                .nodes
                .iter()
                .copied()
                .all(|node_idx| self.type_node_is_deferred_passthrough_for_depth_check(node_idx))
    }

    pub(crate) fn type_node_is_deferred_passthrough_for_depth_check(
        &mut self,
        node_idx: NodeIndex,
    ) -> bool {
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
        if let Some(identifier) = self.ctx.arena.get_identifier(node)
            && self
                .identifier_references_enclosing_infer_binding(node_idx, &identifier.escaped_text)
        {
            return true;
        }

        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
        {
            if type_ref
                .type_arguments
                .as_ref()
                .is_some_and(|type_args| !type_args.nodes.is_empty())
            {
                return false;
            }

            return self.type_name_is_deferred_passthrough_for_depth_check(type_ref.type_name);
        }

        false
    }

    pub(crate) fn type_node_is_bounded_indexed_descent_for_depth_check(
        &mut self,
        alias_sid: tsz_binder::SymbolId,
        node_idx: NodeIndex,
    ) -> bool {
        self.type_node_is_bounded_indexed_descent_for_depth_check_inner(alias_sid, node_idx, 0)
    }

    fn type_node_is_bounded_indexed_descent_for_depth_check_inner(
        &mut self,
        alias_sid: tsz_binder::SymbolId,
        node_idx: NodeIndex,
        depth: u8,
    ) -> bool {
        if depth > 8 {
            return false;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) {
            return self.type_node_is_deferred_passthrough_for_depth_check(indexed.object_type)
                || self.type_node_is_alias_type_parameter_ref(alias_sid, indexed.object_type);
        }

        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
        {
            return self.type_node_is_bounded_indexed_descent_for_depth_check_inner(
                alias_sid,
                wrapped.type_node,
                depth + 1,
            );
        }

        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
        {
            let type_name = type_ref.type_name;
            let arg_nodes = type_ref
                .type_arguments
                .as_ref()
                .map(|args| args.nodes.clone())
                .unwrap_or_default();
            if let Some(passthrough_arg) =
                self.transparent_alias_passthrough_arg_for_depth_check(type_name, &arg_nodes)
            {
                return self.type_node_is_bounded_indexed_descent_for_depth_check_inner(
                    alias_sid,
                    passthrough_arg,
                    depth + 1,
                );
            }
        }

        false
    }

    fn transparent_alias_passthrough_arg_for_depth_check(
        &mut self,
        type_name: NodeIndex,
        arg_nodes: &[NodeIndex],
    ) -> Option<NodeIndex> {
        if arg_nodes.is_empty() {
            return None;
        }

        let resolved = self
            .resolve_type_symbol_for_lowering(type_name)
            .map(tsz_binder::SymbolId)?;
        let symbol = self.ctx.binder.get_symbol(resolved)?;
        if !symbol.has_any_flags(symbol_flags::TYPE_ALIAS) {
            return None;
        }

        let decl_idx = symbol.primary_declaration()?;
        let decl_node = self.ctx.arena.get(decl_idx)?;
        let alias = self.ctx.arena.get_type_alias(decl_node)?;
        let params = alias.type_parameters.as_ref()?;
        let body_name =
            self.type_node_passthrough_reference_name_for_depth_check(alias.type_node)?;
        let passthrough_index = params.nodes.iter().copied().position(|param_idx| {
            self.ctx
                .arena
                .get(param_idx)
                .and_then(|param_node| self.ctx.arena.get_type_parameter(param_node))
                .and_then(|param| self.ctx.arena.get(param.name))
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .is_some_and(|ident| ident.escaped_text == body_name)
        })?;

        arg_nodes.get(passthrough_index).copied()
    }

    pub(crate) fn type_node_contains_unresolved_type_reference_for_depth_check(
        &mut self,
        node_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
            && self
                .resolve_type_symbol_for_lowering(type_ref.type_name)
                .is_none()
            && self
                .ctx
                .arena
                .kind_at(type_ref.type_name)
                .is_some_and(|kind| kind == syntax_kind_ext::QUALIFIED_NAME)
            && !self.type_name_is_deferred_passthrough_for_depth_check(type_ref.type_name)
        {
            return true;
        }

        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child_idx| {
                self.type_node_contains_unresolved_type_reference_for_depth_check(child_idx)
            })
    }

    pub(crate) fn type_args_reset_defaulted_alias_params_with_scoped_transform_for_depth_check(
        &mut self,
        alias_sid: tsz_binder::SymbolId,
        type_args: &NodeList,
    ) -> bool {
        let Some(type_param_nodes) = self.alias_type_parameter_nodes_for_depth_check(alias_sid)
        else {
            return false;
        };
        let supplied_count = type_args.nodes.len();
        if supplied_count >= type_param_nodes.len() {
            return false;
        }
        let omitted = &type_param_nodes[supplied_count..];
        let alias_param_names = self.alias_type_parameter_names_for_depth_check(alias_sid);
        if omitted.is_empty()
            || !omitted.iter().copied().all(|param_idx| {
                self.ctx
                    .arena
                    .get(param_idx)
                    .and_then(|param_node| self.ctx.arena.get_type_parameter(param_node))
                    .is_some_and(|param| {
                        param.default != NodeIndex::NONE
                            && !self.type_node_contains_alias_type_parameter_name_for_depth_check(
                                param.default,
                                &alias_param_names,
                            )
                    })
            })
        {
            return false;
        }

        type_args.nodes.iter().copied().any(|arg_idx| {
            self.type_node_contains_scoped_type_parameter_for_depth_check(arg_idx)
                && !self.type_node_is_deferred_passthrough_for_depth_check(arg_idx)
                && !self.type_node_is_bounded_indexed_descent_for_depth_check(alias_sid, arg_idx)
        })
    }

    pub(crate) fn type_args_contain_subtractive_alias_guard_for_depth_check(
        &mut self,
        alias_sid: tsz_binder::SymbolId,
        type_args: &NodeList,
    ) -> bool {
        let alias_param_names = self.alias_type_parameter_names_for_depth_check(alias_sid);
        type_args.nodes.iter().copied().any(|arg_idx| {
            self.type_node_is_subtractive_alias_guard_for_depth_check(arg_idx, &alias_param_names)
        })
    }

    pub(crate) fn type_alias_has_default_reset_recursive_conditional_body(
        &mut self,
        alias_sid: tsz_binder::SymbolId,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(alias_sid) else {
            return false;
        };
        let declarations = symbol.declarations.clone();
        declarations.into_iter().any(|decl_idx| {
            self.ctx.arena.get(decl_idx).is_some_and(|decl_node| {
                decl_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    && self
                        .ctx
                        .arena
                        .get_type_alias(decl_node)
                        .is_some_and(|alias| {
                            self.ctx
                                .arena
                                .kind_at(alias.type_node)
                                .is_some_and(|kind| kind == syntax_kind_ext::CONDITIONAL_TYPE)
                                && self.type_node_has_default_reset_recursive_alias_ref(
                                    alias.type_node,
                                    alias_sid,
                                )
                        })
            })
        })
    }

    fn type_node_has_default_reset_recursive_alias_ref(
        &mut self,
        node_idx: NodeIndex,
        alias_sid: tsz_binder::SymbolId,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
        {
            let resolved = self
                .resolve_type_symbol_for_lowering(type_ref.type_name)
                .map(tsz_binder::SymbolId);
            if resolved == Some(alias_sid)
                && let Some(type_args) = &type_ref.type_arguments
                && self
                    .type_args_reset_defaulted_alias_params_with_scoped_transform_for_depth_check(
                        alias_sid, type_args,
                    )
            {
                return true;
            }
        }

        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child_idx| {
                self.type_node_has_default_reset_recursive_alias_ref(child_idx, alias_sid)
            })
    }

    fn type_name_is_deferred_passthrough_for_depth_check(&mut self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        let Some(identifier) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };

        self.ctx
            .type_parameter_scope
            .contains_key(&identifier.escaped_text)
            || self
                .identifier_references_enclosing_infer_binding(name_idx, &identifier.escaped_text)
    }

    fn identifier_references_enclosing_infer_binding(
        &self,
        node_idx: NodeIndex,
        name: &str,
    ) -> bool {
        let mut current = node_idx;
        for _ in 0..50 {
            let parent = self
                .ctx
                .arena
                .get_extended(current)
                .map_or(NodeIndex::NONE, |ext| ext.parent);
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            if let Some(conditional) = self.ctx.arena.get_conditional_type(parent_node)
                && self.type_node_contains_infer_binding_named(conditional.extends_type, name)
            {
                return true;
            }
            current = parent;
        }
        false
    }

    fn type_node_contains_infer_binding_named(&self, node_idx: NodeIndex, name: &str) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::TEMPLATE_LITERAL_TYPE_SPAN {
            return self.ctx.arena.get_template_span(node).is_some_and(|span| {
                self.type_node_contains_infer_binding_named(span.expression, name)
            });
        }
        if node.kind == syntax_kind_ext::INFER_TYPE {
            let Some(infer_data) = self.ctx.arena.get_infer_type(node) else {
                return false;
            };
            if let Some(type_param_node) = self.ctx.arena.get(infer_data.type_parameter)
                && let Some(type_param) = self.ctx.arena.get_type_parameter(type_param_node)
                && let Some(name_node) = self.ctx.arena.get(type_param.name)
                && let Some(identifier) = self.ctx.arena.get_identifier(name_node)
                && identifier.escaped_text == name
            {
                return true;
            }
            return self.type_node_contains_infer_binding_named(infer_data.type_parameter, name);
        }
        if node.kind == syntax_kind_ext::TYPE_PARAMETER
            && let Some(type_param) = self.ctx.arena.get_type_parameter(node)
        {
            return (type_param.constraint != NodeIndex::NONE
                && self.type_node_contains_infer_binding_named(type_param.constraint, name))
                || (type_param.default != NodeIndex::NONE
                    && self.type_node_contains_infer_binding_named(type_param.default, name));
        }

        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child_idx| self.type_node_contains_infer_binding_named(child_idx, name))
    }

    fn alias_type_parameter_nodes_for_depth_check(
        &self,
        alias_sid: tsz_binder::SymbolId,
    ) -> Option<Vec<NodeIndex>> {
        let symbol = self.ctx.binder.get_symbol(alias_sid)?;
        let decl_idx = symbol.primary_declaration()?;
        let decl_node = self.ctx.arena.get(decl_idx)?;
        let alias = self.ctx.arena.get_type_alias(decl_node)?;
        let type_params = alias.type_parameters.as_ref()?;
        Some(type_params.nodes.clone())
    }

    fn alias_type_parameter_names_for_depth_check(
        &self,
        alias_sid: tsz_binder::SymbolId,
    ) -> Vec<String> {
        let Some(type_param_nodes) = self.alias_type_parameter_nodes_for_depth_check(alias_sid)
        else {
            return Vec::new();
        };
        type_param_nodes
            .into_iter()
            .filter_map(|param_idx| {
                let param_node = self.ctx.arena.get(param_idx)?;
                let param = self.ctx.arena.get_type_parameter(param_node)?;
                self.ctx
                    .arena
                    .get_identifier_text(param.name)
                    .map(str::to_owned)
            })
            .collect()
    }

    fn type_node_is_subtractive_alias_guard_for_depth_check(
        &mut self,
        node_idx: NodeIndex,
        alias_param_names: &[String],
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
        {
            return self.type_node_is_subtractive_alias_guard_for_depth_check(
                wrapped.type_node,
                alias_param_names,
            );
        }
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return false;
        };
        let Some(type_args) = type_ref.type_arguments.as_ref() else {
            return false;
        };
        if type_args.nodes.len() != 2 {
            return false;
        }
        let Some(source_name) = self.type_node_bare_reference_name(type_args.nodes[0]) else {
            return false;
        };
        let Some(removed_name) = self.type_node_bare_reference_name(type_args.nodes[1]) else {
            return false;
        };
        if source_name == removed_name
            || !alias_param_names.iter().any(|name| name == source_name)
            || !alias_param_names.iter().any(|name| name == removed_name)
        {
            return false;
        }

        self.ctx
            .arena
            .get_identifier_text(type_ref.type_name)
            .is_some_and(|name| name == "Exclude")
            && self.type_name_resolves_to_global_type_for_depth_check(type_ref.type_name, "Exclude")
    }

    fn type_name_resolves_to_global_type_for_depth_check(
        &mut self,
        type_name: NodeIndex,
        expected_name: &str,
    ) -> bool {
        let Some(resolved) = self
            .resolve_type_symbol_for_lowering(type_name)
            .map(tsz_binder::SymbolId)
        else {
            return false;
        };
        let lib_binders = self.get_lib_binders();
        self.ctx
            .binder
            .get_global_type_with_libs(expected_name, &lib_binders)
            .is_some_and(|global| global == resolved)
    }

    fn type_node_contains_alias_type_parameter_name_for_depth_check(
        &self,
        node_idx: NodeIndex,
        names: &[String],
    ) -> bool {
        if names.is_empty() {
            return false;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if self
            .type_node_bare_reference_name(node_idx)
            .is_some_and(|reference_name| names.iter().any(|name| reference_name == name))
        {
            return true;
        }
        if self
            .ctx
            .arena
            .get_identifier(node)
            .is_some_and(|identifier| names.contains(&identifier.escaped_text))
        {
            return true;
        }

        self.ctx
            .arena
            .get_children(node_idx)
            .into_iter()
            .any(|child_idx| {
                self.type_node_contains_alias_type_parameter_name_for_depth_check(child_idx, names)
            })
    }

    fn type_node_is_alias_type_parameter_ref(
        &self,
        alias_sid: tsz_binder::SymbolId,
        node_idx: NodeIndex,
    ) -> bool {
        let Some(name) = self.type_node_bare_reference_name(node_idx) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(alias_sid) else {
            return false;
        };
        let Some(decl_idx) = symbol.primary_declaration() else {
            return false;
        };
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        let Some(alias) = self.ctx.arena.get_type_alias(decl_node) else {
            return false;
        };
        alias.type_parameters.as_ref().is_some_and(|params| {
            params.nodes.iter().copied().any(|param_idx| {
                self.ctx
                    .arena
                    .get(param_idx)
                    .and_then(|param_node| self.ctx.arena.get_type_parameter(param_node))
                    .and_then(|param| self.ctx.arena.get(param.name))
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .is_some_and(|ident| ident.escaped_text == name)
            })
        })
    }

    fn type_node_bare_reference_name(&self, node_idx: NodeIndex) -> Option<&str> {
        let node = self.ctx.arena.get(node_idx)?;
        if let Some(identifier) = self.ctx.arena.get_identifier(node) {
            return Some(identifier.escaped_text.as_str());
        }
        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            let type_ref = self.ctx.arena.get_type_ref(node)?;
            if type_ref
                .type_arguments
                .as_ref()
                .is_some_and(|args| !args.nodes.is_empty())
            {
                return None;
            }
            let name_node = self.ctx.arena.get(type_ref.type_name)?;
            let identifier = self.ctx.arena.get_identifier(name_node)?;
            return Some(identifier.escaped_text.as_str());
        }
        None
    }

    fn type_node_passthrough_reference_name_for_depth_check(
        &self,
        node_idx: NodeIndex,
    ) -> Option<&str> {
        let node = self.ctx.arena.get(node_idx)?;
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
        {
            return self.type_node_passthrough_reference_name_for_depth_check(wrapped.type_node);
        }
        self.type_node_bare_reference_name(node_idx)
    }
}
