use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn type_arg_reference_form(&self, type_arg: TypeId) -> TypeId {
        let db = self.ctx.types.as_type_database();
        if crate::query_boundaries::common::lazy_def_id(db, type_arg).is_some() {
            return type_arg;
        }

        let store = &self.ctx.definition_store;
        let def_id = store
            .find_def_for_type(type_arg)
            .or_else(|| store.find_def_for_type(db.get_display_alias(type_arg)?));
        match def_id {
            Some(def_id)
                if store
                    .get(def_id)
                    .is_some_and(|def| def.type_params.is_empty()) =>
            {
                self.ctx.types.factory().lazy(def_id)
            }
            _ => type_arg,
        }
    }
}
