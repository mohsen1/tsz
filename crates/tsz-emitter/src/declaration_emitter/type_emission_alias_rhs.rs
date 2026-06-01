//! Type-alias RHS helpers for declaration type syntax emission.

use super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn emit_type_alias_rhs(
        &mut self,
        alias_idx: NodeIndex,
        type_idx: NodeIndex,
    ) {
        if self.emit_parenthesized_array_reference_alias_rhs(alias_idx, type_idx) {
            return;
        }
        self.emit_type(type_idx);
    }

    fn emit_parenthesized_array_reference_alias_rhs(
        &mut self,
        alias_idx: NodeIndex,
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
        if element_node.kind != syntax_kind_ext::PARENTHESIZED_TYPE {
            return false;
        }
        let inner = self.peel_paren(array_type.element_type);
        let Some(inner_node) = self.arena.get(inner) else {
            return false;
        };
        let Some(type_ref) = self.arena.get_type_ref(inner_node) else {
            return false;
        };
        if self.type_reference_matches_alias_symbol(type_ref.type_name, alias_idx) {
            self.write("(");
            self.emit_type(inner);
            self.write(")[]");
        } else {
            self.emit_type(inner);
            self.write("[]");
        }
        true
    }

    fn type_reference_matches_alias_symbol(
        &self,
        type_name: NodeIndex,
        alias_idx: NodeIndex,
    ) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let type_name_text = self.identifier_text_from_arena(self.arena, type_name);

        let alias_symbol = binder.get_node_symbol(alias_idx);
        let type_symbol = binder.get_node_symbol(type_name).or_else(|| {
            type_name_text
                .as_deref()
                .and_then(|name| self.resolve_identifier_symbol(type_name, name))
        });

        if let (Some(alias_symbol), Some(type_symbol)) = (alias_symbol, type_symbol) {
            return alias_symbol == type_symbol;
        }

        false
    }
}
