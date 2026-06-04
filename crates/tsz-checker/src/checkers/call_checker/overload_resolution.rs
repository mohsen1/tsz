//! Overload resolution for call expressions.
//!
//! Split from the parent `call_checker` module — pure code motion.

include!("overload_resolution_large_methods/resolve_overloaded_call_with_signatures_13_2.rs");

mod contextual_retry;
mod helpers;
mod return_context;

use crate::context::TypingRequest;
use crate::context::speculation::FullSnapshot;
use crate::query_boundaries::checkers::call::lazy_def_id_for_type;
use crate::query_boundaries::common::{
    CallResult, ContextualTypeContext, PendingDiagnosticBuilder,
};
use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

use super::{CallableContext, OverloadResolution, SelectedTypePredicate};

type NoReturnContextFallback = (Vec<TypeId>, TypeId, SelectedTypePredicate, FullSnapshot);
type BestTypeMismatch = (
    OverloadResolution,
    crate::context::NodeTypeCache,
    Vec<crate::diagnostics::Diagnostic>,
);

impl<'a> CheckerState<'a> {
    pub(super) fn snapshot_overload_retry_state(&mut self) -> FullSnapshot {
        self.ctx.snapshot_full()
    }

    pub(super) fn rollback_overload_retry_state(&mut self, snap: &FullSnapshot) {
        self.ctx.rollback_full(snap);
    }

    __tsz_split_overload_resolution_resolve_overloaded_call_with_signatures_13_2!();

    fn recheck_overload_args_after_mismatch_without_context(
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

    fn arg_source_span(&self, args: &[NodeIndex], index: usize) -> Option<tsz_solver::SourceSpan> {
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
