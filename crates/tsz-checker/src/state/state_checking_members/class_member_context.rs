use crate::context::TypingRequest;
use crate::query_boundaries::common::ContextualTypeContext;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn contextual_class_member_type_from_request(
        &mut self,
        request: &TypingRequest,
        member_name: NodeIndex,
    ) -> Option<TypeId> {
        let ctx_type = request.contextual_type?;
        let prop_name = self.get_property_name(member_name)?;
        let resolved_ctx = self.evaluate_type_for_assignability(ctx_type);
        let ctx_helper = ContextualTypeContext::with_expected(self.ctx.types, resolved_ctx);
        ctx_helper
            .get_property_type(&prop_name)
            .filter(|&ty| ty != TypeId::ANY && !self.type_contains_error(ty))
    }
}
