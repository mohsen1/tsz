use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn generic_new_argument_accepts_contextual_parameter(
        &mut self,
        arg_idx: NodeIndex,
        expected: TypeId,
    ) -> bool {
        if expected == TypeId::ANY || expected == TypeId::ERROR || expected == TypeId::UNKNOWN {
            return false;
        }

        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
            return false;
        };
        if !matches!(
            arg_node.kind,
            tsz_parser::parser::syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                | tsz_parser::parser::syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
        ) {
            return false;
        }

        let request = TypingRequest::with_contextual_type(expected);
        let contextual_actual = self.speculative_type_of_node(arg_idx, &request);

        contextual_actual != TypeId::ANY
            && contextual_actual != TypeId::ERROR
            && self.is_assignable_to(contextual_actual, expected)
    }

    pub(crate) fn recover_new_expression_return_type_after_contextual_argument_match(
        &mut self,
        constructor_type: TypeId,
        fallback_return: TypeId,
    ) -> TypeId {
        if fallback_return != TypeId::ERROR {
            fallback_return
        } else {
            self.instance_type_from_constructor_type(constructor_type)
                .unwrap_or(TypeId::ERROR)
        }
    }
}
