//! Alias evaluated-form registration guards.

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    pub(super) fn can_register_evaluated_alias_form(
        &self,
        alias_def_id: tsz_solver::def::DefId,
        type_id: TypeId,
    ) -> bool {
        let mut pending = self.ctx.collect_lazy_def_ids_cached(type_id).to_vec();
        if pending.is_empty() {
            return true;
        }

        let mut visited = FxHashSet::default();
        let mut steps = 0usize;
        while let Some(def_id) = pending.pop() {
            if !visited.insert(def_id) {
                continue;
            }
            if def_id == alias_def_id {
                return false;
            }

            let Some(body) = self.ctx.definition_store.get_body(def_id) else {
                // Member body not set yet (e.g., forward-declared interface).
                // Skip instead of rejecting: we can still safely evaluate the
                // alias body; unresolved members will stay as Lazy and won't
                // cause incorrect registrations. The evaluation machinery has
                // its own recursion limits to prevent infinite loops.
                continue;
            };

            steps += 1;
            if steps > 64 {
                return false;
            }

            pending.extend(self.ctx.collect_lazy_def_ids_cached(body).iter().copied());
        }

        true
    }
}
