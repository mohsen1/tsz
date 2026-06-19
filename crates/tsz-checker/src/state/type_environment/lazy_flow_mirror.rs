//! Lazy-resolution writes that mirror evaluator env updates into the flow env.

use crate::state::CheckerState;
use tsz_solver::TypeId;
use tsz_solver::def::DefId;

impl CheckerState<'_> {
    /// Insert `type_id` for `def_id` into both type environments through the
    /// race-safe deferred-write path.
    pub(super) fn try_insert_def_in_type_env(&mut self, def_id: DefId, type_id: TypeId) {
        self.ctx.register_resolved_def_in_envs(def_id, type_id);
    }

    pub(super) fn mirror_application_def_resolution(
        &self,
        def_id: Option<DefId>,
        resolved: TypeId,
        type_params: &[tsz_solver::TypeParamInfo],
    ) {
        if let Some(def_id) = def_id {
            self.ctx
                .mirror_def_in_type_environment(def_id, resolved, type_params);
        }
    }
}
