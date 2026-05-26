fn remap_symbol_table_required(
    table: &SymbolTable,
    id_remap: &FxHashMap<SymbolId, SymbolId>,
) -> SymbolTable {
    let mut remapped = SymbolTable::with_capacity(table.len());
    for (name, old_id) in table.iter() {
        if let Some(&new_id) = id_remap.get(old_id) {
            remapped.set(name.clone(), new_id);
        }
    }
    remapped
}

/// Merges `src` into `dst` using first-wins semantics: only names absent from `dst` are added.
fn merge_symbol_table_first_wins(dst: &mut SymbolTable, src: &SymbolTable) {
    for (name, sym_id) in src.iter() {
        if !dst.has(name) {
            dst.set(name.clone(), *sym_id);
        }
    }
}

/// Merges `src` into `dst` (first-wins), pushing `(existing_id, src_id)` pairs into
/// `conflicts` for any name already present in `dst` with a different symbol ID.
/// Used by the nested-merge work-queue to recursively merge colliding namespaces.
fn merge_symbol_table_collecting_conflicts(
    dst: &mut SymbolTable,
    src: &SymbolTable,
    conflicts: &mut Vec<(SymbolId, SymbolId)>,
) {
    for (name, &src_id) in src.iter() {
        if !dst.has(name) {
            dst.set(name.clone(), src_id);
        } else {
            let existing_id = dst.get(name).expect("name presence checked above");
            if existing_id != src_id {
                conflicts.push((existing_id, src_id));
            }
        }
    }
}

fn remap_symbol_table_option(
    table: &SymbolTable,
    id_remap: &FxHashMap<SymbolId, SymbolId>,
) -> Option<SymbolTable> {
    let remapped = remap_symbol_table_required(table, id_remap);
    if remapped.is_empty() {
        None
    } else {
        Some(remapped)
    }
}

fn remap_semantic_def_entry(
    entry: &crate::binder::SemanticDefEntry,
    id_remap: &FxHashMap<SymbolId, SymbolId>,
) -> crate::binder::SemanticDefEntry {
    let mut remapped = entry.clone();
    remapped.parent_namespace = entry
        .parent_namespace
        .and_then(|old_parent| id_remap.get(&old_parent).copied());
    remapped
}

// =============================================================================
// File Skeleton IR
// =============================================================================

// Skeleton types are in the skeleton submodule
use super::skeleton::*;
// Dependency graph built from skeleton import_sources
use super::dep_graph::DepGraph;

/// A bound file ready for type checking
pub struct BoundFile {
    /// File name
    pub file_name: String,
    /// The parsed source file node index
    pub source_file: NodeIndex,
    /// The arena containing all nodes (owned by this file)
    pub arena: Arc<NodeArena>,
    /// Node-to-symbol mapping (symbol IDs are global after merge).
    ///
    /// Shared via `Arc` so cross-file lookup binders (one per file in the
    /// parallel CLI pipeline) can take an O(1) reference to this file's
    /// per-file map instead of deep-cloning the underlying `FxHashMap`. On
    /// large repos (6086 files), the deep clone of `node_symbols` was one
    /// of the largest per-binder allocations. PR #1202 applied the same
    /// template to `semantic_defs`; this extends it to `node_symbols`.
    pub node_symbols: Arc<FxHashMap<u32, SymbolId>>,
    /// Per-file symbol-to-arena mapping captured during binding.
    ///
    /// `Arc`-wrapped so per-file binders constructed by the CLI driver and
    /// the parallel checker can share via `Arc::clone` (atomic increment)
    /// instead of deep-cloning the underlying `FxHashMap`. Same template as
    /// the recently-merged `BoundFile` field `Arc`-wraps (#1399 / #1404 / #1409
    /// / #1416 / #1428 / #1535 / #1559).
    pub symbol_arenas: Arc<FxHashMap<SymbolId, Arc<NodeArena>>>,
    /// Per-file declaration-to-arena mapping captured during binding.
    ///
    /// `Arc`-wrapped to mirror `BinderState.declaration_arenas` (same field)
    /// so per-file binders share via `Arc::clone` instead of deep-cloning
    /// the underlying map.
    pub declaration_arenas: Arc<DeclarationArenaMap>,
    /// Secondary index over this file's declaration arena subset.
    ///
    /// Built once during merge/remap so per-file binder reconstruction can
    /// share it via `Arc::clone` instead of scanning `declaration_arenas` and
    /// allocating a fresh derived map for every reconstructed binder.
    pub sym_to_decl_indices: Arc<SymToDeclIndicesMap>,
    /// Export visibility of namespace/module declaration nodes after binder rules.
    pub module_declaration_exports_publicly: Arc<FxHashMap<u32, bool>>,
    /// Persistent scopes (symbol IDs are global after merge).
    ///
    /// `Arc`-wrapped to mirror `BinderState.scopes` so per-file binders
    /// constructed in the cross-file lookup pipeline share via
    /// `Arc::clone` instead of deep-cloning. Same pattern as the recently-
    /// merged `BoundFile` field `Arc`-wraps (#1399 / #1404 / #1409 / #1416 /
    /// #1428 / #1535).
    pub scopes: Arc<Vec<Scope>>,
    /// Map from AST node to scope ID.
    ///
    /// `Arc`-wrapped to mirror `BinderState.node_scope_ids` so per-file
    /// binders share via `Arc::clone` instead of deep-cloning. Read-only
    /// after binding completes.
    pub node_scope_ids: Arc<FxHashMap<u32, ScopeId>>,
    /// Parse diagnostics
    pub parse_diagnostics: Vec<ParseDiagnostic>,
    /// Global augmentations (interface declarations inside `declare global` blocks).
    ///
    /// `Arc`-wrapped to mirror `BinderState.global_augmentations` so per-file
    /// binders share via `Arc::clone` (atomic increment) instead of
    /// deep-cloning the underlying `FxHashMap` per consumer.
    pub global_augmentations: Arc<FxHashMap<String, Vec<crate::binder::GlobalAugmentation>>>,
    /// Module augmentations (interface/type declarations inside `declare module 'x'` blocks).
    ///
    /// `Arc`-wrapped to mirror `BinderState.module_augmentations` so per-file
    /// binders share via `Arc::clone` (atomic increment) instead of
    /// deep-cloning the underlying `FxHashMap` per consumer.
    pub module_augmentations: Arc<FxHashMap<String, Vec<crate::binder::ModuleAugmentation>>>,
    /// Maps symbols declared inside module augmentation blocks to their target module specifier.
    ///
    /// `Arc`-wrapped to mirror `BinderState.augmentation_target_modules` so
    /// per-file binders share via `Arc::clone` (atomic increment) instead of
    /// deep-cloning the underlying `FxHashMap` per consumer.
    pub augmentation_target_modules: Arc<FxHashMap<SymbolId, String>>,
    /// Flow nodes for control flow analysis.
    ///
    /// `Arc`-wrapped so per-file binders constructed by the CLI driver can
    /// share this file's flow graph via `Arc::clone` (atomic increment)
    /// instead of deep-cloning the underlying `Vec<FlowNode>` (each
    /// `FlowNode` owns a `Vec<FlowNodeId>` antecedents). The driver builds
    /// ~2×N per-file binders (cross-file lookup + per-file checking), so
    /// on N-file projects this previously cost 2N deep clones of the
    /// per-file flow graph.
    pub flow_nodes: Arc<FlowNodeArena>,
    /// Node-to-flow mapping: tracks which flow node was active at each AST node.
    ///
    /// Shared via `Arc` so cross-file lookup binders (one per file in the
    /// parallel CLI pipeline) can take an O(1) reference to this file's
    /// per-file map instead of deep-cloning the underlying `FxHashMap`. On
    /// large repos (6086 files), the deep clone of `node_flow` was one of
    /// the largest per-binder allocations after the `semantic_defs` (#1202)
    /// and `node_symbols` (#1227) Arc migrations.
    pub node_flow: Arc<FxHashMap<u32, FlowNodeId>>,
    /// Map from switch clause `NodeIndex` to parent switch statement `NodeIndex`
    /// Used by control flow analysis for switch exhaustiveness checking.
    ///
    /// `Arc`-wrapped so per-file binders share via `Arc::clone` instead of
    /// deep-cloning. Read-only after binding completes.
    pub switch_clause_to_switch: Arc<FxHashMap<u32, NodeIndex>>,
    /// Whether this file is an external module (has imports/exports)
    pub is_external_module: bool,
    /// Expando property assignments detected during binding.
    ///
    /// `Arc`-wrapped so per-file binders constructed by the CLI driver
    /// (cross-file lookup + primary checker, ~2N for N files) share via
    /// `Arc::clone` instead of deep-cloning the nested map. Read-only
    /// after `bind_source_file` completes.
    pub expando_properties: Arc<FxHashMap<String, FxHashSet<String>>>,
    pub file_features: crate::binder::FileFeatures,
    /// Reverse mapping for merged lib symbols: remapped `SymbolId` ->
    /// (`lib_binder_idx`, original lib-local `SymbolId`).
    /// Reconstructed binders need this to keep lib delegation caches from
    /// polluting file-local symbol state.
    ///
    /// `Arc`-wrapped so per-file binders constructed by the CLI driver
    /// (one cross-file lookup binder + one primary checker binder per
    /// file) can share via `Arc::clone` (atomic increment) instead of
    /// deep-cloning the underlying `FxHashMap`. Read-only after
    /// `merge_lib_contexts_into_binder` completes; the merge path uses
    /// `Arc::make_mut`, which is free when refcount=1.
    pub lib_symbol_reverse_remap: Arc<FxHashMap<SymbolId, (usize, SymbolId)>>,
    /// Per-file semantic definitions for top-level declarations (Phase 1 DefId-first).
    /// Contains only entries that originated in this file (post-remap `SymbolIds`).
    /// This enables file-scoped identity without cloning the entire global map.
    ///
    /// Shared via `Arc` so cross-file lookup binders can take an O(1) reference
    /// instead of deep-cloning the underlying map per file. See
    /// `tsz_cli::driver::check_utils::create_cross_file_lookup_binder_with_augmentations`.
    pub semantic_defs: Arc<FxHashMap<SymbolId, crate::binder::SemanticDefEntry>>,
}

impl BoundFile {
    /// Estimate the heap memory footprint of this bound file in bytes.
    ///
    /// Accounts for the struct itself plus all heap-allocated strings, vecs,
    /// hash map entries, and flow arena contents. The `NodeArena` behind
    /// `Arc` counts only the `Arc` overhead (shared data is tracked
    /// separately via unique-arena deduplication in `MergedProgramResidencyStats`).
    #[must_use]
    pub fn estimated_size_bytes(&self) -> usize {
        let mut size = std::mem::size_of::<Self>();

        // file_name
        size += self.file_name.capacity();

        // arena (Arc overhead only — shared data not double-counted)
        size += std::mem::size_of::<NodeArena>();

        // node_symbols
        size += self.node_symbols.capacity()
            * (std::mem::size_of::<u32>() + std::mem::size_of::<SymbolId>() + 8);

        // symbol_arenas
        size += self.symbol_arenas.capacity()
            * (std::mem::size_of::<SymbolId>() + std::mem::size_of::<Arc<NodeArena>>() + 8);

        // declaration_arenas
        size += self.declaration_arenas.capacity()
            * (std::mem::size_of::<(SymbolId, NodeIndex)>()
                + std::mem::size_of::<Vec<Arc<NodeArena>>>()
                + 8);

        // sym_to_decl_indices
        size += self.sym_to_decl_indices.capacity()
            * (std::mem::size_of::<SymbolId>() + std::mem::size_of::<usize>() * 4 + 8);
        for decl_indices in self.sym_to_decl_indices.values() {
            size += decl_indices.capacity().saturating_sub(4) * std::mem::size_of::<NodeIndex>();
        }

        // module_declaration_exports_publicly
        size += self.module_declaration_exports_publicly.capacity()
            * (std::mem::size_of::<u32>() + 1 + 8);

        // scopes
        size += self.scopes.capacity() * std::mem::size_of::<Scope>();
        for scope in self.scopes.iter() {
            size += scope.table.len() * (32 + std::mem::size_of::<SymbolId>());
        }

        // node_scope_ids
        size += self.node_scope_ids.capacity()
            * (std::mem::size_of::<u32>() + std::mem::size_of::<ScopeId>() + 8);

        // parse_diagnostics
        size += self.parse_diagnostics.capacity() * std::mem::size_of::<ParseDiagnostic>();
        for diag in &self.parse_diagnostics {
            size += diag.message.capacity();
        }

        // global_augmentations
        for (k, v) in self.global_augmentations.iter() {
            size += k.capacity() + std::mem::size_of::<u64>();
            size += v.capacity() * std::mem::size_of::<crate::binder::GlobalAugmentation>();
        }

        // module_augmentations
        for (k, v) in self.module_augmentations.iter() {
            size += k.capacity() + std::mem::size_of::<u64>();
            size += v.capacity() * std::mem::size_of::<crate::binder::ModuleAugmentation>();
            for aug in v {
                size += aug.name.capacity();
            }
        }

        // augmentation_target_modules
        for v in self.augmentation_target_modules.values() {
            size += std::mem::size_of::<SymbolId>() + v.capacity() + 8;
        }

        // flow_nodes
        size += self.flow_nodes.len() * std::mem::size_of::<crate::binder::FlowNode>();
        for flow_node in self.flow_nodes.iter() {
            size += flow_node.antecedent.capacity() * std::mem::size_of::<FlowNodeId>();
        }

        // node_flow
        size += self.node_flow.capacity()
            * (std::mem::size_of::<u32>() + std::mem::size_of::<FlowNodeId>() + 8);

        // switch_clause_to_switch
        size += self.switch_clause_to_switch.capacity()
            * (std::mem::size_of::<u32>() + std::mem::size_of::<NodeIndex>() + 8);

        // expando_properties
        for (k, v) in self.expando_properties.iter() {
            size += k.capacity() + std::mem::size_of::<u64>();
            for s in v {
                size += s.capacity() + std::mem::size_of::<u64>();
            }
        }

        // lib_symbol_reverse_remap
        size += self.lib_symbol_reverse_remap.capacity()
            * (std::mem::size_of::<SymbolId>() + std::mem::size_of::<(usize, SymbolId)>() + 8);

        // semantic_defs (per-file)
        size += self.semantic_defs.capacity()
            * (std::mem::size_of::<SymbolId>()
                + std::mem::size_of::<crate::binder::SemanticDefEntry>()
                + 8);
        for entry in self.semantic_defs.values() {
            size += entry.name.capacity();
            size += entry.enum_member_names.capacity() * 24; // String overhead
            for m in &entry.enum_member_names {
                size += m.capacity();
            }
            size += entry.extends_names.capacity() * 24;
            for h in &entry.extends_names {
                size += h.capacity();
            }
            size += entry.implements_names.capacity() * 24;
            for h in &entry.implements_names {
                size += h.capacity();
            }
        }

        size
    }
}

use crate::tsz_solver::construction::TypeInterner;
use crate::tsz_solver::def::DefinitionStore;

/// Merged program state after parallel binding
pub struct MergedProgram {
    /// All bound files
    pub files: Vec<BoundFile>,
    /// Global symbol arena (all symbols from all files, with remapped IDs)
    pub symbols: SymbolArena,
    /// Symbol-to-arena mapping for declaration lookup (legacy, stores last arena).
    ///
    /// Wrapped in `Arc` so per-file checker binders can share the merged map
    /// via `Arc::clone` (O(1)) instead of building a per-file derived map.
    pub symbol_arenas: Arc<FxHashMap<SymbolId, Arc<NodeArena>>>,
    /// Declaration-to-arena mapping for precise cross-file declaration lookup
    /// Key: (`SymbolId`, `NodeIndex` of declaration) -> Arena(s) containing that declaration.
    ///
    /// `Arc`-wrapped so per-file `BinderState.declaration_arenas` reconstruction
    /// is a cheap atomic increment instead of iterating the entire program-wide
    /// map per file. On large projects this map holds ~100K entries and the
    /// CLI driver builds ~12K per-file binders; the previous per-file materialization
    /// iterated ~100K entries × ~12K binders ≈ 1.2B entry visits at startup.
    pub declaration_arenas: Arc<DeclarationArenaMap>,
    /// Secondary index: `SymbolId` → every `NodeIndex` that appears as a
    /// declaration key for that symbol. Built once at merge time so checker
    /// paths that need to enumerate a symbol's declarations can do a point
    /// lookup instead of iterating the program-wide `declaration_arenas`.
    pub sym_to_decl_indices: Arc<SymToDeclIndicesMap>,
    /// Cross-file `node_symbols`: maps arena pointer → `node_symbols` for that arena.
    /// Enables resolving type references in cross-file interface declarations.
    ///
    /// Arc-wrapped so large-repo drivers can install the merged program-wide
    /// map into shared checker context with an O(1) clone instead of
    /// deep-cloning the outer map before re-sharing it.
    pub cross_file_node_symbols: Arc<CrossFileNodeSymbols>,
    /// Global symbol table (exports from all files)
    pub globals: SymbolTable,
    /// Per-file symbol tables (file-local symbols, symbol IDs remapped)
    pub file_locals: Vec<SymbolTable>,
    /// Ambient module declarations across all files
    pub declared_modules: Arc<FxHashSet<String>>,
    /// Shorthand ambient modules (`declare module "foo"` without body) - imports from these are `any`
    pub shorthand_ambient_modules: Arc<FxHashSet<String>>,
    /// Module exports: maps file name (or module specifier) to its exported symbols
    /// This enables cross-file module resolution: import { X } from './file' can find X's symbol
    /// `Arc`-wrapped so per-file `BinderState` reconstruction is a cheap atomic
    /// increment instead of a deep clone of the merged map.
    pub module_exports: Arc<FxHashMap<String, SymbolTable>>,
    /// Re-exports: tracks `export { x } from 'module'` declarations
    /// Maps (`current_file`, `exported_name`) -> (`source_module`, `original_name`)
    pub reexports: Arc<Reexports>,
    /// Wildcard re-exports: tracks `export * from 'module'` declarations
    /// Maps `current_file` -> Vec of `source_modules`
    /// `Arc`-wrapped so per-file `BinderState` reconstruction is a
    /// cheap atomic increment instead of a deep clone of the merged
    /// `FxHashMap`. Mutations during binding go through `Arc::make_mut`.
    pub wildcard_reexports: Arc<WildcardReexportsMap>,
    /// Wildcard re-export type-only provenance per entry.
    pub wildcard_reexports_type_only: Arc<WildcardReexportsTypeOnlyMap>,
    /// Lib binders for global type resolution (Array, String, Promise, etc.)
    /// These contain symbols from lib.d.ts files and enable resolution of built-in types
    pub lib_binders: Arc<Vec<Arc<BinderState>>>,
    /// Global symbol IDs that originated from lib files (remapped to global arena IDs).
    /// `Arc`-wrapped so the CLI driver can install the same set into
    /// every per-file `BinderState.lib_symbol_ids` via `Arc::clone`
    /// (cheap atomic increment) instead of deep-cloning the
    /// `FxHashSet` for each of N per-file binders.
    pub lib_symbol_ids: Arc<FxHashSet<SymbolId>>,
    /// Global type interner - shared across all threads for type deduplication
    pub type_interner: TypeInterner,
    /// Alias partners: maps `TYPE_ALIAS` `SymbolId` → `ALIAS` `SymbolId` for merged type+namespace exports.
    /// When `export type X = ...` and `export * as X from "..."` coexist, the exports table
    /// holds the `TYPE_ALIAS` symbol and this map links it to the ALIAS symbol for value resolution.
    pub alias_partners: Arc<FxHashMap<SymbolId, SymbolId>>,
    /// Binder-captured semantic definitions for top-level declarations (Phase 1 DefId-first).
    /// Maps post-remap `SymbolId` → `SemanticDefEntry` across all files.
    /// The checker reads this during construction to pre-create solver `DefIds`.
    ///
    /// `Arc`-wrapped so the parallel checker's lib-check pass and the
    /// per-file binder reconstruction paths can share via `Arc::clone`
    /// (atomic increment) instead of deep-cloning the underlying
    /// `FxHashMap`. The lib-check overlays an always-empty per-lib map
    /// on top of this (`build_lib_bound_file_for_interface_checks`
    /// returns an empty `semantic_defs`), so for the lib path the
    /// `Arc::clone` is the entire cost.
    pub semantic_defs: Arc<FxHashMap<SymbolId, crate::binder::SemanticDefEntry>>,
    /// Shared `DefinitionStore` pre-populated with `DefId`s for all top-level
    /// semantic definitions during the merge phase. This moves identity creation
    /// from checker pre-population (per-file, order-dependent) to merge time
    /// (single pass, deterministic). Checker contexts receive this via
    /// `with_options_and_shared_def_store` and only need to warm local caches.
    pub definition_store: std::sync::Arc<DefinitionStore>,
    /// Skeleton index computed alongside the legacy merge path.
    ///
    /// This captures the same merge-relevant topology (symbol merging, augmentation
    /// targets, re-export graph) without retaining any arena or binder state.
    /// It is computed from pre-merge `BindResult`s during `merge_bind_results_ref`
    /// and stored here so downstream consumers can begin migrating off arena-backed
    /// lookups toward skeleton-based queries.
    pub skeleton_index: Option<SkeletonIndex>,
    /// Dependency graph derived from skeleton `import_sources`.
    ///
    /// Built using `DepGraph::build_simple` during merge (name-matching heuristic).
    /// Provides topological ordering for incremental invalidation and ordered
    /// checking. `None` only if no skeletons were extracted (should not happen
    /// in the normal pipeline).
    pub dep_graph: Option<DepGraph>,
    /// Sum of `BindResult::estimated_size_bytes()` across all input files, computed
    /// before the merge consumes per-file data. This captures the pre-merge memory
    /// footprint so it can be compared to the post-merge `MergedProgram` residency.
    pub pre_merge_bind_total_bytes: usize,
}

impl MergedProgram {
    /// Return the topological file ordering from the dependency graph.
    ///
    /// Dependencies come before dependents. Files in cycles are appended
    /// in stable (input) order. Returns `None` if no dep graph was computed.
    #[must_use]
    pub fn topological_file_order(&self) -> Option<super::dep_graph::TopoResult> {
        self.dep_graph.as_ref().map(|dg| dg.topological_order())
    }

    /// Return the set of file indices that directly depend on the given file.
    ///
    /// These are files that `import` from the target file. Useful for
    /// incremental invalidation: when `file_idx` changes, its dependents
    /// may need re-checking.
    #[must_use]
    pub fn dependents_of(&self, file_idx: usize) -> Option<&rustc_hash::FxHashSet<usize>> {
        self.dep_graph.as_ref().map(|dg| dg.dependents(file_idx))
    }

    /// Return the set of file indices that the given file depends on.
    #[must_use]
    pub fn dependencies_of(&self, file_idx: usize) -> Option<&rustc_hash::FxHashSet<usize>> {
        self.dep_graph.as_ref().map(|dg| dg.dependencies(file_idx))
    }

    /// Build the merged `file_locals` for a per-file checker binder.
    ///
    /// Per-file checker binders read globals through `binder.file_locals`
    /// (e.g. when resolving `Promise`, `Iterable`, `React` and other
    /// names), so the post-merge driver paths fold `program.globals` into
    /// each per-file `file_locals`. This is hot — called once per file
    /// per binder reconstruction, in parallel under rayon.
    ///
    /// Fast paths take advantage of `SymbolTable` being internally
    /// `Arc<FxHashMap<String, SymbolId>>` (since #1535):
    ///
    /// - When the per-file local set is empty (common for trivial
    ///   declaration files / pure re-export modules), the merged result
    ///   is just `globals` — `Arc::clone` is O(1).
    /// - When there are no globals (LSP probes / minimal harness setups),
    ///   the merged result is just the per-file locals — again O(1).
    /// - Otherwise, allocate a fresh map at the upper-bound capacity and
    ///   insert both sides. Per-file locals win on key collisions, which
    ///   matches the previous in-place merge behavior.
    #[must_use]
    pub fn build_merged_file_locals(&self, file_idx: usize) -> SymbolTable {
        let local_table = self.file_locals.get(file_idx);
        let local_count = local_table.map(SymbolTable::len).unwrap_or(0);
        let globals_count = self.globals.len();

        if local_count == 0 {
            return self.globals.clone();
        }
        if globals_count == 0 {
            return local_table.cloned().unwrap_or_default();
        }

        let mut file_locals = SymbolTable::with_capacity(local_count + globals_count);
        if let Some(table) = local_table {
            for (name, &sym_id) in table.iter() {
                file_locals.set(name.clone(), sym_id);
            }
        }
        merge_symbol_table_first_wins(&mut file_locals, &self.globals);
        file_locals
    }

    /// Build the `lib_type_namespace` map for a reconstructed binder.
    ///
    /// Scans only the per-file locals for `file_idx` (not merged globals) for
    /// VALUE-only user symbols whose names also appear as TYPE symbols in
    /// `self.globals`. This lets the checker's symbol resolver fall back to
    /// the lib TYPE symbol when a local VALUE-only symbol would otherwise block it.
    #[must_use]
    pub fn build_lib_type_namespace(&self, file_idx: usize) -> FxHashMap<String, SymbolId> {
        use crate::binder::symbol_flags;
        let Some(file_locals) = self.file_locals.get(file_idx) else {
            return FxHashMap::default();
        };
        let mut result = FxHashMap::default();
        for (name, &sym_id) in file_locals.iter() {
            let sym_flags = self.symbols.get(sym_id).map_or(0, |s| s.flags);
            if (sym_flags & symbol_flags::VALUE) == 0 || (sym_flags & symbol_flags::TYPE) != 0 {
                continue;
            }
            if let Some(global_id) = self.globals.get(name) {
                let global_flags = self.symbols.get(global_id).map_or(0, |s| s.flags);
                if (global_flags & symbol_flags::TYPE) != 0 {
                    result.insert(name.clone(), global_id);
                }
            }
        }
        result
    }
}

/// Check if two symbols can be merged across multiple files.
///
/// TypeScript allows merging:
/// - Interface + Interface (declaration merging)
/// - Namespace + Namespace (declaration merging)
/// - Class + Interface (merging for class declarations)
/// - Function + Function (overloads - handled per-file)
pub(super) const fn can_merge_symbols_cross_file(existing_flags: u32, new_flags: u32) -> bool {
    use crate::binder::symbol_flags;

    // Interface can merge with interface
    if (existing_flags & symbol_flags::INTERFACE) != 0 && (new_flags & symbol_flags::INTERFACE) != 0
    {
        return true;
    }

    // Class can merge with interface
    if ((existing_flags & symbol_flags::CLASS) != 0 && (new_flags & symbol_flags::INTERFACE) != 0)
        || ((existing_flags & symbol_flags::INTERFACE) != 0
            && (new_flags & symbol_flags::CLASS) != 0)
    {
        return true;
    }

    // Interface can merge with variable (e.g., `interface Promise<T>` + `declare var Promise: PromiseConstructor`)
    // This is fundamental to how TypeScript lib declarations work: types have both an interface
    // (type side) and a variable declaration (value side).
    if ((existing_flags & symbol_flags::INTERFACE) != 0
        && (new_flags & symbol_flags::VARIABLE) != 0)
        || ((existing_flags & symbol_flags::VARIABLE) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0)
    {
        return true;
    }

    // Interface can merge with function (e.g., `interface Array<T>` + `declare function Array(...)`)
    if ((existing_flags & symbol_flags::INTERFACE) != 0
        && (new_flags & symbol_flags::FUNCTION) != 0)
        || ((existing_flags & symbol_flags::FUNCTION) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0)
    {
        return true;
    }

    // Namespace/module can merge with namespace/module
    if (existing_flags & symbol_flags::MODULE) != 0 && (new_flags & symbol_flags::MODULE) != 0 {
        return true;
    }

    // Variable can merge with variable cross-file (so we can detect and report cross-file redeclarations of let/const)
    if (existing_flags & symbol_flags::VARIABLE) != 0 && (new_flags & symbol_flags::VARIABLE) != 0 {
        return true;
    }

    // Class can merge with Class cross-file (invalid, but merged to report duplicate)
    if (existing_flags & symbol_flags::CLASS) != 0 && (new_flags & symbol_flags::CLASS) != 0 {
        return true;
    }

    // Class can merge with Type Alias (invalid, but merged to report duplicate)
    if ((existing_flags & symbol_flags::CLASS) != 0 && (new_flags & symbol_flags::TYPE_ALIAS) != 0)
        || ((existing_flags & symbol_flags::TYPE_ALIAS) != 0
            && (new_flags & symbol_flags::CLASS) != 0)
    {
        return true;
    }

    // Type Alias can merge with Type Alias (invalid, but merged to report duplicate)
    if (existing_flags & symbol_flags::TYPE_ALIAS) != 0
        && (new_flags & symbol_flags::TYPE_ALIAS) != 0
    {
        return true;
    }

    // Type Alias can merge with Interface (invalid, but merged to report duplicate)
    if ((existing_flags & symbol_flags::TYPE_ALIAS) != 0
        && (new_flags & symbol_flags::INTERFACE) != 0)
        || ((existing_flags & symbol_flags::INTERFACE) != 0
            && (new_flags & symbol_flags::TYPE_ALIAS) != 0)
    {
        return true;
    }

    // Class can merge with Variable (invalid, but merged to report duplicate)
    if ((existing_flags & symbol_flags::CLASS) != 0 && (new_flags & symbol_flags::VARIABLE) != 0)
        || ((existing_flags & symbol_flags::VARIABLE) != 0
            && (new_flags & symbol_flags::CLASS) != 0)
    {
        return true;
    }

    // Type Alias can merge with Variable (invalid, but merged to report duplicate)
    if ((existing_flags & symbol_flags::TYPE_ALIAS) != 0
        && (new_flags & symbol_flags::VARIABLE) != 0)
        || ((existing_flags & symbol_flags::VARIABLE) != 0
            && (new_flags & symbol_flags::TYPE_ALIAS) != 0)
    {
        return true;
    }

    // Namespace can merge with class, function, enum, or variable
    if (existing_flags & symbol_flags::MODULE) != 0
        && (new_flags
            & (symbol_flags::CLASS
                | symbol_flags::FUNCTION
                | symbol_flags::ENUM
                | symbol_flags::VARIABLE))
            != 0
    {
        return true;
    }
    if (new_flags & symbol_flags::MODULE) != 0
        && (existing_flags
            & (symbol_flags::CLASS
                | symbol_flags::FUNCTION
                | symbol_flags::ENUM
                | symbol_flags::VARIABLE))
            != 0
    {
        return true;
    }

    // Enum can merge with enum
    if (existing_flags & symbol_flags::ENUM) != 0 && (new_flags & symbol_flags::ENUM) != 0 {
        return true;
    }

    false
}

/// Append declarations from `incoming` into `existing` without duplicates.
///
/// Small declaration lists are common, so use linear scans there to avoid
/// hash set allocation overhead. Switch to a set only for larger collections.
fn append_unique_declarations(existing: &mut Vec<NodeIndex>, incoming: &[NodeIndex]) {
    for &decl in incoming {
        if !existing.contains(&decl) {
            existing.push(decl);
        }
    }
}

/// Merges `lib_sym` declarations (and conditionally its flags) into an already-allocated global
/// symbol. When `global_aug_nodes` is `Some`, only declarations originating from a
/// `declare global { ... }` augmentation block are merged, and the symbol flags are not
/// propagated (external-module lib binders must not contaminate globals with CLASS/etc. flags).
fn apply_lib_declarations_to_existing(
    existing: &mut crate::binder::Symbol,
    lib_sym: &crate::binder::Symbol,
    global_aug_nodes: &Option<FxHashSet<NodeIndex>>,
) {
    if let Some(aug_nodes) = global_aug_nodes {
        // Only fold in declarations from `declare global` blocks; iterate directly to
        // avoid a temporary Vec allocation on this hot path.
        for &decl in &lib_sym.declarations {
            if aug_nodes.contains(&decl) && !existing.declarations.contains(&decl) {
                existing.declarations.push(decl);
            }
        }
        // Do NOT merge flags — module-scoped CLASS/etc. must not contaminate global types.
    } else {
        existing.flags |= lib_sym.flags;
        append_unique_declarations(&mut existing.declarations, &lib_sym.declarations);
    }
}

/// Remap `__unique_{SymbolId}` keys in `expando_properties` to use global `SymbolIds`.
///
/// During binding, expando property tracking stores unique symbol keys as
/// `__unique_{local_SymbolId}`. After `merge_bind_results` remaps all `SymbolIds`
/// to a global arena, these encoded IDs become stale. This function updates
/// them so the checker's `UniqueSymbol` types (which use global IDs) match.
fn remap_expando_properties(
    expando: &FxHashMap<String, FxHashSet<String>>,
    id_remap: &FxHashMap<SymbolId, SymbolId>,
) -> Arc<FxHashMap<String, FxHashSet<String>>> {
    Arc::new(
        expando
            .iter()
            .map(|(obj_name, props)| {
                let remapped_props = props
                    .iter()
                    .map(|prop| {
                        if let Some(old_id_str) = prop.strip_prefix("__unique_")
                            && let Ok(old_id) = old_id_str.parse::<u32>()
                            && let Some(&new_id) = id_remap.get(&SymbolId(old_id))
                        {
                            return format!("__unique_{}", new_id.0);
                        }
                        prop.clone()
                    })
                    .collect();
                (obj_name.clone(), remapped_props)
            })
            .collect(),
    )
}

fn patch_script_lib_interface_declaration_arenas_for_result(
    result: &BindResult,
    remapped_locals: &SymbolTable,
    globals: &SymbolTable,
    global_lib_symbol_ids: &FxHashSet<SymbolId>,
    declaration_arenas: &mut DeclarationArenaMap,
) {
    if result.is_external_module {
        return;
    }

    let Some(source_file) = result.arena.get_source_file_at(result.source_file) else {
        return;
    };

    for &stmt_idx in &source_file.statements.nodes {
        let Some(stmt_node) = result.arena.get(stmt_idx) else {
            continue;
        };
        if stmt_node.kind != crate::parser::syntax_kind_ext::INTERFACE_DECLARATION {
            continue;
        }
        let Some(iface) = result.arena.get_interface(stmt_node) else {
            continue;
        };
        let Some(name_node) = result.arena.get(iface.name) else {
            continue;
        };
        let Some(ident) = result.arena.get_identifier(name_node) else {
            continue;
        };

        let name = ident.escaped_text.as_str();
        let sym_id = remapped_locals.get(name).or_else(|| globals.get(name));
        let Some(sym_id) = sym_id else {
            continue;
        };
        if !global_lib_symbol_ids.contains(&sym_id) {
            continue;
        }
        let target = declaration_arenas.entry((sym_id, stmt_idx)).or_default();
        if !target.iter().any(|arena| Arc::ptr_eq(arena, &result.arena)) {
            target.push(Arc::clone(&result.arena));
        }
    }
}

fn release_consumed_bind_result(result: &mut BindResult) {
    result.file_name.clear();
    result.source_file = NodeIndex::NONE;
    result.arena = Arc::new(NodeArena::new());
    result.symbols = SymbolArena::new();
    result.file_locals = SymbolTable::default();
    result.declared_modules = Arc::new(FxHashSet::default());
    result.module_exports = Arc::new(FxHashMap::default());
    result.node_symbols = Arc::new(FxHashMap::default());
    result.module_declaration_exports_publicly = Arc::new(FxHashMap::default());
    result.symbol_arenas = Arc::new(FxHashMap::default());
    result.declaration_arenas = Arc::new(DeclarationArenaMap::default());
    result.scopes = Arc::new(Vec::new());
    result.node_scope_ids = Arc::new(FxHashMap::default());
    result.parse_diagnostics.clear();
    result.shorthand_ambient_modules = Arc::new(FxHashSet::default());
    result.global_augmentations = Arc::new(FxHashMap::default());
    result.module_augmentations = Arc::new(FxHashMap::default());
    result.augmentation_target_modules = Arc::new(FxHashMap::default());
    result.reexports = Arc::new(Reexports::default());
    result.wildcard_reexports = Arc::new(WildcardReexportsMap::default());
    result.wildcard_reexports_type_only = Arc::new(WildcardReexportsTypeOnlyMap::default());
    result.lib_binders = Arc::new(Vec::new());
    result.lib_arenas.clear();
    result.lib_symbol_ids = Arc::new(FxHashSet::default());
    result.lib_symbol_reverse_remap = Arc::new(FxHashMap::default());
    result.flow_nodes = Arc::new(FlowNodeArena::new());
    result.node_flow = Arc::new(FxHashMap::default());
    result.switch_clause_to_switch = Arc::new(FxHashMap::default());
    result.expando_properties = Arc::new(FxHashMap::default());
    result.alias_partners = Arc::new(FxHashMap::default());
    result.file_features = crate::binder::FileFeatures::default();
    result.semantic_defs = Arc::new(FxHashMap::default());
    result.file_import_sources.clear();
}

/// Pre-populate a `DefinitionStore` from the merged `semantic_defs` map.
///
/// This converts each `SemanticDefEntry` into a solver `DefinitionInfo`,
/// registers it in the store, and records the `(SymbolId, file_id) → DefId`
/// mapping. The resulting store is shared across all checker contexts so
/// that `DefId` allocation happens once (at merge time) rather than
/// per-file during checker pre-population.
///
/// Delegates to `DefinitionStore::from_semantic_defs` — the canonical
/// solver-owned factory for converting binder identity to solver `DefId`s.
pub fn pre_populate_definition_store(
    semantic_defs: &FxHashMap<SymbolId, crate::binder::SemanticDefEntry>,
    interner: &TypeInterner,
) -> DefinitionStore {
    DefinitionStore::from_semantic_defs(semantic_defs, |s| interner.intern_string(s))
}

/// Resolve heritage names to `DefId`s in a pre-populated `DefinitionStore`.
///
/// For each class/interface with `extends_names` or `implements_names`, look up
/// the target by name in the store's `name_to_defs` index and wire:
/// - `extends`: first `extends_name` matching a Class or Interface (for classes,
///   this is the parent class; for interfaces, the first extended interface)
/// - `implements`: all `implements_names` matching an Interface
///
/// Only simple identifier names are resolved. Property-access names (e.g.,
/// `ns.Base`) contain dots and cannot match any DefId name, so they are
/// silently skipped (the checker resolves them during type checking).
///
/// This is called as Pass 3 of `pre_populate_definition_store` and can also
/// be called standalone for cross-batch heritage resolution.
pub fn resolve_heritage_in_store(
    semantic_defs: &FxHashMap<SymbolId, crate::binder::SemanticDefEntry>,
    store: &DefinitionStore,
    interner: &TypeInterner,
) {
    use crate::tsz_solver::def::DefKind;

    for (&sym_id, entry) in semantic_defs {
        let def_id = match store.find_def_by_symbol(sym_id.0) {
            Some(id) => id,
            None => continue,
        };

        // Resolve extends_names → DefinitionInfo.extends
        if !entry.extends_names.is_empty() {
            for name_str in &entry.extends_names {
                // Skip property-access names (contain dots) — checker resolves these
                if name_str.contains('.') {
                    continue;
                }
                let name_atom = interner.intern_string(name_str);
                if let Some(candidates) = store.find_defs_by_name(name_atom) {
                    for &candidate_id in &candidates {
                        if candidate_id == def_id {
                            continue; // skip self
                        }
                        if let Some(candidate_info) = store.get(candidate_id)
                            && matches!(candidate_info.kind, DefKind::Class | DefKind::Interface)
                        {
                            store.set_extends(def_id, candidate_id);
                            break;
                        }
                    }
                }
                // Only use the first extends name for the `extends` field
                // (classes have at most one extends target)
                break;
            }
        }

        // Resolve implements_names → DefinitionInfo.implements
        if !entry.implements_names.is_empty() {
            let mut resolved_implements = Vec::new();
            for name_str in &entry.implements_names {
                if name_str.contains('.') {
                    continue;
                }
                let name_atom = interner.intern_string(name_str);
                if let Some(candidates) = store.find_defs_by_name(name_atom) {
                    for &candidate_id in &candidates {
                        if candidate_id == def_id {
                            continue;
                        }
                        if let Some(candidate_info) = store.get(candidate_id)
                            && matches!(candidate_info.kind, DefKind::Interface | DefKind::Class)
                        {
                            resolved_implements.push(candidate_id);
                            break;
                        }
                    }
                }
            }
            if !resolved_implements.is_empty() {
                store.set_implements(def_id, resolved_implements);
            }
        }
    }
}

/// Create a `DefinitionStore` from a single binder's `semantic_defs`.
///
/// This is the single-file equivalent of `pre_populate_definition_store`.
/// It allows single-file checker contexts to receive a pre-populated store
/// rather than creating an empty one and relying on checker-side
/// `pre_populate_def_ids_from_binder()` repair.
///
/// The resulting store can be shared via `Arc` and passed to
/// `CheckerState::new_with_shared_def_store`.
pub fn create_definition_store_from_binder(
    binder: &crate::binder::BinderState,
    interner: &TypeInterner,
) -> DefinitionStore {
    pre_populate_definition_store(&binder.semantic_defs, interner)
}

/// Merge bind results into a unified program state
///
/// This is a sequential operation that combines:
/// - All symbol arenas into a single global arena
/// - Merges symbols with the same name across files (for interfaces, namespaces, etc.)
/// - Remaps symbol IDs in `node_symbols` to use global IDs
///
/// # Arguments
/// * `results` - Vector of `BindResult` from parallel binding
///
/// # Returns
/// `MergedProgram` with unified symbol space
pub fn merge_bind_results(mut results: Vec<BindResult>) -> MergedProgram {
    let mut source = OwnedBindResults {
        results: &mut results,
    };
    merge_bind_results_from_source(&mut source)
}

pub fn merge_bind_results_ref(results: &[&BindResult]) -> MergedProgram {
    let mut source = BorrowedBindResults { results };
    merge_bind_results_from_source(&mut source)
}

trait BindResultsSource {
    fn len(&self) -> usize;
    fn get(&self, index: usize) -> &BindResult;

    fn release(&mut self, _index: usize) {}

    fn refs(&self) -> Vec<&BindResult> {
        (0..self.len()).map(|index| self.get(index)).collect()
    }
}

struct OwnedBindResults<'a> {
    results: &'a mut [BindResult],
}

impl BindResultsSource for OwnedBindResults<'_> {
    fn len(&self) -> usize {
        self.results.len()
    }

    fn get(&self, index: usize) -> &BindResult {
        &self.results[index]
    }

    fn release(&mut self, index: usize) {
        release_consumed_bind_result(&mut self.results[index]);
    }
}

struct BorrowedBindResults<'a> {
    results: &'a [&'a BindResult],
}

impl BindResultsSource for BorrowedBindResults<'_> {
    fn len(&self) -> usize {
        self.results.len()
    }

    fn get(&self, index: usize) -> &BindResult {
        self.results[index]
    }
}
