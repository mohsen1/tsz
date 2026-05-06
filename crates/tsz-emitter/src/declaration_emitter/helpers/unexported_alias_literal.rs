//! Literal annotation recovery for inaccessible local alias call initializers.

use super::super::DeclarationEmitter;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn call_initializer_unexported_alias_literal_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(init_node)?;
        let binder = self.binder?;
        let raw_sym_id = self.value_reference_symbol(call.expression)?;
        let sym_id = self
            .resolve_portability_import_alias(raw_sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(raw_sym_id, binder));
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            if !std::ptr::eq(source_arena, self.arena) {
                return None;
            }

            let decl_node = source_arena.get(decl_idx)?;
            let callable = Self::callable_decl_parts_from_node(source_arena, decl_node)?;
            if !callable.type_annotation.is_some()
                || !self.function_signature_accepts_call_arguments(
                    source_arena,
                    callable.parameters,
                    call,
                )
            {
                return None;
            }

            let type_node = source_arena.get(callable.type_annotation)?;
            let type_ref = source_arena.get_type_ref(type_node)?;
            if type_ref.type_arguments.is_some() {
                return None;
            }

            let alias_name = self.identifier_text_from_arena(source_arena, type_ref.type_name)?;
            let alias_sym_id = binder
                .get_node_symbol(type_ref.type_name)
                .or_else(|| self.resolve_identifier_symbol(type_ref.type_name, &alias_name))?;
            let alias_symbol = binder.symbols.get(alias_sym_id)?;
            if alias_symbol.flags & symbol_flags::TYPE_ALIAS == 0
                || alias_symbol.flags & symbol_flags::EXPORT_VALUE != 0
                || alias_symbol.declarations.iter().copied().any(|decl| {
                    source_arena
                        .get(decl)
                        .and_then(|node| source_arena.get_type_alias(node))
                        .is_some_and(|alias| {
                            source_arena.has_modifier(&alias.modifiers, SyntaxKind::ExportKeyword)
                        })
                })
            {
                return None;
            }

            for alias_decl in alias_symbol.declarations.iter().copied() {
                if let Some(alias) = source_arena
                    .get(alias_decl)
                    .and_then(|node| source_arena.get_type_alias(node))
                    && let Some(text) =
                        Self::literal_initializer_text_from_type_node(source_arena, alias.type_node)
                {
                    return Some(text);
                }
            }

            let interner = self.type_interner?;
            let type_id = self.get_node_type(callable.type_annotation)?;
            Self::literal_initializer_text_for_type_id(interner, type_id)
        })
    }

    fn literal_initializer_text_from_type_node(
        arena: &NodeArena,
        type_node_idx: NodeIndex,
    ) -> Option<String> {
        let type_node = arena.get(type_node_idx)?;
        if type_node.kind == syntax_kind_ext::LITERAL_TYPE
            && let Some(lit_type) = arena.get_literal_type(type_node)
        {
            return Self::literal_initializer_text_from_type_node(arena, lit_type.literal);
        }

        if type_node.kind == SyntaxKind::NumericLiteral as u16 {
            let literal = arena.get_literal(type_node)?;
            return Some(Self::normalize_numeric_literal(literal.text.as_ref()));
        }

        if type_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            let unary = arena.get_unary_expr(type_node)?;
            let operand_node = arena.get(unary.operand)?;
            if operand_node.kind != SyntaxKind::NumericLiteral as u16 {
                return None;
            }
            let literal = arena.get_literal(operand_node)?;
            let normalized = Self::normalize_numeric_literal(literal.text.as_ref());
            return match unary.operator {
                k if k == SyntaxKind::MinusToken as u16 => Some(format!("-{normalized}")),
                k if k == SyntaxKind::PlusToken as u16 => Some(normalized),
                _ => None,
            };
        }

        None
    }

    fn literal_initializer_text_for_type_id(
        interner: &tsz_solver::TypeInterner,
        type_id: tsz_solver::types::TypeId,
    ) -> Option<String> {
        if let Some(lit) = tsz_solver::visitor::literal_value(interner, type_id) {
            return Some(Self::format_literal_initializer(&lit, interner));
        }
        let (_def_id, member_type) = tsz_solver::visitor::enum_components(interner, type_id)?;
        let lit = tsz_solver::visitor::literal_value(interner, member_type)?;
        Some(Self::format_literal_initializer(&lit, interner))
    }
}
