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
    /// Heritage clause names (`extends` / `implements`) from the declaration.
    ///
    /// Only populated for class and interface symbols. Changes to heritage
    /// references affect cross-file type resolution and must trigger re-merge.
    pub heritage_names: Vec<String>,
}

/// Augmentation candidate as seen from the skeleton layer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkeletonAugmentation {
    /// Target name (interface name for global augmentations, module specifier for module augmentations).
    pub target: String,
    /// Number of augmentation declarations for this target in this file.
    pub declaration_count: u32,
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
        self.reexports.hash(&mut hasher);
        self.wildcard_reexports.hash(&mut hasher);
        self.declared_modules.hash(&mut hasher);
        self.shorthand_ambient_modules.hash(&mut hasher);
        self.module_export_specifiers.hash(&mut hasher);
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
            let heritage_names = result
                .semantic_defs
                .get(&sym_id)
                .map_or_else(Vec::new, |def| def.heritage_names());
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
                heritage_names,
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
                let heritage_names = result
                    .semantic_defs
                    .get(&sym_id)
                    .map_or_else(Vec::new, |def| def.heritage_names());
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
                    heritage_names,
                });
            }
        }
    }

    // Sort symbols by name for deterministic output.
    symbols.sort_by(|a, b| a.name.cmp(&b.name));

    // Global augmentations
    let mut global_augmentations: Vec<SkeletonAugmentation> = result
        .global_augmentations
        .iter()
        .map(|(target, augs)| SkeletonAugmentation {
            target: target.clone(),
            declaration_count: augs.len() as u32,
        })
        .collect();
    global_augmentations.sort_by(|a, b| a.target.cmp(&b.target));

    // Module augmentations
    let mut module_augmentations: Vec<SkeletonAugmentation> = result
        .module_augmentations
        .iter()
        .map(|(target, augs)| SkeletonAugmentation {
            target: target.clone(),
            declaration_count: augs.len() as u32,
        })
        .collect();
    module_augmentations.sort_by(|a, b| a.target.cmp(&b.target));

    // Named re-exports
    let mut reexports = Vec::new();
    for (file_name, file_reexports) in &result.reexports {
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
    reexports.sort_by(|a, b| a.exported_name.cmp(&b.exported_name));

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
    wildcard_reexports.sort_by(|a, b| a.source_module.cmp(&b.source_module));

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
    expando_properties.sort_by(|a, b| a.0.cmp(&b.0));

    let mut skeleton = FileSkeleton {
        file_name: result.file_name.clone(),
        is_external_module: result.is_external_module,
        symbols,
        global_augmentations,
        module_augmentations,
        reexports,
        wildcard_reexports,
        declared_modules,
        shorthand_ambient_modules,
        module_export_specifiers,
        expando_properties,
        import_sources: result.file_import_sources.clone(),
        file_features: result.file_features,
        fingerprint: 0, // computed below
    };
    skeleton.fingerprint = skeleton.compute_fingerprint();
    skeleton
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
    /// All declared ambient modules across all files.
    pub declared_modules: FxHashSet<String>,
    /// All shorthand ambient modules across all files.
    pub shorthand_ambient_modules: FxHashSet<String>,
    /// All module export specifiers across all files (keys from `module_exports`).
    pub module_export_specifiers: FxHashSet<String>,
    /// Merged expando property assignments across all files.
    /// Maps identifier name -> set of property names assigned via `X.prop = value`.
    pub expando_properties: FxHashMap<String, FxHashSet<String>>,
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

/// Deterministically reduce a set of file skeletons into a `SkeletonIndex`.
///
/// This is a pure function: the same input skeletons (in the same order) always
/// produce the same output. The reduction is sequential and ordered.
///
/// # Arguments
/// * `skeletons` - Slice of file skeletons, in file order.
pub fn reduce_skeletons(skeletons: &[FileSkeleton]) -> SkeletonIndex {
    let mut symbol_map: FxHashMap<String, (u32, Vec<usize>)> = FxHashMap::default();
    let mut global_augmentation_targets: FxHashMap<String, Vec<usize>> = FxHashMap::default();
    let mut module_augmentation_targets: FxHashMap<String, Vec<usize>> = FxHashMap::default();
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
    merge_candidates.sort_by(|a, b| a.name.cmp(&b.name));

    let mut index = SkeletonIndex {
        file_count: skeletons.len(),
        merge_candidates,
        global_augmentation_targets,
        module_augmentation_targets,
        declared_modules,
        shorthand_ambient_modules,
        module_export_specifiers,
        expando_properties,
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
        for (key, props) in &self.expando_properties {
            size += key.capacity();
            size += std::mem::size_of::<(String, FxHashSet<String>)>();
            for p in props {
                size += p.capacity();
                size += std::mem::size_of::<String>();
            }
        }

        size
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
            for h in &sym.heritage_names {
                size += h.capacity();
            }
        }
        for aug in &self.global_augmentations {
            size += std::mem::size_of::<SkeletonAugmentation>();
            size += aug.target.capacity();
        }
        for aug in &self.module_augmentations {
            size += std::mem::size_of::<SkeletonAugmentation>();
            size += aug.target.capacity();
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
            reexports: vec![],
            wildcard_reexports: vec![],
            expando_properties: vec![],
            declared_modules: vec![],
            shorthand_ambient_modules: vec![],
            module_export_specifiers: vec![],
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
    fn heritage_names_affect_skeleton_fingerprint() {
        // Two skeletons identical except for heritage_names on a symbol
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
            heritage_names: vec![],
        };
        let sym_with_heritage = SkeletonSymbol {
            heritage_names: vec!["Bar".to_string()],
            ..sym_no_heritage.clone()
        };

        let mut skel1 = FileSkeleton {
            file_name: "test.ts".to_string(),
            is_external_module: true,
            symbols: vec![sym_no_heritage],
            global_augmentations: vec![],
            module_augmentations: vec![],
            reexports: vec![],
            wildcard_reexports: vec![],
            expando_properties: vec![],
            declared_modules: vec![],
            shorthand_ambient_modules: vec![],
            module_export_specifiers: vec![],
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
            reexports: vec![],
            wildcard_reexports: vec![],
            expando_properties: vec![],
            declared_modules: vec![],
            shorthand_ambient_modules: vec![],
            module_export_specifiers: vec![],
            import_sources: vec![],
            file_features: Default::default(),
            fingerprint: 0,
        };
        skel2.fingerprint = skel2.compute_fingerprint();

        assert_ne!(
            skel1.fingerprint, skel2.fingerprint,
            "Heritage names should change the skeleton fingerprint"
        );
    }

    #[test]
    fn heritage_names_included_in_skeleton_symbol_hash() {
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
            heritage_names: vec![],
        };
        let sym2 = SkeletonSymbol {
            heritage_names: vec!["Base".to_string(), "Iface".to_string()],
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
            "Different heritage_names should produce different hashes"
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
            heritage_names: vec!["OldBase".to_string()],
        };
        let sym2 = SkeletonSymbol {
            heritage_names: vec!["NewBase".to_string()],
            ..sym1.clone()
        };

        let mut prev = FileSkeleton {
            file_name: "a.ts".to_string(),
            is_external_module: true,
            symbols: vec![sym1],
            global_augmentations: vec![],
            module_augmentations: vec![],
            reexports: vec![],
            wildcard_reexports: vec![],
            expando_properties: vec![],
            declared_modules: vec![],
            shorthand_ambient_modules: vec![],
            module_export_specifiers: vec![],
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
            reexports: vec![],
            wildcard_reexports: vec![],
            expando_properties: vec![],
            declared_modules: vec![],
            shorthand_ambient_modules: vec![],
            module_export_specifiers: vec![],
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
}
