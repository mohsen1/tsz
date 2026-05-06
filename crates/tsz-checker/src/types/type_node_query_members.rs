//! Type query member helpers.

use super::type_node::TypeNodeChecker;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    pub(crate) fn value_property_type_query(&mut self, expr_name: NodeIndex) -> Option<TypeId> {
        let expr_name = self.ctx.arena.skip_parenthesized_and_assertions(expr_name);
        let node = self.ctx.arena.get(expr_name)?;
        let (base, property_name_node) =
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = self.ctx.arena.get_access_expr(node)?;
                if access.question_dot_token {
                    return None;
                }
                (access.expression, access.name_or_argument)
            } else if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let qualified = self.ctx.arena.get_qualified_name(node)?;
                (qualified.left, qualified.right)
            } else {
                return None;
            };

        let property_name = self.property_name_text(property_name_node)?;
        let base_type = self.value_type_for_type_query_member_base(base)?;
        match crate::query_boundaries::property_access::resolve_property_access(
            self.ctx.types,
            base_type,
            &property_name,
        ) {
            tsz_solver::operations::property::PropertyAccessResult::Success { type_id, .. }
            | tsz_solver::operations::property::PropertyAccessResult::PossiblyNullOrUndefined {
                property_type: Some(type_id),
                ..
            } if type_id != TypeId::ANY && type_id != TypeId::ERROR => Some(type_id),
            _ => None,
        }
    }

    pub(crate) fn value_type_for_type_query_member_base(
        &mut self,
        expr_name: NodeIndex,
    ) -> Option<TypeId> {
        let expr_name = self.ctx.arena.skip_parenthesized_and_assertions(expr_name);
        let node = self.ctx.arena.get(expr_name)?;
        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let ident = self.ctx.arena.get_identifier(node)?;
            let name = ident.escaped_text.as_str();
            if name == "default" {
                return None;
            }
            let sym_id = self
                .ctx
                .binder
                .resolve_identifier(self.ctx.arena, expr_name)?;
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if symbol.flags & tsz_binder::symbol_flags::VALUE == 0 {
                return None;
            }
            let type_id = self.ctx.symbol_types.get(&sym_id).copied()?;
            return (type_id != TypeId::ANY && type_id != TypeId::ERROR).then_some(type_id);
        }

        self.value_property_type_query(expr_name)
    }
}
