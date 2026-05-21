//! RHS object-literal walker used by the excess-property diagnostic path.
//!
//! Extracted from `assignment_ops.rs` so that adding the multi-literal
//! walker for issue #9681 does not push the parent module over the
//! checker-boundary LOC ceiling enforced by `scripts/arch/check-checker-boundaries.sh`.

use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

impl<'a> CheckerState<'a> {
    /// Collect every object-literal expression node reachable on the RHS of an
    /// assignment through wrapping expressions that pass a contextual type to
    /// their operands. Used when the source of an assignability failure is a
    /// union built from fresh members (e.g. `cond ? {…} : {…}`, `x ?? {…}`,
    /// `a || {…}`) so the checker-level excess-property emit can run against
    /// each branch literal independently.
    ///
    /// Intentionally does NOT walk into `as`/`<T>`/`satisfies`: tsc treats
    /// explicit type assertions as opaque for excess-property checking, and
    /// `ts2353_tests::plain_type_assertion_assignment_keeps_excess_property_opaque`
    /// pins that behavior.
    pub(crate) fn collect_rhs_object_literals(&self, idx: NodeIndex) -> Vec<NodeIndex> {
        const MAX_DEPTH: u32 = 16;
        let mut out = Vec::new();
        let mut stack: Vec<(NodeIndex, u32)> = vec![(idx, 0)];
        while let Some((current, depth)) = stack.pop() {
            if depth > MAX_DEPTH {
                continue;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                continue;
            };
            if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                out.push(current);
                continue;
            }
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(node)
            {
                stack.push((paren.expression, depth + 1));
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(bin) = self.ctx.arena.get_binary_expr(node)
            {
                // `||`, `??`, `,`, `=` may all pass the contextual type to one
                // or both sides; recurse into both operands so every reachable
                // fresh literal participates in the excess-property emit.
                stack.push((bin.right, depth + 1));
                stack.push((bin.left, depth + 1));
                continue;
            }
            if node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
                && let Some(cond) = self.ctx.arena.get_conditional_expr(node)
            {
                stack.push((cond.when_false, depth + 1));
                stack.push((cond.when_true, depth + 1));
                continue;
            }
        }
        out
    }
}
