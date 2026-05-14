//! Object-literal annotation predicates for conditional mapped types.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(crate) fn object_literal_property_has_conditional_mapped_annotation(
        &self,
        property_elem_idx: NodeIndex,
    ) -> bool {
        let Some(object_idx) = self.ctx.arena.parent_of(property_elem_idx) else {
            return false;
        };
        let Some(parent_idx) = self.ctx.arena.parent_of(object_idx) else {
            return false;
        };
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return false;
        }
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(parent_node) else {
            return false;
        };
        if var_decl.initializer != object_idx || var_decl.type_annotation.is_none() {
            return false;
        }

        let mut visited_type_nodes = rustc_hash::FxHashSet::default();
        let mut visited_symbols = rustc_hash::FxHashSet::default();
        self.type_node_contains_conditional_mapped_value_template(
            var_decl.type_annotation,
            &mut visited_type_nodes,
            &mut visited_symbols,
        )
    }

    fn type_node_contains_conditional_mapped_value_template(
        &self,
        type_node_idx: NodeIndex,
        visited_type_nodes: &mut rustc_hash::FxHashSet<NodeIndex>,
        visited_symbols: &mut rustc_hash::FxHashSet<tsz_binder::SymbolId>,
    ) -> bool {
        if type_node_idx.is_none() || !visited_type_nodes.insert(type_node_idx) {
            return false;
        }
        let Some(type_node) = self.ctx.arena.get(type_node_idx) else {
            return false;
        };

        if let Some(mapped) = self.ctx.arena.get_mapped_type(type_node)
            && mapped.type_node.is_some()
            && self.type_node_contains_conditional(
                mapped.type_node,
                visited_type_nodes,
                visited_symbols,
            )
        {
            return true;
        }

        if let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) {
            if self.type_reference_alias_body_contains_conditional_mapped_value_template(
                type_ref.type_name,
                visited_type_nodes,
                visited_symbols,
            ) {
                return true;
            }
            if let Some(args) = &type_ref.type_arguments
                && args.nodes.iter().copied().any(|arg| {
                    self.type_node_contains_conditional_mapped_value_template(
                        arg,
                        visited_type_nodes,
                        visited_symbols,
                    )
                })
            {
                return true;
            }
            return false;
        }

        self.type_node_children_contain_conditional_mapped_value_template(
            type_node,
            visited_type_nodes,
            visited_symbols,
        )
    }

    fn type_node_contains_conditional(
        &self,
        type_node_idx: NodeIndex,
        visited_type_nodes: &mut rustc_hash::FxHashSet<NodeIndex>,
        visited_symbols: &mut rustc_hash::FxHashSet<tsz_binder::SymbolId>,
    ) -> bool {
        if type_node_idx.is_none() || !visited_type_nodes.insert(type_node_idx) {
            return false;
        }
        let Some(type_node) = self.ctx.arena.get(type_node_idx) else {
            return false;
        };

        if type_node.kind == syntax_kind_ext::CONDITIONAL_TYPE {
            return true;
        }

        if let Some(type_ref) = self.ctx.arena.get_type_ref(type_node)
            && self.type_reference_alias_body_contains_conditional(
                type_ref.type_name,
                visited_type_nodes,
                visited_symbols,
            )
        {
            return true;
        }

        self.type_node_children_contain_conditional(type_node, visited_type_nodes, visited_symbols)
    }

    fn type_reference_alias_body_contains_conditional_mapped_value_template(
        &self,
        type_name: NodeIndex,
        visited_type_nodes: &mut rustc_hash::FxHashSet<NodeIndex>,
        visited_symbols: &mut rustc_hash::FxHashSet<tsz_binder::SymbolId>,
    ) -> bool {
        let Some(sym_id) = self.type_reference_alias_symbol(type_name) else {
            return false;
        };
        if !visited_symbols.insert(sym_id) {
            return false;
        }
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        std::iter::once(symbol.value_declaration)
            .chain(symbol.declarations.iter().copied())
            .any(|decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .and_then(|decl_node| self.ctx.arena.get_type_alias(decl_node))
                    .is_some_and(|alias| {
                        self.type_node_contains_conditional_mapped_value_template(
                            alias.type_node,
                            visited_type_nodes,
                            visited_symbols,
                        )
                    })
            })
    }

    fn type_node_children_contain_conditional_mapped_value_template(
        &self,
        type_node: &Node,
        visited_type_nodes: &mut rustc_hash::FxHashSet<NodeIndex>,
        visited_symbols: &mut rustc_hash::FxHashSet<tsz_binder::SymbolId>,
    ) -> bool {
        let mut visit = |child| {
            self.type_node_contains_conditional_mapped_value_template(
                child,
                visited_type_nodes,
                visited_symbols,
            )
        };

        if let Some(composite) = self.ctx.arena.get_composite_type(type_node) {
            return composite.types.nodes.iter().copied().any(visit);
        }
        if let Some(array) = self.ctx.arena.get_array_type(type_node) {
            return visit(array.element_type);
        }
        if let Some(tuple) = self.ctx.arena.get_tuple_type(type_node) {
            return tuple.elements.nodes.iter().copied().any(visit);
        }
        if let Some(wrapped) = self.ctx.arena.get_wrapped_type(type_node) {
            return visit(wrapped.type_node);
        }
        if let Some(indexed_access) = self.ctx.arena.get_indexed_access_type(type_node) {
            return visit(indexed_access.object_type) || visit(indexed_access.index_type);
        }
        if let Some(type_operator) = self.ctx.arena.get_type_operator(type_node) {
            return visit(type_operator.type_node);
        }
        if let Some(parenthesized) = self.ctx.arena.get_parenthesized(type_node) {
            return visit(parenthesized.expression);
        }

        false
    }

    fn type_reference_alias_body_contains_conditional(
        &self,
        type_name: NodeIndex,
        visited_type_nodes: &mut rustc_hash::FxHashSet<NodeIndex>,
        visited_symbols: &mut rustc_hash::FxHashSet<tsz_binder::SymbolId>,
    ) -> bool {
        let Some(sym_id) = self.type_reference_alias_symbol(type_name) else {
            return false;
        };
        if !visited_symbols.insert(sym_id) {
            return false;
        }
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        std::iter::once(symbol.value_declaration)
            .chain(symbol.declarations.iter().copied())
            .any(|decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .and_then(|decl_node| self.ctx.arena.get_type_alias(decl_node))
                    .is_some_and(|alias| {
                        self.type_node_contains_conditional(
                            alias.type_node,
                            visited_type_nodes,
                            visited_symbols,
                        )
                    })
            })
    }

    fn type_node_children_contain_conditional(
        &self,
        type_node: &Node,
        visited_type_nodes: &mut rustc_hash::FxHashSet<NodeIndex>,
        visited_symbols: &mut rustc_hash::FxHashSet<tsz_binder::SymbolId>,
    ) -> bool {
        let mut visit =
            |child| self.type_node_contains_conditional(child, visited_type_nodes, visited_symbols);

        if let Some(type_ref) = self.ctx.arena.get_type_ref(type_node)
            && let Some(args) = &type_ref.type_arguments
            && args.nodes.iter().copied().any(&mut visit)
        {
            return true;
        }
        if let Some(composite) = self.ctx.arena.get_composite_type(type_node) {
            return composite.types.nodes.iter().copied().any(&mut visit);
        }
        if let Some(array) = self.ctx.arena.get_array_type(type_node) {
            return visit(array.element_type);
        }
        if let Some(tuple) = self.ctx.arena.get_tuple_type(type_node) {
            return tuple.elements.nodes.iter().copied().any(&mut visit);
        }
        if let Some(wrapped) = self.ctx.arena.get_wrapped_type(type_node) {
            return visit(wrapped.type_node);
        }
        if let Some(conditional) = self.ctx.arena.get_conditional_type(type_node) {
            return visit(conditional.check_type)
                || visit(conditional.extends_type)
                || visit(conditional.true_type)
                || visit(conditional.false_type);
        }
        if let Some(indexed_access) = self.ctx.arena.get_indexed_access_type(type_node) {
            return visit(indexed_access.object_type) || visit(indexed_access.index_type);
        }
        if let Some(type_operator) = self.ctx.arena.get_type_operator(type_node) {
            return visit(type_operator.type_node);
        }
        if let Some(parenthesized) = self.ctx.arena.get_parenthesized(type_node) {
            return visit(parenthesized.expression);
        }

        false
    }

    fn type_reference_alias_symbol(&self, type_name: NodeIndex) -> Option<tsz_binder::SymbolId> {
        let type_name_node = self.ctx.arena.get(type_name)?;
        if type_name_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        self.ctx
            .binder
            .resolve_identifier(self.ctx.arena, type_name)
    }
}
