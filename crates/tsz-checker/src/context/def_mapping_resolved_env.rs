//! Env-only registration for already-resolved `DefId` bodies.

use crate::context::CheckerContext;
use crate::context::deferred_flow_env_write::DeferredFlowEnvWrite;
use tsz_solver::TypeId;
use tsz_solver::def::DefId;

impl<'a> CheckerContext<'a> {
    /// Register an already-resolved definition body in both type environments
    /// through the race-safe deferred-write path.
    pub(crate) fn register_resolved_def_in_envs(&self, def_id: DefId, body: TypeId) {
        let params = self.get_def_type_params(def_id).unwrap_or_default();
        if params.is_empty() {
            self.register_in_envs(DeferredFlowEnvWrite::InsertDef { def_id, body });
        } else {
            self.register_in_envs(DeferredFlowEnvWrite::InsertDefWithParams {
                def_id,
                body,
                params,
                variances: None,
            });
        }
    }
}
