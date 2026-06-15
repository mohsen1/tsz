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
        let (mut target_sym_id, mut target_file_idx) =
            if let Some(resolved) = self.resolve_value_import_target(base_sym_id) {
                // A type-only import is erased at runtime, so it cannot supply a
                // value member and never recovers an `arrayToEnum` literal.
                if resolved.type_only {
                    return None;
                }
                (resolved.symbol, Some(resolved.file_idx))
            } else if let Some(sym_id) = self.ctx.resolve_import_alias_and_register(base_sym_id) {
                (sym_id, self.ctx.resolve_symbol_file_index(sym_id))
            } else {
                (base_sym_id, self.ctx.resolve_symbol_file_index(base_sym_id))
            };
        let mut target_symbol = if let Some(file_idx) = target_file_idx {
            self.ctx
                .get_binder_for_file(file_idx)?
                .get_symbol(target_sym_id)?
        } else {
            self.get_cross_file_symbol(target_sym_id)?
        };
        if target_symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE == 0
            && let Some((value_sym_id, _, file_idx)) =
                self.same_file_value_symbol_for_type_symbol(target_sym_id)
        {
            target_sym_id = value_sym_id;
            target_file_idx = Some(file_idx);
            target_symbol = self
                .ctx
                .get_binder_for_file(file_idx)?
                .get_symbol(target_sym_id)?;
        }
        if target_symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE == 0 {
            return None;
        }

        let file_idx =
            target_file_idx.or_else(|| self.ctx.resolve_symbol_file_index(target_sym_id))?;
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
        let names =
            crate::symbols_domain::name_text::array_to_enum_call_literal_names(arena, initializer)?;
        names
            .into_iter()
            .find(|name| *name == property_name)
            .map(|name| self.ctx.types.literal_string(&name))
    }
}
