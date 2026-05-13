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
        // get_type_of_node_with_request already checks class-expression members on the
        // normal path. Only run this direct check when the dispatch path skipped
        // member checking due constructor-resolution re-entrancy.
        let is_reentrant = self
            .ctx
            .binder
            .get_node_symbol(initializer)
            .is_some_and(|sym_id| self.ctx.class_constructor_resolution_set.contains(&sym_id));
        if !is_reentrant {
            return;
        }
        let Some(class) = self.ctx.arena.get_class(node).cloned() else {
            return;
        };

        self.check_class_expression_with_request(initializer, &class, request);
    }
}
