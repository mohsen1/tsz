use crate::query_boundaries::common::TypeEnvironment;
use crate::state::CheckerState;
use tsz_solver::def::DefId;

impl<'a> CheckerState<'a> {
    pub(super) fn merge_child_type_env_snapshots(
        &self,
        child_env: &TypeEnvironment,
        _context: &'static str,
    ) {
        let child_defs = child_env.snapshot_def_types();
        let child_class_instances = child_env.snapshot_class_instance_types();
        let child_class_extends = child_env.snapshot_class_extends();

        for (def_id_raw, type_id) in child_defs {
            self.ctx
                .merge_def_if_missing_in_env(DefId(def_id_raw), type_id);
        }

        for (def_id_raw, instance_type) in child_class_instances {
            self.ctx
                .merge_class_instance_if_missing_in_env(DefId(def_id_raw), instance_type);
        }

        for (def_id_raw, parent_def_id) in child_class_extends {
            self.ctx
                .merge_class_extends_if_missing_in_env(DefId(def_id_raw), parent_def_id);
        }
    }
}
