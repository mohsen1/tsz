use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn has_call_signature_for_missing_property_suppression(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::common::get_call_signatures(self.ctx.types, type_id)
            .is_some_and(|signatures| !signatures.is_empty())
    }

    fn has_construct_signature_for_missing_property_suppression(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::common::get_construct_signatures(self.ctx.types, type_id)
            .is_some_and(|signatures| !signatures.is_empty())
    }

    pub(super) fn should_suppress_missing_property_for_callable_source(
        &mut self,
        source: TypeId,
        source_type: TypeId,
        target: TypeId,
    ) -> bool {
        let source_eval = self.evaluate_type_with_env(source);
        let source_type_eval = self.evaluate_type_with_env(source_type);
        let target_eval = self.evaluate_type_with_env(target);

        let source_has_call = self.has_call_signature_for_missing_property_suppression(source)
            || self.has_call_signature_for_missing_property_suppression(source_eval)
            || self.has_call_signature_for_missing_property_suppression(source_type)
            || self.has_call_signature_for_missing_property_suppression(source_type_eval);

        let source_has_construct = self
            .has_construct_signature_for_missing_property_suppression(source)
            || self.has_construct_signature_for_missing_property_suppression(source_eval)
            || self.has_construct_signature_for_missing_property_suppression(source_type)
            || self.has_construct_signature_for_missing_property_suppression(source_type_eval);

        let target_has_call = self.has_call_signature_for_missing_property_suppression(target)
            || self.has_call_signature_for_missing_property_suppression(target_eval);

        source_has_call && !target_has_call && !source_has_construct
    }
}
