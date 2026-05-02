//! Skeleton extraction and reduction for Phase 2 memory optimization.
//!
//! The skeleton captures the minimal per-file information needed for global merge
//! decisions (symbol merging, augmentation stitching, export/re-export graphs)
//! without retaining the full AST arena, flow graph, or scope tree.
//!
//! Pipeline: `BindResult` → `extract_skeleton()` → `FileSkeleton`
//!           `Vec<FileSkeleton>` → `reduce_skeletons()` → `SkeletonIndex`

use super::BindResult;
use super::core::can_merge_symbols_cross_file;
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::{StableLocation, SymbolId, SymbolTable};

/// Per-module-specifier projection of `module_exports`: maps `export_name` to
/// a list of `(file_idx, SymbolId)` entries identifying every file that
/// declared the export and the post-merge global `SymbolId` to use.
///
/// This mirrors the value type of `tsz_checker::context::ModuleExportsByName`
/// but is defined here so [`SkeletonIndex::build_module_exports_index`] can
/// avoid dragging in the checker dependency for a structural alias.
pub type ProjectedModuleExportsByName = FxHashMap<String, Vec<(usize, SymbolId)>>;

/// Cross-file projection of the `module_exports` topology: maps
/// `module_specifier` to its [`ProjectedModuleExportsByName`] inner map.
///
/// This is the legacy shape understood by
/// `ProjectEnv::global_module_exports_index` consumers — produced by
/// [`SkeletonIndex::build_module_exports_index`] from skeleton data plus the
/// post-merge `program.module_exports` map.
pub type ProjectedModuleExportsIndex = FxHashMap<String, ProjectedModuleExportsByName>;

/// Skeleton-internal `(spec, export_name) -> [file_idx]` index — the
/// `SymbolId`-free shape that the reducer fills from per-file
/// `(spec, [export_name])` entries.
///
/// Used as the value type of [`SkeletonIndex::module_exports_index_by_spec`]
/// and as the value/intermediate types in the legacy-validation helper.
pub type SkeletonModuleExportsByName = FxHashMap<String, Vec<usize>>;

/// Spec-keyed view of [`SkeletonModuleExportsByName`].
pub type SkeletonModuleExportsIndex = FxHashMap<String, SkeletonModuleExportsByName>;

/// A top-level symbol as seen from the skeleton layer.
///
/// This contains only the merge-relevant fields from `Symbol`, not the full
/// declaration list or member/export sub-tables (which require arena access).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkeletonSymbol {
    /// The escaped name (same semantics as `Symbol::escaped_name`).
    pub name: String,
    /// Symbol flags (same encoding as `symbol_flags`).
    pub flags: u32,
    /// Whether this symbol is exported from its file.
    pub is_exported: bool,
    /// Number of declarations in the source file.
    pub declaration_count: u32,
    /// Whether the symbol has an `exports` sub-table (namespace/module).
    pub has_exports: bool,
    /// Whether the symbol has a `members` sub-table (class/interface).
    pub has_members: bool,
    /// Whether this symbol originated from a lib file.
    pub is_lib_origin: bool,
    /// Whether this is an external-module import alias.
    pub is_import_alias: bool,
    /// Import module specifier, if this is an import alias.
    pub import_module: Option<String>,
    /// Fingerprint of heritage clause names (`extends` / `implements`).
    ///
    /// The skeleton only needs heritage data to detect topology changes. Store
    /// a compact hash instead of cloning every heritage name into the retained
    /// skeleton index.
    pub heritage_fingerprint: u64,
    /// Number of heritage names represented by `heritage_fingerprint`.
    pub heritage_count: u32,
}

/// Per-declaration augmentation entry recorded inside `SkeletonAugmentation`.
///
/// Each entry corresponds to one [`tsz_binder::ModuleAugmentation`] / inner
/// declaration in the file: the augmenting name (e.g., the augmented
/// interface/type member name) and a [`StableLocation`] pointing back to
/// the AST node so consumers can rehydrate without retaining the arena.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkeletonAugmentationDecl {
    /// Augmentation member name (e.g., the interface/type name being added
    /// inside `declare module 'x' { interface Foo {} }`).
    pub name: String,
    /// File-stable pointer to the augmenting declaration's AST node.
    pub location: StableLocation,
}

/// Augmentation candidate as seen from the skeleton layer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkeletonAugmentation {
    /// Target name (interface name for global augmentations, module specifier for module augmentations).
    pub target: String,
    /// Number of augmentation declarations for this target in this file.
    pub declaration_count: u32,
    /// Per-declaration augmentation entries (Phase 2 step 2 enrichment).
    ///
    /// Carries the same per-declaration data as
    /// [`tsz_binder::ModuleAugmentation`] (name + a [`StableLocation`] for the
    /// AST node) so consumers can rebuild the merged module-augmentation
    /// index from `SkeletonIndex` alone, without iterating per-file binders.
    /// `declarations.len() == declaration_count` is a hard invariant.
    pub declarations: Vec<SkeletonAugmentationDecl>,
}

/// Augmentation-target entry as seen from the skeleton layer (Phase 2 step 3).
///
/// One entry per `(symbol, module_spec)` pair recorded by the binder in
/// [`tsz_binder::BinderState::augmentation_target_modules`]. The
/// [`StableLocation`] points back to the augmenting declaration's AST node so
/// consumers can rehydrate without retaining the arena (Phase 5).
///
/// This is the minimal data needed to reconstruct the checker's
/// `global_augmentation_targets_index` (`module_spec -> Vec<(SymbolId, file_idx)>`)
/// from skeleton data alone — without iterating per-file binders.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkeletonAugmentationTarget {
    /// Symbol whose declaration sits inside `declare module 'spec' { ... }`.
    pub symbol_id: tsz_binder::SymbolId,
    /// Module specifier of the augmenting `declare module 'spec' { ... }`
    /// block (raw form, as stored on the binder — quotes already stripped).
    pub module_spec: String,
    /// File-stable pointer to the augmenting declaration's AST node.
    ///
    /// Defaults to [`StableLocation::NONE`] when the binder did not record a
    /// span for the symbol's value/first declaration. Consumers should use
    /// [`StableLocation::is_known`] before dereferencing.
    pub stable_location: StableLocation,
}

/// Re-export edge as seen from the skeleton layer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkeletonReexport {
    /// The exported name (as visible to importers).
    pub exported_name: String,
    /// Source module specifier.
    pub source_module: String,
    /// Original name in the source module (None = same as `exported_name`).
    pub original_name: Option<String>,
}

/// Wildcard re-export edge (`export * from 'module'`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkeletonWildcardReexport {
    /// Source module specifier.
    pub source_module: String,
    /// Whether this is a type-only re-export.
    pub type_only: bool,
}

/// Minimal per-file skeleton extracted from a `BindResult`.
///
/// Contains only the data needed for:
/// - Determining which top-level symbols exist and whether they can merge.
/// - Tracking augmentation candidates (global and module).
/// - Capturing re-export/wildcard-re-export graph edges.
/// - Identifying declared ambient modules and shorthand modules.
///
/// Does NOT contain: full AST arena, flow graph, scope tree, node-to-symbol
/// mappings, parse diagnostics, or per-node data. Those remain in the legacy
/// `BindResult`/`BoundFile` path.
#[derive(Debug, Clone)]
pub struct FileSkeleton {
    /// Source file name.
    pub file_name: String,
    /// Whether this file is an external module (has imports/exports).
    pub is_external_module: bool,
    /// Top-level symbols (root scope + exported `file_locals`).
    pub symbols: Vec<SkeletonSymbol>,
    /// Global augmentation targets from `declare global {}` blocks.
    pub global_augmentations: Vec<SkeletonAugmentation>,
    /// Module augmentation targets from `declare module 'x' {}` blocks.
    pub module_augmentations: Vec<SkeletonAugmentation>,
    /// Per-file augmentation-target entries (Phase 2 step 3).
    ///
    /// One entry per `(symbol, module_spec)` pair recorded by the binder in
    /// [`tsz_binder::BinderState::augmentation_target_modules`] — i.e. each
    /// symbol declared inside a `declare module 'spec' { ... }` block. The
    /// reducer projects these into [`SkeletonIndex::augmentation_targets_by_spec`].
    pub augmentation_targets: Vec<SkeletonAugmentationTarget>,
    /// Named re-exports (`export { x } from 'module'`).
    pub reexports: Vec<SkeletonReexport>,
    /// Wildcard re-exports (`export * from 'module'`).
    pub wildcard_reexports: Vec<SkeletonWildcardReexport>,
    /// Ambient module declarations (`declare module "foo"`).
    pub declared_modules: Vec<String>,
    /// Shorthand ambient modules (`declare module "foo"` without body).
    pub shorthand_ambient_modules: Vec<String>,
    /// Module export specifiers — keys from `module_exports` map.
    /// These represent module specifiers that have explicit export declarations
    /// (e.g., from `declare module "xxx" { export ... }`).
    pub module_export_specifiers: Vec<String>,
    /// Per-spec list of export names recorded in this file's `module_exports`
    /// map (Phase 2 step 6).
    ///
    /// Each `(spec, export_names)` entry mirrors the `binder.module_exports`
    /// shape: the inner `Vec<String>` is the sorted list of names from the
    /// `SymbolTable` keyed by `spec`. `SymbolId`s are intentionally NOT
    /// recorded here — pre-merge local `SymbolId`s are not stable across the
    /// merge (see PR #1145 for the regression that motivated this design).
    /// The projection helper resolves `SymbolId`s at build time from the
    /// post-merge `program.module_exports` map (which holds globally-remapped
    /// IDs).
    ///
    /// Sorted by `spec`, then by `export_name`, so the per-file fingerprint is
    /// deterministic across `HashMap` iteration order.
    pub module_exports_entries: Vec<(String, Vec<String>)>,
    /// Expando property assignments: maps identifier name -> set of property names
    /// assigned via `X.prop = value` patterns. Used to suppress false TS2339 errors.
    pub expando_properties: Vec<(String, Vec<String>)>,
    /// Static import/export-from module specifiers collected by the binder.
    /// Enables dependency graph construction without re-walking the AST.
    pub import_sources: Vec<String>,
    /// Binder-detected file features (generators, decorators, etc.).
    pub file_features: crate::binder::FileFeatures,
    /// Content fingerprint of all merge-relevant skeleton data.
    ///
    /// Two skeletons with equal fingerprints have identical merge-relevant topology.
    /// Incremental drivers can compare fingerprints to skip re-merging unchanged files.
    /// Computed deterministically at extraction time from sorted, canonical data.
    pub fingerprint: u64,
}

impl FileSkeleton {
    /// Compute a deterministic fingerprint from all merge-relevant skeleton fields.
    ///
    /// Uses `std::hash::Hash` on each field in a canonical order to produce a
    /// stable `u64`. The skeleton's fields are already sorted deterministically
    /// by `extract_skeleton`, so identical source topologies always yield
    /// identical fingerprints.
    pub fn compute_fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = rustc_hash::FxHasher::default();
        // Hash all merge-relevant fields (excluding file_name, which is identity not content).
        self.is_external_module.hash(&mut hasher);
        self.symbols.hash(&mut hasher);
        self.global_augmentations.hash(&mut hasher);
        self.module_augmentations.hash(&mut hasher);
        self.augmentation_targets.hash(&mut hasher);
        self.reexports.hash(&mut hasher);
        self.wildcard_reexports.hash(&mut hasher);
        self.declared_modules.hash(&mut hasher);
        self.shorthand_ambient_modules.hash(&mut hasher);
        self.module_export_specifiers.hash(&mut hasher);
        self.module_exports_entries.hash(&mut hasher);
        self.expando_properties.hash(&mut hasher);
        self.import_sources.hash(&mut hasher);
        self.file_features.hash(&mut hasher);
        hasher.finish()
    }
}

/// Extract a `FileSkeleton` from a `BindResult` without consuming it.
///
/// This is a pure map operation: one `BindResult` → one `FileSkeleton`.
/// The skeleton captures merge-relevant data without retaining the arena.
pub fn extract_skeleton(result: &BindResult) -> FileSkeleton {
    // Collect top-level symbols from root scope + file_locals.
    // We use file_locals as the primary source since it represents what the
    // binder considers top-level (including symbols from nested scopes that
    // are hoisted to file-level, like `declare namespace`).
    let mut symbols = Vec::new();
    let mut seen_names = FxHashSet::default();

    for (name, &sym_id) in result.file_locals.iter() {
        if !seen_names.insert(name.clone()) {
            continue;
        }
        if let Some(sym) = result.symbols.get(sym_id) {
            let (heritage_fingerprint, heritage_count) =
                semantic_def_heritage_fingerprint(result.semantic_defs.get(&sym_id));
            symbols.push(SkeletonSymbol {
                name: name.clone(),
                flags: sym.flags,
                is_exported: sym.is_exported,
                declaration_count: sym.declarations.len() as u32,
                has_exports: sym.exports.is_some(),
                has_members: sym.members.is_some(),
                is_lib_origin: result.lib_symbol_ids.contains(&sym_id),
                is_import_alias: (sym.flags & crate::binder::symbol_flags::ALIAS) != 0,
                import_module: sym.import_module.clone(),
                heritage_fingerprint,
                heritage_count,
            });
        }
    }

    // Also include root-scope symbols NOT in file_locals (rare, but possible
    // for non-exported declarations in script files).
    if let Some(root_scope) = result.scopes.first() {
        for (name, &sym_id) in root_scope.table.iter() {
            if seen_names.contains(name) {
                continue;
            }
            if let Some(sym) = result.symbols.get(sym_id) {
                seen_names.insert(name.clone());
                let (heritage_fingerprint, heritage_count) =
                    semantic_def_heritage_fingerprint(result.semantic_defs.get(&sym_id));
                symbols.push(SkeletonSymbol {
                    name: name.clone(),
                    flags: sym.flags,
                    is_exported: sym.is_exported,
                    declaration_count: sym.declarations.len() as u32,
                    has_exports: sym.exports.is_some(),
                    has_members: sym.members.is_some(),
                    is_lib_origin: result.lib_symbol_ids.contains(&sym_id),
                    is_import_alias: (sym.flags & crate::binder::symbol_flags::ALIAS) != 0,
                    import_module: sym.import_module.clone(),
                    heritage_fingerprint,
                    heritage_count,
                });
            }
        }
    }

    // Sort symbols by name for deterministic output.
    symbols.sort_unstable_by(|a, b| a.name.cmp(&b.name));

    // Global augmentations.
    // The skeleton currently records per-file global augmentations only by name +
    // count. Phase 2 step 2 extends *module* augmentations with per-declaration
    // `StableLocation` entries so the checker can reconstruct the merged
    // `global_module_augmentations_index` without iterating binders. Global
    // augmentations are reserved for a parallel follow-up.
    let mut global_augmentations: Vec<SkeletonAugmentation> = result
        .global_augmentations
        .iter()
        .map(|(target, augs)| SkeletonAugmentation {
            target: target.clone(),
            declaration_count: augs.len() as u32,
            declarations: Vec::new(),
        })
        .collect();
    global_augmentations.sort_unstable_by(|a, b| a.target.cmp(&b.target));

    // Module augmentations: enriched with `SkeletonAugmentationDecl` entries
    // (name + stable AST location) so consumers can rebuild
    // `global_module_augmentations_index` from skeleton data alone.
    //
    // The location's `file_idx` is left as `u32::MAX` (unstamped) and the
    // reducer (or the caller, when iterating skeletons in driver order) is
    // responsible for resolving the file index from the skeleton's slot.
    let module_augmentations_arena = result.arena.as_ref();
    let mut module_augmentations: Vec<SkeletonAugmentation> = result
        .module_augmentations
        .iter()
        .map(|(target, augs)| {
            let mut declarations: Vec<SkeletonAugmentationDecl> = augs
                .iter()
                .map(|aug| {
                    let span = module_augmentations_arena
                        .get(aug.node)
                        .map(|node| (node.pos, node.end));
                    SkeletonAugmentationDecl {
                        name: aug.name.clone(),
                        location: StableLocation::from_span(u32::MAX, span),
                    }
                })
                .collect();
            // Sort declarations deterministically: by (name, pos, end).
            declarations.sort_by(|a, b| {
                a.name
                    .cmp(&b.name)
                    .then(a.location.pos.cmp(&b.location.pos))
                    .then(a.location.end.cmp(&b.location.end))
            });
            SkeletonAugmentation {
                target: target.clone(),
                declaration_count: augs.len() as u32,
                declarations,
            }
        })
        .collect();
    module_augmentations.sort_unstable_by(|a, b| a.target.cmp(&b.target));

    // Augmentation targets (Phase 2 step 3): one entry per (symbol, module_spec)
    // pair recorded by the binder. The `StableLocation` is sourced from the
    // symbol's `stable_value_declaration` when known, falling back to the first
    // `stable_declarations` entry. The location's `file_idx` is left at
    // `u32::MAX` (unstamped) when the binder has not yet stamped it; the
    // reducer stamps it from the owning skeleton's file index.
    let mut augmentation_targets: Vec<SkeletonAugmentationTarget> = result
        .augmentation_target_modules
        .iter()
        .map(|(&sym_id, module_spec)| {
            let stable_location = result
                .symbols
                .get(sym_id)
                .map(|sym| {
                    if sym.stable_value_declaration.is_known() {
                        sym.stable_value_declaration
                    } else {
                        sym.stable_declarations
                            .first()
                            .copied()
                            .unwrap_or(StableLocation::NONE)
                    }
                })
                .unwrap_or(StableLocation::NONE);
            SkeletonAugmentationTarget {
                symbol_id: sym_id,
                module_spec: module_spec.clone(),
                stable_location,
            }
        })
        .collect();
    // Sort deterministically by (module_spec, symbol_id) so the per-file
    // skeleton fingerprint is stable across HashMap iteration order.
    augmentation_targets.sort_by(|a, b| {
        a.module_spec
            .cmp(&b.module_spec)
            .then(a.symbol_id.0.cmp(&b.symbol_id.0))
    });

    // Named re-exports
    let mut reexports = Vec::new();
    for (file_name, file_reexports) in result.reexports.iter() {
        // Only include re-exports from this file (the reexport map key is the file name)
        if file_name == &result.file_name {
            for (exported_name, (source_module, original_name)) in file_reexports {
                reexports.push(SkeletonReexport {
                    exported_name: exported_name.clone(),
                    source_module: source_module.clone(),
                    original_name: original_name.clone(),
                });
            }
        }
    }
    reexports.sort_unstable_by(|a, b| a.exported_name.cmp(&b.exported_name));

    // Wildcard re-exports
    let mut wildcard_reexports = Vec::new();
    if let Some(sources) = result.wildcard_reexports.get(&result.file_name) {
        let type_only_entries = result.wildcard_reexports_type_only.get(&result.file_name);
        for (i, source_module) in sources.iter().enumerate() {
            let type_only = type_only_entries
                .and_then(|entries| entries.get(i).map(|(_, is_to)| *is_to))
                .unwrap_or(false);
            wildcard_reexports.push(SkeletonWildcardReexport {
                source_module: source_module.clone(),
                type_only,
            });
        }
    }
    wildcard_reexports.sort_unstable_by(|a, b| a.source_module.cmp(&b.source_module));

    // Declared modules
    let mut declared_modules: Vec<String> = result.declared_modules.iter().cloned().collect();
    declared_modules.sort();

    // Shorthand ambient modules
    let mut shorthand_ambient_modules: Vec<String> =
        result.shorthand_ambient_modules.iter().cloned().collect();
    shorthand_ambient_modules.sort();

    // Module export specifiers (keys from module_exports map)
    let mut module_export_specifiers: Vec<String> = result.module_exports.keys().cloned().collect();
    module_export_specifiers.sort();

    // Phase 2 step 6: per-spec export names — captures the export-name set of
    // each `binder.module_exports[spec]` SymbolTable. SymbolIds are not stored
    // (pre-merge local IDs are not stable across the merge — see PR #1145).
    // The projection helper resolves SymbolIds at build time from the
    // post-merge `program.module_exports` map.
    let mut module_exports_entries: Vec<(String, Vec<String>)> = result
        .module_exports
        .iter()
        .map(|(spec, table)| {
            let mut names: Vec<String> = table.iter().map(|(name, _)| name.clone()).collect();
            names.sort();
            (spec.clone(), names)
        })
        .collect();
    module_exports_entries.sort_by(|a, b| a.0.cmp(&b.0));

    // Expando properties: convert FxHashMap<String, FxHashSet<String>> to sorted Vec
    let mut expando_properties: Vec<(String, Vec<String>)> = result
        .expando_properties
        .iter()
        .map(|(obj_key, props)| {
            let mut sorted_props: Vec<String> = props.iter().cloned().collect();
            sorted_props.sort();
            (obj_key.clone(), sorted_props)
        })
        .collect();
    expando_properties.sort_unstable_by(|a, b| a.0.cmp(&b.0));

    let mut skeleton = FileSkeleton {
        file_name: result.file_name.clone(),
        is_external_module: result.is_external_module,
        symbols,
        global_augmentations,
        module_augmentations,
        augmentation_targets,
        reexports,
        wildcard_reexports,
        declared_modules,
        shorthand_ambient_modules,
        module_export_specifiers,
        module_exports_entries,
        expando_properties,
        import_sources: result.file_import_sources.clone(),
        file_features: result.file_features,
        fingerprint: 0, // computed below
    };
    skeleton.fingerprint = skeleton.compute_fingerprint();
    skeleton
}

fn semantic_def_heritage_fingerprint(
    entry: Option<&crate::binder::SemanticDefEntry>,
) -> (u64, u32) {
    let Some(entry) = entry else {
        return (0, 0);
    };
    if entry.extends_names.is_empty() && entry.implements_names.is_empty() {
        return (0, 0);
    }

    use std::hash::{Hash, Hasher};
    let mut hasher = rustc_hash::FxHasher::default();
    entry.extends_names.hash(&mut hasher);
    entry.implements_names.hash(&mut hasher);
    let count = entry.extends_names.len() + entry.implements_names.len();
    (hasher.finish(), count as u32)
}

/// A merge candidate discovered during skeleton reduction.
///
/// Records that a symbol name appears in multiple files and can potentially
/// be merged (interfaces, namespaces, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkeletonMergeCandidate {
    /// The symbol name.
    pub name: String,
    /// Combined flags from all contributing files.
    pub merged_flags: u32,
    /// Files contributing to this symbol (indices into the original skeleton slice).
    pub source_files: Vec<usize>,
    /// Whether the merge is valid according to `can_merge_symbols_cross_file`.
    pub is_valid_merge: bool,
}

/// Global index produced by reducing a set of file skeletons.
///
/// This is a lightweight alternative to `MergedProgram` that captures the
/// merge topology without retaining any arena or symbol data.
#[derive(Debug, Clone)]
pub struct SkeletonIndex {
    /// Number of files in the index.
    pub file_count: usize,
    /// Symbols that appear in multiple files and can merge.
    pub merge_candidates: Vec<SkeletonMergeCandidate>,
    /// All global augmentation targets across all files, with contributing file indices.
    pub global_augmentation_targets: FxHashMap<String, Vec<usize>>,
    /// All module augmentation targets across all files, with contributing file indices.
    pub module_augmentation_targets: FxHashMap<String, Vec<usize>>,
    /// Per-module-specifier list of (`file_idx`, augmentation) entries.
    ///
    /// This is the Phase 2 step 2 enrichment over `module_augmentation_targets`:
    /// the legacy field tells you *which* files contribute augmentations for a
    /// given target, this field carries the per-file [`SkeletonAugmentation`]
    /// (with each augmenting declaration's name + [`StableLocation`]) so the
    /// checker can rebuild `global_module_augmentations_index` from skeleton
    /// data alone — without iterating per-file binders.
    ///
    /// Entries are recorded in driver file order (the same order the reducer
    /// observes the input skeletons).
    pub module_augmentations_by_spec: FxHashMap<String, Vec<(usize, SkeletonAugmentation)>>,
    /// Per-module-specifier list of `(file_idx, augmentation_target)` entries
    /// (Phase 2 step 3 enrichment).
    ///
    /// Whereas [`Self::module_augmentation_targets`] tracks only which files
    /// declare augmentations for a target, this field carries the per-symbol
    /// [`SkeletonAugmentationTarget`] entries (with each augmenting symbol's
    /// id + [`StableLocation`]) so the checker can rebuild
    /// `global_augmentation_targets_index` from skeleton data alone — without
    /// iterating per-file binders.
    ///
    /// Entries are recorded in driver file order (the same order the reducer
    /// observes the input skeletons). Within a single `(spec, file)` slot,
    /// targets are appended in `FileSkeleton::augmentation_targets` order
    /// (already sorted by `(module_spec, symbol_id)` at extract time).
    pub augmentation_targets_by_spec: FxHashMap<String, Vec<(usize, SkeletonAugmentationTarget)>>,
    /// Per-module-specifier list of file indices that contain a
    /// `module_exports[module_spec]` entry (Phase 2 step 4).
    ///
    /// This is the skeleton-only projection of the checker's legacy
    /// `global_module_binder_index` (`module_spec -> Vec<file_idx>`). Each
    /// entry records that file `file_idx` declared at least one exported
    /// member under the (raw) module specifier `spec`. The reducer also
    /// records the de-quoted ("normalized") variant when it differs from the
    /// raw form, mirroring the legacy per-binder loop in
    /// `ProjectEnv::build_global_indices`.
    ///
    /// Entries are recorded in driver file order (the same order the reducer
    /// observes the input skeletons). For a single file that declares both
    /// `"foo"` and `'foo'` (extremely rare), the file index is pushed once
    /// per matching specifier — same as the legacy loop.
    pub module_binder_index_by_spec: FxHashMap<String, Vec<usize>>,
    /// Per-module-specifier list of `(file_idx, export_name)` entries
    /// (Phase 2 step 6).
    ///
    /// This is the skeleton-only projection of the checker's legacy
    /// `global_module_exports_index` (`spec -> export_name -> Vec<(file_idx,
    /// SymbolId)>`). Each entry records that file `file_idx` declared the
    /// export `export_name` under module specifier `spec`. `SymbolId`s are
    /// NOT stored here — they are resolved at projection time from the
    /// post-merge `program.module_exports` map (which holds globally-remapped
    /// IDs). Storing pre-merge local `SymbolId`s in the skeleton was the trap
    /// that regressed PR #1145 for the file-locals index.
    ///
    /// Entries are recorded in driver file order, then by export-name within a
    /// single `(spec, file)` slot. Both the raw spec and its de-quoted
    /// ("normalized") variant are recorded when they differ — same as the
    /// legacy per-binder loop in `ProjectEnv::build_global_indices`.
    pub module_exports_index_by_spec: SkeletonModuleExportsIndex,
    /// All declared ambient modules across all files.
    pub declared_modules: FxHashSet<String>,
    /// All shorthand ambient modules across all files.
    pub shorthand_ambient_modules: FxHashSet<String>,
    /// All module export specifiers across all files (keys from `module_exports`).
    pub module_export_specifiers: FxHashSet<String>,
    /// Merged expando property assignments across all files.
    /// Maps identifier name -> set of property names assigned via `X.prop = value`.
    ///
    /// Shared so project drivers can install the skeleton-derived index into
    /// `ProjectEnv` without deep-cloning the program-wide map.
    pub expando_properties: Arc<FxHashMap<String, FxHashSet<String>>>,
    /// Total number of top-level symbols across all files (before merge).
    pub total_symbol_count: usize,
    /// Total number of re-export edges across all files.
    pub total_reexport_count: usize,
    /// Total number of wildcard re-export edges across all files.
    pub total_wildcard_reexport_count: usize,
    /// Aggregate fingerprint across all constituent file skeletons.
    ///
    /// Combines per-file fingerprints (in deterministic file order) with
    /// cross-file topology (merge candidates, augmentation targets) into a
    /// single `u64`. Two `SkeletonIndex` values with equal aggregate
    /// fingerprints have identical merge-relevant project topology.
    ///
    /// Incremental drivers can compare this single value to determine whether
    /// the entire project's merge topology has changed since the last build.
    pub fingerprint: u64,
}

#[derive(Default)]
struct SkeletonReductionCapacities {
    global_augmentations: usize,
    module_augmentations: usize,
    augmentation_targets: usize,
}

impl SkeletonReductionCapacities {
    fn from_skeletons(skeletons: &[FileSkeleton]) -> Self {
        let mut capacities = Self::default();

        for skeleton in skeletons {
            capacities.global_augmentations += skeleton.global_augmentations.len();
            capacities.module_augmentations += skeleton.module_augmentations.len();
            capacities.augmentation_targets += skeleton.augmentation_targets.len();
        }

        capacities
    }
}

/// Deterministically reduce a set of file skeletons into a `SkeletonIndex`.
///
/// This is a pure function: the same input skeletons (in the same order) always
/// produce the same output. The reduction is sequential and ordered.
///
/// # Arguments
/// * `skeletons` - Slice of file skeletons, in file order.
pub fn reduce_skeletons(skeletons: &[FileSkeleton]) -> SkeletonIndex {
    let capacities = SkeletonReductionCapacities::from_skeletons(skeletons);

    let mut symbol_map: FxHashMap<String, (u32, Vec<usize>)> = FxHashMap::default();
    let mut global_augmentation_targets: FxHashMap<String, Vec<usize>> =
        FxHashMap::with_capacity_and_hasher(capacities.global_augmentations, Default::default());
    let mut module_augmentation_targets: FxHashMap<String, Vec<usize>> =
        FxHashMap::with_capacity_and_hasher(capacities.module_augmentations, Default::default());
    let mut module_augmentations_by_spec: FxHashMap<String, Vec<(usize, SkeletonAugmentation)>> =
        FxHashMap::with_capacity_and_hasher(capacities.module_augmentations, Default::default());
    let mut augmentation_targets_by_spec: FxHashMap<
        String,
        Vec<(usize, SkeletonAugmentationTarget)>,
    > = FxHashMap::with_capacity_and_hasher(capacities.augmentation_targets, Default::default());
    let mut module_binder_index_by_spec: FxHashMap<String, Vec<usize>> = FxHashMap::default();
    // Phase 2 step 6: per-spec, per-export-name list of file indices.
    let mut module_exports_index_by_spec: SkeletonModuleExportsIndex = FxHashMap::default();
    let mut declared_modules = FxHashSet::default();
    let mut shorthand_ambient_modules = FxHashSet::default();
    let mut module_export_specifiers = FxHashSet::default();
    let mut expando_properties: FxHashMap<String, FxHashSet<String>> = FxHashMap::default();
    let mut total_symbol_count = 0usize;
    let mut total_reexport_count = 0usize;
    let mut total_wildcard_reexport_count = 0usize;

    for (file_idx, skeleton) in skeletons.iter().enumerate() {
        // Only merge symbols from non-external-module files (script files).
        // External modules' symbols are file-scoped and don't contribute to globals.
        if !skeleton.is_external_module {
            for sym in &skeleton.symbols {
                if sym.is_lib_origin || sym.is_import_alias {
                    continue;
                }
                total_symbol_count += 1;
                let entry = symbol_map
                    .entry(sym.name.clone())
                    .or_insert_with(|| (0, Vec::new()));
                entry.0 |= sym.flags;
                entry.1.push(file_idx);
            }
        } else {
            total_symbol_count += skeleton.symbols.len();
        }

        for aug in &skeleton.global_augmentations {
            global_augmentation_targets
                .entry(aug.target.clone())
                .or_default()
                .push(file_idx);
        }

        for aug in &skeleton.module_augmentations {
            module_augmentation_targets
                .entry(aug.target.clone())
                .or_default()
                .push(file_idx);

            // Phase 2 step 2: also record the per-declaration entries with
            // the file index stamped onto each declaration's `StableLocation`.
            // This lets the checker rebuild `global_module_augmentations_index`
            // from skeleton data without iterating per-file binders.
            let mut stamped = aug.clone();
            for decl in &mut stamped.declarations {
                decl.location.set_file_idx_if_unassigned(file_idx as u32);
            }
            module_augmentations_by_spec
                .entry(aug.target.clone())
                .or_default()
                .push((file_idx, stamped));
        }

        // Phase 2 step 3: project per-file augmentation-target entries into
        // the cross-file `(file_idx, target)` index. The reducer stamps each
        // entry's `StableLocation` with the owning file index so post-Phase-5
        // consumers can route through `node_at_stable_location` without a
        // separate file_idx arg.
        for target in &skeleton.augmentation_targets {
            let mut stamped = target.clone();
            stamped
                .stable_location
                .set_file_idx_if_unassigned(file_idx as u32);
            augmentation_targets_by_spec
                .entry(target.module_spec.clone())
                .or_default()
                .push((file_idx, stamped));
        }

        // Phase 2 step 4: project per-file module-export specifiers into the
        // cross-file `module_spec -> [file_idx]` index. Mirrors the legacy
        // per-binder loop in `ProjectEnv::build_global_indices` which iterates
        // `binder.module_exports.iter()` and pushes `file_idx` for both the
        // raw spec and its de-quoted ("normalized") form when they differ.
        for spec in &skeleton.module_export_specifiers {
            module_binder_index_by_spec
                .entry(spec.clone())
                .or_default()
                .push(file_idx);
            let normalized = spec.trim_matches('"').trim_matches('\'');
            if normalized != spec {
                module_binder_index_by_spec
                    .entry(normalized.to_string())
                    .or_default()
                    .push(file_idx);
            }
        }

        // Phase 2 step 6: project per-file module-export entries into the
        // cross-file `spec -> export_name -> [file_idx]` index. Mirrors the
        // legacy per-binder loop in `ProjectEnv::build_global_indices` which
        // iterates `binder.module_exports[spec].iter()` and pushes
        // `(file_idx, sym_id)` per export name. SymbolIds are looked up at
        // projection time from `program.module_exports` (post-merge global
        // IDs) — see `SkeletonIndex::build_module_exports_index`.
        for (spec, export_names) in &skeleton.module_exports_entries {
            let entry = module_exports_index_by_spec
                .entry(spec.clone())
                .or_default();
            for name in export_names {
                entry.entry(name.clone()).or_default().push(file_idx);
            }
            let normalized = spec.trim_matches('"').trim_matches('\'');
            if normalized != spec {
                let entry = module_exports_index_by_spec
                    .entry(normalized.to_string())
                    .or_default();
                for name in export_names {
                    entry.entry(name.clone()).or_default().push(file_idx);
                }
            }
        }

        declared_modules.extend(skeleton.declared_modules.iter().cloned());
        shorthand_ambient_modules.extend(skeleton.shorthand_ambient_modules.iter().cloned());
        module_export_specifiers.extend(skeleton.module_export_specifiers.iter().cloned());

        for (obj_key, props) in &skeleton.expando_properties {
            expando_properties
                .entry(obj_key.clone())
                .or_default()
                .extend(props.iter().cloned());
        }

        total_reexport_count += skeleton.reexports.len();
        total_wildcard_reexport_count += skeleton.wildcard_reexports.len();
    }

    // Build merge candidates: symbols appearing in >1 file.
    let mut merge_candidates: Vec<SkeletonMergeCandidate> = symbol_map
        .into_iter()
        .filter(|(_, (_, files))| files.len() > 1)
        .map(|(name, (merged_flags, source_files))| {
            // Determine if the merge is valid by checking all pairs.
            // A simple approximation: check if the first file's flags can merge
            // with the combined flags of all others.
            let is_valid_merge = {
                let first_flags = skeletons[source_files[0]]
                    .symbols
                    .iter()
                    .find(|s| s.name == name)
                    .map(|s| s.flags)
                    .unwrap_or(0);
                let rest_flags = merged_flags & !first_flags | first_flags;
                // Check pairwise: for simplicity, check first vs rest_combined.
                // This is an approximation; the full merge uses pairwise checks.
                can_merge_symbols_cross_file(first_flags, merged_flags & !first_flags)
                    || can_merge_symbols_cross_file(first_flags, rest_flags)
            };
            SkeletonMergeCandidate {
                name,
                merged_flags,
                source_files,
                is_valid_merge,
            }
        })
        .collect();

    // Sort for deterministic output.
    merge_candidates.sort_unstable_by(|a, b| a.name.cmp(&b.name));

    let mut index = SkeletonIndex {
        file_count: skeletons.len(),
        merge_candidates,
        global_augmentation_targets,
        module_augmentation_targets,
        module_augmentations_by_spec,
        augmentation_targets_by_spec,
        module_binder_index_by_spec,
        module_exports_index_by_spec,
        declared_modules,
        shorthand_ambient_modules,
        module_export_specifiers,
        expando_properties: Arc::new(expando_properties),
        total_symbol_count,
        total_reexport_count,
        total_wildcard_reexport_count,
        fingerprint: 0, // computed below
    };

    // Compute aggregate fingerprint from per-file fingerprints + cross-file topology.
    let file_fingerprints: Vec<u64> = skeletons.iter().map(|s| s.fingerprint).collect();
    index.fingerprint = SkeletonIndex::compute_fingerprint(&file_fingerprints, &index);

    index
}

impl SkeletonIndex {
    /// Compute a deterministic aggregate fingerprint from all index fields.
    ///
    /// Combines per-file fingerprints (via `file_fingerprints`, in file order)
    /// with cross-file topology (merge candidates, augmentation targets,
    /// declared modules, expando properties, counters) to produce a single `u64`.
    ///
    /// The same project topology always yields the same fingerprint. This is
    /// computed from already-sorted/deterministic data (merge candidates are
    /// sorted by name, sets are iterated in sorted order).
    #[must_use]
    pub fn compute_fingerprint(file_fingerprints: &[u64], index: &SkeletonIndex) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = rustc_hash::FxHasher::default();

        // 1) Per-file fingerprints in file order (captures all per-file topology).
        file_fingerprints.hash(&mut hasher);

        // 2) Merge candidates (already sorted by name in reduce_skeletons).
        for mc in &index.merge_candidates {
            mc.name.hash(&mut hasher);
            mc.merged_flags.hash(&mut hasher);
            mc.source_files.hash(&mut hasher);
            mc.is_valid_merge.hash(&mut hasher);
        }

        // 3) Global augmentation targets (sorted for determinism).
        let mut global_aug_keys: Vec<&String> = index.global_augmentation_targets.keys().collect();
        global_aug_keys.sort();
        for key in &global_aug_keys {
            key.hash(&mut hasher);
            index.global_augmentation_targets[*key].hash(&mut hasher);
        }

        // 4) Module augmentation targets (sorted for determinism).
        let mut mod_aug_keys: Vec<&String> = index.module_augmentation_targets.keys().collect();
        mod_aug_keys.sort();
        for key in &mod_aug_keys {
            key.hash(&mut hasher);
            index.module_augmentation_targets[*key].hash(&mut hasher);
        }

        // 4b) Per-spec augmentation-target entries (Phase 2 step 3),
        //     sorted by spec for determinism. Each entry contributes its
        //     (file_idx, symbol_id, stable_location) so any change to the
        //     skeleton-projected augmentation-target topology invalidates
        //     downstream caches.
        let mut aug_target_keys: Vec<&String> = index.augmentation_targets_by_spec.keys().collect();
        aug_target_keys.sort();
        for key in &aug_target_keys {
            key.hash(&mut hasher);
            for (file_idx, target) in &index.augmentation_targets_by_spec[*key] {
                file_idx.hash(&mut hasher);
                target.hash(&mut hasher);
            }
        }

        // 4c) Per-spec module-binder index (Phase 2 step 4), sorted by spec
        //     for determinism. Each entry contributes the (spec, [file_idx])
        //     vector so any change to the skeleton-projected module-binder
        //     topology invalidates downstream caches.
        let mut binder_idx_keys: Vec<&String> = index.module_binder_index_by_spec.keys().collect();
        binder_idx_keys.sort();
        for key in &binder_idx_keys {
            key.hash(&mut hasher);
            index.module_binder_index_by_spec[*key].hash(&mut hasher);
        }

        // 4d) Per-spec module-exports index (Phase 2 step 6), sorted by spec
        //     and by export-name for determinism. Each entry contributes the
        //     (spec, export_name, [file_idx]) tuple so any change to the
        //     skeleton-projected module-exports topology invalidates
        //     downstream caches. SymbolIds are not hashed here — they are
        //     resolved from the post-merge `program.module_exports` at
        //     projection time, and the merged map already moves the
        //     `pre_merge_bind_total_bytes` / per-file fingerprints when its
        //     contents change.
        let mut exports_idx_keys: Vec<&String> =
            index.module_exports_index_by_spec.keys().collect();
        exports_idx_keys.sort();
        for spec in &exports_idx_keys {
            spec.hash(&mut hasher);
            let by_name = &index.module_exports_index_by_spec[*spec];
            let mut name_keys: Vec<&String> = by_name.keys().collect();
            name_keys.sort();
            for name in &name_keys {
                name.hash(&mut hasher);
                by_name[*name].hash(&mut hasher);
            }
        }

        // 5) Declared modules (sorted for determinism).
        let mut declared: Vec<&String> = index.declared_modules.iter().collect();
        declared.sort();
        for d in &declared {
            d.hash(&mut hasher);
        }

        // 6) Shorthand ambient modules (sorted for determinism).
        let mut shorthand: Vec<&String> = index.shorthand_ambient_modules.iter().collect();
        shorthand.sort();
        for s in &shorthand {
            s.hash(&mut hasher);
        }

        // 7) Module export specifiers (sorted for determinism).
        let mut mod_exp: Vec<&String> = index.module_export_specifiers.iter().collect();
        mod_exp.sort();
        for m in &mod_exp {
            m.hash(&mut hasher);
        }

        // 8) Expando properties (sorted for determinism).
        let mut expando_keys: Vec<&String> = index.expando_properties.keys().collect();
        expando_keys.sort();
        for key in &expando_keys {
            key.hash(&mut hasher);
            let mut props: Vec<&String> = index.expando_properties[*key].iter().collect();
            props.sort();
            for p in &props {
                p.hash(&mut hasher);
            }
        }

        // 9) Aggregate counters.
        index.file_count.hash(&mut hasher);
        index.total_symbol_count.hash(&mut hasher);
        index.total_reexport_count.hash(&mut hasher);
        index.total_wildcard_reexport_count.hash(&mut hasher);

        hasher.finish()
    }

    /// Estimate the in-memory size of this `SkeletonIndex` in bytes.
    ///
    /// Accounts for the struct itself, all heap-allocated strings, vecs, and
    /// hash map entries. Used by `MergedProgramResidencyStats` to report
    /// skeleton memory pressure for eviction decisions.
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        let mut size = std::mem::size_of::<Self>();

        // Merge candidates
        size += self.merge_candidates.capacity() * std::mem::size_of::<SkeletonMergeCandidate>();
        for mc in &self.merge_candidates {
            size += mc.name.capacity();
            size += mc.source_files.capacity() * std::mem::size_of::<usize>();
        }

        // Global augmentation targets (HashMap<String, Vec<usize>>)
        // Each bucket: key string + vec of usize
        for (key, files) in &self.global_augmentation_targets {
            size += key.capacity();
            size += files.capacity() * std::mem::size_of::<usize>();
            size += std::mem::size_of::<(String, Vec<usize>)>(); // bucket overhead
        }

        // Module augmentation targets
        for (key, files) in &self.module_augmentation_targets {
            size += key.capacity();
            size += files.capacity() * std::mem::size_of::<usize>();
            size += std::mem::size_of::<(String, Vec<usize>)>();
        }

        // Module augmentations by spec (HashMap<String, Vec<(usize, SkeletonAugmentation)>>)
        for (key, entries) in &self.module_augmentations_by_spec {
            size += key.capacity();
            size += std::mem::size_of::<(String, Vec<(usize, SkeletonAugmentation)>)>();
            size += entries.capacity() * std::mem::size_of::<(usize, SkeletonAugmentation)>();
            for (_, aug) in entries {
                size += aug.target.capacity();
                size +=
                    aug.declarations.capacity() * std::mem::size_of::<SkeletonAugmentationDecl>();
                for decl in &aug.declarations {
                    size += decl.name.capacity();
                }
            }
        }

        // Augmentation targets by spec (Phase 2 step 3):
        // FxHashMap<String, Vec<(usize, SkeletonAugmentationTarget)>>
        for (key, entries) in &self.augmentation_targets_by_spec {
            size += key.capacity();
            size += std::mem::size_of::<(String, Vec<(usize, SkeletonAugmentationTarget)>)>();
            size += entries.capacity() * std::mem::size_of::<(usize, SkeletonAugmentationTarget)>();
            for (_, target) in entries {
                size += target.module_spec.capacity();
            }
        }

        // Module binder index by spec (Phase 2 step 4):
        // FxHashMap<String, Vec<usize>>
        for (key, files) in &self.module_binder_index_by_spec {
            size += key.capacity();
            size += files.capacity() * std::mem::size_of::<usize>();
            size += std::mem::size_of::<(String, Vec<usize>)>();
        }

        // Module exports index by spec (Phase 2 step 6):
        // FxHashMap<String, FxHashMap<String, Vec<usize>>>
        for (spec, by_name) in &self.module_exports_index_by_spec {
            size += spec.capacity();
            size += std::mem::size_of::<(String, FxHashMap<String, Vec<usize>>)>();
            for (name, files) in by_name {
                size += name.capacity();
                size += files.capacity() * std::mem::size_of::<usize>();
                size += std::mem::size_of::<(String, Vec<usize>)>();
            }
        }

        // Declared modules (HashSet<String>)
        for s in &self.declared_modules {
            size += s.capacity();
            size += std::mem::size_of::<String>(); // set bucket
        }

        // Shorthand ambient modules
        for s in &self.shorthand_ambient_modules {
            size += s.capacity();
            size += std::mem::size_of::<String>();
        }

        // Module export specifiers
        for s in &self.module_export_specifiers {
            size += s.capacity();
            size += std::mem::size_of::<String>();
        }

        // Expando properties (HashMap<String, HashSet<String>>)
        for (key, props) in self.expando_properties.iter() {
            size += key.capacity();
            size += std::mem::size_of::<(String, FxHashSet<String>)>();
            for p in props {
                size += p.capacity();
                size += std::mem::size_of::<String>();
            }
        }

        size
    }

    /// Returns true if `name` is recorded as an ambient module declaration in any file.
    ///
    /// An ambient module is one declared via `declare module "x" { ... }` (with body)
    /// or `declare module "x";` (shorthand). This mirrors the legacy
    /// `MergedProgram.declared_modules` ∪ `MergedProgram.shorthand_ambient_modules`
    /// set membership check used by the CLI module-resolver to decide whether
    /// an unresolved bare specifier should be treated as `any` instead of TS2307.
    ///
    /// The lookup is exact-match against the raw declaration text (which the
    /// binder stores without surrounding quotes — same encoding as the legacy
    /// fields). No normalization is applied; this matches the legacy semantics
    /// of `program.declared_modules.contains(spec) || program.shorthand_ambient_modules.contains(spec)`.
    ///
    /// This is the skeleton-only path for the Phase 5 evict-and-rehydrate
    /// scenario: the consumer can resolve ambient module presence without
    /// retaining the per-file binder/arena state.
    #[must_use]
    pub fn is_ambient_module(&self, name: &str) -> bool {
        self.declared_modules.contains(name) || self.shorthand_ambient_modules.contains(name)
    }

    /// Build the set of all known declared/ambient module names from the skeleton data.
    ///
    /// This produces the same result as the `set_all_binders` loop in the checker
    /// that scans `module_exports` keys, `declared_modules`, and
    /// `shorthand_ambient_modules` — but reads from pre-reduced skeleton data
    /// instead of scanning full binders.
    ///
    /// Returns `(exact_names, wildcard_patterns)` where exact names are normalized
    /// (quotes stripped) module names and wildcard patterns contain `*`.
    #[must_use]
    pub fn build_declared_module_sets(&self) -> (FxHashSet<String>, Vec<String>) {
        let mut exact = FxHashSet::default();
        let mut patterns = Vec::new();

        // Collect from all three sources, normalizing the same way set_all_binders does.
        let all_sources = self
            .declared_modules
            .iter()
            .chain(self.shorthand_ambient_modules.iter())
            .chain(self.module_export_specifiers.iter());

        for name in all_sources {
            let normalized = name.trim_matches('"').trim_matches('\'');
            if normalized.contains('*') {
                patterns.push(normalized.to_string());
            } else {
                exact.insert(normalized.to_string());
            }
        }

        // Deduplicate and sort patterns for determinism.
        patterns.sort();
        patterns.dedup();

        (exact, patterns)
    }

    /// Lookup module-augmentation entries for a given module specifier.
    ///
    /// Returns the per-file [`SkeletonAugmentation`] entries (with each
    /// augmenting declaration's name + [`StableLocation`]) recorded for
    /// `module_spec`. Empty slice if no augmentations target this specifier.
    ///
    /// This is the Phase 2 step 2 skeleton-only path for
    /// `global_module_augmentations_index`: the consumer can rebuild the
    /// merged checker-side index from this accessor alone, without
    /// iterating per-file binders. Once arenas are evictable (Phase 5),
    /// the augmenting `NodeIndex` is rehydrated on demand from the
    /// `StableLocation` via `CheckerContext::node_at_stable_location`.
    ///
    /// Entries are recorded in driver file order — same as the legacy
    /// `binder.module_augmentations.iter()` loop's enumeration order.
    #[must_use]
    pub fn module_augmentations_for(&self, module_spec: &str) -> &[(usize, SkeletonAugmentation)] {
        self.module_augmentations_by_spec
            .get(module_spec)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Build the legacy `module_specifier -> Vec<(file_idx, ModuleAugmentation)>`
    /// map from skeleton data and the driver-aligned arena vector.
    ///
    /// Phase 2 step 2 helper: projects the skeleton-recorded
    /// `(file_idx, SkeletonAugmentation)` entries into the legacy shape
    /// understood by the checker's `global_module_augmentations_index`
    /// consumers. This lets the build path skip the per-binder
    /// `module_augmentations` loop entirely.
    ///
    /// While arenas remain resident (Phase 2-4) the augmenting `NodeIndex` is
    /// rehydrated by scanning the owner file's arena for a node whose
    /// `(pos, end)` matches the stored [`StableLocation`]. Once arenas become
    /// evictable in Phase 5, downstream consumers can defer the rehydration
    /// to `CheckerContext::node_at_stable_location`.
    ///
    /// Spec keys are visited in sorted order; per-spec entries preserve the
    /// driver file order recorded by [`reduce_skeletons`]. Within a single
    /// `(spec, file)` slot, declarations are appended in
    /// `SkeletonAugmentation::declarations` order (already sorted by
    /// `(name, pos, end)` at extract time).
    #[must_use]
    pub fn build_module_augmentations_index(
        &self,
        arenas: &[std::sync::Arc<tsz_parser::parser::node::NodeArena>],
    ) -> FxHashMap<String, Vec<(usize, tsz_binder::ModuleAugmentation)>> {
        use tsz_parser::parser::NodeIndex;

        let mut map: FxHashMap<String, Vec<(usize, tsz_binder::ModuleAugmentation)>> =
            FxHashMap::default();

        let mut keys: Vec<&String> = self.module_augmentations_by_spec.keys().collect();
        keys.sort();

        for spec in keys {
            let entries = &self.module_augmentations_by_spec[spec];
            let mut out: Vec<(usize, tsz_binder::ModuleAugmentation)> =
                Vec::with_capacity(entries.iter().map(|(_, aug)| aug.declarations.len()).sum());
            for (file_idx, aug) in entries {
                let arena = arenas.get(*file_idx);
                for decl in &aug.declarations {
                    let node_idx = arena
                        .and_then(|a| {
                            a.nodes.iter().enumerate().find_map(|(i, node)| {
                                (node.pos == decl.location.pos && node.end == decl.location.end)
                                    .then_some(NodeIndex(i as u32))
                            })
                        })
                        .unwrap_or(NodeIndex::NONE);
                    let mut entry =
                        tsz_binder::ModuleAugmentation::new(decl.name.clone(), node_idx);
                    if let Some(a) = arena {
                        entry.arena = Some(std::sync::Arc::clone(a));
                    }
                    out.push((*file_idx, entry));
                }
            }
            map.insert(spec.clone(), out);
        }

        map
    }

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

    /// Lookup augmentation-target entries for a given module specifier.
    ///
    /// Returns the per-file [`SkeletonAugmentationTarget`] entries (each
    /// carrying a `(SymbolId, module_spec, StableLocation)` triple) recorded
    /// for `module_spec`. Empty slice if no augmentation targets reference
    /// this specifier.
    ///
    /// This is the Phase 2 step 3 skeleton-only path for
    /// `global_augmentation_targets_index`: the consumer can rebuild the
    /// merged checker-side index from this accessor alone, without iterating
    /// per-file binders. Once arenas become evictable in Phase 5 the
    /// augmenting AST node can be rehydrated from the [`StableLocation`] via
    /// `CheckerContext::node_at_stable_location`.
    ///
    /// Entries are recorded in driver file order — the same order the legacy
    /// `binder.augmentation_target_modules.iter()` loop's enumeration would
    /// produce when walking files in driver order.
    #[must_use]
    pub fn augmentation_targets_for(
        &self,
        module_spec: &str,
    ) -> &[(usize, SkeletonAugmentationTarget)] {
        self.augmentation_targets_by_spec
            .get(module_spec)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Build the legacy `module_specifier -> Vec<(SymbolId, file_idx)>` map
    /// from skeleton-recorded augmentation-target entries.
    ///
    /// Phase 2 step 3 helper: projects the skeleton-recorded
    /// `(file_idx, SkeletonAugmentationTarget)` entries into the legacy shape
    /// (`Vec<(SymbolId, file_idx)>`) understood by the checker's
    /// `global_augmentation_targets_index` consumers (e.g.
    /// `module_augmentation.rs`). This lets the build path skip the
    /// per-binder `augmentation_target_modules` loop entirely.
    ///
    /// Spec keys are visited in sorted order; per-spec entries preserve the
    /// driver file order recorded by [`reduce_skeletons`]. Within a single
    /// `(spec, file)` slot, targets are appended in
    /// `SkeletonAugmentationTarget` order (already sorted by
    /// `(module_spec, symbol_id)` at extract time).
    #[must_use]
    pub fn build_augmentation_targets_index(
        &self,
    ) -> FxHashMap<String, Vec<(tsz_binder::SymbolId, usize)>> {
        let mut map: FxHashMap<String, Vec<(tsz_binder::SymbolId, usize)>> = FxHashMap::default();

        let mut keys: Vec<&String> = self.augmentation_targets_by_spec.keys().collect();
        keys.sort();

        for spec in keys {
            let entries = &self.augmentation_targets_by_spec[spec];
            let mut out: Vec<(tsz_binder::SymbolId, usize)> = Vec::with_capacity(entries.len());
            for (file_idx, target) in entries {
                out.push((target.symbol_id, *file_idx));
            }
            map.insert(spec.clone(), out);
        }

        map
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

    /// Lookup module-binder file indices for a given module specifier.
    ///
    /// Returns the per-spec file indices recorded for `module_spec` — i.e. the
    /// list of files whose `module_exports[module_spec]` is non-empty. Empty
    /// slice if no file declares exports under this specifier.
    ///
    /// This is the Phase 2 step 4 skeleton-only path for
    /// `global_module_binder_index`: the consumer can rebuild the merged
    /// checker-side index from this accessor alone, without iterating
    /// per-file binders. Once arenas become evictable in Phase 5 the
    /// per-binder `module_exports` map is no longer needed for this lookup.
    ///
    /// Both the raw module specifier (e.g. `"\"foo\""`) and its de-quoted
    /// ("normalized") variant (e.g. `"foo"`) resolve to the same file index
    /// list — same as the legacy per-binder loop in
    /// `ProjectEnv::build_global_indices`.
    ///
    /// Entries are recorded in driver file order — the same order the legacy
    /// `binder.module_exports.iter()` loop's enumeration would produce when
    /// walking files in driver order.
    #[must_use]
    pub fn module_binders_for(&self, module_spec: &str) -> &[usize] {
        self.module_binder_index_by_spec
            .get(module_spec)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Build the legacy `module_specifier -> Vec<file_idx>` map from
    /// skeleton-recorded module-export-specifier entries.
    ///
    /// Phase 2 step 4 helper: projects the skeleton-recorded
    /// `module_binder_index_by_spec` into the legacy shape understood by
    /// `ProjectEnv::global_module_binder_index` consumers (e.g.
    /// `import/declaration.rs`, `module_entity.rs`, `type_resolution/module.rs`).
    /// This lets the build path skip the per-binder `module_exports` loop
    /// entirely for the binder-index slot.
    ///
    /// Spec keys are visited in sorted order; per-spec entries preserve the
    /// driver file order recorded by [`reduce_skeletons`]. Both the raw and
    /// normalized (de-quoted) spec keys are present when they differ — same
    /// as the legacy per-binder loop.
    #[must_use]
    pub fn build_module_binder_index(&self) -> FxHashMap<String, Vec<usize>> {
        let mut map: FxHashMap<String, Vec<usize>> = FxHashMap::default();

        let mut keys: Vec<&String> = self.module_binder_index_by_spec.keys().collect();
        keys.sort();

        for spec in keys {
            map.insert(spec.clone(), self.module_binder_index_by_spec[spec].clone());
        }

        map
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

    // -------------------------------------------------------------------------
    // Phase 2 step 6: module-exports index served from SkeletonIndex.
    // -------------------------------------------------------------------------

    /// Lookup module-exports entries for a given module specifier.
    ///
    /// Returns the `export_name -> [file_idx]` map recorded for `module_spec`.
    /// Returns `None` if no file declared exports under this spec.
    ///
    /// `SymbolId`s are NOT returned here — they are resolved at projection
    /// time from the post-merge `program.module_exports` map. Use
    /// [`Self::build_module_exports_index`] for the legacy
    /// `(file_idx, SymbolId)` shape that checker consumers expect.
    #[must_use]
    pub fn module_exports_for(&self, module_spec: &str) -> Option<&SkeletonModuleExportsByName> {
        self.module_exports_index_by_spec.get(module_spec)
    }

    /// Build the legacy `spec -> export_name -> Vec<(file_idx, SymbolId)>`
    /// map from skeleton-recorded module-export entries plus the post-merge
    /// `program.module_exports` map.
    ///
    /// Phase 2 step 6 helper: projects the skeleton-recorded
    /// `module_exports_index_by_spec` (which carries `[file_idx]` per
    /// `(spec, export_name)`) into the legacy shape understood by
    /// `ProjectEnv::global_module_exports_index` consumers (e.g.
    /// `type_only.rs`, `state/type_resolution/module.rs`,
    /// `state/type_resolution/import_type.rs`).
    ///
    /// The `SymbolId` for each `(spec, export_name)` pair is looked up in
    /// `merged_module_exports` (the `MergedProgram::module_exports` map),
    /// which holds globally-remapped post-merge `SymbolId`s. Entries whose
    /// `(spec, export_name)` does not appear in `merged_module_exports` are
    /// dropped — mirroring the legacy `remap_symbol_table` filter that drops
    /// entries whose pre-merge `SymbolId` did not survive merging.
    ///
    /// Spec keys are visited in sorted order; per-spec/per-export entries
    /// preserve the driver file order recorded by [`reduce_skeletons`]. Both
    /// the raw and normalized (de-quoted) spec keys are present when they
    /// differ — same as the legacy per-binder loop in
    /// `ProjectEnv::build_global_indices`.
    #[must_use]
    pub fn build_module_exports_index(
        &self,
        merged_module_exports: &FxHashMap<String, SymbolTable>,
    ) -> ProjectedModuleExportsIndex {
        let mut out: ProjectedModuleExportsIndex = FxHashMap::default();

        let mut spec_keys: Vec<&String> = self.module_exports_index_by_spec.keys().collect();
        spec_keys.sort();

        for spec in spec_keys {
            // Resolve SymbolIds via the merged map. The merged
            // `module_exports` may key by the raw spec (e.g. `"\"foo\""`) or
            // by the de-quoted normalized form (e.g. `"foo"`) depending on
            // how the binder recorded it. The skeleton index records both
            // variants, so when looking up the merged map we also try the
            // alternate form. Tries in order: exact match, de-quoted
            // alternate, single-quoted variant, double-quoted variant.
            let trimmed = spec.trim_matches('"').trim_matches('\'');
            let dq = format!("\"{trimmed}\"");
            let sq = format!("'{trimmed}'");
            let merged_table = merged_module_exports
                .get(spec)
                .or_else(|| {
                    if trimmed != spec.as_str() {
                        merged_module_exports.get(trimmed)
                    } else {
                        None
                    }
                })
                .or_else(|| merged_module_exports.get(&dq))
                .or_else(|| merged_module_exports.get(&sq));

            let Some(table) = merged_table else {
                continue;
            };

            let by_name = &self.module_exports_index_by_spec[spec];
            let mut name_keys: Vec<&String> = by_name.keys().collect();
            name_keys.sort();

            let mut projected_inner: ProjectedModuleExportsByName = FxHashMap::default();
            for name in name_keys {
                let Some(sym_id) = table.get(name) else {
                    continue;
                };
                let entries: Vec<(usize, SymbolId)> = by_name[name]
                    .iter()
                    .map(|&file_idx| (file_idx, sym_id))
                    .collect();
                projected_inner.insert(name.clone(), entries);
            }

            if !projected_inner.is_empty() {
                out.insert(spec.clone(), projected_inner);
            }
        }

        out
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

/// Estimate the in-memory size of a `FileSkeleton` in bytes.
///
/// This is a rough estimate for comparison with full `BindResult` size.
/// It counts string allocations and vec capacities.
impl FileSkeleton {
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        let mut size = std::mem::size_of::<Self>();
        size += self.file_name.capacity();
        for sym in &self.symbols {
            size += std::mem::size_of::<SkeletonSymbol>();
            size += sym.name.capacity();
            if let Some(ref m) = sym.import_module {
                size += m.capacity();
            }
        }
        for aug in &self.global_augmentations {
            size += std::mem::size_of::<SkeletonAugmentation>();
            size += aug.target.capacity();
            size += aug.declarations.capacity() * std::mem::size_of::<SkeletonAugmentationDecl>();
            for decl in &aug.declarations {
                size += decl.name.capacity();
            }
        }
        for aug in &self.module_augmentations {
            size += std::mem::size_of::<SkeletonAugmentation>();
            size += aug.target.capacity();
            size += aug.declarations.capacity() * std::mem::size_of::<SkeletonAugmentationDecl>();
            for decl in &aug.declarations {
                size += decl.name.capacity();
            }
        }
        for target in &self.augmentation_targets {
            size += std::mem::size_of::<SkeletonAugmentationTarget>();
            size += target.module_spec.capacity();
        }
        for re in &self.reexports {
            size += std::mem::size_of::<SkeletonReexport>();
            size += re.exported_name.capacity();
            size += re.source_module.capacity();
            if let Some(ref o) = re.original_name {
                size += o.capacity();
            }
        }
        for wre in &self.wildcard_reexports {
            size += std::mem::size_of::<SkeletonWildcardReexport>();
            size += wre.source_module.capacity();
        }
        for dm in &self.declared_modules {
            size += dm.capacity();
        }
        for sm in &self.shorthand_ambient_modules {
            size += sm.capacity();
        }
        for ms in &self.module_export_specifiers {
            size += ms.capacity();
        }
        for (spec, names) in &self.module_exports_entries {
            size += std::mem::size_of::<(String, Vec<String>)>();
            size += spec.capacity();
            size += names.capacity() * std::mem::size_of::<String>();
            for name in names {
                size += name.capacity();
            }
        }
        for (obj_key, props) in &self.expando_properties {
            size += obj_key.capacity();
            for prop in props {
                size += prop.capacity();
            }
        }
        for src in &self.import_sources {
            size += src.capacity();
        }
        size
    }
}

// =============================================================================
// Symbol Merging
// =============================================================================

// =============================================================================
// Skeleton invalidation — compare snapshots to detect changed files
// =============================================================================

/// Result of comparing two skeleton snapshots for incremental invalidation.
///
/// Used by LSP and incremental drivers to determine which files need
/// re-merging after a file change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkeletonDiff {
    /// Files whose merge-relevant skeleton changed (need re-merge).
    pub changed: Vec<String>,
    /// Files that are new (not present in the previous snapshot).
    pub added: Vec<String>,
    /// Files that were removed (present before but not now).
    pub removed: Vec<String>,
    /// Whether the aggregate project topology changed.
    pub topology_changed: bool,
}

impl SkeletonDiff {
    /// Returns true if no merge-relevant changes were detected.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.added.is_empty() && self.removed.is_empty()
    }

    /// Total number of affected files.
    #[must_use]
    pub const fn affected_count(&self) -> usize {
        self.changed.len() + self.added.len() + self.removed.len()
    }
}

/// Compare two sets of file skeletons to identify merge-relevant changes.
///
/// Compares fingerprints per file to detect which files changed their
/// merge-relevant topology (exported symbols, augmentations, re-exports).
/// Files with identical fingerprints are guaranteed unchanged.
///
/// This is a pure function suitable for incremental invalidation drivers.
pub fn diff_skeletons(previous: &[FileSkeleton], current: &[FileSkeleton]) -> SkeletonDiff {
    let prev_map: FxHashMap<&str, u64> = previous
        .iter()
        .map(|s| (s.file_name.as_str(), s.fingerprint))
        .collect();
    let curr_map: FxHashMap<&str, u64> = current
        .iter()
        .map(|s| (s.file_name.as_str(), s.fingerprint))
        .collect();

    let mut changed = Vec::new();
    let mut added = Vec::new();
    let mut removed = Vec::new();

    // Check current files against previous
    for skel in current {
        match prev_map.get(skel.file_name.as_str()) {
            Some(&prev_fp) if prev_fp != skel.fingerprint => {
                changed.push(skel.file_name.clone());
            }
            None => {
                added.push(skel.file_name.clone());
            }
            _ => {} // unchanged
        }
    }

    // Check for removed files
    for skel in previous {
        if !curr_map.contains_key(skel.file_name.as_str()) {
            removed.push(skel.file_name.clone());
        }
    }

    let prev_index = reduce_skeletons(previous);
    let curr_index = reduce_skeletons(current);
    let topology_changed = prev_index.fingerprint != curr_index.fingerprint;

    SkeletonDiff {
        changed,
        added,
        removed,
        topology_changed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to make a minimal skeleton for testing diffs.
    fn make_skeleton(name: &str, fingerprint: u64) -> FileSkeleton {
        FileSkeleton {
            file_name: name.to_string(),
            is_external_module: true,
            symbols: vec![],
            global_augmentations: vec![],
            module_augmentations: vec![],
            augmentation_targets: vec![],
            reexports: vec![],
            wildcard_reexports: vec![],
            expando_properties: vec![],
            declared_modules: vec![],
            shorthand_ambient_modules: vec![],
            module_export_specifiers: vec![],
            module_exports_entries: vec![],
            import_sources: vec![],
            file_features: Default::default(),
            fingerprint,
        }
    }

    #[test]
    fn diff_identical_skeletons_is_empty() {
        let skels = vec![make_skeleton("a.ts", 100), make_skeleton("b.ts", 200)];
        let diff = diff_skeletons(&skels, &skels);
        assert!(diff.is_empty());
        assert_eq!(diff.affected_count(), 0);
        assert!(!diff.topology_changed);
    }

    #[test]
    fn diff_detects_changed_file() {
        let prev = vec![make_skeleton("a.ts", 100), make_skeleton("b.ts", 200)];
        let curr = vec![
            make_skeleton("a.ts", 100),
            make_skeleton("b.ts", 999), // changed
        ];
        let diff = diff_skeletons(&prev, &curr);
        assert_eq!(diff.changed, vec!["b.ts"]);
        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
        assert_eq!(diff.affected_count(), 1);
    }

    #[test]
    fn diff_detects_added_file() {
        let prev = vec![make_skeleton("a.ts", 100)];
        let curr = vec![make_skeleton("a.ts", 100), make_skeleton("new.ts", 300)];
        let diff = diff_skeletons(&prev, &curr);
        assert!(diff.changed.is_empty());
        assert_eq!(diff.added, vec!["new.ts"]);
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn diff_detects_removed_file() {
        let prev = vec![make_skeleton("a.ts", 100), make_skeleton("old.ts", 200)];
        let curr = vec![make_skeleton("a.ts", 100)];
        let diff = diff_skeletons(&prev, &curr);
        assert!(diff.changed.is_empty());
        assert!(diff.added.is_empty());
        assert_eq!(diff.removed, vec!["old.ts"]);
    }

    #[test]
    fn diff_detects_combined_changes() {
        let prev = vec![
            make_skeleton("keep.ts", 100),
            make_skeleton("change.ts", 200),
            make_skeleton("remove.ts", 300),
        ];
        let curr = vec![
            make_skeleton("keep.ts", 100),
            make_skeleton("change.ts", 999),
            make_skeleton("add.ts", 400),
        ];
        let diff = diff_skeletons(&prev, &curr);
        assert_eq!(diff.changed, vec!["change.ts"]);
        assert_eq!(diff.added, vec!["add.ts"]);
        assert_eq!(diff.removed, vec!["remove.ts"]);
        assert_eq!(diff.affected_count(), 3);
        assert!(diff.topology_changed);
    }

    #[test]
    fn heritage_fingerprint_affects_skeleton_fingerprint() {
        // Two skeletons identical except for heritage fingerprint on a symbol
        // should produce different fingerprints.
        let sym_no_heritage = SkeletonSymbol {
            name: "Foo".to_string(),
            flags: 0,
            is_exported: true,
            declaration_count: 1,
            has_exports: false,
            has_members: false,
            is_lib_origin: false,
            is_import_alias: false,
            import_module: None,
            heritage_fingerprint: 0,
            heritage_count: 0,
        };
        let sym_with_heritage = SkeletonSymbol {
            heritage_fingerprint: 42,
            heritage_count: 1,
            ..sym_no_heritage.clone()
        };

        let mut skel1 = FileSkeleton {
            file_name: "test.ts".to_string(),
            is_external_module: true,
            symbols: vec![sym_no_heritage],
            global_augmentations: vec![],
            module_augmentations: vec![],
            augmentation_targets: vec![],
            reexports: vec![],
            wildcard_reexports: vec![],
            expando_properties: vec![],
            declared_modules: vec![],
            shorthand_ambient_modules: vec![],
            module_export_specifiers: vec![],
            module_exports_entries: vec![],
            import_sources: vec![],
            file_features: Default::default(),
            fingerprint: 0,
        };
        skel1.fingerprint = skel1.compute_fingerprint();

        let mut skel2 = FileSkeleton {
            file_name: "test.ts".to_string(),
            is_external_module: true,
            symbols: vec![sym_with_heritage],
            global_augmentations: vec![],
            module_augmentations: vec![],
            augmentation_targets: vec![],
            reexports: vec![],
            wildcard_reexports: vec![],
            expando_properties: vec![],
            declared_modules: vec![],
            shorthand_ambient_modules: vec![],
            module_export_specifiers: vec![],
            module_exports_entries: vec![],
            import_sources: vec![],
            file_features: Default::default(),
            fingerprint: 0,
        };
        skel2.fingerprint = skel2.compute_fingerprint();

        assert_ne!(
            skel1.fingerprint, skel2.fingerprint,
            "Heritage fingerprint should change the skeleton fingerprint"
        );
    }

    #[test]
    fn heritage_fingerprint_included_in_skeleton_symbol_hash() {
        use std::hash::{Hash, Hasher};

        let sym1 = SkeletonSymbol {
            name: "Foo".to_string(),
            flags: 0,
            is_exported: false,
            declaration_count: 1,
            has_exports: false,
            has_members: false,
            is_lib_origin: false,
            is_import_alias: false,
            import_module: None,
            heritage_fingerprint: 0,
            heritage_count: 0,
        };
        let sym2 = SkeletonSymbol {
            heritage_fingerprint: 99,
            heritage_count: 2,
            ..sym1.clone()
        };

        let hash_of = |sym: &SkeletonSymbol| {
            let mut h = rustc_hash::FxHasher::default();
            sym.hash(&mut h);
            h.finish()
        };

        assert_ne!(
            hash_of(&sym1),
            hash_of(&sym2),
            "Different heritage fingerprints should produce different hashes"
        );
    }

    #[test]
    fn diff_detects_heritage_change() {
        // Heritage name change should be detected as a changed file.
        let sym1 = SkeletonSymbol {
            name: "Foo".to_string(),
            flags: 0,
            is_exported: true,
            declaration_count: 1,
            has_exports: false,
            has_members: false,
            is_lib_origin: false,
            is_import_alias: false,
            import_module: None,
            heritage_fingerprint: 10,
            heritage_count: 1,
        };
        let sym2 = SkeletonSymbol {
            heritage_fingerprint: 20,
            heritage_count: 1,
            ..sym1.clone()
        };

        let mut prev = FileSkeleton {
            file_name: "a.ts".to_string(),
            is_external_module: true,
            symbols: vec![sym1],
            global_augmentations: vec![],
            module_augmentations: vec![],
            augmentation_targets: vec![],
            reexports: vec![],
            wildcard_reexports: vec![],
            expando_properties: vec![],
            declared_modules: vec![],
            shorthand_ambient_modules: vec![],
            module_export_specifiers: vec![],
            module_exports_entries: vec![],
            import_sources: vec![],
            file_features: Default::default(),
            fingerprint: 0,
        };
        prev.fingerprint = prev.compute_fingerprint();

        let mut curr = FileSkeleton {
            file_name: "a.ts".to_string(),
            is_external_module: true,
            symbols: vec![sym2],
            global_augmentations: vec![],
            module_augmentations: vec![],
            augmentation_targets: vec![],
            reexports: vec![],
            wildcard_reexports: vec![],
            expando_properties: vec![],
            declared_modules: vec![],
            shorthand_ambient_modules: vec![],
            module_export_specifiers: vec![],
            module_exports_entries: vec![],
            import_sources: vec![],
            file_features: Default::default(),
            fingerprint: 0,
        };
        curr.fingerprint = curr.compute_fingerprint();

        let diff = diff_skeletons(&[prev], &[curr]);
        assert_eq!(
            diff.changed,
            vec!["a.ts"],
            "Heritage name change should be detected"
        );
    }

    // -------------------------------------------------------------------------
    // Phase 2 step 1: ambient module resolution served from SkeletonIndex alone.
    //
    // The CLI driver's `module_resolver.lookup` `is_ambient_module` closure used to
    // read `MergedProgram.declared_modules` and `MergedProgram.shorthand_ambient_modules`
    // directly. The migrated path routes through `SkeletonIndex::is_ambient_module`,
    // which means the consumer can answer the lookup without retaining any arena
    // / binder state from the legacy merge. These tests prove that.
    // -------------------------------------------------------------------------

    /// Helper: build a `SkeletonIndex` with the given ambient module declarations.
    /// Constructs a skeleton WITHOUT any `MergedProgram` involvement, demonstrating
    /// that `is_ambient_module` is fully arena-free.
    fn skeleton_index_with_ambient_modules(declared: &[&str], shorthand: &[&str]) -> SkeletonIndex {
        let skel = FileSkeleton {
            file_name: "ambient.d.ts".to_string(),
            is_external_module: false,
            symbols: vec![],
            global_augmentations: vec![],
            module_augmentations: vec![],
            augmentation_targets: vec![],
            reexports: vec![],
            wildcard_reexports: vec![],
            expando_properties: vec![],
            declared_modules: declared.iter().map(|s| (*s).to_string()).collect(),
            shorthand_ambient_modules: shorthand.iter().map(|s| (*s).to_string()).collect(),
            module_export_specifiers: vec![],
            module_exports_entries: vec![],
            import_sources: vec![],
            file_features: Default::default(),
            fingerprint: 0,
        };
        reduce_skeletons(&[skel])
    }

    #[test]
    fn is_ambient_module_matches_declared_modules() {
        let idx = skeleton_index_with_ambient_modules(&["my-lib", "react"], &[]);
        assert!(idx.is_ambient_module("my-lib"));
        assert!(idx.is_ambient_module("react"));
        assert!(!idx.is_ambient_module("not-declared"));
    }

    #[test]
    fn is_ambient_module_matches_shorthand_modules() {
        let idx = skeleton_index_with_ambient_modules(&[], &["*.json", "shorthand-only"]);
        assert!(idx.is_ambient_module("*.json"));
        assert!(idx.is_ambient_module("shorthand-only"));
        assert!(!idx.is_ambient_module("react"));
    }

    #[test]
    fn is_ambient_module_unions_both_sets() {
        // Mixed: some names from declared_modules, some from shorthand.
        let idx = skeleton_index_with_ambient_modules(&["with-body"], &["bodyless"]);
        assert!(
            idx.is_ambient_module("with-body"),
            "declared_modules entry should be detected"
        );
        assert!(
            idx.is_ambient_module("bodyless"),
            "shorthand_ambient_modules entry should be detected"
        );
        assert!(!idx.is_ambient_module("neither"));
    }

    #[test]
    fn is_ambient_module_returns_false_on_empty_index() {
        let idx = skeleton_index_with_ambient_modules(&[], &[]);
        assert!(!idx.is_ambient_module("anything"));
    }

    #[test]
    fn is_ambient_module_uses_exact_match_no_normalization() {
        // The legacy MergedProgram.declared_modules stores names without quotes
        // (the binder strips quotes before insertion), so the skeleton's contains
        // check must be exact-match in the same encoding. Quoted strings should
        // NOT match unquoted entries (and vice versa).
        let idx = skeleton_index_with_ambient_modules(&["my-lib"], &[]);
        assert!(idx.is_ambient_module("my-lib"));
        assert!(
            !idx.is_ambient_module("\"my-lib\""),
            "raw quoted string must not match unquoted declared name (parity with legacy semantics)"
        );
    }

    #[test]
    fn is_ambient_module_consumer_works_after_legacy_fields_emptied() {
        // Phase 5 scenario: the consumer must still produce the correct answer
        // when the legacy MergedProgram fields are evicted/empty. We model this
        // by constructing `SkeletonIndex` directly (no MergedProgram involvement)
        // and verifying the consumer-shaped closure (mirroring the CLI driver's
        // `is_ambient_module` closure) returns the right answer.
        let idx = skeleton_index_with_ambient_modules(&["my-lib"], &["*.css"]);

        // Mirror the CLI driver's consumer closure (post-migration shape):
        //   |spec| skeleton.is_ambient_module(spec)
        let consumer = |spec: &str| -> bool { idx.is_ambient_module(spec) };

        assert!(
            consumer("my-lib"),
            "declared module must be visible to consumer"
        );
        assert!(
            consumer("*.css"),
            "shorthand ambient must be visible to consumer"
        );
        assert!(!consumer("not-ambient"));
    }

    #[test]
    fn is_ambient_module_aggregates_across_files() {
        // The reducer unions declared_modules and shorthand_ambient_modules from
        // every input skeleton. The consumer must see the cross-file union.
        let skel_a = FileSkeleton {
            file_name: "a.d.ts".to_string(),
            is_external_module: false,
            symbols: vec![],
            global_augmentations: vec![],
            module_augmentations: vec![],
            augmentation_targets: vec![],
            reexports: vec![],
            wildcard_reexports: vec![],
            expando_properties: vec![],
            declared_modules: vec!["from-a".to_string()],
            shorthand_ambient_modules: vec![],
            module_export_specifiers: vec![],
            module_exports_entries: vec![],
            import_sources: vec![],
            file_features: Default::default(),
            fingerprint: 0,
        };
        let skel_b = FileSkeleton {
            file_name: "b.d.ts".to_string(),
            is_external_module: false,
            symbols: vec![],
            global_augmentations: vec![],
            module_augmentations: vec![],
            augmentation_targets: vec![],
            reexports: vec![],
            wildcard_reexports: vec![],
            expando_properties: vec![],
            declared_modules: vec![],
            shorthand_ambient_modules: vec!["from-b".to_string()],
            module_export_specifiers: vec![],
            module_exports_entries: vec![],
            import_sources: vec![],
            file_features: Default::default(),
            fingerprint: 0,
        };
        let idx = reduce_skeletons(&[skel_a, skel_b]);
        assert!(idx.is_ambient_module("from-a"));
        assert!(idx.is_ambient_module("from-b"));
        assert!(!idx.is_ambient_module("from-neither"));
    }

    // -------------------------------------------------------------------------
    // Phase 2 step 2 / step 3: module-augmentations and augmentation-targets
    // indexes served from SkeletonIndex.
    //
    // The checker's `global_module_augmentations_index` was previously built
    // by iterating every binder's `module_augmentations` map. Phase 2 step 2
    // moves the build to `SkeletonIndex::module_augmentations_for(...)` /
    // `build_module_augmentations_index(...)`.
    //
    // The checker's `global_augmentation_targets_index` was previously built
    // by iterating every binder's `augmentation_target_modules` map. Phase 2
    // step 3 moves the build to `SkeletonIndex::augmentation_targets_for(...)` /
    // `build_augmentation_targets_index(...)`.
    //
    // Both let the checker rebuild the merged index from skeleton data alone
    // — required for Phase 5 (arena eviction).
    // -------------------------------------------------------------------------

    /// Helper: build a skeleton with the given module-augmentation entries.
    #[allow(clippy::type_complexity)]
    fn skeleton_with_module_augmentations(
        file_name: &str,
        augs: Vec<(String, Vec<(String, u32, u32)>)>,
    ) -> FileSkeleton {
        let module_augmentations: Vec<SkeletonAugmentation> = augs
            .into_iter()
            .map(|(target, decls)| SkeletonAugmentation {
                target,
                declaration_count: decls.len() as u32,
                declarations: decls
                    .into_iter()
                    .map(|(name, pos, end)| SkeletonAugmentationDecl {
                        name,
                        location: StableLocation::with_unassigned_file(pos, end),
                    })
                    .collect(),
            })
            .collect();
        let mut skel = FileSkeleton {
            file_name: file_name.to_string(),
            is_external_module: true,
            symbols: vec![],
            global_augmentations: vec![],
            module_augmentations,
            augmentation_targets: vec![],
            reexports: vec![],
            wildcard_reexports: vec![],
            expando_properties: vec![],
            declared_modules: vec![],
            shorthand_ambient_modules: vec![],
            module_export_specifiers: vec![],
            module_exports_entries: vec![],
            import_sources: vec![],
            file_features: Default::default(),
            fingerprint: 0,
        };
        skel.fingerprint = skel.compute_fingerprint();
        skel
    }

    /// Helper: build a skeleton with the given augmentation-target entries.
    /// Each tuple is `(symbol_id, module_spec, pos, end)`.
    fn skeleton_with_augmentation_targets(
        file_name: &str,
        targets: Vec<(u32, String, u32, u32)>,
    ) -> FileSkeleton {
        let augmentation_targets: Vec<SkeletonAugmentationTarget> = targets
            .into_iter()
            .map(
                |(sym_id, module_spec, pos, end)| SkeletonAugmentationTarget {
                    symbol_id: tsz_binder::SymbolId(sym_id),
                    module_spec,
                    stable_location: StableLocation::with_unassigned_file(pos, end),
                },
            )
            .collect();
        let mut skel = FileSkeleton {
            file_name: file_name.to_string(),
            is_external_module: true,
            symbols: vec![],
            global_augmentations: vec![],
            module_augmentations: vec![],
            augmentation_targets,
            reexports: vec![],
            wildcard_reexports: vec![],
            expando_properties: vec![],
            declared_modules: vec![],
            shorthand_ambient_modules: vec![],
            module_export_specifiers: vec![],
            module_exports_entries: vec![],
            import_sources: vec![],
            file_features: Default::default(),
            fingerprint: 0,
        };
        skel.fingerprint = skel.compute_fingerprint();
        skel
    }

    #[test]
    fn module_augmentations_for_returns_per_file_entries() {
        let skel_a = skeleton_with_module_augmentations(
            "a.ts",
            vec![("./shared".to_string(), vec![("Foo".to_string(), 10, 20)])],
        );
        let skel_b = skeleton_with_module_augmentations(
            "b.ts",
            vec![("./shared".to_string(), vec![("Bar".to_string(), 30, 40)])],
        );
        let idx = reduce_skeletons(&[skel_a, skel_b]);

        let entries = idx.module_augmentations_for("./shared");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, 0);
        assert_eq!(entries[0].1.declarations[0].name, "Foo");
        assert_eq!(entries[1].0, 1);
        assert_eq!(entries[1].1.declarations[0].name, "Bar");
    }

    #[test]
    fn module_augmentations_for_returns_empty_for_unknown_spec() {
        let idx = reduce_skeletons(&[]);
        assert!(idx.module_augmentations_for("./nope").is_empty());
    }

    #[test]
    fn module_augmentations_stamps_file_idx_into_locations() {
        // The reducer must stamp each declaration's StableLocation with the
        // owning file index so post-Phase-5 consumers can route through
        // `node_at_stable_location` without a separate file_idx arg.
        let skel_a = skeleton_with_module_augmentations(
            "a.ts",
            vec![("./m".to_string(), vec![("X".to_string(), 5, 12)])],
        );
        let skel_b = skeleton_with_module_augmentations(
            "b.ts",
            vec![("./m".to_string(), vec![("Y".to_string(), 100, 200)])],
        );
        let idx = reduce_skeletons(&[skel_a, skel_b]);
        let entries = idx.module_augmentations_for("./m");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].1.declarations[0].location.file_idx, 0);
        assert_eq!(entries[1].1.declarations[0].location.file_idx, 1);
    }

    #[test]
    fn module_augmentations_consumer_works_after_legacy_program_emptied() {
        // Phase 5 invariant: the checker-side merged map must be reproducible
        // from `SkeletonIndex` alone, even if the legacy `MergedProgram`'s
        // per-binder `module_augmentations` field has been emptied.
        //
        // We model the post-eviction state by building the index using only
        // the skeleton (and a per-file arena placeholder) — no MergedProgram
        // and no per-binder loop. The expected (spec, file_idx, name) set
        // recovered from the skeleton must match what the legacy loop would
        // have produced for the same inputs.
        let skel_a = skeleton_with_module_augmentations(
            "a.ts",
            vec![(
                "./shared".to_string(),
                vec![("Alpha".to_string(), 10, 20), ("Beta".to_string(), 25, 35)],
            )],
        );
        let skel_b = skeleton_with_module_augmentations(
            "b.ts",
            vec![("./shared".to_string(), vec![("Gamma".to_string(), 0, 5)])],
        );
        let skeletons = vec![skel_a, skel_b];
        let idx = reduce_skeletons(&skeletons);

        // Recover the legacy `(spec, file_idx, name)` triples directly from
        // the skeleton accessor — no MergedProgram involvement.
        let mut recovered: Vec<(String, usize, String)> = Vec::new();
        for (spec, entries) in &idx.module_augmentations_by_spec {
            for (file_idx, aug) in entries {
                for decl in &aug.declarations {
                    recovered.push((spec.clone(), *file_idx, decl.name.clone()));
                }
            }
        }
        recovered.sort();

        let mut expected: Vec<(String, usize, String)> = vec![
            ("./shared".to_string(), 0, "Alpha".to_string()),
            ("./shared".to_string(), 0, "Beta".to_string()),
            ("./shared".to_string(), 1, "Gamma".to_string()),
        ];
        expected.sort();

        assert_eq!(
            recovered, expected,
            "Skeleton-only recovery must reproduce legacy per-binder topology"
        );
    }

    #[test]
    fn build_module_augmentations_index_matches_legacy_topology() {
        // Cross-check `build_module_augmentations_index` against the
        // skeleton's per-spec data: every (spec, file_idx, name) triple
        // recorded in the skeleton must surface in the rebuilt map.
        let skel_a = skeleton_with_module_augmentations(
            "a.ts",
            vec![(
                "./mod".to_string(),
                vec![("First".to_string(), 1, 2), ("Second".to_string(), 3, 4)],
            )],
        );
        let skel_b = skeleton_with_module_augmentations(
            "b.ts",
            vec![("./mod".to_string(), vec![("Third".to_string(), 5, 6)])],
        );
        let idx = reduce_skeletons(&[skel_a, skel_b]);

        // Empty arenas: the helper should still produce the right (spec,
        // file_idx, name) topology with NodeIndex::NONE for unresolved nodes.
        let arenas: Vec<std::sync::Arc<tsz_parser::parser::node::NodeArena>> = Vec::new();
        let map = idx.build_module_augmentations_index(&arenas);

        let mut got: Vec<(String, usize, String)> = Vec::new();
        for (spec, entries) in &map {
            for (file_idx, aug) in entries {
                got.push((spec.clone(), *file_idx, aug.name.clone()));
            }
        }
        got.sort();
        let mut want: Vec<(String, usize, String)> = vec![
            ("./mod".to_string(), 0, "First".to_string()),
            ("./mod".to_string(), 0, "Second".to_string()),
            ("./mod".to_string(), 1, "Third".to_string()),
        ];
        want.sort();
        assert_eq!(got, want);
    }

    #[test]
    fn augmentation_targets_for_returns_per_file_entries() {
        let skel_a =
            skeleton_with_augmentation_targets("a.ts", vec![(7, "./shared".to_string(), 10, 20)]);
        let skel_b =
            skeleton_with_augmentation_targets("b.ts", vec![(11, "./shared".to_string(), 30, 40)]);
        let idx = reduce_skeletons(&[skel_a, skel_b]);

        let entries = idx.augmentation_targets_for("./shared");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, 0);
        assert_eq!(entries[0].1.symbol_id, tsz_binder::SymbolId(7));
        assert_eq!(entries[1].0, 1);
        assert_eq!(entries[1].1.symbol_id, tsz_binder::SymbolId(11));
    }

    #[test]
    fn augmentation_targets_for_returns_empty_for_unknown_spec() {
        let idx = reduce_skeletons(&[]);
        assert!(idx.augmentation_targets_for("./nope").is_empty());
    }

    #[test]
    fn augmentation_targets_stamps_file_idx_into_locations() {
        // The reducer must stamp each entry's StableLocation with the owning
        // file index so post-Phase-5 consumers can route through
        // `node_at_stable_location` without a separate file_idx arg.
        let skel_a =
            skeleton_with_augmentation_targets("a.ts", vec![(3, "./m".to_string(), 5, 12)]);
        let skel_b =
            skeleton_with_augmentation_targets("b.ts", vec![(4, "./m".to_string(), 100, 200)]);
        let idx = reduce_skeletons(&[skel_a, skel_b]);
        let entries = idx.augmentation_targets_for("./m");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].1.stable_location.file_idx, 0);
        assert_eq!(entries[1].1.stable_location.file_idx, 1);
        // Per-file pos/end preserved.
        assert_eq!(entries[0].1.stable_location.pos, 5);
        assert_eq!(entries[0].1.stable_location.end, 12);
        assert_eq!(entries[1].1.stable_location.pos, 100);
        assert_eq!(entries[1].1.stable_location.end, 200);
    }

    #[test]
    fn augmentation_targets_consumer_works_after_legacy_program_emptied() {
        // Phase 5 invariant: the checker-side merged map must be reproducible
        // from `SkeletonIndex` alone, even if the legacy `MergedProgram`'s
        // per-binder `augmentation_target_modules` field has been emptied.
        //
        // We model the post-eviction state by building the index using only
        // the skeleton — no MergedProgram and no per-binder loop. The expected
        // (spec, sym_id, file_idx) set recovered from the skeleton must match
        // what the legacy loop would have produced for the same inputs.
        let skel_a = skeleton_with_augmentation_targets(
            "a.ts",
            vec![
                (1, "./shared".to_string(), 10, 20),
                (2, "./shared".to_string(), 25, 35),
            ],
        );
        let skel_b =
            skeleton_with_augmentation_targets("b.ts", vec![(3, "./shared".to_string(), 0, 5)]);
        let skeletons = vec![skel_a, skel_b];
        let idx = reduce_skeletons(&skeletons);

        // Recover the legacy `(spec, sym_id, file_idx)` triples directly from
        // the skeleton accessor — no MergedProgram involvement.
        let mut recovered: Vec<(String, u32, usize)> = Vec::new();
        for (spec, entries) in &idx.augmentation_targets_by_spec {
            for (file_idx, target) in entries {
                recovered.push((spec.clone(), target.symbol_id.0, *file_idx));
            }
        }
        recovered.sort();

        let mut expected: Vec<(String, u32, usize)> = vec![
            ("./shared".to_string(), 1, 0),
            ("./shared".to_string(), 2, 0),
            ("./shared".to_string(), 3, 1),
        ];
        expected.sort();

        assert_eq!(
            recovered, expected,
            "Skeleton-only recovery must reproduce legacy per-binder topology"
        );
    }

    #[test]
    fn build_augmentation_targets_index_matches_legacy_topology() {
        // Cross-check `build_augmentation_targets_index` against the
        // skeleton's per-spec data: every (spec, sym_id, file_idx) triple
        // recorded in the skeleton must surface in the rebuilt map in the
        // legacy `Vec<(SymbolId, file_idx)>` shape.
        let skel_a = skeleton_with_augmentation_targets(
            "a.ts",
            vec![
                (5, "./mod".to_string(), 1, 2),
                (6, "./mod".to_string(), 3, 4),
            ],
        );
        let skel_b =
            skeleton_with_augmentation_targets("b.ts", vec![(7, "./mod".to_string(), 5, 6)]);
        let idx = reduce_skeletons(&[skel_a, skel_b]);

        let map = idx.build_augmentation_targets_index();

        let mut got: Vec<(String, u32, usize)> = Vec::new();
        for (spec, entries) in &map {
            for (sym_id, file_idx) in entries {
                got.push((spec.clone(), sym_id.0, *file_idx));
            }
        }
        got.sort();
        let mut want: Vec<(String, u32, usize)> = vec![
            ("./mod".to_string(), 5, 0),
            ("./mod".to_string(), 6, 0),
            ("./mod".to_string(), 7, 1),
        ];
        want.sort();
        assert_eq!(got, want);
    }

    // -------------------------------------------------------------------------
    // Phase 2 step 4: module-binder index served from SkeletonIndex.
    //
    // The checker's `global_module_binder_index` was previously built by
    // iterating every binder's `module_exports` map. Phase 2 step 4 moves the
    // build to `SkeletonIndex::module_binders_for(...)` /
    // `build_module_binder_index(...)`, letting the checker rebuild the
    // merged index from skeleton data alone — required for Phase 5 (arena
    // eviction).
    // -------------------------------------------------------------------------

    /// Helper: build a skeleton with the given module-export specifier list.
    /// `specs` carries the raw (possibly quoted) module specifier keys, exactly
    /// as the binder records them in `module_exports.keys()`.
    fn skeleton_with_module_export_specifiers(file_name: &str, specs: Vec<String>) -> FileSkeleton {
        let mut sorted = specs;
        sorted.sort();
        let mut skel = FileSkeleton {
            file_name: file_name.to_string(),
            is_external_module: true,
            symbols: vec![],
            global_augmentations: vec![],
            module_augmentations: vec![],
            augmentation_targets: vec![],
            reexports: vec![],
            wildcard_reexports: vec![],
            expando_properties: vec![],
            declared_modules: vec![],
            shorthand_ambient_modules: vec![],
            module_export_specifiers: sorted,
            module_exports_entries: vec![],
            import_sources: vec![],
            file_features: Default::default(),
            fingerprint: 0,
        };
        skel.fingerprint = skel.compute_fingerprint();
        skel
    }

    #[test]
    fn module_binders_for_returns_per_file_indices() {
        let skel_a = skeleton_with_module_export_specifiers("a.ts", vec!["my-lib".to_string()]);
        let skel_b = skeleton_with_module_export_specifiers("b.ts", vec!["my-lib".to_string()]);
        let skel_c = skeleton_with_module_export_specifiers("c.ts", vec!["other".to_string()]);
        let idx = reduce_skeletons(&[skel_a, skel_b, skel_c]);

        let mut got = idx.module_binders_for("my-lib").to_vec();
        got.sort();
        assert_eq!(got, vec![0, 1]);

        let other = idx.module_binders_for("other").to_vec();
        assert_eq!(other, vec![2]);
    }

    #[test]
    fn module_binders_for_returns_empty_for_unknown_spec() {
        let idx = reduce_skeletons(&[]);
        assert!(idx.module_binders_for("./nope").is_empty());
    }

    #[test]
    fn module_binders_records_normalized_variant_when_quoted() {
        // The legacy per-binder loop pushes file_idx for both the raw spec
        // (e.g. `"\"my-lib\""`) and its de-quoted form (`"my-lib"`).
        let skel = skeleton_with_module_export_specifiers("a.ts", vec!["\"my-lib\"".to_string()]);
        let idx = reduce_skeletons(&[skel]);

        let raw = idx.module_binders_for("\"my-lib\"").to_vec();
        assert_eq!(raw, vec![0]);

        let normalized = idx.module_binders_for("my-lib").to_vec();
        assert_eq!(normalized, vec![0]);
    }

    #[test]
    fn module_binders_consumer_works_after_legacy_program_emptied() {
        // Phase 5 invariant: the checker-side merged map must be reproducible
        // from `SkeletonIndex` alone, even if the legacy `MergedProgram`'s
        // per-binder `module_exports` field has been emptied.
        //
        // We model the post-eviction state by building the index using only
        // the skeleton — no MergedProgram and no per-binder loop. The expected
        // (spec, file_idx) set recovered from the skeleton must match what the
        // legacy loop would have produced for the same inputs.
        let skel_a = skeleton_with_module_export_specifiers(
            "a.ts",
            vec!["\"shared\"".to_string(), "other".to_string()],
        );
        let skel_b = skeleton_with_module_export_specifiers("b.ts", vec!["\"shared\"".to_string()]);
        let skeletons = vec![skel_a, skel_b];
        let idx = reduce_skeletons(&skeletons);

        // Recover the legacy `(spec, file_idx)` pairs directly from the
        // skeleton accessor — no MergedProgram involvement.
        let mut recovered: Vec<(String, usize)> = Vec::new();
        for (spec, files) in &idx.module_binder_index_by_spec {
            for f in files {
                recovered.push((spec.clone(), *f));
            }
        }
        recovered.sort();

        let mut expected: Vec<(String, usize)> = vec![
            // Raw quoted entries:
            ("\"shared\"".to_string(), 0),
            ("\"shared\"".to_string(), 1),
            // Normalized entries (de-quoted):
            ("shared".to_string(), 0),
            ("shared".to_string(), 1),
            // Unquoted spec only contributes once:
            ("other".to_string(), 0),
        ];
        expected.sort();

        assert_eq!(
            recovered, expected,
            "Skeleton-only recovery must reproduce legacy per-binder topology"
        );
    }

    #[test]
    fn build_module_binder_index_matches_legacy_topology() {
        // Cross-check `build_module_binder_index` against the skeleton's
        // per-spec data: every (spec, file_idx) pair recorded in the skeleton
        // must surface in the rebuilt map, including the de-quoted normalized
        // variant.
        let skel_a = skeleton_with_module_export_specifiers("a.ts", vec!["\"my-lib\"".to_string()]);
        let skel_b = skeleton_with_module_export_specifiers("b.ts", vec!["\"other\"".to_string()]);
        let idx = reduce_skeletons(&[skel_a, skel_b]);

        let map = idx.build_module_binder_index();

        let mut got: Vec<(String, usize)> = Vec::new();
        for (spec, files) in &map {
            for f in files {
                got.push((spec.clone(), *f));
            }
        }
        got.sort();
        let mut want: Vec<(String, usize)> = vec![
            ("\"my-lib\"".to_string(), 0),
            ("my-lib".to_string(), 0),
            ("\"other\"".to_string(), 1),
            ("other".to_string(), 1),
        ];
        want.sort();
        assert_eq!(got, want);
    }

    // -------------------------------------------------------------------------
    // Phase 2 step 6: module-exports index served from SkeletonIndex.
    //
    // The checker's `global_module_exports_index` was previously built by
    // iterating every binder's `module_exports[spec].iter()` map. Phase 2 step
    // 6 moves the build to `SkeletonIndex::module_exports_for(...)` /
    // `build_module_exports_index(merged_module_exports)`, letting the checker
    // rebuild the merged index from skeleton data plus the post-merge
    // `program.module_exports` map alone — required for Phase 5 (arena
    // eviction). SymbolIds are resolved at projection time from the post-merge
    // map (which holds globally-remapped IDs), avoiding the pre-merge local-
    // SymbolId trap that regressed PR #1145.
    // -------------------------------------------------------------------------

    /// Helper: build a skeleton with the given `(spec, [export_name])` entries.
    fn skeleton_with_module_exports_entries(
        file_name: &str,
        entries: Vec<(String, Vec<String>)>,
    ) -> FileSkeleton {
        let mut sorted_entries = entries;
        sorted_entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (_, names) in &mut sorted_entries {
            names.sort();
        }
        let module_export_specifiers: Vec<String> = sorted_entries
            .iter()
            .map(|(spec, _)| spec.clone())
            .collect();
        let mut skel = FileSkeleton {
            file_name: file_name.to_string(),
            is_external_module: true,
            symbols: vec![],
            global_augmentations: vec![],
            module_augmentations: vec![],
            augmentation_targets: vec![],
            reexports: vec![],
            wildcard_reexports: vec![],
            expando_properties: vec![],
            declared_modules: vec![],
            shorthand_ambient_modules: vec![],
            module_export_specifiers,
            module_exports_entries: sorted_entries,
            import_sources: vec![],
            file_features: Default::default(),
            fingerprint: 0,
        };
        skel.fingerprint = skel.compute_fingerprint();
        skel
    }

    #[test]
    fn module_exports_for_returns_per_file_indices_per_export() {
        let skel_a = skeleton_with_module_exports_entries(
            "a.ts",
            vec![(
                "my-lib".to_string(),
                vec!["foo".to_string(), "bar".to_string()],
            )],
        );
        let skel_b = skeleton_with_module_exports_entries(
            "b.ts",
            vec![("my-lib".to_string(), vec!["foo".to_string()])],
        );
        let skel_c = skeleton_with_module_exports_entries(
            "c.ts",
            vec![("other".to_string(), vec!["baz".to_string()])],
        );
        let idx = reduce_skeletons(&[skel_a, skel_b, skel_c]);

        let by_name = idx.module_exports_for("my-lib").unwrap();
        let mut foo = by_name["foo"].clone();
        foo.sort();
        assert_eq!(foo, vec![0, 1]);
        let bar = by_name["bar"].clone();
        assert_eq!(bar, vec![0]);

        let other = idx.module_exports_for("other").unwrap();
        assert_eq!(other["baz"], vec![2]);

        assert!(idx.module_exports_for("nope").is_none());
    }

    #[test]
    fn module_exports_records_normalized_variant_when_quoted() {
        let skel = skeleton_with_module_exports_entries(
            "a.ts",
            vec![("\"my-lib\"".to_string(), vec!["foo".to_string()])],
        );
        let idx = reduce_skeletons(&[skel]);

        let raw = idx.module_exports_for("\"my-lib\"").unwrap();
        assert_eq!(raw["foo"], vec![0]);

        let normalized = idx.module_exports_for("my-lib").unwrap();
        assert_eq!(normalized["foo"], vec![0]);
    }

    #[test]
    fn build_module_exports_index_resolves_sym_ids_from_merged_map() {
        // Two files declare overlapping exports under the same spec.
        let skel_a = skeleton_with_module_exports_entries(
            "a.ts",
            vec![("\"my-lib\"".to_string(), vec!["foo".to_string()])],
        );
        let skel_b = skeleton_with_module_exports_entries(
            "b.ts",
            vec![(
                "\"my-lib\"".to_string(),
                vec!["foo".to_string(), "bar".to_string()],
            )],
        );
        let idx = reduce_skeletons(&[skel_a, skel_b]);

        // Build a fake merged map: post-merge SymbolIds for the (spec,
        // export_name) pairs. The merge typically picks one SymbolId per
        // (spec, export_name) — the projection pairs every recorded
        // file_idx with that single id.
        let mut merged: FxHashMap<String, SymbolTable> = FxHashMap::default();
        let mut shared = SymbolTable::new();
        shared.set("foo".to_string(), SymbolId(101));
        shared.set("bar".to_string(), SymbolId(102));
        merged.insert("\"my-lib\"".to_string(), shared);

        let projected = idx.build_module_exports_index(&merged);

        // Raw spec key resolves to (file_idx, sym_id) entries for each export.
        let raw = &projected["\"my-lib\""];
        let mut foo_entries = raw["foo"].clone();
        foo_entries.sort_by_key(|(f, _)| *f);
        assert_eq!(
            foo_entries,
            vec![(0, SymbolId(101)), (1, SymbolId(101))],
            "foo should appear under both files with the merged sym_id"
        );
        let bar_entries = raw["bar"].clone();
        assert_eq!(
            bar_entries,
            vec![(1, SymbolId(102))],
            "bar should appear only under file 1 with the merged sym_id"
        );

        // The de-quoted normalized variant also resolves (the projection
        // falls back to the raw merged-map key when the normalized one is
        // missing, mirroring the legacy lookup).
        let normalized = &projected["my-lib"];
        let mut foo_norm = normalized["foo"].clone();
        foo_norm.sort_by_key(|(f, _)| *f);
        assert_eq!(foo_norm, vec![(0, SymbolId(101)), (1, SymbolId(101))]);
    }

    #[test]
    fn build_module_exports_index_drops_entries_missing_from_merged_map() {
        // The skeleton recorded an export name that did not survive the
        // merge (e.g., its pre-merge SymbolId was not in `id_remap`). The
        // projection must drop it — same as the legacy `remap_symbol_table`
        // filter.
        let skel = skeleton_with_module_exports_entries(
            "a.ts",
            vec![(
                "my-lib".to_string(),
                vec!["foo".to_string(), "ghost".to_string()],
            )],
        );
        let idx = reduce_skeletons(&[skel]);

        let mut merged: FxHashMap<String, SymbolTable> = FxHashMap::default();
        let mut tbl = SymbolTable::new();
        tbl.set("foo".to_string(), SymbolId(7));
        // "ghost" intentionally absent from the merged map.
        merged.insert("my-lib".to_string(), tbl);

        let projected = idx.build_module_exports_index(&merged);

        let by_name = &projected["my-lib"];
        assert_eq!(by_name["foo"], vec![(0, SymbolId(7))]);
        assert!(
            !by_name.contains_key("ghost"),
            "exports missing from the merged map must be dropped"
        );
    }

    #[test]
    fn build_module_exports_index_drops_specs_missing_from_merged_map() {
        // The skeleton recorded a spec key that was entirely dropped during
        // the merge. The projection must skip it.
        let skel = skeleton_with_module_exports_entries(
            "a.ts",
            vec![("dead-spec".to_string(), vec!["foo".to_string()])],
        );
        let idx = reduce_skeletons(&[skel]);

        let merged: FxHashMap<String, SymbolTable> = FxHashMap::default();
        let projected = idx.build_module_exports_index(&merged);
        assert!(projected.is_empty());
    }

    #[test]
    fn module_exports_consumer_works_after_legacy_program_emptied() {
        // Phase 5 invariant: the checker-side merged map must be reproducible
        // from `SkeletonIndex` + `program.module_exports` alone, even if every
        // per-binder `module_exports` field has been emptied (which is the
        // post-eviction state).
        //
        // We model this by:
        //   1) Building `SkeletonIndex` from skeleton-only inputs (no
        //      MergedProgram, no per-binder loop).
        //   2) Supplying a `merged_module_exports` map that mirrors what
        //      `MergedProgram.module_exports` would carry after merging the
        //      same files.
        //   3) Asserting the projection produces the same `(spec, export_name,
        //      file_idx, sym_id)` set the legacy per-binder loop would have
        //      computed.
        let skel_a = skeleton_with_module_exports_entries(
            "a.ts",
            vec![(
                "\"shared\"".to_string(),
                vec!["foo".to_string(), "bar".to_string()],
            )],
        );
        let skel_b = skeleton_with_module_exports_entries(
            "b.ts",
            vec![("\"shared\"".to_string(), vec!["foo".to_string()])],
        );
        let idx = reduce_skeletons(&[skel_a, skel_b]);

        let mut merged: FxHashMap<String, SymbolTable> = FxHashMap::default();
        let mut shared = SymbolTable::new();
        shared.set("foo".to_string(), SymbolId(50));
        shared.set("bar".to_string(), SymbolId(51));
        merged.insert("\"shared\"".to_string(), shared);

        let projected = idx.build_module_exports_index(&merged);

        // Recover the legacy `(spec, export_name, file_idx, sym_id)` tuples.
        let mut recovered: Vec<(String, String, usize, SymbolId)> = Vec::new();
        for (spec, by_name) in &projected {
            for (name, entries) in by_name {
                for &(file_idx, sym_id) in entries {
                    recovered.push((spec.clone(), name.clone(), file_idx, sym_id));
                }
            }
        }
        recovered.sort();

        let mut expected: Vec<(String, String, usize, SymbolId)> = vec![
            // Raw spec key entries:
            ("\"shared\"".to_string(), "foo".to_string(), 0, SymbolId(50)),
            ("\"shared\"".to_string(), "foo".to_string(), 1, SymbolId(50)),
            ("\"shared\"".to_string(), "bar".to_string(), 0, SymbolId(51)),
            // Normalized (de-quoted) entries:
            ("shared".to_string(), "foo".to_string(), 0, SymbolId(50)),
            ("shared".to_string(), "foo".to_string(), 1, SymbolId(50)),
            ("shared".to_string(), "bar".to_string(), 0, SymbolId(51)),
        ];
        expected.sort();

        assert_eq!(
            recovered, expected,
            "Skeleton + merged-map recovery must reproduce legacy per-binder topology"
        );
    }
}
