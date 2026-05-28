//! Legacy parity assertions for [`SkeletonIndex`].
//!
//! These methods compare skeleton-derived topology against the corresponding
//! pre-skeleton ("legacy") `MergedProgram` / per-binder projections. They are
//! debug-only: the body of each method runs under `cfg!(debug_assertions)` and
//! is a no-op in release builds.
//!
//! They are intentionally split out from `skeleton/mod.rs` so the primary
//! skeleton orchestration (extract → reduce → lookup → build projections)
//! reads as forward data flow, with retained legacy parity checks gathered in
//! one place. Each method is annotated with the test(s) that pin the retained
//! behavior so future cleanup can confirm removal is safe.
//!
//! Removal policy: a `validate_*_against_legacy` method may be deleted once
//! every test that references it (or its driver, `merge_bind_results`) is
//! removed or proven independent of the legacy-merge invariant. Until then,
//! keep the assertion: it is the only thing that detects skeleton drift from
//! the binder-side topology in debug builds.

use super::{
    FxHashMap, FxHashSet, SkeletonIndex, SkeletonModuleExportsByName, SkeletonModuleExportsIndex,
};

impl SkeletonIndex {
    /// Validate that skeleton-derived data matches the legacy `MergedProgram` state.
    ///
    /// In debug builds, asserts that:
    /// - `declared_modules` match exactly
    /// - `shorthand_ambient_modules` match exactly
    /// - `module_export_specifiers` match the keys of `module_exports`
    ///   (excluding user file names that the legacy path inserts as `module_exports` keys)
    ///
    /// This proves the skeleton captures all merge-relevant ambient module topology
    /// without retaining arenas. In release builds, this is a no-op.
    ///
    /// Pinned by (in `crates/tsz-core/tests/parallel_tests_parts/part_02.rs`):
    /// - `skeleton_validate_against_merged_declared_modules`
    /// - `skeleton_validate_against_merged_shorthand_ambient`
    /// - `skeleton_validate_against_merged_module_export_specifiers`
    /// - `skeleton_validate_mixed_ambient_and_user_files`
    ///
    /// Called from `parallel/core/bind_result_reducer.rs::merge_bind_results`.
    pub fn validate_against_merged(
        &self,
        merged_declared_modules: &FxHashSet<String>,
        merged_shorthand_ambient_modules: &FxHashSet<String>,
        merged_module_export_keys: &FxHashSet<String>,
        user_file_names: &FxHashSet<String>,
    ) {
        if cfg!(debug_assertions) {
            // 1) declared_modules must match exactly.
            assert_eq!(
                &self.declared_modules, merged_declared_modules,
                "skeleton declared_modules differs from legacy merge"
            );

            // 2) shorthand_ambient_modules must match exactly.
            assert_eq!(
                &self.shorthand_ambient_modules, merged_shorthand_ambient_modules,
                "skeleton shorthand_ambient_modules differs from legacy merge"
            );

            // 3) module_export_specifiers: both skeleton and legacy include
            //    binder-produced module_exports keys. The legacy merge also
            //    inserts user file names (from the per-file export collection).
            //    The skeleton captures the binder-level keys which may also
            //    include the file's own name for external modules. Filter user
            //    file names from both sides before comparing.
            let legacy_non_file_keys: FxHashSet<String> = merged_module_export_keys
                .iter()
                .filter(|k| !user_file_names.contains(k.as_str()))
                .cloned()
                .collect();
            let skeleton_non_file_keys: FxHashSet<String> = self
                .module_export_specifiers
                .iter()
                .filter(|k| !user_file_names.contains(k.as_str()))
                .cloned()
                .collect();
            assert_eq!(
                &skeleton_non_file_keys, &legacy_non_file_keys,
                "skeleton module_export_specifiers differs from legacy merge (after filtering user file names)"
            );
        }
    }

    /// Validate that the skeleton-derived `module_augmentations_by_spec`
    /// matches the legacy per-binder `module_augmentations` topology.
    ///
    /// `legacy_per_file` is the legacy projection of every file's
    /// `binder.module_augmentations`: a `Vec<FxHashMap<spec, Vec<aug_name>>>`
    /// in driver file order. The skeleton is expected to record, for every
    /// `(file_idx, spec)`, the same multiset of augmenting names (with
    /// matching counts).
    ///
    /// In debug builds, asserts the per-spec, per-file augmenting-name
    /// multisets are equal. In release builds this is a no-op.
    ///
    /// Pinned (indirectly) by `build_module_augmentations_index_matches_legacy_topology`
    /// in `skeleton/tests.rs` plus the `merge_bind_results` debug-assert path
    /// exercised by the parallel-tests integration suite.
    pub fn validate_module_augmentations_against_legacy(
        &self,
        legacy_per_file: &[FxHashMap<String, Vec<String>>],
    ) {
        if !cfg!(debug_assertions) {
            return;
        }

        // Build the legacy map: spec -> Vec<(file_idx, sorted_names)>
        let mut legacy: FxHashMap<String, Vec<(usize, Vec<String>)>> = FxHashMap::default();
        for (file_idx, per_file) in legacy_per_file.iter().enumerate() {
            for (spec, names) in per_file {
                let mut sorted = names.clone();
                sorted.sort();
                legacy
                    .entry(spec.clone())
                    .or_default()
                    .push((file_idx, sorted));
            }
        }
        for entries in legacy.values_mut() {
            entries.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        }

        let mut skeleton: FxHashMap<String, Vec<(usize, Vec<String>)>> = FxHashMap::default();
        for (spec, entries) in &self.module_augmentations_by_spec {
            for (file_idx, aug) in entries {
                let mut names: Vec<String> =
                    aug.declarations.iter().map(|d| d.name.clone()).collect();
                names.sort();
                skeleton
                    .entry(spec.clone())
                    .or_default()
                    .push((*file_idx, names));
            }
        }
        for entries in skeleton.values_mut() {
            entries.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        }

        assert_eq!(
            skeleton, legacy,
            "skeleton module_augmentations_by_spec differs from legacy per-binder module_augmentations"
        );
    }

    /// Validate that the skeleton-derived `augmentation_targets_by_spec`
    /// matches the legacy per-binder `augmentation_target_modules` topology.
    ///
    /// `legacy_per_file` is the legacy projection of every file's
    /// `binder.augmentation_target_modules`: a `Vec<FxHashMap<SymbolId, String>>`
    /// in driver file order. The skeleton is expected to record, for every
    /// `(file_idx, spec)`, the same multiset of `(symbol_id)` entries (with
    /// matching counts).
    ///
    /// In debug builds, asserts the per-spec, per-file `(SymbolId, file_idx)`
    /// multisets are equal. In release builds this is a no-op.
    ///
    /// Pinned (indirectly) by `build_augmentation_targets_index_matches_legacy_topology`
    /// in `skeleton/tests.rs` plus the `merge_bind_results` debug-assert path.
    pub fn validate_augmentation_targets_against_legacy(
        &self,
        legacy_per_file: &[FxHashMap<tsz_binder::SymbolId, String>],
    ) {
        if !cfg!(debug_assertions) {
            return;
        }

        // Build the legacy map: spec -> sorted Vec<(file_idx, symbol_id)>
        let mut legacy: FxHashMap<String, Vec<(usize, tsz_binder::SymbolId)>> =
            FxHashMap::default();
        for (file_idx, per_file) in legacy_per_file.iter().enumerate() {
            for (sym_id, spec) in per_file {
                legacy
                    .entry(spec.clone())
                    .or_default()
                    .push((file_idx, *sym_id));
            }
        }
        for entries in legacy.values_mut() {
            entries.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.0.cmp(&b.1.0)));
        }

        let mut skeleton: FxHashMap<String, Vec<(usize, tsz_binder::SymbolId)>> =
            FxHashMap::default();
        for (spec, entries) in &self.augmentation_targets_by_spec {
            for (file_idx, target) in entries {
                skeleton
                    .entry(spec.clone())
                    .or_default()
                    .push((*file_idx, target.symbol_id));
            }
        }
        for entries in skeleton.values_mut() {
            entries.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.0.cmp(&b.1.0)));
        }

        assert_eq!(
            skeleton, legacy,
            "skeleton augmentation_targets_by_spec differs from legacy per-binder augmentation_target_modules"
        );
    }

    /// Validate that the skeleton-derived `module_binder_index_by_spec`
    /// matches the legacy per-binder `module_exports` topology.
    ///
    /// `legacy_per_file` is the legacy projection of every file's
    /// `binder.module_exports`: a `Vec<Vec<String>>` in driver file order
    /// where the inner `Vec<String>` is the list of module-spec keys. The
    /// skeleton is expected to record, for every `(file_idx, spec)`, the
    /// same multiset of file indices (with matching counts), including the
    /// de-quoted normalized variant when it differs.
    ///
    /// In debug builds, asserts the per-spec, sorted `file_idx` vectors are
    /// equal. In release builds this is a no-op.
    ///
    /// Pinned (indirectly) by `build_module_binder_index_matches_legacy_topology`
    /// in `skeleton/tests.rs` plus the `merge_bind_results` debug-assert path.
    pub fn validate_module_binders_against_legacy(&self, legacy_per_file: &[Vec<String>]) {
        if !cfg!(debug_assertions) {
            return;
        }

        // Build the legacy map: spec -> sorted Vec<file_idx>, mirroring the
        // legacy per-binder loop's raw + normalized push behavior.
        let mut legacy: FxHashMap<String, Vec<usize>> = FxHashMap::default();
        for (file_idx, per_file) in legacy_per_file.iter().enumerate() {
            for spec in per_file {
                legacy.entry(spec.clone()).or_default().push(file_idx);
                let normalized = spec.trim_matches('"').trim_matches('\'');
                if normalized != spec {
                    legacy
                        .entry(normalized.to_string())
                        .or_default()
                        .push(file_idx);
                }
            }
        }
        for entries in legacy.values_mut() {
            entries.sort();
        }

        let mut skeleton: FxHashMap<String, Vec<usize>> = FxHashMap::default();
        for (spec, entries) in &self.module_binder_index_by_spec {
            let mut sorted = entries.clone();
            sorted.sort();
            skeleton.insert(spec.clone(), sorted);
        }

        assert_eq!(
            skeleton, legacy,
            "skeleton module_binder_index_by_spec differs from legacy per-binder module_exports"
        );
    }

    /// Validate that the skeleton-derived `module_exports_index_by_spec`
    /// matches the legacy per-binder `module_exports` topology.
    ///
    /// `legacy_per_file` is the legacy projection of every file's
    /// `binder.module_exports`: a `Vec<Vec<(String, Vec<String>)>>` in driver
    /// file order where each `(spec, export_names)` entry is the list of names
    /// from `binder.module_exports[spec]`. The skeleton is expected to record,
    /// for every `(file_idx, spec, export_name)`, the same multiset of file
    /// indices (with matching counts), including the de-quoted normalized
    /// variant when it differs.
    ///
    /// In debug builds, asserts the per-spec, per-export, sorted `file_idx`
    /// vectors are equal. In release builds this is a no-op.
    ///
    /// Pinned (indirectly) by `build_module_exports_index_resolves_sym_ids_from_merged_map`
    /// and `build_module_exports_index_drops_*` in `skeleton/tests.rs`, plus
    /// the `merge_bind_results` debug-assert path exercised by the parallel-tests
    /// integration suite.
    pub fn validate_module_exports_against_legacy(
        &self,
        legacy_per_file: &[Vec<(String, Vec<String>)>],
    ) {
        if !cfg!(debug_assertions) {
            return;
        }

        // Build the legacy map: spec -> export_name -> sorted Vec<file_idx>,
        // mirroring the legacy per-binder loop's raw + normalized push behavior.
        let mut legacy: SkeletonModuleExportsIndex = FxHashMap::default();
        for (file_idx, per_file) in legacy_per_file.iter().enumerate() {
            for (spec, names) in per_file {
                let entry = legacy.entry(spec.clone()).or_default();
                for name in names {
                    entry.entry(name.clone()).or_default().push(file_idx);
                }
                let normalized = spec.trim_matches('"').trim_matches('\'');
                if normalized != spec {
                    let entry = legacy.entry(normalized.to_string()).or_default();
                    for name in names {
                        entry.entry(name.clone()).or_default().push(file_idx);
                    }
                }
            }
        }
        for inner in legacy.values_mut() {
            for entries in inner.values_mut() {
                entries.sort();
            }
        }

        let mut skeleton: SkeletonModuleExportsIndex = FxHashMap::default();
        for (spec, by_name) in &self.module_exports_index_by_spec {
            let mut inner: SkeletonModuleExportsByName = FxHashMap::default();
            for (name, entries) in by_name {
                let mut sorted = entries.clone();
                sorted.sort();
                inner.insert(name.clone(), sorted);
            }
            skeleton.insert(spec.clone(), inner);
        }

        assert_eq!(
            skeleton, legacy,
            "skeleton module_exports_index_by_spec differs from legacy per-binder module_exports"
        );
    }
}
