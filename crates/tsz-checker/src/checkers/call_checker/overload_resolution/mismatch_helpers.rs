use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    pub(super) fn recheck_overload_args_after_mismatch_without_context(
        &mut self,
        args: &[NodeIndex],
        mismatch_index: usize,
    ) {
        for &arg_idx in args.iter().skip(mismatch_index.saturating_add(1)) {
            if !self.is_callback_like_argument(arg_idx) {
                continue;
            }

            for callback_idx in self.callback_function_indices(arg_idx) {
                self.ctx
                    .implicit_any_contextual_closures
                    .remove(&callback_idx);
                self.ctx.implicit_any_checked_closures.remove(&callback_idx);
            }
            self.invalidate_expression_for_contextual_retry(arg_idx);
            let _ = self.get_type_of_node_with_request(arg_idx, &TypingRequest::NONE);
        }
    }

    pub(super) fn arg_source_span(
        &self,
        args: &[NodeIndex],
        index: usize,
    ) -> Option<tsz_solver::SourceSpan> {
        let &arg_idx = args.get(index)?;
        self.ctx.arena.get(arg_idx).map(|node| {
            tsz_solver::SourceSpan::new(
                self.ctx.file_name.as_str(),
                node.pos,
                node.end.saturating_sub(node.pos),
            )
        })
    }
}
