use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn retry_jsx_attr(
        &mut self,
        value_node_idx: NodeIndex,
        request: &TypingRequest,
        provided_attrs: &mut [(String, TypeId)],
    ) {
        self.invalidate_function_like_for_contextual_retry(value_node_idx);
        let _ = self.compute_type_of_node_with_request(
            value_node_idx,
            &request.read().normal_origin().contextual_opt(None),
        );
        if let Some(entry) = provided_attrs.last_mut() {
            entry.1 = TypeId::ANY;
        }
    }
}
