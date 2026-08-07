use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl CheckerState<'_> {
    pub(crate) fn optional_chain_invalid_assignment_target_context(&self, idx: NodeIndex) -> bool {
        if !super::optional_chain::is_optional_chain(self.ctx.arena, idx) {
            return false;
        }

        // Answer for `idx` itself alone — a continuation *below* the target
        // (the receiver of a further `.prop`/`[expr]` access) is an ordinary
        // read, not part of the invalid target, so this must not walk up
        // through the chain looking for an eventual write-target ancestor.
        // Walking up made every link below the real target short-circuit to
        // `any` too: for `a?.b.c = 1`, resolving the outer `.c` access needs
        // `a?.b`'s own type, and if that lookup answered "yes, I'm also
        // heading toward a write target" it never got a real type — silently
        // erasing the receiver's own possibly-nullish diagnostic, and (via
        // `get_type_of_write_target_base_expression`'s write-flow probe of a
        // receiver even during an otherwise ordinary read) any read-before-write
        // check on a deeper chain link.
        if self.property_access_is_direct_write_target(idx) {
            return true;
        }

        let Some(parent) = self.ctx.arena.parent_of(idx) else {
            return false;
        };
        let Some(parent_node) = self.ctx.arena.get(parent) else {
            return false;
        };

        if (parent_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
            || parent_node.kind == syntax_kind_ext::FOR_OF_STATEMENT)
            && self
                .ctx
                .arena
                .get_for_in_of(parent_node)
                .is_some_and(|for_data| for_data.initializer == idx)
        {
            return true;
        }

        if self.ctx.in_destructuring_target {
            if parent_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                && self
                    .ctx
                    .arena
                    .get_property_assignment(parent_node)
                    .is_some_and(|prop| prop.initializer == idx)
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
                if spread_expr == Some(idx) {
                    return true;
                }
            }

            if parent_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
                return true;
            }
        }

        false
    }
}
