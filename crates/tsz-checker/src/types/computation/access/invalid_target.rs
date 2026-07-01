use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(crate) fn optional_chain_invalid_assignment_target_context(&self, idx: NodeIndex) -> bool {
        if !super::optional_chain::is_optional_chain(self.ctx.arena, idx) {
            return false;
        }

        let mut current = idx;
        loop {
            if self.property_access_is_direct_write_target(current) {
                return true;
            }

            let Some(parent) = self.ctx.arena.parent_of(current) else {
                return false;
            };
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };

            if (parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                && self
                    .ctx
                    .arena
                    .get_access_expr(parent_node)
                    .is_some_and(|access| access.expression == current)
            {
                current = parent;
                continue;
            }

            if (parent_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
                || parent_node.kind == syntax_kind_ext::FOR_OF_STATEMENT)
                && self
                    .ctx
                    .arena
                    .get_for_in_of(parent_node)
                    .is_some_and(|for_data| for_data.initializer == current)
            {
                return true;
            }

            if self.ctx.in_destructuring_target {
                if parent_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                    && self
                        .ctx
                        .arena
                        .get_property_assignment(parent_node)
                        .is_some_and(|prop| prop.initializer == current)
                {
                    return true;
                }

                if parent_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                    || parent_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
                {
                    let spread_expr = self
                        .ctx
                        .arena
                        .get_spread(parent_node)
                        .map(|spread| spread.expression)
                        .or_else(|| {
                            self.ctx
                                .arena
                                .get_unary_expr_ex(parent_node)
                                .map(|unary| unary.expression)
                        });
                    if spread_expr == Some(current) {
                        return true;
                    }
                }

                if parent_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
                    return true;
                }
            }

            return false;
        }
    }
}
