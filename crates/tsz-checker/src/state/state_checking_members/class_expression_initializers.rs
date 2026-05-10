use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_direct_class_expression_initializer(
        &mut self,
        initializer: NodeIndex,
        request: &TypingRequest,
    ) {
        let Some(node) = self.ctx.arena.get(initializer) else {
            return;
        };
        if node.kind != syntax_kind_ext::CLASS_EXPRESSION {
            return;
        }
        let Some(class) = self.ctx.arena.get_class(node).cloned() else {
            return;
        };

        self.check_class_expression_with_request(initializer, &class, request);
    }
}
