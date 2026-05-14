use crate::state::CheckerState;
use tsz_solver::TypeId;

/// A target signature can supply contextual types for `source_param_count`
/// callback parameters when it has a rest parameter (which absorbs any
/// trailing positions) or its fixed parameter list is at least that long.
pub(super) fn signature_has_param_capacity(
    params: &[tsz_solver::ParamInfo],
    source_param_count: usize,
) -> bool {
    if params.iter().any(|p| p.rest) {
        return true;
    }
    params.len() >= source_param_count
}

impl<'a> CheckerState<'a> {
    pub(super) fn evaluate_lazy_alias_for_assignability(
        &mut self,
        type_id: TypeId,
    ) -> Option<TypeId> {
        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, type_id)?;
        if !self
            .ctx
            .definition_store
            .get(def_id)
            .is_some_and(|def| def.kind == tsz_solver::def::DefKind::TypeAlias)
        {
            return None;
        }
        let body = self.ctx.definition_store.get_body(def_id)?;
        if body == TypeId::ERROR || body == TypeId::ANY || body == type_id {
            return None;
        }
        let evaluated = self.evaluate_type_with_env(type_id);
        if evaluated != TypeId::ERROR && evaluated != TypeId::ANY && evaluated != type_id {
            Some(evaluated)
        } else {
            Some(self.evaluate_type_for_assignability_inner(body))
        }
    }
}
