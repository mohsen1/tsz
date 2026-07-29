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

type OwnerQualifiedSymbol = (usize, tsz_binder::SymbolId);

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
        let mut primed_targets = FxHashSet::default();

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

            let Some(target) = self.resolve_module_augmentation_prime_target(
                &task.module_spec,
                &task.interface_name,
                task.augmenting_file_idx,
            ) else {
                continue;
            };
            let (target_file_idx, target_sym) = target;
            let target_key = (target_file_idx, target_sym, task.interface_name.clone());
            if primed_targets.contains(&target_key) {
                continue;
            }

            // Priming publishes the merged INSTANCE-side body under the
            // target's DefId. A class target shares that DefId between its
            // static (value) and instance sides, so publishing here replaces
            // the static surface — `Observable.prototype` then misses class
            // statics (moduleAugmentationExtendFileModule1/2). Classes keep
            // the demand-driven merge path.
            if self
                .ctx
                .get_binder_for_file(target_file_idx)
                .and_then(|binder| binder.get_symbol(target_sym))
                .is_some_and(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::CLASS))
            {
                continue;
            }

            // `type_reference_symbol_type` already applies import/self module
            // augmentations. Folding again here made N augmenting files perform
            // N+1 full-program merges and changed overload group order.
            let bailout_epoch_before = Self::cross_arena_bailout_epoch();
            let previous_owner = self.ctx.local_symbol_file_target_override(target_sym);
            self.ctx
                .register_symbol_file_target(target_sym, target_file_idx);
            let merged_type = self.type_reference_symbol_type(target_sym);
            self.ctx
                .restore_local_symbol_file_target_override(target_sym, previous_owner);
            if Self::cross_arena_bailout_epoch() != bailout_epoch_before
                || matches!(merged_type, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN)
            {
                continue;
            }

            if self.publish_primed_module_augmentation_body(
                target,
                &task.interface_name,
                merged_type,
            ) {
                primed_targets.insert(target_key);
                primed += 1;
            }
        }

        self.ctx.set_current_file_idx(original_file_idx);
        self.ctx.file_name = original_file_name;
        self.ctx.diagnostics = saved_diagnostics;

        if primed != 0 {
            tracing::debug!(primed, "primed module augmentation bodies");
        }
    }

    fn resolve_module_augmentation_prime_target(
        &self,
        module_spec: &str,
        interface_name: &str,
        augmenting_file_idx: usize,
    ) -> Option<OwnerQualifiedSymbol> {
        let target_sym = self.resolve_cross_file_export_from_file(
            module_spec,
            interface_name,
            Some(augmenting_file_idx),
        )?;

        // Export resolution records the exact owner in the checker-local
        // overlay. Raw `SymbolId`s are binder-local, while the immutable
        // program index necessarily picks only one file for each raw number;
        // under a collision that lossy entry can name an unrelated symbol.
        // Consume the exact dynamic owner first, retaining the immutable owner
        // only as a fallback for resolution paths that did not need to record
        // a dynamic edge.
        let target_file_idx = self
            .ctx
            .resolve_dynamic_symbol_file_index(target_sym)
            .or_else(|| self.ctx.resolve_symbol_declaring_file_index(target_sym))?;
        self.ctx
            .get_binder_for_file(target_file_idx)?
            .get_symbol(target_sym)?;
        Some((target_file_idx, target_sym))
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
        target: OwnerQualifiedSymbol,
        interface_name: &str,
        merged_type: TypeId,
    ) -> bool {
        let (target_file_idx, target_sym) = target;
        let Some(def_id) =
            self.ctx
                .def_id_for_declaration_in_file(target_sym, target_file_idx, interface_name)
        else {
            return false;
        };
        let type_params = self.ctx.get_def_type_params(def_id).unwrap_or_default();
        self.ctx
            .register_def_auto_params_in_envs(def_id, merged_type, type_params);
        self.ctx
            .definition_store
            .register_type_to_def(merged_type, def_id);

        if let Some(shape) = type_environment::object_shape(self.ctx.types, merged_type) {
            self.ctx.definition_store.set_instance_shape(def_id, shape);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{CheckerContext, CheckerOptions};
    use crate::module_resolution::build_module_resolution_maps;
    use std::sync::Arc;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;
    use tsz_solver::construction::TypeInterner;

    fn parse_and_bind(
        file_name: &str,
        source: &str,
    ) -> (
        Arc<tsz_parser::parser::NodeArena>,
        Arc<tsz_binder::BinderState>,
    ) {
        let mut parser = ParserState::new(file_name.to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        (Arc::new(parser.into_arena()), Arc::new(binder))
    }

    #[test]
    fn prime_target_prefers_exact_dynamic_owner_for_colliding_raw_symbol_id() {
        let fixtures = [
            ("decoy.ts", "export interface Decoy { decoy: true }"),
            ("home.ts", "export interface Target { home: true }"),
            (
                "augmentation.ts",
                "import './home'; declare module './home' { interface Target { augmented: true } }",
            ),
        ];
        let mut arenas = Vec::new();
        let mut binders = Vec::new();
        for (file_name, source) in fixtures {
            let (arena, binder) = parse_and_bind(file_name, source);
            arenas.push(arena);
            binders.push(binder);
        }
        let all_arenas = Arc::new(arenas);
        let all_binders = Arc::new(binders);
        let decoy_sym = all_binders[0]
            .file_locals
            .get("Decoy")
            .expect("decoy interface symbol");
        let target_sym = all_binders[1]
            .file_locals
            .get("Target")
            .expect("target interface symbol");
        assert_eq!(
            decoy_sym, target_sym,
            "fixture must reuse one raw SymbolId in distinct binders"
        );

        let file_names = vec![
            "decoy.ts".to_string(),
            "home.ts".to_string(),
            "augmentation.ts".to_string(),
        ];
        let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);
        let types = TypeInterner::new();
        let mut checker = CheckerState {
            ctx: CheckerContext::new(
                all_arenas[2].as_ref(),
                all_binders[2].as_ref(),
                &types,
                file_names[2].clone(),
                CheckerOptions::default(),
            ),
        };
        checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
        checker.ctx.set_all_binders(Arc::clone(&all_binders));
        checker.ctx.set_current_file_idx(2);
        checker
            .ctx
            .set_resolved_module_paths(Arc::new(resolved_module_paths));
        checker.ctx.set_resolved_modules(resolved_modules);

        let mut global_symbol_file_index = rustc_hash::FxHashMap::default();
        for (file_idx, binder) in all_binders.iter().enumerate() {
            for symbol in binder.symbols.iter() {
                global_symbol_file_index
                    .entry(symbol.id)
                    .or_insert(file_idx);
            }
        }
        checker
            .ctx
            .set_global_symbol_file_index(Arc::new(global_symbol_file_index));
        assert_eq!(
            checker.ctx.resolve_symbol_declaring_file_index(target_sym),
            Some(0),
            "the immutable raw-id index should expose the intentional collision"
        );

        let target = checker
            .resolve_module_augmentation_prime_target("./home", "Target", 2)
            .expect("owner-qualified priming target");
        assert_eq!(target, (1, target_sym));
        assert_eq!(
            checker.ctx.resolve_dynamic_symbol_file_index(target_sym),
            Some(1),
            "export resolution must record the exact target binder"
        );
        assert_eq!(
            checker
                .ctx
                .get_binder_for_file(target.0)
                .and_then(|binder| binder.get_symbol(target.1))
                .map(|symbol| symbol.escaped_name.as_str()),
            Some("Target"),
            "priming must carry the owner-qualified pair rather than read the decoy binder"
        );
    }
}
