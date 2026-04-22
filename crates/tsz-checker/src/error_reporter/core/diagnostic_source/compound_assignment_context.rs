use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    /// Returns true when `anchor_idx` sits inside an arithmetic/bitwise
    /// compound assignment (`+=`, `-=`, `*=`, `&=`, etc.) — as the LHS/RHS of
    /// the compound binary, the binary itself, or the enclosing expression
    /// statement.
    ///
    /// For such contexts tsc widens literal types in the TS2322 message
    /// because the effective source of `x op= y` is the binary result `x op y`,
    /// which tsc reports with literal widening applied. Logical compound
    /// assignments (`&&=`, `||=`, `??=`) preserve the narrow RHS type and are
    /// deliberately excluded.
    pub(super) fn in_arithmetic_compound_assignment_context(&self, anchor_idx: NodeIndex) -> bool {
        let is_arith_compound_op = |op: u16| -> bool {
            crate::query_boundaries::common::is_compound_assignment_operator(op)
                && !crate::query_boundaries::common::is_logical_compound_assignment_operator(op)
        };

        let node_is_arith_compound_bin = |idx: NodeIndex| -> bool {
            self.ctx.arena.get(idx).is_some_and(|node| {
                node.kind == syntax_kind_ext::BINARY_EXPRESSION
                    && self
                        .ctx
                        .arena
                        .get_binary_expr(node)
                        .is_some_and(|bin| is_arith_compound_op(bin.operator_token))
            })
        };

        if node_is_arith_compound_bin(anchor_idx) {
            return true;
        }

        if let Some(node) = self.ctx.arena.get(anchor_idx)
            && node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
            && let Some(stmt) = self.ctx.arena.get_expression_statement(node)
            && node_is_arith_compound_bin(stmt.expression)
        {
            return true;
        }

        let Some(ext) = self.ctx.arena.get_extended(anchor_idx) else {
            return false;
        };
        node_is_arith_compound_bin(ext.parent)
    }

    pub(super) fn is_property_assignment_initializer(&self, anchor_idx: NodeIndex) -> bool {
        let current = self.ctx.arena.skip_parenthesized_and_assertions(anchor_idx);
        let Some(ext) = self.ctx.arena.get_extended(current) else {
            return false;
        };
        let parent_idx = ext.parent;
        let Some(parent) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        parent.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
            && self
                .ctx
                .arena
                .get_property_assignment(parent)
                .is_some_and(|prop| prop.initializer == current)
    }
}
