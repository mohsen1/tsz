//! Flow-env-only mirrors for lazy `DefId` resolution writes.

use crate::context::CheckerContext;
use crate::context::deferred_flow_env_write::DeferredFlowEnvWrite;
use tsz_solver::TypeId;
use tsz_solver::def::DefId;

impl CheckerContext<'_> {
    /// Mirror a `def_id -> body` registration into the flow-analyzer env only,
    /// leaving the evaluator env untouched.
    ///
    /// Resolution-time paths that write the *evaluator* env directly (during
    /// recursive lazy resolution, where they already hold the `type_env` borrow)
    /// must keep the flow-analyzer env's `def_types` in lock-step. Unlike
    /// `register_def_*_in_envs`, this performs no evaluator-side
    /// `DefinitionStore` write or cache invalidation.
    pub fn mirror_def_to_flow_env(
        &self,
        def_id: DefId,
        body: TypeId,
        params: Vec<tsz_solver::TypeParamInfo>,
    ) {
        self.mirror_to_flow_env(DeferredFlowEnvWrite::InsertDefWithParams {
            def_id,
            body,
            params,
            variances: None,
        });
    }

    /// Mirror a class-instance-type registration into the flow-analyzer env only.
    pub fn mirror_class_instance_to_flow_env(&self, def_id: DefId, instance_type: TypeId) {
        self.mirror_to_flow_env(DeferredFlowEnvWrite::InsertClassInstance {
            def_id,
            instance_type,
        });
    }
}
