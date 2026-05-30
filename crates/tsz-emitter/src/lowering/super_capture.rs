//! ES5 `super` member-call receiver capture analysis for the lowering pass.
//!
//! At ES5, a `super.m(...)` / `super[e](...)` call lowers to
//! `_super.prototype.m.call(R, ...)`. When the call appears inside a
//! `this`-capturing arrow, the `.call(...)` receiver `R` must be the captured
//! lexical `this` (`_this`). These helpers locate the callee's `super`
//! keyword so the lowering pass can mark it with the active capture name.

use super::*;

impl<'a> LoweringPass<'a> {
    /// If `callee_idx` is a direct `super` member-access call target
    /// (`super.m` or `super[e]`, allowing for parentheses), return the
    /// `NodeIndex` of the underlying `super` keyword. Chained accesses such as
    /// `super.a.b` are normal calls on `super.a` and return `None`.
    pub(super) fn super_member_call_super_keyword(
        &self,
        callee_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let callee_idx = self.lowering_unwrap_parentheses(callee_idx);
        let node = self.arena.get(callee_idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }
        let access = self.arena.get_access_expr(node)?;
        let base_idx = self.lowering_unwrap_parentheses(access.expression);
        let base = self.arena.get(base_idx)?;
        if base.kind == SyntaxKind::SuperKeyword as u16 {
            Some(base_idx)
        } else {
            None
        }
    }

    /// Unwrap nested parenthesized expressions to the inner expression index.
    fn lowering_unwrap_parentheses(&self, mut idx: NodeIndex) -> NodeIndex {
        while let Some(node) = self.arena.get(idx) {
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.arena.get_parenthesized(node)
            {
                idx = paren.expression;
                continue;
            }
            break;
        }
        idx
    }
}
