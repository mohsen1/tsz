use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

impl<'a> CheckerState<'a> {
    pub(super) fn call_is_property_like_with_multiple_args(&self, call_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(call_idx) else {
            return false;
        };
        let Some(call) = self.ctx.arena.get_call_expr(node) else {
            return false;
        };
        let Some(args) = call.arguments.as_ref() else {
            return false;
        };
        if args.nodes.len() <= 1 {
            return false;
        }
        self.ctx.arena.get(call.expression).is_some_and(|callee| {
            matches!(
                callee.kind,
                k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            )
        })
    }
}
