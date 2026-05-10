//! Anchor helpers for assignability diagnostics.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn callback_initializer_for_assignability_anchor(
        &self,
        anchor_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let callback_argument_in_call_like = |expr_idx: NodeIndex| {
            let expr_node = self.ctx.arena.get(expr_idx)?;
            let args = if matches!(
                expr_node.kind,
                syntax_kind_ext::CALL_EXPRESSION | syntax_kind_ext::NEW_EXPRESSION
            ) {
                self.ctx
                    .arena
                    .get_call_expr(expr_node)?
                    .arguments
                    .as_ref()?
            } else {
                return None;
            };
            args.nodes.iter().find_map(|&arg_idx| {
                let arg_idx = self.ctx.arena.skip_parenthesized_and_assertions(arg_idx);
                let arg_node = self.ctx.arena.get(arg_idx)?;
                (arg_node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || arg_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION)
                    .then_some(arg_idx)
            })
        };

        let mut current = anchor_idx;
        for _ in 0..8 {
            let anchor_node = self.ctx.arena.get(current)?;
            if anchor_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || anchor_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            {
                return Some(current);
            }
            if anchor_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                let property = self.ctx.arena.get_property_assignment(anchor_node)?;
                let initializer = self
                    .ctx
                    .arena
                    .skip_parenthesized_and_assertions(property.initializer);
                let initializer_node = self.ctx.arena.get(initializer)?;
                return (initializer_node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || initializer_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION)
                    .then_some(initializer)
                    .or_else(|| callback_argument_in_call_like(initializer));
            }

            let parent = self.ctx.arena.get_extended(current)?.parent;
            if parent.is_none() {
                return None;
            }
            current = parent;
        }

        None
    }
}
