//! Constructor accessibility helpers for generic constraint validation.

use crate::state::{CheckerState, MemberAccessLevel};
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn type_query_constructor_access_level(
        &self,
        type_arg_idx: NodeIndex,
    ) -> Option<MemberAccessLevel> {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let node = self.ctx.arena.get(type_arg_idx)?;
        if node.kind != syntax_kind_ext::TYPE_QUERY {
            return None;
        }
        let type_query = self.ctx.arena.get_type_query(node)?;
        let expr_node = self.ctx.arena.get(type_query.expr_name)?;
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let ident = self.ctx.arena.get_identifier(expr_node)?;
        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, type_query.expr_name)
            .or_else(|| self.ctx.binder.get_node_symbol(type_query.expr_name))
            .or_else(|| self.ctx.binder.file_locals.get(&ident.escaped_text))?;

        if let Some(access) = self.class_constructor_access_level(sym_id) {
            return Some(access);
        }

        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(
            tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE
                | tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE,
        ) {
            return None;
        }
        let decl_node = self.ctx.arena.get(symbol.value_declaration)?;
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        let class_expr_idx = self.class_expression_from_expr(var_decl.initializer)?;
        let class = self.ctx.arena.get_class_at(class_expr_idx)?;
        for &member_idx in &class.members.nodes {
            let member_node = self.ctx.arena.get(member_idx)?;
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let ctor = self.ctx.arena.get_constructor(member_node)?;
            if self.has_private_modifier(&ctor.modifiers) {
                return Some(MemberAccessLevel::Private);
            }
            if self.has_protected_modifier(&ctor.modifiers) {
                return Some(MemberAccessLevel::Protected);
            }
            return None;
        }
        None
    }

    pub(crate) fn constructor_accessibility_blocks_type_arg_constraint(
        &mut self,
        type_arg: TypeId,
        constraint: TypeId,
    ) -> bool {
        if !self.constraint_is_constructable(constraint) {
            return false;
        }

        let source_has_construct_sig = self.has_construct_sig(type_arg) || {
            let evaluated = self.evaluate_type_for_assignability(type_arg);
            evaluated != type_arg && self.has_construct_sig(evaluated)
        };
        if !source_has_construct_sig {
            return false;
        }

        self.constructor_accessibility_mismatch(type_arg, constraint, None)
            .is_some()
    }
}
