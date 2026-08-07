use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl CheckerState<'_> {
    /// Whether `idx` is the write target (assignment LHS, compound-assignment
    /// LHS, increment/decrement operand, `for...of` head, destructuring-
    /// assignment slot) of an optional chain, whose access type is
    /// short-circuited to `any` so an invalid target cannot cascade into
    /// assignability diagnostics.
    ///
    /// Only the target link itself qualifies — this checks `idx`'s own parent
    /// once, it does not walk up through further receiver links. A chain
    /// continuation *below* the target (`a?.b` in `a?.b.c = 1`) is an
    /// ordinary read in tsc: it is the receiver the target link is judged
    /// against, so it keeps its normal type and its normal diagnostics (e.g.
    /// a genuinely optional `b` still reports TS18048 while resolving `.c`).
    /// Walking further up used to sweep every receiver link into the same
    /// `any` short-circuit as the target, silently losing that receiver's own
    /// possibly-nullish read (see #16654).
    pub(crate) fn optional_chain_invalid_assignment_target_context(&self, idx: NodeIndex) -> bool {
        if !super::optional_chain::is_optional_chain(self.ctx.arena, idx) {
            return false;
        }

        if self.property_access_is_direct_write_target(idx) {
            return true;
        }

        let Some(parent) = self.ctx.arena.parent_of(idx) else {
            return false;
        };
        let Some(parent_node) = self.ctx.arena.get(parent) else {
            return false;
        };

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
