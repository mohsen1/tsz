//! Helpers for the class-member prescan used while constructing instance types.

use crate::state::CheckerState;
use tsz_parser::parser::node::ClassData;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn inherited_prescan_this_base_type(
        &mut self,
        class: &ClassData,
        needs_inherited_prescan_this: bool,
    ) -> Option<TypeId> {
        if !needs_inherited_prescan_this {
            return None;
        }

        class
            .heritage_clauses
            .as_ref()
            .and_then(|heritage_clauses| {
                heritage_clauses.nodes.iter().find_map(|&clause_idx| {
                    let clause_node = self.ctx.arena.get(clause_idx)?;
                    let heritage = self.ctx.arena.get_heritage_clause(clause_node)?;
                    if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                        return None;
                    }
                    let &type_idx = heritage.types.nodes.first()?;
                    let type_node = self.ctx.arena.get(type_idx)?;
                    let (expr_idx, type_arguments) = if let Some(expr_type_args) =
                        self.ctx.arena.get_expr_type_args(type_node)
                    {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else {
                        (type_idx, None)
                    };
                    self.base_instance_type_from_expression(expr_idx, type_arguments)
                })
            })
    }
}
