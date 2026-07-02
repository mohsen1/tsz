//! Source-file environment reconciliation before flow analysis.

use crate::state::CheckerState;

impl CheckerState<'_> {
    pub(crate) fn reconcile_flow_and_evaluator_envs(&mut self) {
        let (overlay_filled, divergences): (
            bool,
            Vec<(u32, tsz_solver::TypeId, tsz_solver::TypeId)>,
        ) = {
            let type_env_snapshot = self.ctx.type_env.borrow();
            let mut flow_env = self.ctx.type_environment.borrow_mut();
            let overlay_filled = flow_env.overlay_missing_from(&type_env_snapshot);
            (
                overlay_filled,
                flow_env.collect_def_type_divergences_from(&type_env_snapshot),
            )
        };
        if overlay_filled {
            // Every env write should reach both environments through the
            // authority helpers; the overlay is the legacy repair for writes
            // that bypassed it. Surface survivors so the repair can eventually
            // be deleted (#14348) — this firing means some write path still
            // touches only the evaluator env.
            tracing::warn!(
                target: "tsz::env_authority",
                "reconcile overlay filled flow-env entries the authority missed (#14348)"
            );
        }

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
