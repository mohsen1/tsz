//! Core implementation for `TypeCache` and `CheckerContext`.
//!
//! Contains the `impl` blocks and methods extracted from `mod.rs` to keep
//! the module entry point focused on type/struct definitions.

use rustc_hash::{FxHashMap, FxHashSet};
#[cfg(test)]
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;

use crate::control_flow::FlowGraph;
use crate::module_resolution::build_file_name_index;
use tsz_binder::symbols::StableLocation;
use tsz_binder::{BinderState, SymbolId};
use tsz_common::file_extensions::strip_known_extension;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_solver::TypeId;

use super::{CheckerContext, LibContext, ResolutionError, TypeCache};

/// Build the union of every name declared in any lib context's `file_locals`.
///
/// Used to populate [`CheckerContext::lib_file_local_names`]. Returns `None`
/// when there are no lib contexts (no index needed; lib scans are already
/// skipped via `ignore_libs`/`has_lib_loaded`). The set owns its `String` keys
/// so it is lifetime-free and can be shared (`Arc`) across per-file checkers.
#[must_use]
pub fn build_lib_file_local_names(lib_contexts: &[LibContext]) -> Option<Arc<FxHashSet<String>>> {
    if lib_contexts.is_empty() {
        return None;
    }
    let mut names: FxHashSet<String> = FxHashSet::default();
    for lib_ctx in lib_contexts {
        for (name, _sym_id) in lib_ctx.binder.file_locals.iter() {
            if !names.contains(name.as_str()) {
                names.insert(name.clone());
            }
        }
    }
    Some(Arc::new(names))
}

/// Kill-switch for order-independent cross-file alias/`export =` resolution.
///
/// When enabled (default), overlay writes that record a symbol's owning file
/// prefer the stable, immutable `global_symbol_file_index` (declaring file)
/// before consulting the monotonically-growing dynamic overlay, so the same
/// `(file, symbol)` resolves to the same endpoint regardless of processing
/// order. Set `TSZ_DISABLE_ORDER_INDEP_RESOLUTION=1` to restore the legacy
/// dynamic-first behaviour for a clean A/B comparison (refs #7574, #12148).
///
/// Cached in a `OnceLock` so the environment is read at most once per process.
pub(crate) fn order_independent_resolution_disabled() -> bool {
    use std::sync::OnceLock;
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| {
        std::env::var("TSZ_DISABLE_ORDER_INDEP_RESOLUTION")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
    })
}

#[cfg(test)]
thread_local! {
    static TYPE_NODE_RESOLUTION_COUNTS: RefCell<FxHashMap<NodeIndex, u32>> =
        RefCell::new(FxHashMap::default());
}

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
        self.class_instance_type_cache.get_mut().clear();
        self.class_constructor_type_cache.get_mut().clear();
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
            .get_mut()
            .extend(other.class_instance_type_cache.into_inner());
        self.class_constructor_type_cache
            .get_mut()
            .extend(other.class_constructor_type_cache.into_inner());
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

        // Merge def_to_symbol and def_to_name mappings
        self.def_to_symbol.extend(other.def_to_symbol);
        self.def_to_name.extend(other.def_to_name);
        self.def_types.extend(other.def_types);
        self.def_type_params.extend(other.def_type_params);
        self.well_known_symbol_names
            .extend(other.well_known_symbol_names);
        self.boxed_types.extend(other.boxed_types);
        for (kind, def_ids) in other.boxed_def_ids {
            let target = self.boxed_def_ids.entry(kind).or_default();
            for def_id in def_ids {
                if !target.contains(&def_id) {
                    target.push(def_id);
                }
            }
        }
    }
}

impl<'a> CheckerContext<'a> {
    /// Clear test-observable type-node resolution counts for a fresh check.
    #[cfg(test)]
    pub(crate) fn reset_type_node_resolution_counts_for_test(&self) {
        TYPE_NODE_RESOLUTION_COUNTS.with(|counts| counts.borrow_mut().clear());
    }

    /// Record a test-observable entry into type-node resolution.
    #[cfg(test)]
    pub(crate) fn record_type_node_resolution_for_test(&self, idx: NodeIndex) {
        TYPE_NODE_RESOLUTION_COUNTS.with(|counts| {
            *counts.borrow_mut().entry(idx).or_insert(0) += 1;
        });
    }

    /// Return how often `idx` entered type-node resolution in this checker.
    #[cfg(test)]
    pub(crate) fn type_node_resolution_count_for_test(&self, idx: NodeIndex) -> u32 {
        TYPE_NODE_RESOLUTION_COUNTS.with(|counts| counts.borrow().get(&idx).copied().unwrap_or(0))
    }

    /// Resolve a `SymbolId` to its owning file index.
    ///
    /// Checks the layered `cross_file_symbol_targets` overlay first, then falls
    /// back to the shared `global_symbol_file_index` base map.
    /// Returns `None` if the symbol has no known cross-file owner.
    pub fn resolve_symbol_file_index(&self, sym_id: SymbolId) -> Option<usize> {
        if let Some(idx) = self.resolve_dynamic_symbol_file_index(sym_id) {
            return Some(idx);
        }
        if let Some(&idx) = self
            .global_symbol_file_index
            .as_ref()
            .and_then(|map| map.get(&sym_id))
        {
            return Some(idx);
        }
        None
    }

    /// Resolve only dynamically-discovered `SymbolId` ownership.
    pub fn resolve_dynamic_symbol_file_index(&self, sym_id: SymbolId) -> Option<usize> {
        self.cross_file_symbol_targets.borrow().get(sym_id)
    }

    /// Resolve a `SymbolId` to its *declaring* file index from the shared,
    /// immutable `global_symbol_file_index` only.
    ///
    /// Unlike [`resolve_symbol_file_index`], this never consults the
    /// monotonically-growing `cross_file_symbol_targets` overlay, so its answer
    /// is a pure function of the bound program and is therefore identical
    /// regardless of file/symbol processing order. Use this when the value is
    /// fed back into the overlay (e.g. pinning an alias to its target's owning
    /// file): reading the dynamic overlay there would let a prior, order-
    /// dependent resolution choice propagate into a later pin, which is the
    /// root cause of order-dependent cross-file alias resolution (refs #7574,
    /// #12148).
    ///
    /// Returns `None` if the symbol has no statically-known declaring file
    /// (e.g. ambient-module exports discovered only during resolution); callers
    /// fall back to the dynamic overlay in that case.
    pub fn resolve_symbol_declaring_file_index(&self, sym_id: SymbolId) -> Option<usize> {
        self.global_symbol_file_index
            .as_ref()
            .and_then(|map| map.get(&sym_id).copied())
    }

    /// Resolve a `SymbolId` to a *stable* owning file index for overlay writes.
    ///
    /// Prefers the order-independent declaring-file index; falls back to the
    /// dynamic overlay only for symbols with no statically-known declaring file.
    /// Gated by the order-independence kill-switch so an A/B against the legacy
    /// dynamic-first behaviour stays clean
    /// (`TSZ_DISABLE_ORDER_INDEP_RESOLUTION=1`).
    pub fn resolve_symbol_file_index_stable(&self, sym_id: SymbolId) -> Option<usize> {
        if order_independent_resolution_disabled() {
            return self.resolve_symbol_file_index(sym_id);
        }
        self.resolve_symbol_declaring_file_index(sym_id)
            .or_else(|| self.resolve_dynamic_symbol_file_index(sym_id))
    }

    /// Check whether a `SymbolId` has a known cross-file owner.
    pub fn has_symbol_file_index(&self, sym_id: SymbolId) -> bool {
        self.global_symbol_file_index
            .as_ref()
            .is_some_and(|map| map.contains_key(&sym_id))
            || self.cross_file_symbol_targets.borrow().contains_key(sym_id)
    }

    /// Register a dynamically-discovered `SymbolId` → file index mapping in the local overlay.
    pub fn register_symbol_file_target(&self, sym_id: SymbolId, file_idx: usize) {
        self.cross_file_symbol_targets
            .borrow_mut()
            .register(sym_id, file_idx);
    }

    pub fn register_symbol_file_index(&self, sym_id: SymbolId, file_idx: usize) {
        self.register_symbol_file_target(sym_id, file_idx);
    }

    /// Attach the local overlay of symbol-file targets to a child checker context.
    ///
    /// The dynamically-discovered overlay is frozen into an immutable parent
    /// snapshot and shared with the child. No map entries are cloned; only the
    /// `Arc` parent pointer is copied.
    pub fn copy_symbol_file_targets_to(&self, child: &mut CheckerContext<'_>) {
        // Untracked variant: attributes to `CheckerCreationReason::Other`.
        // Prefer `copy_symbol_file_targets_to_attributed` at call sites we
        // want to see in the per-reason dump.
        self.copy_symbol_file_targets_to_attributed(
            child,
            tsz_common::perf_counters::CheckerCreationReason::Other,
        );
    }

    /// Attributed overlay inheritance. Records the child's *visible* entry
    /// count (own + transitive parents) — meaningful under the Arc-snapshot
    /// model where nothing is physically copied. See `PERFORMANCE_PLAN.md` §4.T0.3.
    pub fn copy_symbol_file_targets_to_attributed(
        &self,
        child: &mut CheckerContext<'_>,
        reason: tsz_common::perf_counters::CheckerCreationReason,
    ) {
        let parent_snapshot = self
            .cross_file_symbol_targets
            .borrow_mut()
            .snapshot_for_child();
        if let Some(snap) = parent_snapshot.as_ref() {
            tsz_common::perf_counters::record_overlay_copy(reason, snap.total_entries() as u64);
        }
        child
            .cross_file_symbol_targets
            .borrow_mut()
            .install_parent_snapshot(parent_snapshot);
    }

    /// Merge the child checker's local overlay back into this context.
    ///
    /// After a child checker finishes, any new dynamically-discovered mappings
    /// it found are merged back into the parent's overlay.
    pub fn merge_symbol_file_targets_from(&self, child: &CheckerContext<'_>) {
        let child_overlay = child.cross_file_symbol_targets.borrow();
        if !child_overlay.is_empty() {
            let mut parent_overlay = self.cross_file_symbol_targets.borrow_mut();
            parent_overlay.merge_from(&child_overlay, true);
        }
    }

    /// Merge child symbol-file targets without overwriting mappings already
    /// known by the parent.
    pub fn merge_missing_symbol_file_targets_from(&self, child: &CheckerContext<'_>) {
        let child_overlay = child.cross_file_symbol_targets.borrow();
        if !child_overlay.is_empty() {
            let mut parent_overlay = self.cross_file_symbol_targets.borrow_mut();
            parent_overlay.merge_from(&child_overlay, false);
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
        self.lib_binders_cached = Arc::new(
            lib_contexts
                .iter()
                .map(|lc| Arc::clone(&lc.binder))
                .collect(),
        );
        self.lib_file_local_names = build_lib_file_local_names(&lib_contexts);
        self.lib_contexts = Arc::new(lib_contexts);
    }

    /// Set pre-wrapped Arc lib contexts (for O(1) sharing between checkers).
    ///
    /// Clears [`Self::lib_file_local_names`] rather than rebuilding it: the name
    /// index is `O(total lib symbols)` to build and this is the hot per-file
    /// path, so the caller shares a prebuilt index via
    /// [`Self::set_lib_file_local_names`] *after* this call (the parallel checker
    /// builds it once). Clearing also guarantees a stale index from a previous
    /// `lib_contexts` can never pair with these contexts; when the index is
    /// absent, identifier resolution falls back to the full lib scan, so
    /// correctness never depends on it being set.
    pub fn set_lib_contexts_shared(&mut self, lib_contexts: Arc<Vec<LibContext>>) {
        self.lib_binders_cached = Arc::new(
            lib_contexts
                .iter()
                .map(|lc| Arc::clone(&lc.binder))
                .collect(),
        );
        self.lib_file_local_names = None;
        self.lib_contexts = lib_contexts;
    }

    /// Share a prebuilt lib `file_locals` name index (see
    /// [`Self::lib_file_local_names`]). Cheap `Arc` clone; built once per program.
    pub fn set_lib_file_local_names(&mut self, names: Option<Arc<rustc_hash::FxHashSet<String>>>) {
        self.lib_file_local_names = names;
    }

    /// Whether `name` could be declared in any loaded lib `file_locals`.
    ///
    /// Returns `true` when the index is absent (forcing the full scan, so
    /// behavior is unchanged) and otherwise `true` only when the prebuilt index
    /// contains `name`. A `false` result means no lib context declares `name`,
    /// so a direct `lib_contexts` `file_locals` scan is a guaranteed no-op and
    /// can be skipped without changing the resolved symbol.
    #[must_use]
    pub fn lib_name_possible(&self, name: &str) -> bool {
        self.lib_file_local_names
            .as_ref()
            .is_none_or(|names| names.contains(name))
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
        //
        // These two maps depend only on the arena set (not the current file), so
        // they are identical for every per-file checker in a program. When
        // `ProgramContext` pre-populated them once via `apply_to`, skip the
        // per-file rebuild: recomputing them in every checker is an O(files)
        // pass (each calls `strip_known_extension` per source file) repeated
        // once per file, i.e. an O(files^2) scale-cliff term. The pre-populated
        // path keeps it O(files) for the whole program.
        if !self.module_specifiers_prebuilt {
            self.module_specifiers = Arc::new(Self::build_module_specifiers(&arenas));
            self.module_path_specifiers = Arc::new(Self::build_module_path_specifiers(&arenas));
        }
        // Build the reverse file-name index lazily when not pre-populated by ProgramContext.
        if self.global_file_name_index.is_none() && !arenas.is_empty() {
            self.global_file_name_index = Some(Arc::new(build_file_name_index(&arenas)));
        }
        self.all_arenas = Some(arenas);
    }

    /// Build a mapping from `file_id` -> module specifier for import-qualified type display.
    /// Returns `file_idx -> stem` for each source file in the arenas.
    pub(crate) fn build_module_specifiers(arenas: &[Arc<NodeArena>]) -> FxHashMap<u32, String> {
        let mut map = FxHashMap::default();
        for (idx, arena) in arenas.iter().enumerate() {
            for sf in &arena.source_files {
                let file_name = &sf.file_name;
                // Strip .ts/.tsx/.d.ts/.js/.jsx extension to get the module specifier
                let specifier = strip_known_extension(file_name);
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

    /// Build a mapping from `file_id` -> project-relative stripped path, used
    /// by cross-module diagnostic disambiguation. Unlike `build_module_specifiers`,
    /// this preserves any directory prefix (e.g. `src/library-a/index`) so
    /// `import("<path>").X` messages match tsc when the same short name lives
    /// in two different modules. The common absolute-directory prefix shared
    /// by all source files is stripped so temp-dir paths (e.g.
    /// `/private/var/folders/.../T/tmpABC/`) don't leak into diagnostics.
    pub(crate) fn build_module_path_specifiers(
        arenas: &[Arc<NodeArena>],
    ) -> FxHashMap<u32, String> {
        let mut paths: Vec<(u32, String)> = Vec::new();
        for (idx, arena) in arenas.iter().enumerate() {
            for sf in &arena.source_files {
                let specifier = strip_known_extension(&sf.file_name);
                paths.push((idx as u32, specifier.to_string()));
            }
        }

        // Compute the longest common directory prefix across all absolute
        // paths. Only absolute-path entries participate (lib / built-in files
        // often come in with their own absolute root and should not pull the
        // common prefix to `/`).
        let absolute: Vec<&str> = paths
            .iter()
            .filter_map(|(_, p)| p.starts_with('/').then_some(p.as_str()))
            .collect();
        let common = if absolute.len() >= 2 {
            let common = Self::longest_common_directory_prefix(&absolute);
            let common_dir = common.trim_end_matches('/');
            let common_basename = common_dir.rsplit('/').next().unwrap_or(common_dir);
            if common_basename == "src" {
                // Conformance virtual projects commonly root files under `/src`;
                // tsc keeps that segment in `import("src/...")` diagnostics.
                common_dir
                    .rsplit_once('/')
                    .map(|(parent, _)| {
                        if parent.is_empty() {
                            "/".to_string()
                        } else {
                            format!("{parent}/")
                        }
                    })
                    .unwrap_or_default()
            } else if common
                .trim_matches('/')
                .split('/')
                .filter(|component| !component.is_empty())
                .count()
                > 1
            {
                common
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let mut map = FxHashMap::default();
        for (idx, specifier) in paths {
            let trimmed = if !common.is_empty() && specifier.starts_with(&common) {
                specifier[common.len()..]
                    .trim_start_matches('/')
                    .to_string()
            } else {
                specifier.trim_start_matches('/').to_string()
            };
            map.insert(idx, trimmed);
        }
        map
    }

    /// Return the longest common directory prefix shared by all paths (may be
    /// empty). The returned prefix never splits a path component: it ends at
    /// the last `/` that every input has in the same position.
    fn longest_common_directory_prefix(paths: &[&str]) -> String {
        if paths.is_empty() {
            return String::new();
        }
        let first = paths[0];
        let mut end = first.len();
        for other in &paths[1..] {
            let new_end = first
                .char_indices()
                .zip(other.char_indices())
                .take_while(|((_, a), (_, b))| a == b)
                .map(|((i, c), _)| i + c.len_utf8())
                .last()
                .unwrap_or(0);
            end = end.min(new_end);
            if end == 0 {
                return String::new();
            }
        }
        // Trim back to the last `/` so we don't split a filename component.
        let prefix = &first[..end];
        match prefix.rfind('/') {
            Some(last_slash) => first[..=last_slash].to_string(),
            None => String::new(),
        }
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
        self.allow_source_file_test_pragmas = parent.allow_source_file_test_pragmas;
        self.current_directory = parent.current_directory.clone();
        self.resolved_modules = parent.resolved_modules.clone();
        self.global_file_locals_index = parent.global_file_locals_index.clone();
        self.global_module_exports_index = parent.global_module_exports_index.clone();
        self.global_declared_modules = parent.global_declared_modules.clone();
        self.global_expando_index = parent.global_expando_index.clone();
        self.global_module_augmentations_index = parent.global_module_augmentations_index.clone();
        self.global_augmentation_targets_index = parent.global_augmentation_targets_index.clone();
        self.global_module_binder_index = parent.global_module_binder_index.clone();
        self.global_arena_index = parent.global_arena_index.clone();
        self.global_file_name_index = parent.global_file_name_index.clone();
        self.lib_contexts = parent.lib_contexts.clone();
        self.lib_binders_cached = parent.lib_binders_cached.clone();
        self.set_actual_lib_file_count(parent.actual_lib_file_count);
        self.shared_lib_type_cache = parent.shared_lib_type_cache.clone();
        self.shared_constraint_proofs = parent.shared_constraint_proofs.clone();
        self.cross_file_type_params_cache = parent.cross_file_type_params_cache.clone();
        self.program_reexports = parent.program_reexports.clone();
        self.program_wildcard_reexports = parent.program_wildcard_reexports.clone();
        self.program_module_exports = parent.program_module_exports.clone();
        self.program_cross_file_node_symbols = parent.program_cross_file_node_symbols.clone();
        self.program_alias_partners = parent.program_alias_partners.clone();
        self.global_symbol_file_index = parent.global_symbol_file_index.clone();
        self.resolved_module_paths = parent.resolved_module_paths.clone();
        self.resolved_module_ts_extension_flags = parent.resolved_module_ts_extension_flags.clone();
        self.resolved_module_errors = parent.resolved_module_errors.clone();
        self.module_specifiers = parent.module_specifiers.clone();
        self.module_path_specifiers = parent.module_path_specifiers.clone();
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
        // If the 5 name-based global indices are already pre-populated (from ProgramContext),
        // skip the O(N) binder scans entirely. This is the fast path for multi-file
        // checking where ProgramContext::build_global_indices was called once at the driver level.
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
                    for module_spec in binder.module_exports.keys() {
                        dm.insert_module_name(module_spec);
                    }
                    for name in binder
                        .declared_modules
                        .iter()
                        .chain(binder.shorthand_ambient_modules.iter())
                    {
                        dm.insert_module_name(name);
                    }
                }
                dm.finish();
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
        // callers that don't use ProgramContext).
        let mut file_locals_index: FxHashMap<String, Vec<(usize, SymbolId)>> = FxHashMap::default();
        // outer_key = module specifier, inner = export name
        let mut module_exports_index: crate::context::ModuleExportsIndexMap = FxHashMap::default();
        let mut module_binder_index: FxHashMap<String, Vec<usize>> = FxHashMap::default();

        let has_skeleton_declared_modules = self.global_declared_modules.is_some();
        let mut declared_modules = if has_skeleton_declared_modules {
            None
        } else {
            Some(super::GlobalDeclaredModules::default())
        };

        for (file_idx, binder) in binders.iter().enumerate() {
            for (name, &sym_id) in binder.file_locals.iter() {
                if !binder.cross_file_local_is_visible(file_idx, name, sym_id) {
                    continue;
                }
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
                    dm.insert_module_name(module_spec);
                }
            }

            if let Some(ref mut dm) = declared_modules {
                for name in binder
                    .declared_modules
                    .iter()
                    .chain(binder.shorthand_ambient_modules.iter())
                {
                    dm.insert_module_name(name);
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
            dm.finish();
            self.global_declared_modules = Some(Arc::new(dm));
        }

        let mut module_augs_index: FxHashMap<String, Vec<(usize, tsz_binder::ModuleAugmentation)>> =
            FxHashMap::default();
        let mut aug_targets_index: FxHashMap<String, Vec<(tsz_binder::SymbolId, usize)>> =
            FxHashMap::default();
        let arena_to_file_idx = self.all_arenas.as_ref().map(|arenas| {
            arenas
                .iter()
                .enumerate()
                .map(|(file_idx, arena)| (Arc::as_ptr(arena) as usize, file_idx))
                .collect::<FxHashMap<_, _>>()
        });
        for (file_idx, binder) in binders.iter().enumerate() {
            for (module_spec, augmentations) in binder.module_augmentations.iter() {
                module_augs_index
                    .entry(module_spec.clone())
                    .or_default()
                    .extend(augmentations.iter().map(|aug| {
                        let owner_idx = aug
                            .arena
                            .as_ref()
                            .and_then(|arena| {
                                arena_to_file_idx.as_ref().and_then(|map| {
                                    map.get(&(Arc::as_ptr(arena) as usize)).copied()
                                })
                            })
                            .unwrap_or(file_idx);
                        (owner_idx, aug.clone())
                    }));
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
        paths: Arc<crate::context::ResolvedModuleRequestPathMap>,
    ) {
        self.resolved_module_request_paths = Some(paths);
    }

    /// Set resolved module specifiers (module names that exist in the project).
    /// Used to suppress TS2307 errors for known modules.
    ///
    /// Accepts either an owned `FxHashSet<String>` or an existing
    /// `Arc<FxHashSet<String>>`. The production per-file CLI driver
    /// shares the pre-bucketed set via `Arc::clone`; tests pass owned
    /// sets and pay a single `Arc::new` wrapping.
    pub fn set_resolved_modules(&mut self, modules: impl Into<Arc<FxHashSet<String>>>) {
        self.resolved_modules = Some(modules.into());
    }

    /// Set resolved module errors map for cross-file import resolution.
    /// Populated by the driver when `ModuleResolver` returns specific errors (TS2834, TS2835, TS2792, etc.).
    pub fn set_resolved_module_errors(
        &mut self,
        errors: Arc<crate::context::ResolvedModuleErrorMap>,
    ) {
        self.resolved_module_errors = Some(errors);
    }

    /// Set resolved module errors keyed by the full driver lookup request.
    pub fn set_resolved_module_request_errors(
        &mut self,
        errors: Arc<crate::context::ResolvedModuleRequestErrorMap>,
    ) {
        self.resolved_module_request_errors = Some(errors);
    }

    /// Get the resolution error for a specifier, if any.
    /// Returns the specific error (TS2834, TS2835, TS2792, etc.) if the module resolution failed with a known error.
    ///
    /// The driver records errors keyed on the exact user-written specifier,
    /// so this lookup must NOT fan out to extension-stripped stems. Two
    /// specifiers that share a stem can resolve very differently (e.g.
    /// `./index.js` succeeds via .js→.ts substitution while `./index`
    /// fails with TS2835), and matching by stem would attribute one
    /// specifier's error to the wrong import site.
    pub fn get_resolution_error(&self, specifier: &str) -> Option<&ResolutionError> {
        let errors = self.resolved_module_errors.as_ref()?;

        for candidate in crate::module_resolution::module_specifier_error_candidates(specifier) {
            if let Some(error) = errors.get(&(self.current_file_idx, candidate)) {
                return Some(error);
            }
        }
        None
    }

    /// Set the current file index.
    pub const fn set_current_file_idx(&mut self, idx: usize) {
        self.current_file_idx = idx;
    }

    /// Begin a fresh inference-placeholder naming scope for the file about to
    /// be checked.
    ///
    /// Names are namespaced by the current file index (program-unique) and the
    /// per-file counter is reset, so the `__infer_*` placeholder names a file
    /// produces are deterministic across runs and never collide with another
    /// file's placeholders under parallel checking. Must be called once per
    /// top-level file check, after `current_file_idx` is set to that file —
    /// not on the transient `current_file_idx` switches that cross-file import
    /// resolution performs.
    pub fn begin_file_inference_placeholders(&mut self) {
        // High 32 bits = file namespace, low 32 bits = sequence (reset to 0).
        self.inference_placeholder_state
            .set((self.current_file_idx as u64) << 32);
    }

    /// Allocate the next deterministic, program-unique inference-placeholder id
    /// for the current file. The high 32 bits carry the file namespace and the
    /// low 32 bits the per-file sequence, so distinct files never share an id.
    #[must_use]
    pub fn next_inference_placeholder_id(&self) -> u64 {
        let id = self.inference_placeholder_state.get();
        // Incrementing the packed value advances only the low (sequence) half;
        // a file would need 2^32 placeholders to overflow into the namespace.
        self.inference_placeholder_state.set(id.wrapping_add(1));
        id
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

    /// Resolve a [`StableLocation`] to a concrete `(NodeIndex, &NodeArena)`
    /// without going through [`tsz_binder::Symbol::value_declaration`] or
    /// [`tsz_binder::Symbol::declarations`].
    ///
    /// This is the Phase 1 step-2 bridge helper for the
    /// [global query graph architecture][plan]: consumers that used to read
    /// `symbol.primary_declaration()` (a raw `NodeIndex`) can instead read
    /// `symbol.stable_value_declaration` or `symbol.stable_declarations` and
    /// rehydrate the `NodeIndex` on demand. The resolved arena survives
    /// declaration-arena reshuffles because the lookup is driven by
    /// `(file_idx, pos, end)` rather than arena-local index identity.
    ///
    /// Returns `None` when:
    /// - `loc.is_known()` is false (pos/end both zero — unknown span),
    /// - the requested arena is not available (`all_arenas` not populated
    ///   and the location's `file_idx` is not the current file's), or
    /// - no node in the resolved arena matches `(pos, end)` exactly.
    ///
    /// When `loc.file_idx == u32::MAX` (single-file binding or not yet
    /// stamped), the current arena is used. This mirrors the fallback
    /// behavior of [`Self::get_arena_for_file`] for `u32::MAX`.
    ///
    /// The scan is currently O(N) over `arena.nodes`. The only caller at
    /// the moment (`CheckerState::class_extends_any_base`) is on the TS2551
    /// "Did you mean?" diagnostic path, which is cold. A span-index can be
    /// added later if hot paths migrate to this helper.
    ///
    /// [plan]: ../../../../../docs/plan/ROADMAP.md
    pub fn node_at_stable_location(&self, loc: StableLocation) -> Option<(NodeIndex, &NodeArena)> {
        if !loc.is_known() {
            return None;
        }
        let arena = if loc.has_file_idx() {
            // Only trust the stamped file_idx when we have the arena table
            // to resolve against. If `all_arenas` is absent (single-file
            // mode or cross-arena delegation not yet initialized), fall
            // back to the current arena — same contract as
            // `get_arena_for_file`.
            if self.all_arenas.is_some() {
                self.get_arena_for_file(loc.file_idx)
            } else {
                self.arena
            }
        } else {
            // Unstamped: use the current arena. This matches how
            // `class_extends_any_base` and similar legacy consumers
            // resolved `symbol.primary_declaration()` against
            // `self.ctx.arena`.
            self.arena
        };
        let node_idx = arena.nodes.iter().enumerate().find_map(|(i, node)| {
            (node.pos == loc.pos && node.end == loc.end).then_some(NodeIndex(i as u32))
        })?;
        Some((node_idx, arena))
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

    /// Try every file-name key variant (`./foo.ts`, `foo.ts`,
    /// backslash-normalized) against `map` and return the first match.
    ///
    /// Avoids allocating a candidate `Vec<String>` up front: direct
    /// matches and `./`-strip return immediately without building any
    /// owned strings, and the backslash-normalize / `./`-prefix branches
    /// only run when the common case misses.
    #[inline]
    fn lookup_any_file_key<'m, T>(
        file_name: &str,
        map: &'m rustc_hash::FxHashMap<String, T>,
    ) -> Option<&'m T> {
        // Direct match — common case, zero allocations.
        if let Some(v) = map.get(file_name) {
            return Some(v);
        }
        // Strip a leading `./` without allocating.
        if let Some(stripped) = file_name.strip_prefix("./")
            && let Some(v) = map.get(stripped)
        {
            return Some(v);
        }
        // Backslash-normalized variant (only allocates when input has backslashes).
        let normalized: Option<String> = if file_name.as_bytes().contains(&b'\\') {
            let n = file_name.replace('\\', "/");
            if let Some(v) = map.get(&n) {
                return Some(v);
            }
            Some(n)
        } else {
            None
        };
        let bare_prefix_needed = |c: &str| {
            !c.starts_with("./")
                && !c.starts_with("../")
                && !c.starts_with('/')
                && !c.starts_with(".\\")
                && !c.starts_with("..\\")
        };
        if bare_prefix_needed(file_name) {
            let prefixed = format!("./{file_name}");
            if let Some(v) = map.get(&prefixed) {
                return Some(v);
            }
        }
        if let Some(ref n) = normalized {
            if let Some(stripped) = n.strip_prefix("./")
                && let Some(v) = map.get(stripped)
            {
                return Some(v);
            }
            if bare_prefix_needed(n) {
                let prefixed = format!("./{n}");
                if let Some(v) = map.get(&prefixed) {
                    return Some(v);
                }
            }
        }
        None
    }

    /// Look up the re-export entries for `file_name` in the cross-file
    /// program-wide re-export map.
    ///
    /// Prefers `ProgramContext`-level `program_reexports` (a single `Arc`-shared
    /// allocation across all N cross-file lookup binders). Falls back to
    /// `binder.reexports` for standalone callers without a `ProgramContext`.
    /// Tries file-name key variants (`./foo.ts` / `foo.ts` / backslash-
    /// normalized).
    pub fn reexports_for_file<'b>(
        &'b self,
        binder: &'b tsz_binder::BinderState,
        file_name: &str,
    ) -> Option<&'b tsz_binder::FileReexports> {
        if let Some(ref idx) = self.program_reexports {
            return Self::lookup_any_file_key(file_name, idx);
        }
        Self::lookup_any_file_key(file_name, &binder.reexports)
    }

    /// See [`reexports_for_file`]: wildcard `export * from`.
    ///
    /// Each entry is `(source_module, is_type_only)`. `is_type_only` is `true`
    /// for `export type * from "X"` chains.
    pub fn wildcard_reexports_for_file<'b>(
        &'b self,
        binder: &'b tsz_binder::BinderState,
        file_name: &str,
    ) -> Option<&'b Vec<(String, bool)>> {
        if let Some(ref idx) = self.program_wildcard_reexports {
            return Self::lookup_any_file_key(file_name, idx.as_ref());
        }
        Self::lookup_any_file_key(file_name, &binder.wildcard_reexports)
    }

    /// Look up the module-exports table for a given module/file key.
    ///
    /// Prefers the project-wide `program_module_exports` (an `Arc`-shared
    /// allocation across all N cross-file lookup binders). Falls back to
    /// `binder.module_exports` for standalone callers without a
    /// `ProgramContext`. Tries file-name key variants
    /// (`./foo.ts` / `foo.ts` / backslash-normalized).
    pub fn module_exports_for_module<'b>(
        &'b self,
        binder: &'b tsz_binder::BinderState,
        module_key: &str,
    ) -> Option<&'b tsz_binder::SymbolTable> {
        let map: &'b rustc_hash::FxHashMap<String, tsz_binder::SymbolTable> =
            if let Some(ref idx) = self.program_module_exports {
                idx.as_ref()
            } else {
                binder.module_exports.as_ref()
            };
        if let Some(table) = Self::lookup_any_file_key(module_key, map) {
            return Some(table);
        }
        // Wildcard ambient-module fallback: a concrete specifier (e.g.
        // `./logo.svg`) satisfied by a *pattern* module (`declare module
        // "*.svg"`) stores its exports under the pattern key. Resolve the
        // specifier onto its matching pattern as tsc does, else bindings = `any`.
        self.lookup_wildcard_module_exports(module_key, map)
    }

    /// Resolve a concrete module specifier onto a declared *wildcard* ambient
    /// module's export table, when no exact `module_exports` key matched.
    ///
    /// Returns `None` for keys that are themselves wildcard patterns (a pattern
    /// is never resolved against another pattern) and when no declared pattern
    /// matches. The chosen pattern follows tsc's longest-prefix preference.
    fn lookup_wildcard_module_exports<'b>(
        &self,
        module_key: &str,
        map: &'b rustc_hash::FxHashMap<String, tsz_binder::SymbolTable>,
    ) -> Option<&'b tsz_binder::SymbolTable> {
        let normalized = module_key.trim().trim_matches('"').trim_matches('\'');
        if normalized.contains('*') {
            return None;
        }
        // Fast path: the project-wide skeleton index already separates the
        // wildcard patterns, so most projects (which declare none) skip the scan
        // entirely, and those that do match against a small pre-built list.
        if let Some(dm) = &self.global_declared_modules {
            if dm.patterns.is_empty() {
                return None;
            }
            return dm
                .best_matching_pattern(normalized)
                .and_then(|pattern| map.get(pattern));
        }
        // Standalone/test fallback (no skeleton index): scan the export map's own
        // keys for wildcard patterns, ranked by the same longest-prefix rule.
        crate::context::global_declared_modules::best_wildcard_match(
            map.keys().map(String::as_str),
            normalized,
        )
        .and_then(|key| map.get(key))
    }

    /// Like `module_exports_for_module` but tests existence only.
    pub fn module_exports_contains_module(
        &self,
        binder: &tsz_binder::BinderState,
        module_key: &str,
    ) -> bool {
        self.module_exports_for_module(binder, module_key).is_some()
    }

    /// Resolve a node → symbol lookup by arena pointer against the
    /// cross-file node-symbol map. Prefers the shared project-wide map
    /// installed by `ProgramContext::apply_to`; falls back to the per-binder
    /// copy for tests and standalone callers.
    pub fn cross_file_node_symbols_for_arena<'b>(
        &'b self,
        binder: &'b tsz_binder::BinderState,
        arena_ptr: usize,
    ) -> Option<&'b Arc<FxHashMap<u32, SymbolId>>> {
        if let Some(ref idx) = self.program_cross_file_node_symbols {
            return idx.get(&arena_ptr);
        }
        binder.cross_file_node_symbols.get(&arena_ptr)
    }

    /// Test whether `module_name` is declared as an ambient module anywhere
    /// in the project. Prefers the project-wide `global_declared_modules`
    /// index built from the skeleton; falls back to the per-binder
    /// `declared_modules` set for tests / standalone callers.
    pub fn declared_modules_contains(
        &self,
        binder: &tsz_binder::BinderState,
        module_name: &str,
    ) -> bool {
        if let Some(ref dm) = self.global_declared_modules {
            return dm.exact.contains(module_name);
        }
        binder.declared_modules.contains(module_name)
    }

    /// Resolve `sym_id` to its alias partner. Prefers the project-wide
    /// `program_alias_partners` map installed by `ProgramContext::apply_to`;
    /// falls back to per-binder `alias_partners` for tests/standalone callers.
    pub fn alias_partner_for(
        &self,
        binder: &tsz_binder::BinderState,
        sym_id: SymbolId,
    ) -> Option<SymbolId> {
        if let Some(ref ap) = self.program_alias_partners {
            return ap.get(&sym_id).copied();
        }
        binder.alias_partners.get(&sym_id).copied()
    }

    /// Test whether `sym_id` has an alias partner. Prefers the project-wide
    /// map; falls back to per-binder.
    pub fn alias_partners_contains(
        &self,
        binder: &tsz_binder::BinderState,
        sym_id: SymbolId,
    ) -> bool {
        if let Some(ref ap) = self.program_alias_partners {
            return ap.contains_key(&sym_id);
        }
        binder.alias_partners.contains_key(&sym_id)
    }

    /// Reverse lookup: find the `TYPE_ALIAS` partner that points at
    /// `alias_sym_id`. Used by the type-position symbol resolver to redirect
    /// an ALIAS symbol back to its merged `TYPE_ALIAS` counterpart. Prefers
    /// the project-wide map; falls back to the per-binder map for
    /// standalone callers.
    pub fn alias_partner_reverse(
        &self,
        binder: &tsz_binder::BinderState,
        alias_sym_id: SymbolId,
    ) -> Option<SymbolId> {
        if let Some(ref ap) = self.program_alias_partners {
            return ap.iter().find_map(|(&type_alias_id, &alias_id)| {
                (alias_id == alias_sym_id).then_some(type_alias_id)
            });
        }
        binder
            .alias_partners
            .iter()
            .find_map(|(&type_alias_id, &alias_id)| {
                (alias_id == alias_sym_id).then_some(type_alias_id)
            })
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
        let mut visited = FxHashSet::default();
        self.resolve_export_in_target_file(target_idx, member_name, &mut visited)
    }

    /// Resolve `export_name` from `target_idx`'s public surface, following named
    /// and wildcard re-export edges across binder boundaries.
    ///
    /// Unlike [`tsz_binder::BinderState::resolve_import_with_reexports_type_only`],
    /// this consults the program-wide export/re-export indexes through
    /// [`Self::module_exports_for_module`], [`Self::reexports_for_file`], and
    /// [`Self::wildcard_reexports_for_file`]. Those indexes are the authoritative
    /// source of truth in multi-file mode, where each file's own binder keeps its
    /// `module_exports`/`reexports`/`wildcard_reexports` tables empty and the
    /// data is hoisted into the program skeleton instead. Resolving a member
    /// through a re-exported namespace/alias that lives in another binder must go
    /// through this program-aware path or the lookup silently misses every
    /// export (the cause of the cross-binder TS2503/TS2339 family).
    pub fn resolve_export_in_target_file(
        &self,
        target_idx: usize,
        export_name: &str,
        visited: &mut FxHashSet<usize>,
    ) -> Option<tsz_binder::SymbolId> {
        if !visited.insert(target_idx) {
            return None;
        }
        let target_binder = self.get_binder_for_file(target_idx)?;
        let target_arena = self.get_arena_for_file(target_idx as u32);
        let file_name = target_arena.source_files.first()?.file_name.clone();

        // Direct exports (program-aware).
        if let Some(exports) = self.module_exports_for_module(target_binder, &file_name)
            && let Some(sym_id) = exports.get(export_name)
            && target_binder.get_symbol(sym_id).is_some()
        {
            self.register_symbol_file_target(sym_id, target_idx);
            return Some(sym_id);
        }

        // Named re-exports: `export { foo } from './other'` (and `as` renames).
        if let Some(reexports) = self.reexports_for_file(target_binder, &file_name)
            && let Some((source_module, original_name)) = reexports.get(export_name)
        {
            let name = original_name.as_deref().unwrap_or(export_name);
            if let Some(source_idx) =
                self.resolve_import_target_from_file(target_idx, source_module)
                && let Some(resolved) =
                    self.resolve_export_in_target_file(source_idx, name, visited)
            {
                return Some(resolved);
            }
        }

        // Wildcard re-exports: `export * from './other'`.
        if let Some(source_modules) = self.wildcard_reexports_for_file(target_binder, &file_name) {
            let source_modules = source_modules.clone();
            for (source_module, _is_type_only) in &source_modules {
                if let Some(source_idx) =
                    self.resolve_import_target_from_file(target_idx, source_module)
                    && let Some(resolved) =
                        self.resolve_export_in_target_file(source_idx, export_name, visited)
                {
                    return Some(resolved);
                }
            }
        }

        // Fallback: the target binder's own re-export resolution for
        // single-file / ambient-module binders whose local tables are
        // populated (e.g. `declare module "x" { ... }`).
        target_binder
            .resolve_import_with_reexports_type_only(&file_name, export_name)
            .map(|(sym_id, _)| {
                self.register_symbol_file_target(sym_id, target_idx);
                sym_id
            })
    }

    /// When `alias_id` is a *named* import bound to an `export * as NS from '<m>'`
    /// namespace re-export, return the file index of the re-exported module `<m>`
    /// — the backing module whose exports are the anchor's members. Returns
    /// `None` for any other alias shape.
    ///
    /// `tsc` treats such a named import as a type-position namespace anchor whose
    /// members are the exports of `<m>`. Because the member is not part of the
    /// *importing* module's own export surface, the ordinary re-export member
    /// lookup misses it; [`Self::resolve_member_via_namespace_reexport`] resolves
    /// the member through this backing file instead. The hop is keyed by file
    /// index + module specifier (never raw `SymbolId`), so cross-binder id
    /// collisions cannot interfere. This is also the structural predicate behind
    /// the "missing member is TS2694, not TS2503" diagnostic.
    pub(crate) fn namespace_reexport_anchor_backing_file(
        &self,
        alias_id: tsz_binder::SymbolId,
    ) -> Option<usize> {
        let alias = self.binder.get_symbol(alias_id)?;
        if !alias.has_any_flags(tsz_binder::symbol_flags::ALIAS) {
            return None;
        }
        // A whole-namespace import (`import * as NS`) is handled by the ordinary
        // re-export path; this targets a *named* binding (`import { NS }` /
        // `import { NS as X }`) of an `export * as NS` re-export. Check before
        // allocating below so the common star-import case stays allocation-free.
        let import_name = alias.import_name()?;
        if import_name == "*" {
            return None;
        }
        let import_module = alias.import_module()?.to_string();
        let import_name = import_name.to_string();
        // The alias's declaring file is the base for resolving the relative
        // `import_module` specifier. `resolve_symbol_file_index` can be polluted
        // by an earlier cross-file `register_symbol_file_target` (it pins the
        // importing alias to the *target* file), and a named import is local to
        // the current file anyway, so try the current file first and fall back to
        // the recorded index when it differs.
        let recorded = self
            .resolve_symbol_file_index(alias_id)
            .filter(|&recorded| recorded != self.current_file_idx);
        std::iter::once(self.current_file_idx)
            .chain(recorded)
            .find_map(|source_file_idx| {
                self.namespace_reexport_anchor_backing_file_from(
                    source_file_idx,
                    &import_module,
                    &import_name,
                )
            })
    }

    /// Resolve `NS.member` to its target `SymbolId` when `NS` (`alias_id`) is a
    /// named import bound to an `export * as NS` namespace re-export. Returns
    /// `None` when `alias_id` is not such an anchor or the module has no such
    /// member. Shared by the qualified-name type resolvers so the
    /// backing-file + export lookup lives in one place.
    pub(crate) fn resolve_member_via_namespace_reexport(
        &self,
        alias_id: tsz_binder::SymbolId,
        member_name: &str,
    ) -> Option<tsz_binder::SymbolId> {
        let backing_idx = self.namespace_reexport_anchor_backing_file(alias_id)?;
        let mut visited = rustc_hash::FxHashSet::default();
        self.resolve_export_in_target_file(backing_idx, member_name, &mut visited)
    }

    /// One attempt of [`Self::namespace_reexport_anchor_backing_file`] using
    /// `source_file_idx` as the base for the relative `import_module` specifier.
    fn namespace_reexport_anchor_backing_file_from(
        &self,
        source_file_idx: usize,
        import_module: &str,
        import_name: &str,
    ) -> Option<usize> {
        let target_idx = self.resolve_import_target_from_file(source_file_idx, import_module)?;
        let target_binder = self.get_binder_for_file(target_idx)?;
        let target_arena = self.get_arena_for_file(target_idx as u32);
        let target_file_name = target_arena.source_files.first()?.file_name.clone();
        let ns_sym_id = self
            .module_exports_for_module(target_binder, &target_file_name)
            .and_then(|exports| exports.get(import_name))?;
        let ns_sym = target_binder.get_symbol(ns_sym_id)?;
        // Only an `export * as NS` namespace re-export qualifies: it carries an
        // import module and the wildcard `*` import name.
        if !ns_sym.has_any_flags(tsz_binder::symbol_flags::ALIAS)
            || ns_sym.import_name() != Some("*")
        {
            return None;
        }
        let ns_module = ns_sym.import_module()?;
        self.resolve_import_target_from_file(target_idx, ns_module)
    }

    /// Extract the persistent cache from this context.
    /// This allows saving type checking results for future queries.
    pub fn extract_cache(self) -> TypeCache {
        let type_env = self.type_environment.into_inner();
        let boxed_types = type_env.snapshot_boxed_types();
        let boxed_def_ids = type_env.snapshot_boxed_def_ids();
        let mut def_to_symbol = self.def_to_symbol.into_inner();
        // The emitter reads `def_to_symbol` with no shared-store fallback (it
        // has no live `DefinitionStore`). When local caches populate lazily
        // during check (rather than via the eager whole-program warm), this
        // live map only holds the cross-file symbols this file happened to
        // touch, so complete it from the store's authoritative reverse mapping
        // before freezing. This keeps DTS/type-print symbol-name resolution
        // independent of how the live map was populated, and runs
        // O(program-symbols) once at extract time (emit/cache path only), never
        // in the check hot loop.
        for (raw_sym_id, def_id) in self.definition_store.all_symbol_mappings() {
            def_to_symbol
                .entry(def_id)
                .or_insert(tsz_binder::SymbolId(raw_sym_id));
        }
        // Build def_to_name from DefinitionStore so the emitter can print lib
        // symbol names (e.g., "Promise") without needing the lib binder arena.
        let mut def_to_name: FxHashMap<_, _> = def_to_symbol
            .keys()
            .filter_map(|&def_id| {
                self.definition_store
                    .get(def_id)
                    .map(|info| (def_id, self.types.resolve_atom(info.name)))
            })
            .collect();
        for (def_id, name_path) in self.definition_store.all_definition_names() {
            let name = name_path
                .iter()
                .map(|&atom| self.types.resolve_atom(atom))
                .collect::<Vec<_>>()
                .join(".");
            def_to_name.entry(def_id).or_insert(name);
        }
        TypeCache {
            symbol_types: self.symbol_types,
            symbol_instance_types: self.symbol_instance_types,
            node_types: self.node_types,
            symbol_dependencies: self.symbol_dependencies,
            def_to_symbol,
            def_to_name,
            def_types: type_env.snapshot_def_types(),
            def_type_params: type_env.snapshot_def_type_params(),
            boxed_types,
            boxed_def_ids,
            well_known_symbol_names: type_env.snapshot_well_known_symbol_names(),
            // Drop structural reference-path entries: their ids are assigned by
            // the per-run `flow_reference_keys` interner, so persisting them
            // across runs could alias a different path to the same id. They are
            // pure memo entries and are cheaply recomputed; real-symbol and
            // per-node keys are program-stable and kept.
            flow_analysis_cache: {
                let mut cache = self
                    .flow_shared
                    .flow_analysis_cache
                    .into_inner()
                    .into_inner();
                cache.retain(|(_, symbol, _), _| {
                    crate::control_flow::is_session_stable_flow_cache_symbol(*symbol)
                });
                cache
            },
            class_instance_type_to_decl: self.class_instance_type_to_decl,
            class_instance_type_cache: self.class_instance_type_cache,
            class_constructor_type_cache: self.class_constructor_type_cache,
            type_only_nodes: self.type_only_nodes,
            namespace_module_names: self.namespace_module_names,
        }
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
    use super::{CheckerContext, TypeCache};
    use rustc_hash::{FxHashMap, FxHashSet};
    use std::sync::Arc;
    use tsz_binder::BinderState;
    use tsz_binder::SymbolId;
    use tsz_common::checker_options::CheckerOptions;
    use tsz_parser::parser::NodeIndex;
    use tsz_parser::parser::node::NodeArena;
    use tsz_solver::TypeId;
    use tsz_solver::construction::TypeInterner;
    use tsz_solver::def::{DefinitionInfo, DefinitionStore};

    fn empty_cache() -> TypeCache {
        TypeCache {
            symbol_types: crate::context::SymbolTypeCache::new(),
            symbol_instance_types: crate::context::SymbolTypeCache::new(),
            node_types: crate::context::NodeTypeCache::new(),
            symbol_dependencies: FxHashMap::default(),
            def_to_symbol: FxHashMap::default(),
            def_to_name: FxHashMap::default(),
            def_types: FxHashMap::default(),
            def_type_params: FxHashMap::default(),
            boxed_types: FxHashMap::default(),
            boxed_def_ids: FxHashMap::default(),
            well_known_symbol_names: FxHashMap::default(),
            flow_analysis_cache: FxHashMap::default(),
            class_instance_type_to_decl: FxHashMap::default(),
            class_instance_type_cache: std::cell::RefCell::new(FxHashMap::default()),
            class_constructor_type_cache: std::cell::RefCell::new(FxHashMap::default()),
            type_only_nodes: FxHashSet::default(),
            namespace_module_names: FxHashMap::default(),
        }
    }

    #[test]
    fn type_cache_merge_keeps_constructor_type_cache() {
        let mut lhs = empty_cache();
        let rhs = empty_cache();

        rhs.class_constructor_type_cache
            .borrow_mut()
            .insert(NodeIndex(42), TypeId::STRING);

        lhs.merge(rhs);

        assert_eq!(
            lhs.class_constructor_type_cache
                .borrow()
                .get(&NodeIndex(42)),
            Some(&TypeId::STRING)
        );
    }

    #[test]
    fn type_cache_merge_keeps_error_class_type_cache_entries() {
        let mut lhs = empty_cache();
        let rhs = empty_cache();

        rhs.class_instance_type_cache
            .borrow_mut()
            .insert(NodeIndex(10), TypeId::ERROR);
        rhs.class_constructor_type_cache
            .borrow_mut()
            .insert(NodeIndex(11), TypeId::ERROR);

        lhs.merge(rhs);

        assert_eq!(
            lhs.class_instance_type_cache.borrow().get(&NodeIndex(10)),
            Some(&TypeId::ERROR)
        );
        assert_eq!(
            lhs.class_constructor_type_cache
                .borrow()
                .get(&NodeIndex(11)),
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
            .borrow_mut()
            .insert(NodeIndex(1), TypeId::NUMBER);
        cache
            .class_constructor_type_cache
            .borrow_mut()
            .insert(NodeIndex(2), TypeId::STRING);
        cache
            .class_instance_type_to_decl
            .insert(TypeId::BOOLEAN, NodeIndex(3));

        let affected = cache.invalidate_symbols(&[sym]);

        assert_eq!(affected, 1);
        assert!(cache.class_instance_type_cache.borrow().is_empty());
        assert!(cache.class_constructor_type_cache.borrow().is_empty());
        assert!(cache.class_instance_type_to_decl.is_empty());
    }

    #[test]
    fn extract_cache_keeps_definition_names_without_symbol_mapping() {
        let arena = NodeArena::new();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let store = Arc::new(DefinitionStore::new());
        let name = types.intern_string("ConcatArray");
        let def_id = store.register(DefinitionInfo::interface(name, Vec::new(), Vec::new()));

        let ctx = CheckerContext::new_with_shared_def_store(
            &arena,
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions::default(),
            store,
        );

        let cache = ctx.extract_cache();

        assert_eq!(
            cache.def_to_name.get(&def_id).map(String::as_str),
            Some("ConcatArray")
        );
    }

    #[test]
    fn lib_name_possible_gates_on_index_membership() {
        let arena = NodeArena::new();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let store = Arc::new(DefinitionStore::new());
        let mut ctx = CheckerContext::new_with_shared_def_store(
            &arena,
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions::default(),
            store,
        );

        // No index: every name forces the full scan (behavior unchanged).
        assert!(ctx.lib_name_possible("Anything"));
        assert!(ctx.lib_name_possible("HTMLDivElement"));

        // With an index, only names present in it may be in a lib `file_locals`.
        // A name absent from the index cannot match any `file_locals.get(name)`,
        // so the scan is safely skippable (`lib_name_possible == false`).
        let mut names = FxHashSet::default();
        names.insert("HTMLDivElement".to_string());
        names.insert("Array".to_string());
        ctx.set_lib_file_local_names(Some(Arc::new(names)));

        assert!(ctx.lib_name_possible("HTMLDivElement"));
        assert!(ctx.lib_name_possible("Array"));
        assert!(!ctx.lib_name_possible("MyProjectUtility"));
        assert!(!ctx.lib_name_possible("BuildTuple"));
    }

    #[test]
    fn type_cache_merge_dedupes_boxed_def_ids() {
        let mut lhs = empty_cache();
        let mut rhs = empty_cache();
        let def_id = tsz_solver::DefId(42);

        lhs.boxed_def_ids
            .insert(tsz_solver::IntrinsicKind::Function, vec![def_id]);
        rhs.boxed_def_ids
            .insert(tsz_solver::IntrinsicKind::Function, vec![def_id]);

        lhs.merge(rhs);

        assert_eq!(
            lhs.boxed_def_ids
                .get(&tsz_solver::IntrinsicKind::Function)
                .map(Vec::as_slice),
            Some(&[def_id][..])
        );
    }
}

impl super::ProgramContext {
    /// Build the shared `SymbolId` → file-index map from `symbol_file_targets`.
    ///
    /// Call this once after populating `symbol_file_targets`. The resulting
    /// `Arc<FxHashMap>` is shared (O(1) clone) across all checkers, eliminating
    /// the per-checker O(N) copy into `cross_file_symbol_targets`.
    pub fn build_global_symbol_file_index(&mut self) {
        let mut map: FxHashMap<SymbolId, usize> =
            FxHashMap::with_capacity_and_hasher(self.symbol_file_targets.len(), Default::default());
        for &(sym_id, file_idx) in self.symbol_file_targets.iter() {
            map.insert(sym_id, file_idx);
        }
        self.global_symbol_file_index = Some(Arc::new(map));
    }

    /// Build global indices only when the skeleton fingerprint has changed.
    ///
    /// Compares `new_fingerprint` against `self.last_skeleton_fingerprint`.
    /// If they match, the global indices are already valid and the expensive
    /// O(N) binder scan is skipped entirely. If they differ (or this is the
    /// first build), delegates to `build_global_indices` and stores the new
    /// fingerprint for future comparisons.
    ///
    /// Returns `true` if indices were rebuilt, `false` if cached.
    pub fn build_global_indices_if_changed(&mut self, new_fingerprint: u64) -> bool {
        if self.last_skeleton_fingerprint == Some(new_fingerprint) {
            // All global indices (name-based + arena) + skeleton indices are still valid.
            return false;
        }
        self.build_global_indices();
        self.last_skeleton_fingerprint = Some(new_fingerprint);
        true
    }
}

#[cfg(test)]
#[path = "core_index_tests.rs"]
mod index_tests;
