use crate::context::TypingRequest;
use crate::context::speculation::DiagnosticSpeculationSnapshot;
use crate::diagnostics::{Diagnostic, diagnostic_codes, diagnostic_messages, format_message};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn emit_generic_conditional_return_mismatch(
        &mut self,
        body: NodeIndex,
        actual_return: TypeId,
        expected_return_type: TypeId,
        is_async_for_context: bool,
    ) {
        let conditional_branch_mismatch = self
            .ctx
            .arena
            .get(body)
            .and_then(|body_node| self.ctx.arena.get_conditional_expr(body_node))
            .is_some_and(|cond| {
                let snap = DiagnosticSpeculationSnapshot::new(&self.ctx);
                let return_req = TypingRequest::with_contextual_type(expected_return_type);
                let mut when_true = self.get_type_of_node_with_request(cond.when_true, &return_req);
                let mut when_false =
                    self.get_type_of_node_with_request(cond.when_false, &return_req);
                snap.rollback(&mut self.ctx.diagnostic_state());
                if is_async_for_context {
                    when_true = self.unwrap_promise_type(when_true).unwrap_or(when_true);
                    when_false = self.unwrap_promise_type(when_false).unwrap_or(when_false);
                }
                !self.diagnostic_relation_boolean_guard(when_true, expected_return_type)
                    || !self.diagnostic_relation_boolean_guard(when_false, expected_return_type)
            });
        if conditional_branch_mismatch
            && !self
                .is_nested_same_wrapper_application_assignment(actual_return, expected_return_type)
            && let Some(loc) = self.get_source_location(body)
        {
            let src_str = self.format_type(actual_return);
            let tgt_str = self.format_type(expected_return_type);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            self.ctx.diagnostics.push(Diagnostic::error(
                self.ctx.file_name.clone(),
                loc.start,
                loc.length(),
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            ));
        }
    }
}
