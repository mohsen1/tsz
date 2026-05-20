//! Source accessor recovery for property-access declaration types.

use super::super::DeclarationEmitter;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn property_access_source_accessor_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(expr_node)?;
        let member_name = self.get_identifier_text(access.name_or_argument)?;
        let class_sym = self.property_access_base_class_symbol(access.expression)?;
        self.class_accessor_type_text(class_sym, &member_name)
    }

    fn property_access_base_class_symbol(&self, base_idx: NodeIndex) -> Option<SymbolId> {
        let base_idx = self.skip_parenthesized_expression(base_idx)?;
        let base_node = self.arena.get(base_idx)?;
        if base_node.kind == SyntaxKind::Identifier as u16 {
            let base_name = self.get_identifier_text(base_idx)?;
            if let Some(sym_id) = self.resolve_identifier_symbol(base_idx, &base_name)
                && self
                    .binder?
                    .symbols
                    .get(sym_id)
                    .is_some_and(|symbol| (symbol.flags & symbol_flags::CLASS) != 0)
            {
                return Some(sym_id);
            }
        }

        let base_sym_id = self.value_reference_symbol(base_idx)?;
        self.with_symbol_declarations(base_sym_id, |source_arena, decl_idx| {
            if !std::ptr::eq(source_arena, self.arena) {
                return None;
            }
            let decl_idx = Self::variable_decl_index_from_symbol_decl(source_arena, decl_idx)?;
            let decl_node = source_arena.get(decl_idx)?;
            let var_decl = source_arena.get_variable_declaration(decl_node)?;
            let init_idx = self.skip_parenthesized_expression(var_decl.initializer)?;
            let init_node = self.arena.get(init_idx)?;
            if init_node.kind != syntax_kind_ext::NEW_EXPRESSION {
                return None;
            }
            let new_expr = self.arena.get_call_expr(init_node)?;
            let ctor_idx = self.skip_parenthesized_expression(new_expr.expression)?;
            let ctor_name = self.get_identifier_text(ctor_idx)?;
            self.resolve_identifier_symbol(ctor_idx, &ctor_name)
        })
    }

    fn variable_decl_index_from_symbol_decl(
        arena: &NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = decl_idx;
        for _ in 0..8 {
            let node = arena.get(current)?;
            if arena.get_variable_declaration(node).is_some() {
                return Some(current);
            }
            current = arena.parent_of(current)?;
        }
        None
    }

    fn class_accessor_type_text(&self, class_sym: SymbolId, member_name: &str) -> Option<String> {
        self.with_symbol_declarations(class_sym, |source_arena, decl_idx| {
            let class_decl_idx = Self::class_decl_index_from_symbol_decl(source_arena, decl_idx)?;
            let class_node = source_arena.get(class_decl_idx)?;
            let class_decl = source_arena.get_class(class_node)?;
            self.class_member_accessor_type_text(source_arena, &class_decl.members, member_name)
        })
    }

    fn class_member_accessor_type_text(
        &self,
        source_arena: &NodeArena,
        members: &NodeList,
        member_name: &str,
    ) -> Option<String> {
        for &member_idx in &members.nodes {
            let member_node = source_arena.get(member_idx)?;
            let Some(accessor) = source_arena.get_accessor(member_node) else {
                continue;
            };
            if self
                .property_name_text_from_arena(source_arena, accessor.name)
                .as_deref()
                != Some(member_name)
            {
                continue;
            }
            if accessor.type_annotation.is_some() {
                return self
                    .type_annotation_text_from_arena_node(source_arena, accessor.type_annotation);
            }
            if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                && let Some(type_text) = self.class_accessor_matching_setter_type_text(
                    source_arena,
                    members,
                    member_name,
                )
            {
                return Some(type_text);
            }
        }
        None
    }

    fn class_accessor_matching_setter_type_text(
        &self,
        source_arena: &NodeArena,
        members: &NodeList,
        member_name: &str,
    ) -> Option<String> {
        for &member_idx in &members.nodes {
            let member_node = source_arena.get(member_idx)?;
            if member_node.kind != syntax_kind_ext::SET_ACCESSOR {
                continue;
            }
            let setter = source_arena.get_accessor(member_node)?;
            if self
                .property_name_text_from_arena(source_arena, setter.name)
                .as_deref()
                != Some(member_name)
            {
                continue;
            }
            let param_idx = setter.parameters.nodes.first().copied()?;
            let param_node = source_arena.get(param_idx)?;
            let param = source_arena.get_parameter(param_node)?;
            if param.type_annotation.is_some() {
                return self
                    .type_annotation_text_from_arena_node(source_arena, param.type_annotation);
            }
        }
        None
    }
}
