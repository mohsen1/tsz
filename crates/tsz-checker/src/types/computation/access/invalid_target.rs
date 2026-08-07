use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl CheckerState<'_> {
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

            // Only `for...of` short-circuits its optional-chain LHS to `any` here.
            // `for...in`'s LHS is not a genuinely invalid target the way a plain
            // `a?.b = 1` write is: tsc's checkForInStatement computes the LHS's
            // real (possibly-undefined) type and checks it against the index type
            // BEFORE deciding whether to also flag the chain itself (TS2405 wins
            // over TS2780 whenever the type check fails) — see
            // `check_for_in_of_expression_initializer` in
            // `state/variable_checking/for_loop.rs`, which needs that real type.
            if parent_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
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
