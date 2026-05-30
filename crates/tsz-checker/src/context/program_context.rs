//! Project-wide context shared across all per-file [`CheckerContext`]s.
//!
//! Extracted from `mod.rs` to keep that file within the 2000-line hard limit (CLAUDE.md §19).
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::{BinderState, LibContext, ModuleAugmentation, SymbolId};
use tsz_parser::parser::node::NodeArena;
use tsz_solver::def::DefinitionStore;

use super::{
    CheckerContext, CrossFileTypeParamsCache, GlobalAugmentationTargetsIndex,
    GlobalDeclaredModules, GlobalFileLocalsIndex, GlobalModuleAugmentationsIndex,
    GlobalModuleExportsIndex, ModuleExportsIndexMap, ProgramWildcardReexportsTypeOnly,
    ResolvedModuleErrorMap, ResolvedModulePathMap, ResolvedModuleRequestErrorMap,
    ResolvedModuleRequestPathMap, ResolvedModuleTsExtensionMap,
    next_source_file_symbol_type_cache_scope,
};

/// Project-wide shared environment for multi-file type checking.
///
/// Captures all the state that is identical across every per-file `CheckerContext`
/// in a project check run. Drivers (CLI, LSP) build one `ProgramContext` after merge
/// and call [`ProgramContext::apply_to`] on each checker instead of repeating 10+
/// setter calls per file.
///
/// This struct is `Clone`-cheap because every field is either `Arc`-wrapped or `Copy`.
#[derive(Clone)]
pub struct ProgramContext {
    /// Lib file contexts for global type resolution.
    pub lib_contexts: Arc<Vec<LibContext>>,
    /// All AST arenas for cross-file resolution (indexed by `file_idx`).
    pub all_arenas: Arc<Vec<Arc<NodeArena>>>,
    /// All binders for cross-file resolution (indexed by `file_idx`).
    pub all_binders: Arc<Vec<Arc<BinderState>>>,
    pub source_file_symbol_type_cache_scope: u64,
    /// Pre-computed declared modules from skeleton index.
    pub skeleton_declared_modules: Option<Arc<GlobalDeclaredModules>>,
    /// Pre-computed expando index from skeleton index.
    pub skeleton_expando_index: Option<Arc<FxHashMap<String, FxHashSet<String>>>>,
    /// Pre-computed module-augmentations index built from `SkeletonIndex`.
    ///
    /// Phase 2 step 2: when set, [`Self::build_global_indices`] skips the
    /// per-binder `module_augmentations` loop and reuses this `Arc` for the
    /// `global_module_augmentations_index` slot. Drivers populate this from
    /// `SkeletonIndex::module_augmentations_for(...)` so that — once arenas
    /// become evictable in Phase 5 — the merged augmentations index can be
    /// built without retaining per-file binder state.
    pub skeleton_module_augmentations_index: Option<GlobalModuleAugmentationsIndex>,
    /// Pre-computed augmentation-targets index built from `SkeletonIndex`
    /// (Phase 2 step 3).
    ///
    /// When set, [`Self::build_global_indices`] skips the per-binder
    /// `augmentation_target_modules` loop and reuses this `Arc` for the
    /// `global_augmentation_targets_index` slot. Drivers populate this from
    /// `SkeletonIndex::build_augmentation_targets_index(...)` so that — once
    /// arenas become evictable in Phase 5 — the merged augmentation-targets
    /// index can be built without retaining per-file binder state.
    pub skeleton_augmentation_targets_index: Option<GlobalAugmentationTargetsIndex>,
    /// Pre-computed module-binder index built from `SkeletonIndex`
    /// (Phase 2 step 4).
    ///
    /// When set, [`Self::build_global_indices`] skips the per-binder
    /// `module_binder_index.entry(...).push(file_idx)` lines in the
    /// `binder.module_exports.iter()` loop and reuses this `Arc` for the
    /// `global_module_binder_index` slot. Drivers populate this from
    /// `SkeletonIndex::build_module_binder_index(...)` so that — once arenas
    /// become evictable in Phase 5 — the merged module-binder index can be
    /// built without retaining per-file binder state.
    ///
    /// Note: the surrounding loop also builds `module_exports_index` and the
    /// `declared_modules` projection from the same iteration, so only the
    /// `module_binder_index` portion is skipped — the rest of the loop body
    /// continues to run.
    pub skeleton_module_binder_index: Option<Arc<FxHashMap<String, Vec<usize>>>>,
    /// Pre-computed module-exports index built from `SkeletonIndex`
    /// (Phase 2 step 6).
    ///
    /// When set, [`Self::build_global_indices`] skips the per-binder
    /// `for (export_name, &sym_id) in exports.iter()` inner loop in the
    /// `binder.module_exports.iter()` pass and reuses this `Arc` for the
    /// `global_module_exports_index` slot. Drivers populate this from
    /// `SkeletonIndex::build_module_exports_index(&program.module_exports)`
    /// — note the projection consumes the post-merge `program.module_exports`
    /// (which holds globally-remapped `SymbolIds`), NOT per-binder data, so
    /// once arenas become evictable in Phase 5 the merged module-exports
    /// index can still be built without retaining per-file binder state.
    ///
    /// SymbolId-coupling rationale: the projection passes through globally-
    /// remapped `SymbolIds` — exactly what consumers (e.g. `type_only.rs`,
    /// `state/type_resolution/module.rs`) expect to dereference against
    /// `all_binders[file_idx]`. Pre-merge local `SymbolIds` (the trap in
    /// PR #1145 for the file-locals index) are intentionally NOT recorded
    /// in the skeleton at extract time.
    pub skeleton_module_exports_index: Option<GlobalModuleExportsIndex>,
    /// Pre-computed symbol-to-file ownership targets (legacy vec form).
    pub symbol_file_targets: Arc<Vec<(SymbolId, usize)>>,
    /// Pre-built O(1) index: `SymbolId` -> owning file index.
    ///
    /// Built once from `symbol_file_targets` by `build_global_symbol_file_index()`.
    /// Shared across all checkers via `Arc` — child checkers clone the `Arc`
    /// reference (O(1)) instead of cloning the entire `FxHashMap`.
    /// Read sites fall back to this base map when the local
    /// `cross_file_symbol_targets` overlay has no entry.
    pub global_symbol_file_index: Option<Arc<FxHashMap<SymbolId, usize>>>,
    /// Pre-computed global `file_locals` index: name -> Vec<(`file_idx`, SymbolId)>.
    /// Built once from all binders; shared across all checkers via `Arc`.
    pub global_file_locals_index: Option<GlobalFileLocalsIndex>,
    /// Pre-computed global `module_exports` index: (specifier, `export_name`) -> Vec<(`file_idx`, SymbolId)>.
    /// Built once from all binders; shared across all checkers via `Arc`.
    pub global_module_exports_index: Option<GlobalModuleExportsIndex>,
    /// Pre-computed global module augmentations index: specifier -> Vec<(`file_idx`, `ModuleAugmentation`)>.
    /// Built once from all binders; shared across all checkers via `Arc`.
    pub global_module_augmentations_index: Option<GlobalModuleAugmentationsIndex>,
    /// Pre-computed global augmentation targets index: specifier -> Vec<(SymbolId, `file_idx`)>.
    /// Built once from all binders; shared across all checkers via `Arc`.
    pub global_augmentation_targets_index: Option<GlobalAugmentationTargetsIndex>,
    /// Pre-computed global module binder index: module name -> Vec<`binder_idx`>.
    /// Built once from all binders; shared across all checkers via `Arc`.
    pub global_module_binder_index: Option<Arc<FxHashMap<String, Vec<usize>>>>,
    /// Pre-computed arena-pointer → file-index map. O(1) arena→binder lookups.
    pub global_arena_index: Option<Arc<FxHashMap<usize, usize>>>,
    /// Pre-computed filename reverse index; see `CheckerContext::global_file_name_index`.
    pub global_file_name_index: Option<Arc<crate::module_resolution::FileNameIndex>>,
    /// Program-wide re-export index shared across cross-file lookup binders;
    /// see `CheckerContext::program_reexports`.
    pub program_reexports: Option<Arc<tsz_binder::FileReexportsMap>>,
    pub program_wildcard_reexports: Option<Arc<FxHashMap<String, Vec<String>>>>,
    pub program_wildcard_reexports_type_only: Option<ProgramWildcardReexportsTypeOnly>,
    /// Program-wide module-exports index; see `CheckerContext::program_module_exports`.
    pub program_module_exports: Option<Arc<FxHashMap<String, tsz_binder::SymbolTable>>>,
    /// Program-wide cross-file node-symbol map; see
    /// `CheckerContext::program_cross_file_node_symbols`.
    pub program_cross_file_node_symbols: Option<Arc<tsz_binder::CrossFileNodeSymbols>>,
    /// see `CheckerContext::program_alias_partners`.
    pub program_alias_partners: Option<Arc<FxHashMap<SymbolId, SymbolId>>>,
    /// Resolved module paths: (`source_file_idx`, specifier) -> `target_file_idx`.
    pub resolved_module_paths: Arc<ResolvedModulePathMap>,
    /// Resolved module paths keyed by (`source_file_idx`, specifier, resolution-mode override).
    pub resolved_module_request_paths: Arc<ResolvedModuleRequestPathMap>,
    /// `resolvedUsingTsExtension` flag per resolved import, mirroring tsc.
    /// Populated by the driver from `ModuleLookupResult.resolved_using_ts_extension`
    /// and consulted by the TS2877 emission gate.
    pub resolved_module_ts_extension_flags: Arc<ResolvedModuleTsExtensionMap>,
    /// Resolved module errors: (`source_file_idx`, specifier) -> error details.
    pub resolved_module_errors: Arc<ResolvedModuleErrorMap>,
    /// Resolved module errors keyed by (`source_file_idx`, specifier, resolution-mode override).
    pub resolved_module_request_errors: Arc<ResolvedModuleRequestErrorMap>,
    /// Per-file external module status.
    pub is_external_module_by_file: Arc<FxHashMap<String, bool>>,
    /// Per-file ESM/CJS determination.
    pub file_is_esm_map: Arc<FxHashMap<String, bool>>,
    /// Whether a @typescript/lib-dom replacement was loaded, and its window/self globals.
    pub typescript_dom_replacement_globals: (bool, bool, bool),
    /// Whether TS5107/TS5101 deprecation diagnostics are present.
    pub has_deprecation_diagnostics: bool,
    /// Skeleton fingerprint from the last `build_global_indices` call.
    ///
    /// When set, `build_global_indices_if_changed` can compare the new skeleton
    /// fingerprint against this value and skip the expensive O(N) binder scan
    /// when the project topology is unchanged.
    pub last_skeleton_fingerprint: Option<u64>,
    /// Shared `DefinitionStore` for parallel checking.
    /// When set, all parallel checkers share this store for globally unique `DefIds`.
    pub shared_definition_store: Option<Arc<DefinitionStore>>,
    /// T2.2 program-wide memo for cross-file type-parameter extraction.
    /// Mirrors `CheckerContext::cross_file_type_params_cache`; built once
    /// per project run by the driver and shared via `Arc` across every
    /// checker.
    pub cross_file_type_params_cache: Option<CrossFileTypeParamsCache>,
}

impl Default for ProgramContext {
    fn default() -> Self {
        Self {
            lib_contexts: Arc::new(vec![]),
            all_arenas: Arc::new(vec![]),
            all_binders: Arc::new(vec![]),
            source_file_symbol_type_cache_scope: next_source_file_symbol_type_cache_scope(),
            skeleton_declared_modules: None,
            skeleton_expando_index: None,
            skeleton_module_augmentations_index: None,
            skeleton_augmentation_targets_index: None,
            skeleton_module_binder_index: None,
            skeleton_module_exports_index: None,
            symbol_file_targets: Arc::new(vec![]),
            global_symbol_file_index: None,
            global_file_locals_index: None,
            global_module_exports_index: None,
            global_module_augmentations_index: None,
            global_augmentation_targets_index: None,
            global_module_binder_index: None,
            global_arena_index: None,
            global_file_name_index: None,
            program_reexports: None,
            program_wildcard_reexports: None,
            program_wildcard_reexports_type_only: None,
            program_module_exports: None,
            program_cross_file_node_symbols: None,
            program_alias_partners: None,
            resolved_module_paths: Arc::new(FxHashMap::default()),
            resolved_module_request_paths: Arc::new(FxHashMap::default()),
            resolved_module_ts_extension_flags: Arc::new(FxHashMap::default()),
            resolved_module_errors: Arc::new(FxHashMap::default()),
            resolved_module_request_errors: Arc::new(FxHashMap::default()),
            is_external_module_by_file: Arc::new(FxHashMap::default()),
            file_is_esm_map: Arc::new(FxHashMap::default()),
            typescript_dom_replacement_globals: (false, false, false),
            has_deprecation_diagnostics: false,
            last_skeleton_fingerprint: None,
            shared_definition_store: None,
            cross_file_type_params_cache: None,
        }
    }
}

impl ProgramContext {
    /// Apply all project-level shared state to a checker context.
    ///
    /// This replaces the 10+ individual setter calls that drivers previously
    /// repeated at every checker creation site. The order of operations matches
    /// the original driver pattern: skeleton indices are set before `set_all_binders`
    /// so the binder scan can be skipped for `declared_modules` and expando.
    pub fn apply_to(&self, ctx: &mut CheckerContext<'_>) {
        if !self.lib_contexts.is_empty() {
            ctx.set_lib_contexts_shared(Arc::clone(&self.lib_contexts));
            ctx.set_actual_lib_file_count(self.lib_contexts.len());
        }
        ctx.set_typescript_dom_replacement_globals(
            self.typescript_dom_replacement_globals.0,
            self.typescript_dom_replacement_globals.1,
            self.typescript_dom_replacement_globals.2,
        );
        ctx.set_has_deprecation_diagnostics(self.has_deprecation_diagnostics);
        // Pre-install global indices before set_all_arenas/set_all_binders so
        // those methods can skip re-computing indices already provided here.
        if let Some(ref idx) = self.global_file_name_index {
            ctx.global_file_name_index = Some(Arc::clone(idx));
        }
        ctx.set_all_arenas(Arc::clone(&self.all_arenas));
        if let Some(ref dm) = self.skeleton_declared_modules {
            ctx.set_declared_modules_from_skeleton(Arc::clone(dm));
        }
        if let Some(ref ei) = self.skeleton_expando_index {
            ctx.set_expando_index_from_skeleton(Arc::clone(ei));
        }
        // Pre-install remaining global indices before set_all_binders so it
        // can skip re-computing them. This avoids O(N) binder scans per checker.
        if let Some(ref idx) = self.global_file_locals_index {
            ctx.global_file_locals_index = Some(Arc::clone(idx));
        }
        if let Some(ref idx) = self.global_module_exports_index {
            ctx.global_module_exports_index = Some(Arc::clone(idx));
        }
        if let Some(ref idx) = self.global_module_augmentations_index {
            ctx.global_module_augmentations_index = Some(Arc::clone(idx));
        }
        if let Some(ref idx) = self.global_augmentation_targets_index {
            ctx.global_augmentation_targets_index = Some(Arc::clone(idx));
        }
        if let Some(ref idx) = self.global_module_binder_index {
            ctx.global_module_binder_index = Some(Arc::clone(idx));
        }
        if let Some(ref idx) = self.global_arena_index {
            ctx.global_arena_index = Some(Arc::clone(idx));
        }
        if let Some(ref m) = self.program_reexports {
            ctx.program_reexports = Some(Arc::clone(m));
        }
        if let Some(ref m) = self.program_wildcard_reexports {
            ctx.program_wildcard_reexports = Some(Arc::clone(m));
        }
        if let Some(ref m) = self.program_wildcard_reexports_type_only {
            ctx.program_wildcard_reexports_type_only = Some(Arc::clone(m));
        }
        if let Some(ref m) = self.program_module_exports {
            ctx.program_module_exports = Some(Arc::clone(m));
        }
        if let Some(ref m) = self.program_cross_file_node_symbols {
            ctx.program_cross_file_node_symbols = Some(Arc::clone(m));
        }
        if let Some(ref m) = self.program_alias_partners {
            ctx.program_alias_partners = Some(Arc::clone(m));
        }
        // Install the shared DefinitionStore before gating expensive semantic-def
        // prepopulation so `is_fully_populated()` reflects project-wide state.
        if let Some(ref store) = self.shared_definition_store {
            ctx.definition_store = Arc::clone(store);
            ctx.share_owner_symbol_type_results = true;
        }
        ctx.definition_store
            .set_source_file_symbol_type_cache_scope(self.source_file_symbol_type_cache_scope);
        ctx.set_all_binders(Arc::clone(&self.all_binders));
        // When the shared DefinitionStore was fully populated (via from_semantic_defs
        // during project setup), skip the expensive per-binder iteration. Instead,
        // rely on warm_local_caches_from_shared_store() (called below) to populate
        // local caches from the globally unique DefId assignments. Heritage resolution
        // was already done in from_semantic_defs, so skip that too.
        //
        // This eliminates O(files * total_defs * DashMap_lookup) work when many files
        // are checked in parallel -- the root cause of hangs on large type libraries
        // like ts-toolbelt with hundreds of interrelated type definitions.
        if !ctx.definition_store.is_fully_populated() {
            // Pre-populate DefIds from all cross-file binders' semantic_defs.
            ctx.pre_populate_def_ids_from_all_binders();
            // Resolve cross-batch heritage now that all DefIds from all binders
            // are registered. This wires up extends/implements at the DefId level.
            ctx.resolve_cross_batch_heritage();
        }
        // Install the shared O(1) symbol→file index. When present, all base entries
        // are accessible via `resolve_symbol_file_index()`, so we skip the O(N) copy
        // into the local overlay. Only fall back to the O(N) copy when no global
        // index was built (e.g., in tests that don't call `build_global_symbol_file_index`).
        if let Some(ref idx) = self.global_symbol_file_index {
            ctx.global_symbol_file_index = Some(Arc::clone(idx));
        } else if !self.symbol_file_targets.is_empty() {
            let mut targets = ctx.cross_file_symbol_targets.borrow_mut();
            for &(sym_id, owner_idx) in self.symbol_file_targets.iter() {
                targets.insert(sym_id, owner_idx);
            }
        }
        ctx.set_resolved_module_paths(Arc::clone(&self.resolved_module_paths));
        ctx.set_resolved_module_request_paths(Arc::clone(&self.resolved_module_request_paths));
        ctx.set_resolved_module_ts_extension_flags(Arc::clone(
            &self.resolved_module_ts_extension_flags,
        ));
        ctx.set_resolved_module_errors(Arc::clone(&self.resolved_module_errors));
        ctx.set_resolved_module_request_errors(Arc::clone(&self.resolved_module_request_errors));
        ctx.is_external_module_by_file = Some(Arc::clone(&self.is_external_module_by_file));
        ctx.file_is_esm_map = Some(Arc::clone(&self.file_is_esm_map));
        // Warm local caches from the already-installed shared DefinitionStore.
        if self.shared_definition_store.is_some() {
            ctx.warm_local_caches_from_shared_store();
        }
        if let Some(ref m) = self.cross_file_type_params_cache {
            ctx.cross_file_type_params_cache = Some(Arc::clone(m));
        }
    }

    /// Build the 4 global binder indices from `all_binders`.
    ///
    /// This is the same computation that `set_all_binders` does, but factored out
    /// so drivers can compute it once and share via `Arc` across all checkers.
    /// When these fields are `Some`, `set_all_binders` skips re-computing them.
    pub fn build_global_indices(&mut self) {
        self.source_file_symbol_type_cache_scope = next_source_file_symbol_type_cache_scope();

        // Phase 2 step 2: when the driver pre-built
        // `skeleton_module_augmentations_index` from `SkeletonIndex`, skip the
        // per-binder `module_augmentations` loop entirely and reuse the
        // pre-built map. This unblocks Phase 5 — the merged augmentations
        // index no longer needs per-file binder state.
        let has_skeleton_module_augmentations = self.skeleton_module_augmentations_index.is_some();
        // Phase 2 step 3: when the driver pre-built
        // `skeleton_augmentation_targets_index` from `SkeletonIndex`, skip the
        // per-binder `augmentation_target_modules` loop entirely and reuse the
        // pre-built map. This unblocks Phase 5 — the merged augmentation-targets
        // index no longer needs per-file binder state.
        let has_skeleton_aug_targets = self.skeleton_augmentation_targets_index.is_some();
        // Phase 2 step 4: when the driver pre-built
        // `skeleton_module_binder_index` from `SkeletonIndex`, skip the
        // module-binder-index push lines inside the per-binder
        // `module_exports.iter()` loop and reuse the pre-built map. This
        // unblocks Phase 5 — the merged module-binder index no longer needs
        // per-file binder state.
        let has_skeleton_module_binders = self.skeleton_module_binder_index.is_some();
        // Phase 2 step 6: when the driver pre-built
        // `skeleton_module_exports_index` from `SkeletonIndex` +
        // `program.module_exports`, skip the inner `for (export_name, sym_id)
        // in exports.iter()` push loop and reuse the pre-built map. This
        // unblocks Phase 5 — the merged module-exports index no longer needs
        // per-file binder state.
        let has_skeleton_module_exports = self.skeleton_module_exports_index.is_some();

        let mut file_locals_name_counts: FxHashMap<&str, usize> = FxHashMap::default();
        let mut module_exports_capacity = 0usize;
        let mut module_binder_capacity = 0usize;
        let mut module_augs_capacity = 0usize;
        let mut aug_targets_capacity = 0usize;
        let mut declared_modules_capacity = 0usize;
        let mut expando_capacity = 0usize;
        for binder in self.all_binders.iter() {
            for (name, _) in binder.file_locals.iter() {
                *file_locals_name_counts.entry(name.as_str()).or_default() += 1;
            }
            if !has_skeleton_module_exports {
                module_exports_capacity += binder.module_exports.len();
            }
            if !has_skeleton_module_binders {
                module_binder_capacity += binder.module_exports.len().saturating_mul(2);
            }
            if !has_skeleton_module_augmentations {
                module_augs_capacity += binder.module_augmentations.len();
            }
            if !has_skeleton_aug_targets {
                aug_targets_capacity += binder.augmentation_target_modules.len();
            }
            if self.skeleton_declared_modules.is_none() {
                declared_modules_capacity += binder.module_exports.len();
                declared_modules_capacity += binder.declared_modules.len();
                declared_modules_capacity += binder.shorthand_ambient_modules.len();
            }
            if self.skeleton_expando_index.is_none() {
                expando_capacity += binder.expando_properties.len();
            }
        }

        let mut file_locals_index: FxHashMap<String, Vec<(usize, SymbolId)>> =
            FxHashMap::with_capacity_and_hasher(file_locals_name_counts.len(), Default::default());
        let mut module_exports_index: ModuleExportsIndexMap =
            FxHashMap::with_capacity_and_hasher(module_exports_capacity, Default::default());
        let mut module_augs_index: FxHashMap<String, Vec<(usize, ModuleAugmentation)>> =
            FxHashMap::with_capacity_and_hasher(module_augs_capacity, Default::default());
        let mut aug_targets_index: FxHashMap<String, Vec<(SymbolId, usize)>> =
            FxHashMap::with_capacity_and_hasher(aug_targets_capacity, Default::default());
        let mut module_binder_index: FxHashMap<String, Vec<usize>> =
            FxHashMap::with_capacity_and_hasher(module_binder_capacity, Default::default());
        // Also build declared_modules if not already from skeleton.
        let mut declared_modules = if self.skeleton_declared_modules.is_some() {
            None
        } else {
            Some(GlobalDeclaredModules {
                exact: FxHashSet::with_capacity_and_hasher(
                    declared_modules_capacity,
                    Default::default(),
                ),
                patterns: Vec::new(),
                pattern_set: None,
            })
        };
        let arena_to_file_idx: FxHashMap<usize, usize> = self
            .all_arenas
            .iter()
            .enumerate()
            .map(|(file_idx, arena)| (Arc::as_ptr(arena) as usize, file_idx))
            .collect();

        for (file_idx, binder) in self.all_binders.iter().enumerate() {
            let arena = self.all_arenas.get(file_idx).map(Arc::as_ref);
            for (name, &sym_id) in binder.file_locals.iter() {
                if !binder.cross_file_local_is_visible(arena, file_idx, name, sym_id) {
                    continue;
                }
                file_locals_index
                    .entry(name.to_string())
                    .or_insert_with(|| {
                        Vec::with_capacity(
                            file_locals_name_counts
                                .get(name.as_str())
                                .copied()
                                .unwrap_or(1),
                        )
                    })
                    .push((file_idx, sym_id));
            }
            for (module_spec, exports) in binder.module_exports.iter() {
                // Phase 2 step 4: skip the per-binder module_binder_index
                // pushes when the skeleton-built map is already installed.
                // The driver pre-built it from
                // `SkeletonIndex::build_module_binder_index(...)`.
                if !has_skeleton_module_binders {
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
                }
                // Phase 2 step 6: skip the per-binder module_exports_index
                // pushes when the skeleton-built map is already installed.
                // The driver pre-built it from
                // `SkeletonIndex::build_module_exports_index(&program.module_exports)`.
                if !has_skeleton_module_exports {
                    for (export_name, &sym_id) in exports.iter() {
                        module_exports_index
                            .entry(module_spec.clone())
                            .or_insert_with(|| {
                                FxHashMap::with_capacity_and_hasher(
                                    exports.len(),
                                    Default::default(),
                                )
                            })
                            .entry(export_name.to_string())
                            .or_default()
                            .push((file_idx, sym_id));
                    }
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
            // Phase 2 step 2: skip the per-binder module_augmentations loop
            // when the skeleton-built map is already installed. The driver
            // pre-built it from `SkeletonIndex::module_augmentations_for(...)`.
            if !has_skeleton_module_augmentations {
                for (module_spec, augmentations) in binder.module_augmentations.iter() {
                    module_augs_index
                        .entry(module_spec.clone())
                        .or_default()
                        .extend(augmentations.iter().map(|aug| {
                            let owner_idx = aug
                                .arena
                                .as_ref()
                                .and_then(|arena| {
                                    arena_to_file_idx
                                        .get(&(Arc::as_ptr(arena) as usize))
                                        .copied()
                                })
                                .unwrap_or(file_idx);
                            (owner_idx, aug.clone())
                        }));
                }
            }
            // Phase 2 step 3: skip the per-binder augmentation_target_modules
            // loop when the skeleton-built map is already installed. The driver
            // pre-built it from `SkeletonIndex::build_augmentation_targets_index(...)`.
            if !has_skeleton_aug_targets {
                for (&sym_id, module_spec) in binder.augmentation_target_modules.iter() {
                    aug_targets_index
                        .entry(module_spec.clone())
                        .or_default()
                        .push((sym_id, file_idx));
                }
            }
        }

        // Build expando index if not already from skeleton.
        if self.skeleton_expando_index.is_none() {
            let mut expando_index: FxHashMap<String, FxHashSet<String>> =
                FxHashMap::with_capacity_and_hasher(expando_capacity, Default::default());
            for binder in self.all_binders.iter() {
                for (obj_key, props) in binder.expando_properties.iter() {
                    expando_index
                        .entry(obj_key.clone())
                        .or_default()
                        .extend(props.iter().cloned());
                }
            }
            self.skeleton_expando_index = Some(Arc::new(expando_index));
        }

        if let Some(mut dm) = declared_modules {
            dm.finish();
            self.skeleton_declared_modules = Some(Arc::new(dm));
        }

        self.global_file_locals_index = Some(Arc::new(file_locals_index));
        // Phase 2 step 6: prefer the skeleton-pre-built map when available;
        // otherwise install the binder-derived one we just computed.
        self.global_module_exports_index = self
            .skeleton_module_exports_index
            .as_ref()
            .map(Arc::clone)
            .or_else(|| Some(Arc::new(module_exports_index)));
        // Phase 2 step 2: prefer the skeleton-pre-built map when available;
        // otherwise install the binder-derived one we just computed.
        self.global_module_augmentations_index = self
            .skeleton_module_augmentations_index
            .as_ref()
            .map(Arc::clone)
            .or_else(|| Some(Arc::new(module_augs_index)));
        // Phase 2 step 3: prefer the skeleton-pre-built map when available;
        // otherwise install the binder-derived one we just computed.
        self.global_augmentation_targets_index = self
            .skeleton_augmentation_targets_index
            .as_ref()
            .map(Arc::clone)
            .or_else(|| Some(Arc::new(aug_targets_index)));
        // Phase 2 step 4: prefer the skeleton-pre-built map when available;
        // otherwise install the binder-derived one we just computed.
        self.global_module_binder_index = self
            .skeleton_module_binder_index
            .as_ref()
            .map(Arc::clone)
            .or_else(|| Some(Arc::new(module_binder_index)));

        // Build arena-pointer → file-index map
        let mut arena_idx: FxHashMap<usize, usize> =
            FxHashMap::with_capacity_and_hasher(self.all_arenas.len(), Default::default());
        for (file_idx, arena) in self.all_arenas.iter().enumerate() {
            arena_idx.insert(Arc::as_ptr(arena) as usize, file_idx);
        }
        self.global_arena_index = Some(Arc::new(arena_idx));

        // Filename reverse index: one O(N) build replaces the O(N²) fallback rebuild.
        let file_name_idx = crate::module_resolution::build_file_name_index(&self.all_arenas);
        self.global_file_name_index = Some(Arc::new(file_name_idx));
    }
}
