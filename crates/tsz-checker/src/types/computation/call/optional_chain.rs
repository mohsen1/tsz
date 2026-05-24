use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;

impl<'a> CheckerState<'a> {
    pub(super) fn call_expression_is_optional_chain(
        &self,
        call_node: &Node,
        callee_expr: NodeIndex,
    ) -> bool {
        call_node.is_optional_chain()
            || crate::types_domain::computation::access::is_optional_chain(
                self.ctx.arena,
                callee_expr,
            )
    }
}
