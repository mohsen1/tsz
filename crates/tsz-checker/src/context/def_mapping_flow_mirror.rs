//! Flow-env-only mirrors for lazy `DefId` resolution writes.

use crate::context::CheckerContext;
use crate::context::deferred_flow_env_write::DeferredFlowEnvWrite;
use tsz_solver::TypeId;
use tsz_solver::def::DefId;

impl CheckerContext<'_> {
    /// Mirror a definition body into the flow-analyzer env (`type_environment`)
    /// **only**, deferring on a borrow race so the write is replayed at
    /// [`Self::flush_deferred_flow_env_writes`] rather than dropped.
    ///
    /// The `get_type_of_symbol` epilogue writes the authoritative body into the
    /// evaluator env (`type_env`) directly while it already holds that env's
    /// borrow; this keeps the flow-analyzer env in lock-step with that write.
    pub fn mirror_def_in_type_environment(
        &self,
        def_id: DefId,
        body: TypeId,
        params: &[tsz_solver::TypeParamInfo],
    ) {
        // Variances are left unset here (`None`), matching the prior direct
        // `insert_def_with_params` mirror at this site.
        let op =
            DeferredFlowEnvWrite::insert_def_choosing_params(def_id, body, params.to_vec(), None);
        self.mirror_to_flow_env(op);
    }

    /// Mirror a class instance type into the flow-analyzer env **only**,
    /// deferring on a borrow race.
    pub fn mirror_class_instance_in_type_environment(&self, def_id: DefId, instance_type: TypeId) {
        self.mirror_to_flow_env(DeferredFlowEnvWrite::InsertClassInstance {
            def_id,
            instance_type,
        });
    }
}
