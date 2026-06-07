use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) const fn function_body_statement_request(
        &self,
        body_is_expression: bool,
        effective_body_ctx: Option<TypeId>,
    ) -> TypingRequest {
        if body_is_expression {
            TypingRequest::NONE.contextual_opt(effective_body_ctx)
        } else {
            TypingRequest::NONE
        }
    }

    pub(super) fn check_function_body_statement_with_own_literal_context(
        &mut self,
        body: NodeIndex,
        body_request: &TypingRequest,
    ) {
        // A function body makes its own literal-widening decisions. When this
        // closure is type-checked as a generic-call argument, the call's
        // argument collection may leave `preserve_literal_types` set for
        // inference. That flag must not leak into body-local declarations; return
        // statements re-establish their own preservation policy separately.
        let saved_preserve_literals = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = false;
        self.check_statement_with_request(body, body_request);
        self.ctx.preserve_literal_types = saved_preserve_literals;
    }
}
