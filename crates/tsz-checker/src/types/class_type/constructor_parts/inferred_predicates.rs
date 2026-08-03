use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::MethodDeclData;
use tsz_solver::{ParamInfo, TypeId, TypePredicate};

impl<'a> CheckerState<'a> {
    pub(super) fn method_return_type_and_predicate_for_class_summary(
        &mut self,
        method_idx: NodeIndex,
        method: &MethodDeclData,
        params: &[ParamInfo],
    ) -> (TypeId, Option<TypePredicate>) {
        let (return_type, mut type_predicate) = if method.type_annotation.is_none()
            && method.body != NodeIndex::NONE
        {
            (
                self.infer_return_type_from_body(method_idx, method.body, None),
                None,
            )
        } else {
            self.return_type_and_predicate(method.type_annotation, params, &method.parameters.nodes)
        };
        if type_predicate.is_none()
            && method.type_annotation.is_none()
            && matches!(return_type, TypeId::BOOLEAN | TypeId::UNKNOWN)
            && method.body != NodeIndex::NONE
        {
            self.prewarm_inferred_predicate_operand_types(method.body);
            if let Some(pred) = self.flow_analyzer().try_infer_type_predicate_from_body(
                method.body,
                &method.parameters.nodes,
                params,
            ) {
                type_predicate = Some(pred);
            }
        }
        (return_type, type_predicate)
    }
}
