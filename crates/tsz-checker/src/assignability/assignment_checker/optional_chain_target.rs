//! Optional-chain recognition for assignment-like write targets.
//!
//! tsc decides `a?.b.c = 1` is an optional-chain target from the syntactic
//! chain (`NodeFlags.OptionalChain`), which the parser stops propagating at a
//! `ParenthesizedExpression` but carries through assertions. These helpers are
//! the tsz equivalent, and they own the TS2777/TS2779/TS2780/TS2781 predicate.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl CheckerState<'_> {
    /// Check if a node is part of an optional chain (has `?.` somewhere in its left spine).
    ///
    /// Walks through property access, element access, and call expression chains looking
    /// for any node with `question_dot_token: true` (for accesses) or the `OPTIONAL_CHAIN`
    /// flag (for calls). For example, in `obj?.a.b`, both `obj?.a` and `obj?.a.b` are
    /// considered part of the optional chain.
    ///
    /// Skips through transparent wrappers (parenthesized, non-null, type assertions, satisfies).
    pub(crate) fn is_optional_chain_access(&self, idx: NodeIndex) -> bool {
        // tsc's `checkReferenceExpression` looks through the outer parentheses
        // and assertions of the target itself before testing the chain flag, so
        // `(a?.b) = 1` is still an optional-chain target.
        self.chain_continues_through(self.ctx.arena.skip_parenthesized_and_assertions(idx))
    }

    /// Whether `idx` carries — or continues — an optional chain.
    ///
    /// Parentheses END a chain: tsc's parser stops setting `NodeFlags.OptionalChain`
    /// at a `ParenthesizedExpression`, so `(a?.b).c` is an ordinary property access
    /// on a possibly-nullish receiver, not an optional-chain target. Assertions do
    /// not end one — `a?.b!.c` stays a chain.
    fn chain_continues_through(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = self.ctx.arena.get_access_expr(node) {
                    // This node itself is an optional chain root (has `?.`)
                    if access.question_dot_token {
                        return true;
                    }
                    // Check if the base expression is part of an optional chain
                    self.chain_continues_through(self.skip_chain_assertions(access.expression))
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                // Call expressions get the OPTIONAL_CHAIN flag from the parser
                if node.is_optional_chain() {
                    return true;
                }
                // Check if the callee is part of an optional chain
                if let Some(call) = self.ctx.arena.get_call_expr(node) {
                    self.chain_continues_through(self.skip_chain_assertions(call.expression))
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Skip the assertion forms that a chain continues through (`!`, `as`,
    /// angle-bracket assertion, `satisfies`) without skipping parentheses.
    fn skip_chain_assertions(&self, mut idx: NodeIndex) -> NodeIndex {
        for _ in 0..100 {
            let Some(node) = self.ctx.arena.get(idx) else {
                return idx;
            };
            let inner = if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION {
                self.ctx.arena.get_unary_expr_ex(node).map(|u| u.expression)
            } else if node.kind == syntax_kind_ext::AS_EXPRESSION
                || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION
                || node.kind == syntax_kind_ext::TYPE_ASSERTION
            {
                self.ctx
                    .arena
                    .get_type_assertion(node)
                    .map(|assertion| assertion.expression)
            } else {
                None
            };
            match inner {
                Some(next) => idx = next,
                None => return idx,
            }
        }
        idx
    }
}
