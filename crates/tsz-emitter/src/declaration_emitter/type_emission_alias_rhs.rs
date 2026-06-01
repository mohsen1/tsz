//! Type-alias RHS helpers for declaration type syntax emission.

use super::DeclarationEmitter;
use tsz_common::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn emit_type_alias_rhs(
        &mut self,
        alias_name: NodeIndex,
        type_idx: NodeIndex,
    ) {
        if self.emit_parenthesized_recursive_array_alias_rhs(alias_name, type_idx) {
            return;
        }
        self.emit_type(type_idx);
    }

    fn emit_parenthesized_recursive_array_alias_rhs(
        &mut self,
        alias_name: NodeIndex,
        type_idx: NodeIndex,
    ) -> bool {
        let Some(type_node) = self.arena.get(type_idx) else {
            return false;
        };
        if type_node.kind != syntax_kind_ext::ARRAY_TYPE {
            return false;
        }
        let Some(array_type) = self.arena.get_array_type(type_node) else {
            return false;
        };
        let Some(element_node) = self.arena.get(array_type.element_type) else {
            return false;
        };
        let source_parenthesized_element = element_node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            || self.array_type_source_wraps_element(type_node.pos, type_node.end);
        if !source_parenthesized_element {
            return false;
        }
        let inner = if element_node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
            self.peel_paren(array_type.element_type)
        } else {
            array_type.element_type
        };
        let Some(inner_node) = self.arena.get(inner) else {
            return false;
        };
        let Some(type_ref) = self.arena.get_type_ref(inner_node) else {
            return false;
        };
        if !self.type_reference_matches_alias_symbol(type_ref.type_name, alias_name) {
            return false;
        }

        self.write("(");
        self.emit_type(inner);
        self.write(")[]");
        true
    }

    fn array_type_source_wraps_element(&self, pos: u32, end: u32) -> bool {
        self.get_source_slice(pos, end)
            .is_some_and(|source| source.starts_with('(') && source.ends_with(")[]"))
    }

    fn type_reference_matches_alias_symbol(
        &self,
        type_name: NodeIndex,
        alias_name: NodeIndex,
    ) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let alias_text = self.identifier_text_from_arena(self.arena, alias_name);
        let type_name_text = self.identifier_text_from_arena(self.arena, type_name);

        let alias_symbol = binder.get_node_symbol(alias_name).or_else(|| {
            alias_text
                .as_deref()
                .and_then(|name| self.resolve_identifier_symbol(alias_name, name))
        });
        let type_symbol = binder.get_node_symbol(type_name).or_else(|| {
            type_name_text
                .as_deref()
                .and_then(|name| self.resolve_identifier_symbol(type_name, name))
        });

        if let (Some(alias_symbol), Some(type_symbol)) = (alias_symbol, type_symbol) {
            return alias_symbol == type_symbol;
        }

        self.identifier_atoms_match(alias_name, type_name)
    }

    fn identifier_atoms_match(&self, left: NodeIndex, right: NodeIndex) -> bool {
        let Some(left_ident) = self
            .arena
            .get(left)
            .and_then(|node| self.arena.get_identifier(node))
        else {
            return false;
        };
        let Some(right_ident) = self
            .arena
            .get(right)
            .and_then(|node| self.arena.get_identifier(node))
        else {
            return false;
        };
        left_ident.atom != Atom::NONE && left_ident.atom == right_ident.atom
    }
}
