//! Literal member recovery for imported `arrayToEnum` results.

use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn imported_array_to_enum_member_literal_type(
        &self,
        base_expr: NodeIndex,
        member_name_idx: NodeIndex,
    ) -> Option<TypeId> {
        let member_node = self.ctx.arena.get(member_name_idx)?;
        let property_name = if member_node.kind == SyntaxKind::Identifier as u16 {
            self.ctx
                .arena
                .get_identifier(member_node)
                .map(|ident| ident.escaped_text.clone())?
        } else if member_node.kind == SyntaxKind::StringLiteral as u16 {
            self.ctx
                .arena
                .get_literal(member_node)
                .map(|lit| lit.text.clone())?
        } else {
            return None;
        };

        let base_expr = self.ctx.arena.skip_parenthesized_and_assertions(base_expr);
        let base_node = self.ctx.arena.get(base_expr)?;
        if base_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let base_sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, base_expr)?;
        let target_sym_id = self
            .ctx
            .resolve_import_alias_and_register(base_sym_id)
            .unwrap_or(base_sym_id);
        let target_symbol = self.get_cross_file_symbol(target_sym_id)?;
        if target_symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE == 0 {
            return None;
        }

        let file_idx = self.ctx.resolve_symbol_file_index(target_sym_id)?;
        let arena = self.ctx.get_arena_for_file(file_idx as u32);
        let mut value_decl = if target_symbol.value_declaration.is_some() {
            target_symbol.value_declaration
        } else {
            target_symbol.primary_declaration()?
        };
        let mut value_node = arena.get(value_decl)?;
        if value_node.kind == SyntaxKind::Identifier as u16 {
            value_decl = arena.get_extended(value_decl)?.parent;
            value_node = arena.get(value_decl)?;
        }
        if value_node.kind != syntax_kind_ext::VARIABLE_DECLARATION
            || !arena.is_const_variable_declaration(value_decl)
        {
            return None;
        }

        let variable = arena.get_variable_declaration(value_node)?;
        let initializer = arena.skip_parenthesized_and_assertions(variable.initializer);
        let call_node = arena.get(initializer)?;
        if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = arena.get_call_expr(call_node)?;
        let callee_name = crate::symbols_domain::name_text::expression_name_text_in_arena(
            arena,
            call.expression,
        )?;
        if callee_name != "arrayToEnum" && !callee_name.ends_with(".arrayToEnum") {
            return None;
        }

        let first_arg = call.arguments.as_ref()?.nodes.first().copied()?;
        let arg = arena.skip_parenthesized_and_assertions(first_arg);
        let arg_node = arena.get(arg)?;
        if arg_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }
        let array = arena.get_literal_expr(arg_node)?;
        for &element in &array.elements.nodes {
            let element = arena.skip_parenthesized_and_assertions(element);
            let element_node = arena.get(element)?;
            if (element_node.kind == SyntaxKind::StringLiteral as u16
                || element_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16)
                && let Some(lit) = arena.get_literal(element_node)
                && lit.text == property_name
            {
                return Some(self.ctx.types.literal_string(&lit.text));
            }
        }

        None
    }
}
