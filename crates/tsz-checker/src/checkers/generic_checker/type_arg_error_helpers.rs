use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    /// Returns true when an arity diagnostic was emitted inside `type_arg_idx`.
    pub(super) fn type_arg_subtree_has_arity_error(&self, type_arg_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(type_arg_idx) else {
            return false;
        };
        let (start, end) = (node.pos, node.end);
        if end <= start {
            return false;
        }
        self.ctx
            .diagnostics
            .iter()
            .any(|d| matches!(d.code, 2314 | 2315 | 2707) && d.start >= start && d.start < end)
    }

    pub(super) fn type_arg_subtree_has_value_used_as_type_error(
        &self,
        type_arg_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(type_arg_idx) else {
            return false;
        };
        let (start, end) = (node.pos, node.end);
        if end <= start {
            return false;
        }
        let code = crate::diagnostics::diagnostic_codes::REFERS_TO_A_VALUE_BUT_IS_BEING_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF;
        self.ctx
            .diagnostics
            .iter()
            .any(|d| d.code == code && d.start >= start && d.start < end)
    }
}
