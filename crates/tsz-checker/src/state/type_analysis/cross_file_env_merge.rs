use crate::query_boundaries::common::TypeEnvironment;
use crate::state::CheckerState;
use tsz_solver::TypeId;
use tsz_solver::def::DefId;

impl<'a> CheckerState<'a> {
    pub(super) fn merge_child_type_env_snapshots(
        &self,
        child_env: &TypeEnvironment,
        context: &'static str,
    ) {
        let child_defs = child_env.snapshot_def_types();
        let child_class_instances = child_env.snapshot_class_instance_types();

        if !child_defs.is_empty() {
            if let Ok(mut parent_env) = self.ctx.type_env.try_borrow_mut() {
                for (def_id_raw, type_id) in child_defs {
                    let def_id = DefId(def_id_raw);
                    if parent_env.get_def(def_id).is_none() {
                        parent_env.insert_def(def_id, type_id);
                    }
                }
            } else {
                tracing::warn!("{context}: could not borrow parent type_env for def merge");
            }
        }

        if !child_class_instances.is_empty() {
            self.merge_class_instances_into_type_env(&child_class_instances, context);
            self.merge_class_instances_into_type_environment(&child_class_instances, context);
        }
    }

    fn merge_class_instances_into_type_env(
        &self,
        child_class_instances: &rustc_hash::FxHashMap<u32, TypeId>,
        context: &'static str,
    ) {
        if let Ok(mut parent_env) = self.ctx.type_env.try_borrow_mut() {
            for (&def_id_raw, &instance_type) in child_class_instances {
                let def_id = DefId(def_id_raw);
                if parent_env.get_class_instance_type(def_id).is_none() {
                    parent_env.insert_class_instance_type(def_id, instance_type);
                }
            }
        } else {
            tracing::warn!("{context}: could not borrow parent type_env for class-instance merge");
        }
    }

    fn merge_class_instances_into_type_environment(
        &self,
        child_class_instances: &rustc_hash::FxHashMap<u32, TypeId>,
        context: &'static str,
    ) {
        if let Ok(mut parent_env) = self.ctx.type_environment.try_borrow_mut() {
            for (&def_id_raw, &instance_type) in child_class_instances {
                let def_id = DefId(def_id_raw);
                if parent_env.get_class_instance_type(def_id).is_none() {
                    parent_env.insert_class_instance_type(def_id, instance_type);
                }
            }
        } else {
            tracing::warn!(
                "{context}: could not borrow parent type_environment for class-instance merge"
            );
        }
    }
}
