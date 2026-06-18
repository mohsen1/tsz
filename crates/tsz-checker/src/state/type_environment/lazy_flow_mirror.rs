//! Lazy-resolution writes that mirror evaluator env updates into the flow env.

use crate::state::CheckerState;
use tsz_solver::TypeId;
use tsz_solver::def::DefId;

impl<'a> CheckerState<'a> {
    /// Insert `type_id` for `def_id` into the evaluator env and mirror it into
    /// the flow-analyzer env through the deferred mirror path.
    pub(super) fn try_insert_def_in_type_env(&mut self, def_id: DefId, type_id: TypeId) {
        let params = self.ctx.get_def_type_params(def_id).unwrap_or_default();
        match self.ctx.type_env.try_borrow_mut() {
            Ok(mut env) => env.insert_def_with_params(def_id, type_id, params.clone()),
            Err(e) => tracing::warn!(
                target_env = "type_env",
                error = ?e,
                "try_insert_def_in_type_env: borrow failed; insert skipped"
            ),
        }
        self.ctx.mirror_def_to_flow_env(def_id, type_id, params);
    }
}
