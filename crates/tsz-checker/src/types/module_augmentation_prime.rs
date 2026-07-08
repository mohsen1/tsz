//! Once-per-program priming for cross-file module augmentation bodies.
//!
//! The ordinary augmentation merge path is demand driven: whichever file first
//! resolves the home interface can publish an empty pre-merge body before a
//! sibling augmentation is checked. Project schedulers call this pre-pass after
//! global indices are installed and before per-file checking, so deferred
//! `Lazy(DefId)` consumers see the merged registry body deterministically.

use crate::query_boundaries::state::type_environment;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_solver::TypeId;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ModuleAugmentationPrimeTask {
    augmenting_file_idx: usize,
    module_spec: String,
    interface_name: String,
}

impl<'a> CheckerState<'a> {
    /// Prime merged bodies for every cross-file module augmentation registered
    /// in the project before ordinary per-file checking starts.
    ///
    /// Structural rule: when a module augmentation targets an exported interface
    /// from another file, `tsc` makes the merged body visible to later indexed
    /// access, `keyof`, and assignability reads. `tsz` owns that publication in
    /// the checker, by resolving the target from the augmenting file and writing
    /// the merged body through the existing `DefId`/definition-store channels.
    pub fn prime_module_augmentation_bodies(&mut self) {
        if !self.ctx.program_has_module_augmentations() {
            return;
        }

        let tasks = self.module_augmentation_prime_tasks();
        if tasks.is_empty() {
            return;
        }

        let saved_diagnostics = std::mem::take(&mut self.ctx.diagnostics);
        let original_file_idx = self.ctx.current_file_idx;
        let original_file_name = self.ctx.file_name.clone();
        let mut primed = 0usize;

        for task in tasks {
            self.ctx.set_current_file_idx(task.augmenting_file_idx);
            if let Some(file_name) = self
                .ctx
                .get_arena_for_file(task.augmenting_file_idx as u32)
                .source_files
                .first()
                .map(|source_file| source_file.file_name.clone())
            {
                self.ctx.file_name = file_name;
            } else {
                self.ctx.file_name = original_file_name.clone();
            }

            let Some(target_sym) = self.resolve_cross_file_export_from_file(
                &task.module_spec,
                &task.interface_name,
                Some(task.augmenting_file_idx),
            ) else {
                continue;
            };

            // Priming publishes the merged INSTANCE-side body under the
            // target's DefId. A class target shares that DefId between its
            // static (value) and instance sides, so publishing here replaces
            // the static surface — `Observable.prototype` then misses class
            // statics (moduleAugmentationExtendFileModule1/2). Classes keep
            // the demand-driven merge path.
            if self
                .ctx
                .binder
                .get_symbol(target_sym)
                .is_some_and(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::CLASS))
            {
                continue;
            }

            let base_type = self.type_reference_symbol_type(target_sym);
            if matches!(base_type, TypeId::ERROR | TypeId::UNKNOWN) {
                continue;
            }

            let merged_type =
                self.apply_module_augmentations(&task.module_spec, &task.interface_name, base_type);
            if matches!(merged_type, TypeId::ERROR | TypeId::UNKNOWN) {
                continue;
            }

            self.publish_primed_module_augmentation_body(target_sym, merged_type);
            primed += 1;
        }

        self.ctx.set_current_file_idx(original_file_idx);
        self.ctx.file_name = original_file_name;
        self.ctx.diagnostics = saved_diagnostics;

        if primed != 0 {
            tracing::debug!(primed, "primed module augmentation bodies");
        }
    }

    fn module_augmentation_prime_tasks(&self) -> Vec<ModuleAugmentationPrimeTask> {
        let mut tasks = Vec::new();
        let mut seen = FxHashSet::default();

        let mut push_task =
            |augmenting_file_idx: usize, module_spec: &str, interface_name: &str| {
                if interface_name.is_empty()
                    || !seen.insert((
                        augmenting_file_idx,
                        module_spec.to_string(),
                        interface_name.to_string(),
                    ))
                {
                    return;
                }
                tasks.push(ModuleAugmentationPrimeTask {
                    augmenting_file_idx,
                    module_spec: module_spec.to_string(),
                    interface_name: interface_name.to_string(),
                });
            };

        if let Some(index) = self.ctx.global_module_augmentations_index.as_ref() {
            for (module_spec, entries) in index.iter() {
                for (file_idx, aug) in entries {
                    push_task(*file_idx, module_spec, &aug.name);
                }
            }
        } else if let Some(all_binders) = self.ctx.all_binders.as_ref() {
            for (file_idx, binder) in all_binders.iter().enumerate() {
                for (module_spec, augmentations) in binder.module_augmentations.iter() {
                    for aug in augmentations {
                        push_task(file_idx, module_spec, &aug.name);
                    }
                }
            }
        } else {
            for (module_spec, augmentations) in self.ctx.binder.module_augmentations.iter() {
                for aug in augmentations {
                    push_task(self.ctx.current_file_idx, module_spec, &aug.name);
                }
            }
        }

        tasks.sort();
        tasks
    }

    fn publish_primed_module_augmentation_body(
        &mut self,
        target_sym: tsz_binder::SymbolId,
        merged_type: TypeId,
    ) {
        let def_id = self.ctx.get_or_create_def_id(target_sym);
        let type_params = self.ctx.get_def_type_params(def_id).unwrap_or_default();
        self.ctx
            .register_def_auto_params_in_envs(def_id, merged_type, type_params);
        self.ctx
            .definition_store
            .register_type_to_def(merged_type, def_id);

        if let Some(shape) = type_environment::object_shape(self.ctx.types, merged_type) {
            self.ctx.definition_store.set_instance_shape(def_id, shape);
        }
    }
}
