//! Source-file environment reconciliation before flow analysis.

use crate::state::CheckerState;

impl CheckerState<'_> {
    pub(crate) fn reconcile_flow_and_evaluator_envs(&mut self) {
        let divergences: Vec<(u32, tsz_solver::TypeId, tsz_solver::TypeId)> = {
            let type_env_snapshot = self.ctx.type_env.borrow();
            let flow_env = self.ctx.type_environment.borrow();
            flow_env.collect_def_type_divergences_from(&type_env_snapshot)
        };

        if !divergences.is_empty() {
            self.ctx.eval_session.reset_lazy_resolution_fuel();
            self.ctx.eval_session.reset_lazy_readiness_guards();

            let mut converged: Vec<(u32, tsz_solver::TypeId)> = Vec::new();
            for (key, flow_val, eval_val) in divergences {
                if crate::query_boundaries::assignability::are_types_structurally_identical(
                    self.ctx.types,
                    &self.ctx,
                    flow_val,
                    eval_val,
                ) {
                    converged.push((key, eval_val));
                }
            }

            if !converged.is_empty() {
                let mut flow_env = self.ctx.type_environment.borrow_mut();
                for (key, eval_val) in converged {
                    flow_env.set_local_def_type(key, eval_val);
                }
            }
        }

        if cfg!(debug_assertions) {
            let type_env_snapshot = self.ctx.type_env.borrow();
            let flow_env = self.ctx.type_environment.borrow();
            if let Some((map, key)) = flow_env.first_missing_entry_from(&type_env_snapshot) {
                debug_assert!(
                    false,
                    "flow-analyzer env is missing evaluator env entry after deferred replay \
                     (#14348): {map}[{key}]"
                );
            }
            if let Some((map, key, lhs, rhs)) =
                flow_env.first_def_divergence_from(&type_env_snapshot)
            {
                debug_assert!(
                    false,
                    "flow-analyzer env diverges from evaluator env after reconciliation \
                     (#13086): {map}[{key}] is {lhs} in type_environment vs {rhs} in type_env"
                );
            }
        }
    }
}
