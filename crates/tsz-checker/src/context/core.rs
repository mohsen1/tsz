//! Core implementation for `TypeCache` and `CheckerContext`.
//!
//! Contains the `impl` blocks and methods extracted from `mod.rs` to keep
//! the module entry point focused on type/struct definitions.

use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use std::sync::Arc;

use crate::control_flow::FlowGraph;
use crate::diagnostics::{Diagnostic, diagnostic_codes};
use crate::module_resolution::module_specifier_candidates;
use tsz_binder::{BinderState, SymbolId};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_solver::TypeId;

use super::{CheckerContext, LibContext, ResolutionError, ResolutionModeOverride, TypeCache};

impl TypeCache {
    /// Invalidate cached symbol types that depend on the provided roots.
    /// Returns the number of affected symbols.
    pub fn invalidate_symbols(&mut self, roots: &[SymbolId]) -> usize {
        if roots.is_empty() {
            return 0;
        }

        let mut reverse: FxHashMap<SymbolId, Vec<SymbolId>> = FxHashMap::default();
        for (symbol, deps) in &self.symbol_dependencies {
            for dep in deps {
                reverse.entry(*dep).or_default().push(*symbol);
            }
        }

        let mut affected: FxHashSet<SymbolId> = FxHashSet::default();
        let mut pending = VecDeque::new();
        for &root in roots {
            if affected.insert(root) {
                pending.push_back(root);
            }
        }

        while let Some(sym_id) = pending.pop_front() {
            if let Some(dependents) = reverse.get(&sym_id) {
                for &dependent in dependents {
                    if affected.insert(dependent) {
                        pending.push_back(dependent);
                    }
                }
            }
        }

        for sym_id in &affected {
            self.symbol_types.remove(sym_id);
            self.symbol_instance_types.remove(sym_id);
            self.symbol_dependencies.remove(sym_id);
        }
        self.node_types.clear();
        self.class_instance_type_cache.clear();
        self.class_constructor_type_cache.clear();
        self.class_instance_type_to_decl.clear();
        affected.len()
    }

    /// Merge another `TypeCache` into this one.
    /// Used to accumulate type information from multiple file checks for declaration emit.
    pub fn merge(&mut self, other: Self) {
        self.symbol_types.extend(other.symbol_types);
        self.symbol_instance_types
            .extend(other.symbol_instance_types);
        self.node_types.extend(other.node_types.iter());
        self.class_instance_type_to_decl
            .extend(other.class_instance_type_to_decl);
        self.class_instance_type_cache
            .extend(other.class_instance_type_cache);
        self.class_constructor_type_cache
            .extend(other.class_constructor_type_cache);
        self.type_only_nodes.extend(other.type_only_nodes);
        self.namespace_module_names
            .extend(other.namespace_module_names);

        // Merge symbol dependencies sets
        for (sym, deps) in other.symbol_dependencies {
            self.symbol_dependencies
                .entry(sym)
                .or_default()
                .extend(deps);
        }

        // Merge def_to_symbol mapping
        self.def_to_symbol.extend(other.def_to_symbol);
    }
}

impl<'a> CheckerContext<'a> {
    /// Resolve a `SymbolId` to its owning file index.
    ///
    /// Checks the shared `global_symbol_file_index` first (pre-built, read-only,
    /// no RefCell overhead), then falls back to the local `cross_file_symbol_targets`
    /// overlay for dynamically-discovered mappings. Returns `None` if the symbol
    /// has no known cross-file owner.
    pub fn resolve_symbol_file_index(&self, sym_id: SymbolId) -> Option<usize> {
        // Check shared base map first (covers all pre-computed entries, no RefCell cost)
        if let Some(&idx) = self
            .global_symbol_file_index
            .as_ref()
            .and_then(|map| map.get(&sym_id))
        {
            return Some(idx);
        }
        // Fall back to local overlay (dynamically discovered during this check)
        self.cross_file_symbol_targets
            .borrow()
            .get(&sym_id)
            .copied()
    }

    /// Check whether a `SymbolId` has a known cross-file owner.
    pub fn has_symbol_file_index(&self, sym_id: SymbolId) -> bool {
        self.global_symbol_file_index
            .as_ref()
            .is_some_and(|map| map.contains_key(&sym_id))
            || self
                .cross_file_symbol_targets
                .borrow()
                .contains_key(&sym_id)
    }

    /// Register a dynamically-discovered `SymbolId` → file index mapping
    /// in the local overlay.
    pub fn register_symbol_file_target(&self, sym_id: SymbolId, file_idx: usize) {
        self.cross_file_symbol_targets
            .borrow_mut()
            .insert(sym_id, file_idx);
    }

    pub fn register_symbol_file_index(&self, sym_id: SymbolId, file_idx: usize) {
        self.register_symbol_file_target(sym_id, file_idx);
    }

    /// Copy the local overlay of symbol-file targets to a child checker context.
    ///
    /// This copies only the dynamically-discovered overlay entries, NOT the
    /// entries from `global_symbol_file_index` (which is already shared via
    /// `copy_cross_file_state_from`). This makes child-checker creation O(D)
    /// where D = number of dynamically discovered entries, instead of O(N)
    /// where N = total entries (base + dynamic).
    pub fn copy_symbol_file_targets_to(&self, child: &mut CheckerContext<'_>) {
        let overlay = self.cross_file_symbol_targets.borrow();
        if !overlay.is_empty() {
            *child.cross_file_symbol_targets.borrow_mut() = overlay.clone();
        }
    }

    /// Merge the child checker's local overlay back into this context.
    ///
    /// After a child checker finishes, any new dynamically-discovered mappings
    /// it found are merged back into the parent's overlay.
    pub fn merge_symbol_file_targets_from(&self, child: &CheckerContext<'_>) {
        let child_overlay = child.cross_file_symbol_targets.borrow();
        if !child_overlay.is_empty() {
            let mut parent_overlay = self.cross_file_symbol_targets.borrow_mut();
            for (&sym_id, &file_idx) in child_overlay.iter() {
                parent_overlay.insert(sym_id, file_idx);
            }
        }
    }

    /// Check whether any symbol-file targets exist (overlay or global).
    pub fn has_any_symbol_file_targets(&self) -> bool {
        self.global_symbol_file_index
            .as_ref()
            .is_some_and(|map| !map.is_empty())
            || !self.cross_file_symbol_targets.borrow().is_empty()
    }

    /// Set the shared read-only symbol→file index.
    ///
    /// This replaces the per-checker O(N) loop that called `register_symbol_file_target`
    /// for each pre-computed entry. The `Arc` map is shared across all checkers (O(1) clone).
    /// Dynamically-discovered mappings still go through `register_symbol_file_target`
    /// into the local `cross_file_symbol_targets` overlay.
    pub fn set_global_symbol_file_index(&mut self, index: Arc<FxHashMap<SymbolId, usize>>) {
        self.global_symbol_file_index = Some(index);
    }

    /// Set lib contexts for global type resolution.
    /// Note: `lib_contexts` may include both actual lib files AND user files for cross-file
    /// resolution. Use `set_actual_lib_file_count()` to track how many are actual lib files.
    pub fn set_lib_contexts(&mut self, lib_contexts: Vec<LibContext>) {
        self.lib_binders_cached = lib_contexts
            .iter()
            .map(|lc| Arc::clone(&lc.binder))
            .collect();
        self.lib_contexts = lib_contexts;
    }

    /// Set the count of actual lib files loaded (not including user files).
    /// This is used by `has_lib_loaded()` to correctly determine if standard library is available.
    /// Also updates the capabilities matrix `has_lib` flag.
    pub const fn set_actual_lib_file_count(&mut self, count: usize) {
        self.actual_lib_file_count = count;
        // Update the precomputed capabilities matrix
        let has_lib = !self.compiler_options.no_lib && count > 0;
        self.capabilities.has_lib = has_lib;
    }

    /// Record whether a project-local `@typescript/lib-dom` replacement package was loaded
    /// and which common globals it explicitly provides.
    pub const fn set_typescript_dom_replacement_globals(
        &mut self,
        loaded: bool,
        has_window: bool,
        has_self: bool,
    ) {
        self.typescript_dom_replacement_loaded = loaded;
        self.typescript_dom_replacement_has_window = has_window;
        self.typescript_dom_replacement_has_self = has_self;
    }

    /// Set all arenas for cross-file resolution.
    pub fn set_all_arenas(&mut self, arenas: Arc<Vec<Arc<NodeArena>>>) {
        // Build module specifiers map from arena file names.
        // Each file (other than the current file) gets its name stem as the module specifier.
        // This enables import-qualified type display like `import("a").F`.
        self.module_specifiers = Self::build_module_specifiers(&arenas);
        self.all_arenas = Some(arenas);
    }

    /// Build a mapping from `file_id` -> module specifier for import-qualified type display.
    /// Returns `file_idx -> stem` for each source file in the arenas.
    fn build_module_specifiers(arenas: &[Arc<NodeArena>]) -> FxHashMap<u32, String> {
        let mut map = FxHashMap::default();
        for (idx, arena) in arenas.iter().enumerate() {
            for sf in &arena.source_files {
                let file_name = &sf.file_name;
                // Strip .ts/.tsx/.d.ts/.js/.jsx extension to get the module specifier
                let specifier = Self::strip_ts_extension(file_name);
                // Use just the filename component (without directory path) to match tsc's
                // diagnostic display. tsc shows `import("a").F` not `import("/full/path/a").F`.
                let basename = specifier
                    .rsplit_once('/')
                    .map(|(_, name)| name)
                    .unwrap_or(specifier);
                map.insert(idx as u32, basename.to_string());
            }
        }
        map
    }

    /// Strip TypeScript/JavaScript extension from a file path to get the module specifier.
    fn strip_ts_extension(path: &str) -> &str {
        // Check extensions in order: longer extensions first to avoid partial matches
        for ext in &[
            ".d.ts", ".d.tsx", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx",
            ".mjs", ".cjs",
        ] {
            if let Some(stripped) = path.strip_suffix(ext) {
                return stripped;
            }
        }
        path
    }

    /// Pre-populate `global_declared_modules` from skeleton-derived data.
    ///
    /// When called before `set_all_binders`, this avoids the O(N) binder scan
    /// for declared modules — the skeleton already captured all `module_exports`
    /// keys, `declared_modules`, and `shorthand_ambient_modules` during the parallel
    /// parse/bind phase.
    ///
    /// If `global_declared_modules` is already `Some` when `set_all_binders` runs,
    /// the binder-scanning loop for declared modules is skipped entirely.
    ///
    /// The caller should compute `GlobalDeclaredModules` once from
    /// `SkeletonIndex::build_declared_module_sets()` and wrap it in an `Arc` so
    /// multiple checkers can share the same allocation.
    pub fn set_declared_modules_from_skeleton(
        &mut self,
        declared_modules: Arc<super::GlobalDeclaredModules>,
    ) {
        self.global_declared_modules = Some(declared_modules);
    }

    /// Pre-populate `global_expando_index` from skeleton-derived data.
    ///
    /// When called before `set_all_binders`, this avoids the O(N) binder scan
    /// for expando property assignments — the skeleton already captured all
    /// `expando_properties` during the parallel parse/bind phase and the
    /// `SkeletonIndex` merged them across files.
    ///
    /// If `global_expando_index` is already `Some` when `set_all_binders` runs,
    /// the binder-scanning loop for expando properties is skipped entirely.
    pub fn set_expando_index_from_skeleton(
        &mut self,
        expando_index: Arc<FxHashMap<String, FxHashSet<String>>>,
    ) {
        self.global_expando_index = Some(expando_index);
    }

    /// Copy all pre-built global indices from another `CheckerContext`.
    ///
    /// This should be called when creating nested cross-file checkers to ensure
    /// they inherit the O(1) lookup indices built by `set_all_binders`. Without
    /// this, nested checkers fall back to O(N) `all_binders` scans.
    ///
    /// Copies all 6 global indices plus `all_arenas`, `all_binders`,
    /// `resolved_module_paths`, and `module_specifiers`.
    pub fn copy_cross_file_state_from(&mut self, parent: &CheckerContext<'_>) {
        self.all_arenas = parent.all_arenas.clone();
        self.all_binders = parent.all_binders.clone();
        self.report_unresolved_imports = parent.report_unresolved_imports;
        self.resolved_modules = parent.resolved_modules.clone();
        self.global_file_locals_index = parent.global_file_locals_index.clone();
        self.global_module_exports_index = parent.global_module_exports_index.clone();
        self.global_declared_modules = parent.global_declared_modules.clone();
        self.global_expando_index = parent.global_expando_index.clone();
        self.global_module_augmentations_index = parent.global_module_augmentations_index.clone();
        self.global_augmentation_targets_index = parent.global_augmentation_targets_index.clone();
        self.global_module_binder_index = parent.global_module_binder_index.clone();
        self.global_arena_index = parent.global_arena_index.clone();
        self.global_symbol_file_index = parent.global_symbol_file_index.clone();
        self.resolved_module_paths = parent.resolved_module_paths.clone();
        self.resolved_module_errors = parent.resolved_module_errors.clone();
        self.module_specifiers = parent.module_specifiers.clone();
        self.is_external_module_by_file = parent.is_external_module_by_file.clone();
        self.file_is_esm_map = parent.file_is_esm_map.clone();
    }

    /// Set all binders for cross-file resolution.
    ///
    /// Also builds the `global_file_locals_index` and `global_module_exports_index`
    /// so that subsequent cross-file symbol lookups are O(1) instead of O(N).
    ///
    /// If `global_declared_modules` was already populated (e.g., via
    /// `set_declared_modules_from_skeleton`), the declared-modules binder scan
    /// is skipped — the skeleton-derived data is used instead.
    pub fn set_all_binders(&mut self, binders: Arc<Vec<Arc<BinderState>>>) {
        // If the 5 name-based global indices are already pre-populated (from ProjectEnv),
        // skip the O(N) binder scans entirely. This is the fast path for multi-file
        // checking where ProjectEnv::build_global_indices was called once at the driver level.
        // Note: global_arena_index, global_declared_modules, and global_expando_index
        // are handled separately below (they're built on demand if not pre-set).
        let has_prebuilt_indices = self.global_file_locals_index.is_some()
            && self.global_module_exports_index.is_some()
            && self.global_module_augmentations_index.is_some()
            && self.global_augmentation_targets_index.is_some()
            && self.global_module_binder_index.is_some();

        if has_prebuilt_indices {
            // Indices already set — just store the binders and handle remaining
            // non-indexed data (declared_modules, expando) if needed.
            if self.global_declared_modules.is_none() {
                let mut dm = super::GlobalDeclaredModules::default();
                for binder in binders.iter() {
                    for (module_spec, _) in binder.module_exports.iter() {
                        let normalized = module_spec.trim_matches('"').trim_matches('\'');
                        if normalized.contains('*') {
                            dm.patterns.push(normalized.to_string());
                        } else {
                            dm.exact.insert(normalized.to_string());
                        }
                    }
                    for name in binder
                        .declared_modules
                        .iter()
                        .chain(binder.shorthand_ambient_modules.iter())
                    {
                        let normalized = name.trim_matches('"').trim_matches('\'');
                        if normalized.contains('*') {
                            dm.patterns.push(normalized.to_string());
                        } else {
                            dm.exact.insert(normalized.to_string());
                        }
                    }
                }
                dm.patterns.sort();
                dm.patterns.dedup();
                self.global_declared_modules = Some(Arc::new(dm));
            }
            if self.global_expando_index.is_none() {
                let mut expando_index: FxHashMap<String, FxHashSet<String>> = FxHashMap::default();
                for binder in binders.iter() {
                    for (obj_key, props) in binder.expando_properties.iter() {
                        expando_index
                            .entry(obj_key.clone())
                            .or_default()
                            .extend(props.iter().cloned());
                    }
                }
                self.global_expando_index = Some(Arc::new(expando_index));
            }
            if self.global_arena_index.is_none() {
                self.build_arena_index();
            }
            self.all_binders = Some(binders);
            return;
        }

        // Fallback: build all indices from scratch (legacy path for tests and
        // callers that don't use ProjectEnv).
        let mut file_locals_index: FxHashMap<String, Vec<(usize, SymbolId)>> = FxHashMap::default();
        let mut module_exports_index: FxHashMap<String, FxHashMap<String, Vec<(usize, SymbolId)>>> =
            FxHashMap::default();
        let mut module_binder_index: FxHashMap<String, Vec<usize>> = FxHashMap::default();

        let has_skeleton_declared_modules = self.global_declared_modules.is_some();
        let mut declared_modules = if has_skeleton_declared_modules {
            None
        } else {
            Some(super::GlobalDeclaredModules::default())
        };

        for (file_idx, binder) in binders.iter().enumerate() {
            for (name, &sym_id) in binder.file_locals.iter() {
                file_locals_index
                    .entry(name.to_string())
                    .or_default()
                    .push((file_idx, sym_id));
            }
            for (module_spec, exports) in binder.module_exports.iter() {
                // Build module_binder_index: module_spec -> [binder_idx]
                module_binder_index
                    .entry(module_spec.clone())
                    .or_default()
                    .push(file_idx);
                let normalized = module_spec.trim_matches('"').trim_matches('\'');
                if normalized != module_spec {
                    module_binder_index
                        .entry(normalized.to_string())
                        .or_default()
                        .push(file_idx);
                }
                for (export_name, &sym_id) in exports.iter() {
                    module_exports_index
                        .entry(module_spec.clone())
                        .or_default()
                        .entry(export_name.to_string())
                        .or_default()
                        .push((file_idx, sym_id));
                }
                if let Some(ref mut dm) = declared_modules {
                    let normalized = module_spec.trim_matches('"').trim_matches('\'');
                    if normalized.contains('*') {
                        dm.patterns.push(normalized.to_string());
                    } else {
                        dm.exact.insert(normalized.to_string());
                    }
                }
            }

            if let Some(ref mut dm) = declared_modules {
                for name in binder
                    .declared_modules
                    .iter()
                    .chain(binder.shorthand_ambient_modules.iter())
                {
                    let normalized = name.trim_matches('"').trim_matches('\'');
                    if normalized.contains('*') {
                        dm.patterns.push(normalized.to_string());
                    } else {
                        dm.exact.insert(normalized.to_string());
                    }
                }
            }
        }

        let has_skeleton_expando = self.global_expando_index.is_some();
        if !has_skeleton_expando {
            let mut expando_index: FxHashMap<String, FxHashSet<String>> = FxHashMap::default();
            for binder in binders.iter() {
                for (obj_key, props) in binder.expando_properties.iter() {
                    expando_index
                        .entry(obj_key.clone())
                        .or_default()
                        .extend(props.iter().cloned());
                }
            }
            self.global_expando_index = Some(Arc::new(expando_index));
        }

        if let Some(mut dm) = declared_modules {
            dm.patterns.sort();
            dm.patterns.dedup();
            self.global_declared_modules = Some(Arc::new(dm));
        }

        let mut module_augs_index: FxHashMap<String, Vec<(usize, tsz_binder::ModuleAugmentation)>> =
            FxHashMap::default();
        let mut aug_targets_index: FxHashMap<String, Vec<(tsz_binder::SymbolId, usize)>> =
            FxHashMap::default();
        for (file_idx, binder) in binders.iter().enumerate() {
            for (module_spec, augmentations) in binder.module_augmentations.iter() {
                module_augs_index
                    .entry(module_spec.clone())
                    .or_default()
                    .extend(augmentations.iter().map(|aug| (file_idx, aug.clone())));
            }
            for (&sym_id, module_spec) in binder.augmentation_target_modules.iter() {
                aug_targets_index
                    .entry(module_spec.clone())
                    .or_default()
                    .push((sym_id, file_idx));
            }
        }

        self.global_file_locals_index = Some(Arc::new(file_locals_index));
        self.global_module_exports_index = Some(Arc::new(module_exports_index));
        self.global_module_augmentations_index = Some(Arc::new(module_augs_index));
        self.global_augmentation_targets_index = Some(Arc::new(aug_targets_index));
        self.global_module_binder_index = Some(Arc::new(module_binder_index));
        self.build_arena_index();
        self.all_binders = Some(binders);
    }

    /// Build the `global_arena_index` from `all_arenas`.
    ///
    /// Maps `Arc::as_ptr(arena) as usize` → file index for O(1) arena→binder lookups.
    fn build_arena_index(&mut self) {
        if let Some(arenas) = self.all_arenas.as_ref() {
            let mut arena_idx: FxHashMap<usize, usize> = FxHashMap::default();
            for (file_idx, arena) in arenas.iter().enumerate() {
                arena_idx.insert(Arc::as_ptr(arena) as usize, file_idx);
            }
            debug_assert_eq!(
                arena_idx.len(),
                arenas.len(),
                "global_arena_index has {} entries but all_arenas has {} — \
                 duplicate Arc<NodeArena> pointers detected",
                arena_idx.len(),
                arenas.len(),
            );
            self.global_arena_index = Some(Arc::new(arena_idx));
        }
    }

    /// Validate that skeleton-derived declared modules match the binder-built ones.
    ///
    /// Called from the orchestration layer after `set_all_binders` when a
    /// `SkeletonIndex` is available. In debug builds, asserts exact match between
    /// the two construction paths, proving the skeleton captures all the data
    /// needed for this index. In release builds, this is a no-op.
    ///
    /// # Arguments
    /// * `skeleton_exact` - Exact module names from `SkeletonIndex::build_declared_module_sets()`
    /// * `skeleton_patterns` - Wildcard patterns from `SkeletonIndex::build_declared_module_sets()`
    pub fn validate_skeleton_declared_modules(
        &self,
        skeleton_exact: &FxHashSet<String>,
        skeleton_patterns: &[String],
    ) {
        if cfg!(debug_assertions)
            && let Some(ref binder_built) = self.global_declared_modules
        {
            // Exact names must match.
            assert_eq!(
                &binder_built.exact, skeleton_exact,
                "skeleton declared_modules exact set differs from binder-built"
            );
            // Patterns must match (both are sorted+deduped).
            assert_eq!(
                &binder_built.patterns, skeleton_patterns,
                "skeleton declared_modules patterns differ from binder-built"
            );
        }
    }

    /// Validate that skeleton-derived expando index matches the binder-built one.
    ///
    /// Called from the orchestration layer after `set_all_binders` when a
    /// `SkeletonIndex` is available. In debug builds, asserts exact match between
    /// the two construction paths. In release builds, this is a no-op.
    pub fn validate_skeleton_expando_index(
        &self,
        skeleton_expando: &FxHashMap<String, FxHashSet<String>>,
    ) {
        if cfg!(debug_assertions)
            && let Some(ref built) = self.global_expando_index
        {
            assert_eq!(
                built.as_ref(),
                skeleton_expando,
                "skeleton expando_index differs from binder-built"
            );
        }
    }

    /// Set resolved module paths map for cross-file import resolution.
    pub fn set_resolved_module_paths(&mut self, paths: Arc<FxHashMap<(usize, String), usize>>) {
        self.resolved_module_paths = Some(paths);
    }

    /// Set resolved module paths keyed by the full driver lookup request.
    pub fn set_resolved_module_request_paths(
        &mut self,
        paths: Arc<FxHashMap<(usize, String, Option<ResolutionModeOverride>), usize>>,
    ) {
        self.resolved_module_request_paths = Some(paths);
    }

    /// Set resolved module specifiers (module names that exist in the project).
    /// Used to suppress TS2307 errors for known modules.
    pub fn set_resolved_modules(&mut self, modules: FxHashSet<String>) {
        self.resolved_modules = Some(modules);
    }

    /// Set resolved module errors map for cross-file import resolution.
    /// Populated by the driver when `ModuleResolver` returns specific errors (TS2834, TS2835, TS2792, etc.).
    pub fn set_resolved_module_errors(
        &mut self,
        errors: Arc<FxHashMap<(usize, String), ResolutionError>>,
    ) {
        self.resolved_module_errors = Some(errors);
    }

    /// Set resolved module errors keyed by the full driver lookup request.
    pub fn set_resolved_module_request_errors(
        &mut self,
        errors: Arc<FxHashMap<(usize, String, Option<ResolutionModeOverride>), ResolutionError>>,
    ) {
        self.resolved_module_request_errors = Some(errors);
    }

    /// Get the resolution error for a specifier, if any.
    /// Returns the specific error (TS2834, TS2835, TS2792, etc.) if the module resolution failed with a known error.
    pub fn get_resolution_error(&self, specifier: &str) -> Option<&ResolutionError> {
        let errors = self.resolved_module_errors.as_ref()?;

        for candidate in module_specifier_candidates(specifier) {
            if let Some(error) = errors.get(&(self.current_file_idx, candidate)) {
                return Some(error);
            }
        }
        None
    }

    /// Get the resolution error for a specifier under an explicit resolution-mode override.
    pub fn get_resolution_error_with_mode(
        &self,
        specifier: &str,
        resolution_mode_override: Option<ResolutionModeOverride>,
    ) -> Option<&ResolutionError> {
        if let Some(errors) = self.resolved_module_request_errors.as_ref() {
            for candidate in module_specifier_candidates(specifier) {
                if let Some(error) =
                    errors.get(&(self.current_file_idx, candidate, resolution_mode_override))
                {
                    return Some(error);
                }
            }
        }

        self.get_resolution_error(specifier)
    }

    /// Set the current file index.
    pub const fn set_current_file_idx(&mut self, idx: usize) {
        self.current_file_idx = idx;
    }

    /// Set the deprecation diagnostics state on the capability boundary.
    ///
    /// When TS5107/TS5101 deprecation diagnostics are present, tsc stops compilation
    /// early and never resolves lib types. This sets both the canonical flag on
    /// `EnvironmentCapabilities` and the `skip_lib_type_resolution` shortcut.
    pub const fn set_has_deprecation_diagnostics(&mut self, has_deprecation: bool) {
        self.capabilities.has_deprecation_diagnostics = has_deprecation;
        self.skip_lib_type_resolution = has_deprecation;
    }

    /// Get the arena for a specific file index.
    /// Returns the current arena if `file_idx` is `u32::MAX` (single-file mode).
    pub fn get_arena_for_file(&self, file_idx: u32) -> &NodeArena {
        if file_idx == u32::MAX {
            return self.arena;
        }
        if let Some(arenas) = self.all_arenas.as_ref()
            && let Some(arena) = arenas.get(file_idx as usize)
        {
            return arena.as_ref();
        }
        self.arena
    }

    /// Get the binder for a specific file index.
    /// Returns None if `file_idx` is out of bounds or `all_binders` is not set.
    pub fn get_binder_for_file(&self, file_idx: usize) -> Option<&BinderState> {
        self.all_binders
            .as_ref()
            .and_then(|binders| binders.get(file_idx))
            .map(Arc::as_ref)
    }

    /// Look up which file indices have `module_exports` for the given specifier.
    ///
    /// Uses the O(1) `global_module_binder_index` when available,
    /// otherwise returns `None` (caller should fall back to linear scan).
    #[inline]
    pub fn files_for_module_specifier(&self, specifier: &str) -> Option<&[usize]> {
        self.global_module_binder_index
            .as_ref()
            .and_then(|idx| idx.get(specifier))
            .map(Vec::as_slice)
    }

    /// Get the binder that owns a specific arena.
    ///
    /// This is used when cross-file resolution discovers a declaration arena
    /// directly (via `symbol_arenas` / `declaration_arenas`) without already
    /// knowing the originating file index.
    pub fn get_binder_for_arena(&self, arena: &NodeArena) -> Option<&BinderState> {
        let binders = self.all_binders.as_ref()?;
        let arena_ptr = arena as *const NodeArena as usize;

        // O(1) path via pre-built arena index
        if let Some(arena_idx) = self.global_arena_index.as_ref() {
            let file_idx = *arena_idx.get(&arena_ptr)?;
            return binders.get(file_idx).map(Arc::as_ref);
        }

        // O(N) fallback when index not built
        let arenas = self.all_arenas.as_ref()?;
        arenas.iter().enumerate().find_map(|(idx, candidate)| {
            (Arc::as_ptr(candidate) as usize == arena_ptr)
                .then(|| binders.get(idx).map(Arc::as_ref))
                .flatten()
        })
    }

    /// Get the file index that owns a specific arena.
    ///
    /// This keeps delegated child contexts aligned with the declaring file when
    /// cross-file resolution discovers an arena directly from declaration metadata.
    pub fn get_file_idx_for_arena(&self, arena: &NodeArena) -> Option<usize> {
        let arena_ptr = arena as *const NodeArena as usize;

        // O(1) path via pre-built arena index
        if let Some(arena_idx) = self.global_arena_index.as_ref() {
            return arena_idx.get(&arena_ptr).copied();
        }

        // O(N) fallback when index not built
        let arenas = self.all_arenas.as_ref()?;
        arenas.iter().enumerate().find_map(|(idx, candidate)| {
            (Arc::as_ptr(candidate) as usize == arena_ptr).then_some(idx)
        })
    }

    /// Resolve an import specifier to its target file index.
    /// Uses the `resolved_module_paths` map populated by the driver.
    /// Returns None if the import cannot be resolved (e.g., external module).
    pub fn resolve_import_target(&self, specifier: &str) -> Option<usize> {
        self.resolve_import_target_from_file(self.current_file_idx, specifier)
    }

    /// Resolve an import specifier from a specific file to its target file index.
    /// Like `resolve_import_target` but for any source file, not just the current one.
    pub fn resolve_import_target_from_file(
        &self,
        source_file_idx: usize,
        specifier: &str,
    ) -> Option<usize> {
        if let Some(paths) = self.resolved_module_paths.as_ref() {
            for candidate in module_specifier_candidates(specifier) {
                if let Some(target_idx) = paths.get(&(source_file_idx, candidate)) {
                    return Some(*target_idx);
                }
            }
        }

        let arenas = self.all_arenas.as_ref()?;
        let normalized_specifier = specifier.replace('\\', "/");
        let stripped_specifier = Self::strip_ts_extension(&normalized_specifier);
        if let Some((target_idx, _)) = arenas.iter().enumerate().find(|(_, arena)| {
            arena.source_files.first().is_some_and(|sf| {
                let file_name = sf.file_name.replace('\\', "/");
                file_name == normalized_specifier
                    || Self::strip_ts_extension(&file_name) == stripped_specifier
            })
        }) {
            return Some(target_idx);
        }
        let file_names: Vec<String> = arenas
            .iter()
            .filter_map(|arena| arena.source_files.first().map(|sf| sf.file_name.clone()))
            .collect();
        let (fallback_paths, _) =
            crate::module_resolution::build_module_resolution_maps(&file_names);
        for candidate in module_specifier_candidates(specifier) {
            if let Some(target_idx) = fallback_paths.get(&(source_file_idx, candidate)) {
                return Some(*target_idx);
            }
        }
        None
    }

    /// Resolve an import specifier from a specific file using an explicit
    /// `resolution-mode` override when one was present in the original request.
    pub fn resolve_import_target_from_file_with_mode(
        &self,
        source_file_idx: usize,
        specifier: &str,
        resolution_mode_override: Option<ResolutionModeOverride>,
    ) -> Option<usize> {
        if let Some(paths) = self.resolved_module_request_paths.as_ref() {
            for candidate in module_specifier_candidates(specifier) {
                if let Some(target_idx) =
                    paths.get(&(source_file_idx, candidate.clone(), resolution_mode_override))
                {
                    return Some(*target_idx);
                }
            }
        }

        self.resolve_import_target_from_file(source_file_idx, specifier)
    }

    /// Resolve a member exported by the target module of an ALIAS symbol.
    ///
    /// When an ALIAS symbol's `import_module` holds a relative specifier
    /// (e.g., `"./Something"`), it must be resolved from the ALIAS's source
    /// file, not the current file.  This helper uses `cross_file_symbol_targets`
    /// to find the ALIAS's origin file, resolves the specifier from that file's
    /// perspective, then looks up the member in the target module's exports.
    pub fn resolve_alias_import_member(
        &self,
        alias_id: tsz_binder::SymbolId,
        module_specifier: &str,
        member_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let source_file_idx = self.resolve_symbol_file_index(alias_id)?;
        let target_idx = self.resolve_import_target_from_file(source_file_idx, module_specifier)?;
        let target_binder = self.get_binder_for_file(target_idx)?;
        let target_arena = self.get_arena_for_file(target_idx as u32);
        let file_name = &target_arena.source_files.first()?.file_name;
        // Use the target binder's own re-export resolution (handles
        // direct exports, named re-exports, and wildcard re-exports).
        target_binder
            .resolve_import_with_reexports_type_only(file_name, member_name)
            .map(|(sym_id, _)| {
                self.register_symbol_file_target(sym_id, target_idx);
                sym_id
            })
    }

    /// Follow an import alias to its actual target symbol across file boundaries.
    ///
    /// For ALIAS symbols (created by `import {A} from "./file"`), resolves
    /// the module specifier from the alias's source file, then looks up the
    /// exported name in the target file's binder. Returns None if the symbol
    /// is not an alias or resolution fails.
    ///
    /// This is a pure lookup — it does NOT register the result in
    /// `cross_file_symbol_targets`. Callers that need cross-arena delegation
    /// (e.g., lazy type resolution) should call [`resolve_import_alias_and_register`]
    /// instead.
    pub fn resolve_import_alias(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<tsz_binder::SymbolId> {
        let symbol = self.binder.symbols.get(sym_id).or_else(|| {
            self.all_binders
                .as_ref()
                .and_then(|bs| bs.iter().find_map(|b| b.symbols.get(sym_id)))
        })?;

        if (symbol.flags & tsz_binder::symbol_flags::ALIAS) == 0 {
            return None;
        }
        let module_specifier = symbol.import_module.as_ref()?;
        let import_name = symbol.import_name.as_ref().unwrap_or(&symbol.escaped_name);

        let source_file_idx = symbol.decl_file_idx as usize;
        if let Some(target_idx) =
            self.resolve_import_target_from_file(source_file_idx, module_specifier)
        {
            let target_binder = self.get_binder_for_file(target_idx)?;
            return target_binder.file_locals.get(import_name);
        }

        // Fallback: check ambient module exports (declare module "X" { ... }).
        // These are keyed by the module specifier in binder.module_exports.
        self.resolve_import_from_ambient_module(module_specifier, import_name)
    }

    /// Like [`resolve_import_alias`], but also registers the resolved symbol in
    /// `cross_file_symbol_targets` so that `delegate_cross_arena_symbol_resolution`
    /// can create a child checker with the correct arena when computing its type.
    pub fn resolve_import_alias_and_register(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<tsz_binder::SymbolId> {
        let symbol = self.binder.symbols.get(sym_id).or_else(|| {
            self.all_binders
                .as_ref()
                .and_then(|bs| bs.iter().find_map(|b| b.symbols.get(sym_id)))
        })?;

        if (symbol.flags & tsz_binder::symbol_flags::ALIAS) == 0 {
            return None;
        }
        let module_specifier = symbol.import_module.as_ref()?;
        let import_name = symbol.import_name.as_ref().unwrap_or(&symbol.escaped_name);

        let source_file_idx = symbol.decl_file_idx as usize;
        if let Some(target_idx) =
            self.resolve_import_target_from_file(source_file_idx, module_specifier)
        {
            let target_binder = self.get_binder_for_file(target_idx)?;
            let result = target_binder.file_locals.get(import_name)?;
            self.register_symbol_file_target(result, target_idx);
            return Some(result);
        }

        // Fallback: check ambient module exports (declare module "X" { ... }).
        // These are keyed by the module specifier in binder.module_exports.
        // For ambient modules, the symbol lives in the same binder that declared
        // the module, so we also register it in cross_file_symbol_targets with
        // the declaring file's index for proper cross-arena delegation.
        if let Some((result, file_idx)) =
            self.resolve_import_from_ambient_module_with_file_idx(module_specifier, import_name)
        {
            self.register_symbol_file_target(result, file_idx);
            return Some(result);
        }
        None
    }

    /// Resolve an import name from ambient module exports (`declare module "X" { ... }`).
    ///
    /// When file-based module resolution fails (the module specifier doesn't correspond
    /// to any file), this fallback checks `module_exports` in the current binder and
    /// all cross-file binders. Ambient module declarations populate `module_exports`
    /// keyed by their string-literal module specifier (e.g., `"A"` for `declare module "A"`).
    fn resolve_import_from_ambient_module(
        &self,
        module_specifier: &str,
        import_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        // Check current binder first
        if let Some(exports) = self.binder.module_exports.get(module_specifier)
            && let Some(sym_id) = exports.get(import_name)
        {
            return Some(sym_id);
        }
        // Use the pre-built global module_exports index for O(1) lookup (no allocation)
        if let Some(entries) = self
            .global_module_exports_index
            .as_ref()
            .and_then(|idx| idx.get(module_specifier))
            .and_then(|inner| inner.get(import_name))
            && let Some(&(_file_idx, sym_id)) = entries.first()
        {
            return Some(sym_id);
        }
        None
    }

    /// Like [`resolve_import_from_ambient_module`] but also returns the file index
    /// of the binder that owns the resolved symbol, for `cross_file_symbol_targets`
    /// registration.
    fn resolve_import_from_ambient_module_with_file_idx(
        &self,
        module_specifier: &str,
        import_name: &str,
    ) -> Option<(tsz_binder::SymbolId, usize)> {
        // Check current binder first
        if let Some(exports) = self.binder.module_exports.get(module_specifier)
            && let Some(sym_id) = exports.get(import_name)
        {
            return Some((sym_id, self.current_file_idx));
        }
        // Use the pre-built global module_exports index for O(1) lookup (no allocation)
        if let Some(entries) = self
            .global_module_exports_index
            .as_ref()
            .and_then(|idx| idx.get(module_specifier))
            .and_then(|inner| inner.get(import_name))
            && let Some(&(file_idx, sym_id)) = entries.first()
        {
            return Some((sym_id, file_idx));
        }
        None
    }

    /// Extract the persistent cache from this context.
    /// This allows saving type checking results for future queries.
    pub fn extract_cache(self) -> TypeCache {
        TypeCache {
            symbol_types: self.symbol_types,
            symbol_instance_types: self.symbol_instance_types,
            node_types: self.node_types,
            symbol_dependencies: self.symbol_dependencies,
            def_to_symbol: self.def_to_symbol.into_inner(),
            flow_analysis_cache: self.flow_analysis_cache.into_inner(),
            class_instance_type_to_decl: self.class_instance_type_to_decl,
            class_instance_type_cache: self.class_instance_type_cache,
            class_constructor_type_cache: self.class_constructor_type_cache,
            type_only_nodes: self.type_only_nodes,
            namespace_module_names: self.namespace_module_names,
        }
    }

    fn diagnostic_dedup_key_from_parts(&self, start: u32, code: u32, message: &str) -> (u32, u32) {
        if code == 2318 && start == 0 {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            message.hash(&mut hasher);
            (hasher.finish() as u32, code)
        } else if code == 2411 || code == 2430 || code == 2536 {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            message.hash(&mut hasher);
            (start ^ (hasher.finish() as u32), code)
        } else {
            (start, code)
        }
    }

    pub fn diagnostic_dedup_key(&self, diag: &Diagnostic) -> (u32, u32) {
        self.diagnostic_dedup_key_from_parts(diag.start, diag.code, &diag.message_text)
    }

    pub fn rebuild_emitted_diagnostics_from_current(&mut self) {
        self.emitted_diagnostics.clear();
        // Also synchronize the TS2454 dedup set: remove entries for TS2454
        // diagnostics that are no longer in the diagnostics list (e.g., removed
        // by a prior `retain` call). Without this, removed TS2454 errors stay
        // in the dedup set and cannot be re-emitted on subsequent passes.
        let ts2454_positions: rustc_hash::FxHashSet<u32> = self
            .diagnostics
            .iter()
            .filter(|d| d.code == 2454)
            .map(|d| d.start)
            .collect();
        self.emitted_ts2454_errors
            .retain(|&(pos, _)| ts2454_positions.contains(&pos));
        for diag in &self.diagnostics {
            let key = self.diagnostic_dedup_key(diag);
            self.emitted_diagnostics.insert(key);
        }
    }

    /// Add an error diagnostic (with deduplication).
    /// Diagnostics with the same (start, code) are only emitted once.
    /// Exceptions:
    /// - TS2411 uses (start ^ `message_hash`, code) to allow a single property to
    ///   fail against both string and number index signatures at the same span.
    /// - TS2430 uses (start ^ `message_hash`, code) to allow multiple
    ///   "incorrectly extends" errors at the same interface name when an interface
    ///   incompatibly extends several distinct bases.
    /// - TS2536 uses the same scheme so nested indexed-access failures can report
    ///   multiple distinct messages at the same indexed-access start.
    pub fn error(&mut self, start: u32, length: u32, message: String, code: u32) {
        // TS2304 ("Cannot find name"), TS2552 ("Cannot find name ... Did you mean?"),
        // and TS2663 ("Did you mean the instance member 'this.X'?") are suppressed when
        // TS2301 already exists at the same position, since TS2301
        // ("Initializer of instance member cannot reference identifier declared in constructor")
        // already explains the problem more precisely.
        if (code == 2304 || code == 2552 || code == 2663)
            && self
                .diagnostics
                .iter()
                .any(|diag| diag.start == start && diag.code == 2301)
        {
            return;
        }
        if code == 2301 {
            self.diagnostics.retain(|diag| {
                !(diag.start == start
                    && (diag.code == 2304 || diag.code == 2552 || diag.code == 2663))
            });
            self.emitted_diagnostics.remove(&(start, 2304));
            self.emitted_diagnostics.remove(&(start, 2552));
            self.emitted_diagnostics.remove(&(start, 2663));
        }

        // TS2304 and TS2552 are mutually exclusive at the same position.
        // TS2552 (with spelling suggestion) takes priority over TS2304 (without).
        // Multiple code paths can emit these for the same unresolved name.
        if code == 2304 && self.emitted_diagnostics.contains(&(start, 2552)) {
            return;
        }
        if code == 2552 {
            self.diagnostics
                .retain(|diag| !(diag.start == start && diag.code == 2304));
            self.emitted_diagnostics.remove(&(start, 2304));
        }

        // Check if we've already emitted this diagnostic
        let key = self.diagnostic_dedup_key_from_parts(start, code, &message);
        if self.emitted_diagnostics.contains(&key) {
            return;
        }
        self.emitted_diagnostics.insert(key);
        tracing::debug!(
            code,
            start,
            length,
            file = %self.file_name,
            message = %message,
            "diagnostic"
        );
        self.diagnostics.push(Diagnostic::error(
            self.file_name.clone(),
            start,
            length,
            message,
            code,
        ));
    }

    /// Push a diagnostic with deduplication.
    /// Diagnostics with the same (start, code) are only emitted once.
    /// Exceptions:
    /// - TS2318 (missing global type) at position 0 uses message hash to allow multiple distinct
    ///   global type errors.
    /// - TS2411 uses (start ^ `message_hash`, code) to allow a single property to
    ///   report both string and number index incompatibilities.
    /// - TS2430 (incorrectly extends interface) uses (start ^ `message_hash`, code) to allow
    ///   multiple per-base diagnostics at the same interface name position.
    pub fn push_diagnostic(&mut self, diag: Diagnostic) {
        if (diag.code == 2304 || diag.code == 2552 || diag.code == 2663)
            && self
                .diagnostics
                .iter()
                .any(|existing| existing.start == diag.start && existing.code == 2301)
        {
            return;
        }
        if diag.code == 2301 {
            self.diagnostics.retain(|existing| {
                !(existing.start == diag.start
                    && (existing.code == 2304 || existing.code == 2552 || existing.code == 2663))
            });
            self.emitted_diagnostics.remove(&(diag.start, 2304));
            self.emitted_diagnostics.remove(&(diag.start, 2552));
            self.emitted_diagnostics.remove(&(diag.start, 2663));
        }
        // TS2304 and TS2552 are mutually exclusive at the same position.
        // TS2552 (with spelling suggestion) takes priority over TS2304 (without).
        if diag.code == 2304 && self.emitted_diagnostics.contains(&(diag.start, 2552)) {
            return;
        }
        if diag.code == 2552 {
            self.diagnostics
                .retain(|existing| !(existing.start == diag.start && existing.code == 2304));
            self.emitted_diagnostics.remove(&(diag.start, 2304));
        }
        if diag.code == 2322 {
            let diag_end = diag.start.saturating_add(diag.length);
            // TS2353/TS2561 on a property inside an object literal should suppress
            // a redundant enclosing TS2322 on the whole literal.
            if self.diagnostics.iter().any(|existing| {
                (existing.code
                    == diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
                    || existing.code
                        == diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_BUT_DOES_NOT_EXIST_IN_TYPE_DID)
                    && existing.start >= diag.start
                    && existing.start < diag_end
            }) {
                return;
            }
        }

        let key = self.diagnostic_dedup_key(&diag);

        if self.emitted_diagnostics.contains(&key) {
            return;
        }
        self.emitted_diagnostics.insert(key);
        tracing::debug!(
            code = diag.code,
            start = diag.start,
            length = diag.length,
            file = %diag.file,
            message = %diag.message_text,
            "diagnostic"
        );
        self.diagnostics.push(diag);
    }

    /// Get node span (pos, end) from index.
    pub fn get_node_span(&self, idx: NodeIndex) -> Option<(u32, u32)> {
        let node = self.arena.get(idx)?;
        Some((node.pos, node.end))
    }

    /// Push an expected return type onto the stack.
    pub fn push_return_type(&mut self, return_type: TypeId) {
        self.return_type_stack.push(return_type);
    }

    /// Pop the expected return type from the stack.
    pub fn pop_return_type(&mut self) {
        self.return_type_stack.pop();
    }

    /// Get the current expected return type.
    pub fn current_return_type(&self) -> Option<TypeId> {
        self.return_type_stack.last().copied()
    }

    /// Push a contextual yield type for a generator function.
    pub fn push_yield_type(&mut self, yield_type: Option<TypeId>) {
        self.yield_type_stack.push(yield_type);
    }

    /// Pop the contextual yield type from the stack.
    pub fn pop_yield_type(&mut self) {
        self.yield_type_stack.pop();
    }

    /// Get the current contextual yield type for the enclosing generator.
    pub fn current_yield_type(&self) -> Option<TypeId> {
        self.yield_type_stack.last().copied().flatten()
    }

    pub fn push_generator_next_type(&mut self, next_type: Option<TypeId>) {
        self.generator_next_type_stack.push(next_type);
    }

    pub fn pop_generator_next_type(&mut self) {
        self.generator_next_type_stack.pop();
    }

    pub fn current_generator_next_type(&self) -> Option<TypeId> {
        self.generator_next_type_stack.last().copied().flatten()
    }

    /// Enter an async context (increment async depth).
    pub const fn enter_async_context(&mut self) {
        self.async_depth += 1;
    }

    /// Exit an async context (decrement async depth).
    pub const fn exit_async_context(&mut self) {
        if self.async_depth > 0 {
            self.async_depth -= 1;
        }
    }

    /// Check if we're currently inside an async function.
    pub const fn in_async_context(&self) -> bool {
        self.async_depth > 0
    }

    /// Consume one unit of type resolution fuel.
    /// Returns true if fuel is still available, false if exhausted.
    /// When exhausted, type resolution should return ERROR to prevent timeout.
    /// Also tracks a thread-local global fuel counter that is NOT reset when
    /// child contexts are created for cross-arena delegation, preventing
    /// unbounded total work across multiple contexts.
    pub fn consume_fuel(&self) -> bool {
        let fuel = self.type_resolution_fuel.get();
        if fuel == 0 {
            return false;
        }
        self.type_resolution_fuel.set(fuel - 1);
        // Thread-local global fuel prevents OOM when child contexts each get
        // fresh per-context fuel (cross-arena delegation). This is the only
        // fuel counter that survives context boundaries.
        if crate::state_domain::type_environment::lazy::global_resolution_fuel_exhausted() {
            return false;
        }
        crate::state_domain::type_environment::lazy::increment_global_resolution_fuel();
        true
    }

    /// Enter a recursive call. Returns true if recursion is allowed,
    /// false if the depth limit has been reached (caller should bail out).
    #[inline]
    pub fn enter_recursion(&self) -> bool {
        self.recursion_depth.borrow_mut().enter()
    }

    /// Leave a recursive call (decrement depth counter).
    #[inline]
    pub fn leave_recursion(&self) {
        self.recursion_depth.borrow_mut().leave();
    }

    // =========================================================================
    // Flow Graph Queries
    // =========================================================================

    /// Check flow usage at a specific AST node.
    ///
    /// This method queries the control flow graph to determine flow-sensitive
    /// information at a given node. Returns `None` if flow graph is not available.
    ///
    /// # Arguments
    /// * `node_idx` - The AST node to query flow information for
    ///
    /// # Returns
    /// * `Some(FlowNodeId)` - The flow node ID at this location
    /// * `None` - If flow graph is not available or node has no flow info
    pub fn check_flow_usage(&self, node_idx: NodeIndex) -> Option<tsz_binder::FlowNodeId> {
        if let Some(ref _graph) = self.flow_graph {
            // Look up the flow node for this AST node from the binder's node_flow mapping
            self.binder.node_flow.get(&node_idx.0).copied()
        } else {
            None
        }
    }

    /// Get a reference to the flow graph.
    pub const fn flow_graph(&self) -> Option<&FlowGraph<'a>> {
        self.flow_graph.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::TypeCache;
    use rustc_hash::{FxHashMap, FxHashSet};
    use tsz_binder::SymbolId;
    use tsz_parser::parser::NodeIndex;
    use tsz_solver::TypeId;

    fn empty_cache() -> TypeCache {
        TypeCache {
            symbol_types: crate::context::SymbolTypeCache::new(),
            symbol_instance_types: crate::context::SymbolTypeCache::new(),
            node_types: crate::context::NodeTypeCache::new(),
            symbol_dependencies: FxHashMap::default(),
            def_to_symbol: FxHashMap::default(),
            flow_analysis_cache: FxHashMap::default(),
            class_instance_type_to_decl: FxHashMap::default(),
            class_instance_type_cache: FxHashMap::default(),
            class_constructor_type_cache: FxHashMap::default(),
            type_only_nodes: FxHashSet::default(),
            namespace_module_names: FxHashMap::default(),
        }
    }

    #[test]
    fn type_cache_merge_keeps_constructor_type_cache() {
        let mut lhs = empty_cache();
        let mut rhs = empty_cache();

        rhs.class_constructor_type_cache
            .insert(NodeIndex(42), TypeId::STRING);

        lhs.merge(rhs);

        assert_eq!(
            lhs.class_constructor_type_cache.get(&NodeIndex(42)),
            Some(&TypeId::STRING)
        );
    }

    #[test]
    fn type_cache_merge_keeps_error_class_type_cache_entries() {
        let mut lhs = empty_cache();
        let mut rhs = empty_cache();

        rhs.class_instance_type_cache
            .insert(NodeIndex(10), TypeId::ERROR);
        rhs.class_constructor_type_cache
            .insert(NodeIndex(11), TypeId::ERROR);

        lhs.merge(rhs);

        assert_eq!(
            lhs.class_instance_type_cache.get(&NodeIndex(10)),
            Some(&TypeId::ERROR)
        );
        assert_eq!(
            lhs.class_constructor_type_cache.get(&NodeIndex(11)),
            Some(&TypeId::ERROR)
        );
    }

    #[test]
    fn invalidate_symbols_clears_class_type_caches() {
        let mut cache = empty_cache();
        let sym = SymbolId(7);
        cache
            .symbol_dependencies
            .insert(sym, FxHashSet::<SymbolId>::default());
        cache
            .class_instance_type_cache
            .insert(NodeIndex(1), TypeId::NUMBER);
        cache
            .class_constructor_type_cache
            .insert(NodeIndex(2), TypeId::STRING);
        cache
            .class_instance_type_to_decl
            .insert(TypeId::BOOLEAN, NodeIndex(3));

        let affected = cache.invalidate_symbols(&[sym]);

        assert_eq!(affected, 1);
        assert!(cache.class_instance_type_cache.is_empty());
        assert!(cache.class_constructor_type_cache.is_empty());
        assert!(cache.class_instance_type_to_decl.is_empty());
    }
}

#[cfg(test)]
mod index_tests {
    use std::sync::Arc;
    use tsz_binder::{BinderState, ModuleAugmentation, SymbolId};
    use tsz_parser::parser::NodeIndex;

    /// Build the global module augmentation indices from a list of binders
    /// (same logic as `set_all_binders` but isolated for testing).
    fn build_module_augmentation_indices(
        binders: &[Arc<BinderState>],
    ) -> (
        rustc_hash::FxHashMap<String, Vec<(usize, ModuleAugmentation)>>,
        rustc_hash::FxHashMap<String, Vec<(SymbolId, usize)>>,
    ) {
        use rustc_hash::FxHashMap;
        let mut module_augs_index: FxHashMap<String, Vec<(usize, ModuleAugmentation)>> =
            FxHashMap::default();
        let mut aug_targets_index: FxHashMap<String, Vec<(SymbolId, usize)>> = FxHashMap::default();
        for (file_idx, binder) in binders.iter().enumerate() {
            for (module_spec, augmentations) in binder.module_augmentations.iter() {
                module_augs_index
                    .entry(module_spec.clone())
                    .or_default()
                    .extend(augmentations.iter().map(|aug| (file_idx, aug.clone())));
            }
            for (&sym_id, module_spec) in binder.augmentation_target_modules.iter() {
                aug_targets_index
                    .entry(module_spec.clone())
                    .or_default()
                    .push((sym_id, file_idx));
            }
        }
        (module_augs_index, aug_targets_index)
    }

    #[test]
    fn global_module_augmentations_index_merges_across_binders() {
        let mut binder1 = BinderState::new();
        binder1.module_augmentations.insert(
            "./module-a".to_string(),
            vec![ModuleAugmentation::new(
                "MyInterface".to_string(),
                NodeIndex(10),
            )],
        );
        let mut binder2 = BinderState::new();
        binder2.module_augmentations.insert(
            "./module-a".to_string(),
            vec![ModuleAugmentation::new(
                "MyOtherInterface".to_string(),
                NodeIndex(20),
            )],
        );

        let binders = vec![Arc::new(binder1), Arc::new(binder2)];
        let (aug_index, _) = build_module_augmentation_indices(&binders);

        let entries = &aug_index["./module-a"];
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, 0); // file_idx 0
        assert_eq!(entries[0].1.name, "MyInterface");
        assert_eq!(entries[1].0, 1); // file_idx 1
        assert_eq!(entries[1].1.name, "MyOtherInterface");
    }

    #[test]
    fn global_module_augmentations_index_separates_module_specifiers() {
        let mut binder = BinderState::new();
        binder.module_augmentations.insert(
            "./module-a".to_string(),
            vec![ModuleAugmentation::new("Foo".to_string(), NodeIndex(10))],
        );
        binder.module_augmentations.insert(
            "./module-b".to_string(),
            vec![ModuleAugmentation::new("Bar".to_string(), NodeIndex(20))],
        );

        let binders = vec![Arc::new(binder)];
        let (aug_index, _) = build_module_augmentation_indices(&binders);

        assert!(aug_index.contains_key("./module-a"));
        assert!(aug_index.contains_key("./module-b"));
        assert!(!aug_index.contains_key("./module-c"));
    }

    #[test]
    fn global_augmentation_targets_index_maps_module_to_symbols() {
        let mut binder1 = BinderState::new();
        binder1
            .augmentation_target_modules
            .insert(SymbolId(100), "./target".to_string());
        let mut binder2 = BinderState::new();
        binder2
            .augmentation_target_modules
            .insert(SymbolId(200), "./target".to_string());
        binder2
            .augmentation_target_modules
            .insert(SymbolId(201), "./other".to_string());

        let binders = vec![Arc::new(binder1), Arc::new(binder2)];
        let (_, targets_index) = build_module_augmentation_indices(&binders);

        let target_entries = &targets_index["./target"];
        assert_eq!(target_entries.len(), 2);
        assert_eq!(target_entries[0], (SymbolId(100), 0));
        assert_eq!(target_entries[1], (SymbolId(200), 1));

        let other_entries = &targets_index["./other"];
        assert_eq!(other_entries.len(), 1);
        assert_eq!(other_entries[0], (SymbolId(201), 1));
    }

    #[test]
    fn global_augmentation_indices_empty_for_no_augmentations() {
        let binder = BinderState::new();
        let binders = vec![Arc::new(binder)];
        let (aug_index, targets_index) = build_module_augmentation_indices(&binders);

        assert!(aug_index.is_empty());
        assert!(targets_index.is_empty());
    }
}
