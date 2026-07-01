//! Env-only registration for already-resolved `DefId` bodies.

use crate::context::CheckerContext;
use crate::context::deferred_env_write::DeferredEnvWrite;
use tsz_solver::TypeId;
use tsz_solver::def::DefId;

impl CheckerContext<'_> {
    /// Register an already-resolved definition body in the type environment
    /// through the race-safe deferred-write path.
    pub(crate) fn register_resolved_def_in_env(&self, def_id: DefId, body: TypeId) {
        let params = self.get_def_type_params(def_id).unwrap_or_default();
        self.register_in_env(DeferredEnvWrite::insert_def_choosing_params(
            def_id, body, params, None,
        ));
    }
}
