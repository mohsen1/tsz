/// Accumulates bind-result merge state during the multi-phase merge of
/// per-file bind results into a single `MergedProgram`.
///
/// The top-level `merge_bind_results_from_source` function is thin orchestration;
/// all merge reconciliation policy lives in this type.
struct BindResultReducer {
    // Lib processing
    lib_binders: Vec<Arc<BinderState>>,
    lib_binder_arena_map: FxHashMap<usize, Arc<NodeArena>>,
    lib_symbol_remap: FxHashMap<(usize, SymbolId), SymbolId>,
    lib_name_to_global: FxHashMap<Atom, SymbolId>,

    // Global symbol accumulators
    global_symbols: SymbolArena,
    symbol_arenas: FxHashMap<SymbolId, Arc<NodeArena>>,
    declaration_arenas: DeclarationArenaMap,
    global_lib_symbol_ids: FxHashSet<SymbolId>,

    // Name-interning / merge-dedup
    name_interner: Interner,
    merged_symbols: FxHashMap<Atom, SymbolId>,

    // Program-wide output accumulators
    globals: SymbolTable,
    files: Vec<BoundFile>,
    file_locals_list: Vec<SymbolTable>,
    cross_file_node_symbols: CrossFileNodeSymbols,
    declared_modules: FxHashSet<String>,
    shorthand_ambient_modules: FxHashSet<String>,
    module_exports: FxHashMap<String, SymbolTable>,
    alias_partners: FxHashMap<SymbolId, SymbolId>,
    semantic_defs: FxHashMap<SymbolId, crate::binder::SemanticDefEntry>,
    reexports: Reexports,
    wildcard_reexports: WildcardReexportsMap,
    wildcard_reexports_type_only: WildcardReexportsTypeOnlyMap,

    // Pre-merge metrics
    pre_merge_bind_total_bytes: usize,
}

impl BindResultReducer {
    fn new(results: &impl BindResultsSource) -> Self {
        // Capture aggregate pre-merge memory footprint before we start consuming data.
        let pre_merge_bind_total_bytes: usize = (0..results.len())
            .map(|index| results.get(index).estimated_size_bytes())
            .sum();

        // Collect lib_binders from all results (deduplicated by address), paired with their arenas.
        // Use `lib_binder_arena_map`'s Vacant entry as the dedup gate — no separate set needed.
        let mut lib_binders: Vec<Arc<BinderState>> = Vec::new();
        let mut lib_binder_arena_map: FxHashMap<usize, Arc<NodeArena>> = FxHashMap::default();
        for index in 0..results.len() {
            let result = results.get(index);
            for (lib_binder, lib_arena) in result.lib_binders.iter().zip(result.lib_arenas.iter()) {
                let binder_addr = Arc::as_ptr(lib_binder) as usize;
                if let std::collections::hash_map::Entry::Vacant(e) =
                    lib_binder_arena_map.entry(binder_addr)
                {
                    e.insert(Arc::clone(lib_arena));
                    lib_binders.push(Arc::clone(lib_binder));
                }
            }
        }

        // Calculate total symbols needed (including lib symbols)
        let lib_symbol_count: usize = lib_binders.iter().map(|b| b.symbols.len()).sum();
        let user_symbol_count: usize = (0..results.len())
            .map(|index| results.get(index).symbols.len())
            .sum();
        let total_symbols = lib_symbol_count + user_symbol_count;

        // Create global symbol arena with pre-allocated capacity
        let global_symbols = SymbolArena::with_capacity(total_symbols);
        // Pre-size to `total_symbols`: each merged symbol gets exactly one entry
        // in `symbol_arenas` (the arena that contributed the symbol). On a
        // 6086-file project this is hundreds of thousands of entries and the
        // default-`FxHashMap` doubling schedule rehashes ~18 times during the
        // merge phase. The capacity is known up-front, so skip the rehash chain.
        let symbol_arenas = FxHashMap::with_capacity_and_hasher(total_symbols, Default::default());
        // Pre-size to `total_symbols` as an upper bound: most symbols have a
        // single declaration so this approximates the final size; the few that
        // have multiple declarations only push extra entries into the same key's
        // `SmallVec`, not new map entries. Cuts another long doubling chain on
        // large repos.
        let declaration_arenas: DeclarationArenaMap =
            FxHashMap::with_capacity_and_hasher(total_symbols, Default::default());
        // Pre-size to the file count: this map ends up holding exactly one entry
        // per file (`Arc::as_ptr(&file.arena) -> file.node_symbols`), so the
        // capacity is known up-front. Skipping the default `FxHashMap` growth
        // schedule (1 → 3 → 7 → 15 → … bucket doublings) on a 6086-file project
        // saves ~13 rehashes during the merge phase.
        let cross_file_node_symbols: CrossFileNodeSymbols =
            FxHashMap::with_capacity_and_hasher(results.len(), Default::default());
        let estimated_global_count: usize = (0..results.len())
            .map(|index| results.get(index).file_locals.len())
            .sum();
        let globals = SymbolTable::with_capacity(estimated_global_count);
        let files = Vec::with_capacity(results.len());
        let file_locals_list = Vec::with_capacity(results.len());
        let declared_modules = FxHashSet::default();
        let shorthand_ambient_modules = FxHashSet::default();
        let module_exports: FxHashMap<String, SymbolTable> = FxHashMap::default();
        let alias_partners: FxHashMap<SymbolId, SymbolId> = FxHashMap::default();
        let semantic_defs: FxHashMap<SymbolId, crate::binder::SemanticDefEntry> =
            FxHashMap::default();
        let reexports: Reexports = FxHashMap::default();
        let wildcard_reexports: WildcardReexportsMap = FxHashMap::default();
        let wildcard_reexports_type_only: WildcardReexportsTypeOnlyMap = FxHashMap::default();
        let global_lib_symbol_ids: FxHashSet<SymbolId> = FxHashSet::default();

        // Use interned atoms to avoid repeated String hashing/cloning on hot merge paths.
        let name_interner = Interner::new();
        // IMPORTANT: This map is ONLY for symbols in the ROOT scope (ScopeId(0)).
        // Symbols from nested scopes should NEVER be merged across files/scopes.
        let merged_symbols: FxHashMap<Atom, SymbolId> = FxHashMap::default();

        let lib_symbol_remap: FxHashMap<(usize, SymbolId), SymbolId> = FxHashMap::default();
        let lib_name_to_global: FxHashMap<Atom, SymbolId> = FxHashMap::default();

        Self {
            lib_binders,
            lib_binder_arena_map,
            lib_symbol_remap,
            lib_name_to_global,
            global_symbols,
            symbol_arenas,
            declaration_arenas,
            global_lib_symbol_ids,
            name_interner,
            merged_symbols,
            globals,
            files,
            file_locals_list,
            cross_file_node_symbols,
            declared_modules,
            shorthand_ambient_modules,
            module_exports,
            alias_partners,
            semantic_defs,
            reexports,
            wildcard_reexports,
            wildcard_reexports_type_only,
            pre_merge_bind_total_bytes,
        }
    }

    // ==========================================================================
    // PHASE 1: Remap lib symbols to global arena
    // ==========================================================================
    // This creates a mapping from (lib_binder_ptr, local_id) -> global_id
    // so that file_locals can reference lib symbols using global IDs
    fn merge_lib_binders(&mut self) {
        // Track nested-symbol merging keyed by (global_parent_id, name_atom).
        // A nested symbol (e.g., `Intl.ResolvedDateTimeFormatOptions`) declared in two
        // sibling lib files must collapse into one merged symbol once their parent namespace
        // has merged. Without this, each lib file allocates its own copy, causing members
        // from one lib (e.g. `calendar`) to silently disappear from the merged namespace.
        // Used only within this phase; not needed by subsequent phases.
        let mut nested_merged: FxHashMap<(SymbolId, Atom), SymbolId> = FxHashMap::default();
        // Iterate by index to avoid holding a borrow on `self.lib_binders` while the
        // loop body mutates other fields of `self` through the stable NLL split-borrow.
        for lib_binder_idx in 0..self.lib_binders.len() {
            let lib_binder = Arc::clone(&self.lib_binders[lib_binder_idx]);
            let lib_binder_ptr = Arc::as_ptr(&lib_binder) as usize;

            // Pre-build a set of top-level symbol IDs from file_locals for O(1) lookup.
            // This avoids an O(N*F) quadratic scan where each symbol would linearly
            // search file_locals to check if it's top-level.
            let top_level_ids: FxHashSet<SymbolId> =
                lib_binder.file_locals.iter().map(|(_, id)| *id).collect();

            // For external module lib files (e.g. esnext.iterator.d.ts with
            // `export {}`), build a set of declaration NodeIndices from
            // `declare global { ... }` blocks. Module-scoped declarations
            // must NOT be merged into existing global symbols.
            let global_aug_nodes: Option<FxHashSet<NodeIndex>> = if lib_binder.is_external_module {
                let mut nodes = FxHashSet::default();
                for augs in lib_binder.global_augmentations.values() {
                    for aug in augs {
                        nodes.insert(aug.node);
                    }
                }
                Some(nodes)
            } else {
                None
            };

            // Process all symbols in this lib binder
            for i in 0..lib_binder.symbols.len() {
                let local_id = SymbolId(i as u32);
                if let Some(lib_sym) = lib_binder.symbols.get(local_id) {
                    // Determine if this is a top-level symbol by checking file_locals.
                    // In lib files, declarations like `declare namespace Reflect` may appear
                    // in a child scope (e.g., ScopeId(1)) even though they're conceptually
                    // top-level. Using file_locals is more reliable than the scope check
                    // for determining which lib symbols should be globally merged.
                    let is_top_level = top_level_ids.contains(&local_id);

                    // Check if a symbol with this name already exists (cross-lib merging)
                    // IMPORTANT: Only merge top-level symbols (those in file_locals)
                    // Nested symbols (namespace members, etc.) should NEVER be merged across scopes
                    let global_id = if is_top_level {
                        // For external module lib binders (e.g. esnext.iterator.d.ts
                        // with `export {}`), do NOT merge top-level symbols into
                        // the global symbol table. Their module-scoped declarations
                        // (class/interface) would contaminate global symbols with
                        // the same name. Global contributions come solely via
                        // `declare global { ... }` augmentation blocks (filtered below).
                        let name_atom = self.name_interner.intern(&lib_sym.escaped_name);
                        if let Some(&existing_id) = self.merged_symbols.get(&name_atom) {
                            // Symbol already exists - check if we can merge
                            if let Some(existing_sym) = self.global_symbols.get(existing_id) {
                                if can_merge_symbols_cross_file(existing_sym.flags, lib_sym.flags) {
                                    if let Some(existing_mut) =
                                        self.global_symbols.get_mut(existing_id)
                                    {
                                        apply_lib_declarations_to_existing(
                                            existing_mut,
                                            lib_sym,
                                            &global_aug_nodes,
                                        );
                                    }
                                    existing_id
                                } else {
                                    // Cannot merge - allocate new (shadowing)
                                    let new_id = self.global_symbols.alloc_from(lib_sym);
                                    self.merged_symbols.insert(name_atom, new_id);
                                    new_id
                                }
                            } else {
                                // Shouldn't happen - allocate new
                                let new_id = self.global_symbols.alloc_from(lib_sym);
                                self.merged_symbols.insert(name_atom, new_id);
                                new_id
                            }
                        } else {
                            // New symbol - allocate in global arena
                            let new_id = self.global_symbols.alloc_from(lib_sym);
                            self.merged_symbols.insert(name_atom, new_id);
                            new_id
                        }
                    } else {
                        // Nested symbol (e.g. a namespace member). Two lib files may
                        // declare the same nested name under a parent that *itself*
                        // has merged across files — for example
                        // `interface Intl.ResolvedDateTimeFormatOptions` is split
                        // across lib.es5.d.ts and lib.es2021.intl.d.ts, both inside
                        // the merged `Intl` namespace. Without merging the nested
                        // pair, interface lowering only sees one lib's declaration
                        // body and members from the other lib (e.g. `calendar` from
                        // es5) silently disappear from the resolved shape.
                        //
                        // Keyed by (global parent id, name): unrelated `Foo`s nested
                        // inside *different* namespaces have different parent ids
                        // and therefore do not collide.
                        let nested_key = self
                            .lib_symbol_remap
                            .get(&(lib_binder_ptr, lib_sym.parent))
                            .copied()
                            .map(|gp| (gp, self.name_interner.intern(&lib_sym.escaped_name)));
                        let existing_mergeable = nested_key
                            .and_then(|key| nested_merged.get(&key).copied())
                            .filter(|&existing_id| {
                                self.global_symbols
                                    .get(existing_id)
                                    .is_some_and(|existing| {
                                        can_merge_symbols_cross_file(existing.flags, lib_sym.flags)
                                    })
                            });
                        if let Some(existing_id) = existing_mergeable {
                            if let Some(existing_mut) = self.global_symbols.get_mut(existing_id) {
                                apply_lib_declarations_to_existing(
                                    existing_mut,
                                    lib_sym,
                                    &global_aug_nodes,
                                );
                            }
                            existing_id
                        } else {
                            let new_id = self.global_symbols.alloc_from(lib_sym);
                            if let Some(key) = nested_key {
                                nested_merged.insert(key, new_id);
                            }
                            new_id
                        }
                    };

                    self.lib_symbol_remap
                        .insert((lib_binder_ptr, local_id), global_id);
                    self.global_lib_symbol_ids.insert(global_id);

                    // Set arena mappings for this lib symbol using the lib file's arena.
                    // The original lib binder's symbol_arenas/declaration_arenas are empty
                    // (only populated during per-file merge which uses a different binder).
                    // We use lib_binder_arena_map to get the correct arena for this lib file.
                    if let Some(lib_arena) = self.lib_binder_arena_map.get(&lib_binder_ptr) {
                        self.symbol_arenas
                            .entry(global_id)
                            .or_insert_with(|| Arc::clone(lib_arena));
                        for &decl in &lib_sym.declarations {
                            self.declaration_arenas
                                .entry((global_id, decl))
                                .or_default()
                                .push(Arc::clone(lib_arena));
                        }
                    }
                }
            }
        }
    }

    // ==========================================================================
    // PHASE 1.25: Clear un-remapped exports/members from global symbols
    // ==========================================================================
    // Phase 1's `alloc_from()` copies symbols including their exports/members
    // tables, but those tables contain lib-LOCAL SymbolIds. In the global arena,
    // those same numeric IDs map to DIFFERENT symbols (e.g., lib-local SymbolId(2)
    // might be DateTimeFormat in es5.d.ts, but SymbolId(2) in the global arena is
    // cancelIdleCallback from dom.d.ts). Phase 1.5 will rebuild exports/members
    // with correctly remapped global IDs, so we must clear the corrupt data first.
    fn clear_unremapped_exports(&mut self) {
        // Snapshot IDs into a Vec before the loop: the `.values()` iterator on
        // `lib_symbol_remap` would hold `&self.lib_symbol_remap` for the full loop duration,
        // preventing the `&mut self.global_symbols` borrow needed inside. Vec avoids the
        // hashing overhead of a set; setting to None twice is idempotent so dedup is unnecessary.
        let lib_global_ids: Vec<SymbolId> = self.lib_symbol_remap.values().copied().collect();
        for global_id in lib_global_ids {
            if let Some(sym) = self.global_symbols.get_mut(global_id) {
                sym.exports = None;
                sym.members = None;
            }
        }
    }

    // Remap one exports-or-members table on a lib symbol from local IDs to global IDs.
    //
    // When two lib binders both declare a nested symbol under the same parent (e.g.
    // `Intl.DateTimeFormat` in es5.d.ts and es2017.intl.d.ts), the second binder's
    // entry collides with the first. Collision entries are pushed into `merge_targets`
    // and their flags/declarations/arenas are merged into the winner. New (non-colliding)
    // entries are added directly to the parent symbol's exports or members table.
    fn remap_lib_symbol_table_entries(
        &mut self,
        global_id: SymbolId,
        lib_binder_ptr: usize,
        local_table: &SymbolTable,
        is_exports: bool,
    ) {
        let mut new_items: Vec<(String, SymbolId)> = Vec::new();
        let mut merge_targets: Vec<(SymbolId, SymbolId)> = Vec::new();

        if let Some(sym) = self.global_symbols.get(global_id) {
            let existing = if is_exports {
                sym.exports.as_ref()
            } else {
                sym.members.as_ref()
            };
            for (name, &local_id) in local_table.iter() {
                if let Some(&new_id) = self.lib_symbol_remap.get(&(lib_binder_ptr, local_id)) {
                    let prev = existing.and_then(|t| t.get(name));
                    if let Some(prev_id) = prev {
                        if prev_id != new_id {
                            merge_targets.push((prev_id, new_id));
                        }
                    } else {
                        new_items.push((name.clone(), new_id));
                    }
                }
            }
        }

        for (dst_id, src_id) in merge_targets {
            // Extract data from src before taking a mutable borrow for dst.
            let src_data = self
                .global_symbols
                .get(src_id)
                .map(|s| (s.flags, s.declarations.to_vec(), s.value_declaration));
            if let Some((src_flags, src_decls, src_value_decl)) = src_data {
                if let Some(dst) = self.global_symbols.get_mut(dst_id) {
                    dst.flags |= src_flags;
                    append_unique_declarations(&mut dst.declarations, &src_decls);
                    if dst.value_declaration.is_none() && src_value_decl.is_some() {
                        dst.value_declaration = src_value_decl;
                    }
                }
                // Copy declaration_arenas and symbol_arenas entries from src to dst.
                // When symbols inside namespaces are merged, the dst symbol gets src's
                // declarations appended, but the checker needs declaration_arenas[(dst_id, decl)]
                // to find the correct arena for each declaration.
                for decl_idx in src_decls {
                    if let Some(arenas) = self.declaration_arenas.get(&(src_id, decl_idx)).cloned()
                    {
                        self.declaration_arenas
                            .entry((dst_id, decl_idx))
                            .or_default()
                            .extend(arenas);
                    }
                }
            }
            if let Some(src_arena) = self.symbol_arenas.get(&src_id).cloned() {
                self.symbol_arenas.entry(dst_id).or_insert(src_arena);
            }
        }

        if !new_items.is_empty()
            && let Some(sym) = self.global_symbols.get_mut(global_id)
        {
            let table = if is_exports {
                sym.exports
                    .get_or_insert_with(|| Box::new(SymbolTable::with_capacity(new_items.len())))
            } else {
                sym.members
                    .get_or_insert_with(|| Box::new(SymbolTable::with_capacity(new_items.len())))
            };
            for (name, id) in new_items {
                table.set(name, id);
            }
        }
    }

    // ==========================================================================
    // PHASE 1.5: Remap internal references (parent, exports, members)
    // ==========================================================================
    // After all lib symbols have been allocated in the global arena, we need a
    // second pass to fix up internal SymbolId references. The `alloc_from()` call
    // copies the symbol data including members/exports/parent, but those fields
    // still contain LOCAL SymbolIds from the original lib binder. We must remap
    // them to the corresponding global IDs using lib_symbol_remap.
    // (This mirrors Phase 2 in state.rs merge_lib_contexts_into_binder.)
    fn remap_lib_references(&mut self) {
        for lib_binder_idx in 0..self.lib_binders.len() {
            let lib_binder = Arc::clone(&self.lib_binders[lib_binder_idx]);
            let lib_binder_ptr = Arc::as_ptr(&lib_binder) as usize;

            for i in 0..lib_binder.symbols.len() {
                let local_id = SymbolId(i as u32);
                let Some(&global_id) = self.lib_symbol_remap.get(&(lib_binder_ptr, local_id))
                else {
                    continue;
                };
                let Some(lib_sym) = lib_binder.symbols.get(local_id) else {
                    continue;
                };

                // Remap parent
                if lib_sym.parent.is_some()
                    && let Some(&new_parent) =
                        self.lib_symbol_remap.get(&(lib_binder_ptr, lib_sym.parent))
                    && let Some(sym) = self.global_symbols.get_mut(global_id)
                {
                    sym.parent = new_parent;
                }

                // Remap exports/members: replace local IDs with global IDs.
                // When an export/member name was already remapped by a previous lib binder,
                // merge the new symbol's flags/declarations into the existing one
                // (e.g., INTERFACE from one lib + VALUE from another, like
                // DateTimeFormat in Intl across es5.d.ts and es2017.intl.d.ts).
                if let Some(exports) = lib_sym.exports.as_deref() {
                    self.remap_lib_symbol_table_entries(global_id, lib_binder_ptr, exports, true);
                }
                if let Some(members) = lib_sym.members.as_deref() {
                    self.remap_lib_symbol_table_entries(global_id, lib_binder_ptr, members, false);
                }
            }
        }

        // Also remap lib file_locals entries that reference symbols by name
        // (for exported lib symbols like Array, Object, console)
        for lib_binder_idx in 0..self.lib_binders.len() {
            let lib_binder = Arc::clone(&self.lib_binders[lib_binder_idx]);
            let lib_binder_ptr = Arc::as_ptr(&lib_binder) as usize;
            for (name, &local_id) in lib_binder.file_locals.iter() {
                // When a lib file is an external module (has `export {}`), its
                // file_locals contain module-scoped declarations that must NOT
                // pollute the global scope. Only include symbols that originate
                // from `declare global { ... }` blocks.
                if lib_binder.is_external_module
                    && !lib_binder.global_augmentations.contains_key(name)
                {
                    continue;
                }
                if let Some(&global_id) = self.lib_symbol_remap.get(&(lib_binder_ptr, local_id)) {
                    // Only keep the first mapping for each name (lib files are processed in order)
                    let name_atom = self.name_interner.intern(name);
                    if let std::collections::hash_map::Entry::Vacant(entry) =
                        self.lib_name_to_global.entry(name_atom)
                    {
                        entry.insert(global_id);
                        // `lib_binder` is an Arc clone — no borrow on `self.lib_binders` here,
                        // so mutating `self.globals` is safe in the same loop body.
                        if !self.globals.has(name) {
                            self.globals.set(name.clone(), global_id);
                        }
                    }
                }
            }
        }
    }

    // ==========================================================================
    // PHASE 1.6: Propagate lib semantic_defs directly to global semantic_defs
    // ==========================================================================
    // Lib binders record `semantic_defs` for their top-level declarations during
    // binding (TypeAlias, Interface, Class, Enum, Namespace, Function, Variable).
    // Phase 1 already remapped lib SymbolIds to global IDs. We propagate the
    // semantic_defs using that remap so the checker can pre-create solver DefIds
    // for ALL lib symbols at construction time.
    //
    // Previously, lib semantic_defs only reached the global map indirectly through
    // per-file binders (which ran `merge_lib_symbols` Phase 4). That path is
    // redundant and order-dependent — by propagating directly here, the merge is
    // self-contained and deterministic.
    fn propagate_lib_semantic_defs(&mut self) {
        for lib_binder_idx in 0..self.lib_binders.len() {
            let lib_binder = Arc::clone(&self.lib_binders[lib_binder_idx]);
            let lib_binder_ptr = Arc::as_ptr(&lib_binder) as usize;
            for (&old_sym_id, entry) in lib_binder.semantic_defs.iter() {
                if let Some(&global_id) = self.lib_symbol_remap.get(&(lib_binder_ptr, old_sym_id)) {
                    // Keep first occurrence (declaration merging keeps first identity).
                    self.semantic_defs.entry(global_id).or_insert_with(|| {
                        let mut remapped = entry.clone();
                        // Update file_id to match the global symbol's decl_file_idx
                        // so DefinitionStore composite key lookups stay consistent.
                        remapped.file_id = self
                            .global_symbols
                            .get(global_id)
                            .map_or(entry.file_id, |s| s.decl_file_idx);
                        // Remap parent_namespace to global SymbolId
                        remapped.parent_namespace = entry.parent_namespace.and_then(|old_parent| {
                            self.lib_symbol_remap
                                .get(&(lib_binder_ptr, old_parent))
                                .copied()
                        });
                        remapped
                    });
                }
            }
        }
        // `global_lib_symbol_ids` is populated incrementally in `merge_lib_binders`
        // at the point of each `lib_symbol_remap` insertion, so no batch extend needed here.
    }

    // ==========================================================================
    // PHASE 2: Process user files
    // ==========================================================================
    fn merge_user_files(&mut self, results: &mut impl BindResultsSource) {
        for file_idx in 0..results.len() {
            {
                let result = results.get(file_idx);
                self.declared_modules
                    .extend(result.declared_modules.iter().cloned());
                self.shorthand_ambient_modules
                    .extend(result.shorthand_ambient_modules.iter().cloned());

                // Merge reexports from this file
                for (file_name, file_reexports) in result.reexports.iter() {
                    let entry = self.reexports.entry(file_name.clone()).or_default();
                    for (export_name, mapping) in file_reexports {
                        entry.insert(export_name.clone(), mapping.clone());
                    }
                }

                // Merge wildcard reexports from this file
                for (file_name, source_modules) in result.wildcard_reexports.iter() {
                    let entry = self
                        .wildcard_reexports
                        .entry(file_name.clone())
                        .or_default();
                    let type_only_entry = self
                        .wildcard_reexports_type_only
                        .entry(file_name.clone())
                        .or_default();
                    let source_type_only = result.wildcard_reexports_type_only.get(file_name);

                    if entry.len() + source_modules.len() <= 16 {
                        for (i, source_module) in source_modules.iter().enumerate() {
                            // Use index-based access to get the correct type-only flag
                            let source_is_type_only = source_type_only
                                .and_then(|entries| entries.get(i).map(|(_, is_to)| *is_to))
                                .unwrap_or(false);

                            if let Some(pos) = entry.iter().position(|m| m == source_module) {
                                // Already have this source — if this path is non-type-only,
                                // override the existing flag (value re-export takes priority).
                                if !source_is_type_only {
                                    type_only_entry[pos].1 = false;
                                }
                            } else {
                                entry.push(source_module.clone());
                                type_only_entry.push((source_module.clone(), source_is_type_only));
                            }
                        }
                    } else {
                        let mut seen: FxHashMap<String, usize> =
                            entry.iter().cloned().zip(0..).collect();
                        for (i, source_module) in source_modules.iter().enumerate() {
                            let source_is_type_only = source_type_only
                                .and_then(|entries| entries.get(i).map(|(_, is_to)| *is_to))
                                .unwrap_or(false);

                            if let Some(&pos) = seen.get(source_module) {
                                if !source_is_type_only {
                                    type_only_entry[pos].1 = false;
                                }
                            } else {
                                let pos = entry.len();
                                seen.insert(source_module.clone(), pos);
                                entry.push(source_module.clone());
                                type_only_entry.push((source_module.clone(), source_is_type_only));
                            }
                        }
                    }
                }
                // Copy symbols from this file to global arena, getting new IDs
                let mut id_remap: FxHashMap<SymbolId, SymbolId> = FxHashMap::default();
                for i in 0..result.symbols.len() {
                    let old_id = SymbolId(i as u32);
                    if let Some(sym) = result.symbols.get(old_id) {
                        // For lib-originated symbols, reuse the Phase 1 global IDs rather than
                        // allocating new ones. This prevents duplicate lib symbols and ensures
                        // the Phase 1.5 remapped exports/members are preserved.
                        if result.lib_symbol_ids.contains(&old_id) {
                            // For lib-originated symbols, use the reverse remap to find the
                            // original (lib_binder_ptr, local_id), then look up the Phase 1
                            // global ID via lib_symbol_remap. This ensures all lib symbols
                            // (both top-level and nested) map to their Phase 1 global IDs,
                            // preserving the Phase 1.5 export/member remapping.
                            let mut resolved_global_id = None;
                            if let Some(&(binder_ptr, original_local_id)) =
                                result.lib_symbol_reverse_remap.get(&old_id)
                                && let Some(&global_id) =
                                    self.lib_symbol_remap.get(&(binder_ptr, original_local_id))
                            {
                                resolved_global_id = Some(global_id);
                            }
                            // Fallback: look up by name in merged_symbols or lib_name_to_global
                            if resolved_global_id.is_none() {
                                let name_atom = self.name_interner.intern(&sym.escaped_name);
                                if let Some(&global_id) = self.merged_symbols.get(&name_atom) {
                                    resolved_global_id = Some(global_id);
                                }
                                if resolved_global_id.is_none()
                                    && let Some(&global_id) =
                                        self.lib_name_to_global.get(&name_atom)
                                {
                                    resolved_global_id = Some(global_id);
                                }
                            }
                            if let Some(global_id) = resolved_global_id {
                                // The user binder may have merged additional flags and declarations
                                // into this lib symbol (e.g., user `interface Event<T>` augments
                                // lib's non-generic `Event`, or user `type Proxy<T>` adds TYPE_ALIAS
                                // to lib's `declare var Proxy`). Always propagate extra flags and
                                // user-local declarations to the global symbol so that type parameter
                                // resolution can find them.
                                if let Some(global_sym) = self.global_symbols.get_mut(global_id) {
                                    let extra_flags = sym.flags & !global_sym.flags;
                                    if extra_flags != 0 {
                                        global_sym.flags |= extra_flags;
                                    }
                                    // Always copy user declarations that were merged into this symbol,
                                    // even when flags are identical. Without this, user declarations
                                    // (e.g., a generic `interface Event<T>`) are lost and
                                    // get_type_params_for_symbol won't find their type parameters.
                                    append_unique_declarations(
                                        &mut global_sym.declarations,
                                        &sym.declarations,
                                    );
                                }
                                id_remap.insert(old_id, global_id);
                                continue;
                            }
                            // Last resort: allocate a new ID (shouldn't happen normally)
                            let new_id = self
                                .global_symbols
                                .alloc(sym.flags, sym.escaped_name.clone());
                            self.symbol_arenas.insert(new_id, Arc::clone(&result.arena));
                            id_remap.insert(old_id, new_id);
                            continue;
                        }

                        // Check if this symbol is from a nested scope.
                        // We check whether this symbol ID appears in the ROOT scope table
                        // (ScopeId(0) = SourceFile scope). This is more reliable than checking
                        // node_scope_ids because not all declaration types create scopes
                        // (e.g., InterfaceDeclaration does not create a scope, so its node
                        // won't appear in node_scope_ids, causing false negatives).
                        let is_nested_symbol = !result.scopes.first().is_some_and(|root_scope| {
                            root_scope
                                .table
                                .get(&sym.escaped_name)
                                .is_some_and(|root_sym_id| root_sym_id == old_id)
                        });

                        // Check if symbol already exists in globals (cross-file merging)
                        // IMPORTANT: Only merge symbols from ROOT scope (ScopeId(0))
                        // Nested scope symbols should NEVER be merged across scopes
                        let new_id = if !is_nested_symbol && !result.is_external_module {
                            let name_atom = self.name_interner.intern(&sym.escaped_name);
                            if let Some(&existing_id) = self.merged_symbols.get(&name_atom) {
                                // Symbol exists - check if we can merge
                                if let Some(existing_sym) = self.global_symbols.get(existing_id) {
                                    // Check if symbols can merge (interface+interface, namespace+namespace, etc.)
                                    if can_merge_symbols_cross_file(existing_sym.flags, sym.flags) {
                                        // Merge: reuse existing symbol ID, will merge declarations below
                                        existing_id
                                    } else {
                                        // Cannot merge - allocate new symbol (shadowing or duplicate)
                                        let new_id = self
                                            .global_symbols
                                            .alloc(sym.flags, sym.escaped_name.clone());
                                        self.symbol_arenas
                                            .insert(new_id, Arc::clone(&result.arena));
                                        self.merged_symbols.insert(name_atom, new_id);
                                        new_id
                                    }
                                } else {
                                    // Shouldn't happen - allocate new
                                    let new_id = self
                                        .global_symbols
                                        .alloc(sym.flags, sym.escaped_name.clone());
                                    self.symbol_arenas.insert(new_id, Arc::clone(&result.arena));
                                    self.merged_symbols.insert(name_atom, new_id);
                                    new_id
                                }
                            } else {
                                // New symbol - allocate
                                let new_id = self
                                    .global_symbols
                                    .alloc(sym.flags, sym.escaped_name.clone());
                                self.symbol_arenas.insert(new_id, Arc::clone(&result.arena));
                                self.merged_symbols.insert(name_atom, new_id);
                                new_id
                            }
                        } else {
                            // Nested symbol - always allocate new, never merge or add to merged_symbols
                            let new_id = self
                                .global_symbols
                                .alloc(sym.flags, sym.escaped_name.clone());
                            self.symbol_arenas.insert(new_id, Arc::clone(&result.arena));
                            // NOTE: Don't add to merged_symbols - nested symbols should never be cross-file merged
                            new_id
                        };
                        id_remap.insert(old_id, new_id);
                    }
                }

                // Track remapped lib symbol IDs for unused-checking exclusion
                for &old_lib_id in result.lib_symbol_ids.iter() {
                    if let Some(&new_id) = id_remap.get(&old_lib_id) {
                        self.global_lib_symbol_ids.insert(new_id);
                    }
                }

                // Copy symbol_arenas entries from user file, remapping IDs
                // This propagates lib symbol arena mappings that were created during merge_lib_symbols
                for (&old_sym_id, arena) in result.symbol_arenas.iter() {
                    if let Some(&new_sym_id) = id_remap.get(&old_sym_id) {
                        self.symbol_arenas
                            .entry(new_sym_id)
                            .or_insert_with(|| Arc::clone(arena));
                    }
                }

                // Copy declaration_arenas entries from user file, remapping symbol IDs.
                // Skip lib-originated symbols: their declaration_arenas were already set up
                // in Phase 1 from the original lib binder. The per-file binder has duplicate
                // arenas for the same declarations (from merge_lib_contexts_into_binder),
                // which would cause interface members to be lowered multiple times.
                for (&(old_sym_id, decl_idx), arenas) in result.declaration_arenas.iter() {
                    if result.lib_symbol_ids.contains(&old_sym_id) {
                        continue;
                    }
                    if let Some(&new_sym_id) = id_remap.get(&old_sym_id) {
                        let target = self
                            .declaration_arenas
                            .entry((new_sym_id, decl_idx))
                            .or_default();
                        for arena in arenas {
                            target.push(Arc::clone(arena));
                        }
                    }
                }

                // Collect exported symbols for this file (for module_exports map).
                //
                // Note: `export default ...` must be represented under the `"default"` export name
                // so that `import X from "./mod"` can resolve correctly.
                //
                // We intentionally do *not* depend solely on `sym.is_exported` for determining whether
                // a file is an external module, because default exports may not correspond to a named
                // export in `file_locals`.
                let mut exports =
                    SymbolTable::with_capacity(result.file_locals.len().saturating_add(1));
                let mut export_equals_old: Option<SymbolId> = None;

                // 1) Named exports collected from file_locals.
                for (name, &sym_id) in result.file_locals.iter() {
                    // Skip lib/global symbols (e.g. `escape`, `unescape`) that were merged
                    // into file_locals from lib.d.ts. These are global builtins that should
                    // not appear in a user module's module_exports.
                    if result.lib_symbol_ids.contains(&sym_id) {
                        continue;
                    }
                    if name == "export=" {
                        export_equals_old = Some(sym_id);
                    }
                    if let Some(sym) = result.symbols.get(sym_id)
                        && (sym.is_exported || name == "export=")
                        && let Some(&remapped_id) = id_remap.get(&sym_id)
                    {
                        exports.set(name.clone(), remapped_id);
                    }
                }

                // 1b) `export = target` should also expose namespace members from `target`.
                if let Some(old_export_equals_sym) = export_equals_old
                    && let Some(target_symbol) = result.symbols.get(old_export_equals_sym)
                {
                    if let Some(target_exports) = target_symbol.exports.as_ref() {
                        for (export_name, old_sym_id) in target_exports.iter() {
                            // Skip "default" — the `export =` target itself IS the default
                            // export. A static member named `default` (e.g. `static default: "foo"`)
                            // must not shadow the `export=` symbol in module_exports.
                            if export_name == "default" {
                                continue;
                            }
                            if let Some(&remapped_id) = id_remap.get(old_sym_id) {
                                exports.set(export_name.clone(), remapped_id);
                            }
                        }
                    }
                    // The `.members` table on a class symbol holds INSTANCE members (e.g.
                    // `bar` from `class D { bar: string; }`). They are accessible only
                    // through an instance — never at the module-namespace level. Static
                    // members and namespace augmentations live in `.exports`, merged above.
                    // Without this guard, `import x = require()` of an `export = D` module
                    // synthesizes a phantom `{ bar }` namespace surface and produces
                    // `typeof D & { bar }` instead of tsc's plain `typeof D`, which in turn
                    // causes assignment failures to be reported as TS2322 instead of TS2741.
                    let target_is_class =
                        (target_symbol.flags & crate::binder::symbol_flags::CLASS) != 0;
                    if !target_is_class && let Some(target_members) = target_symbol.members.as_ref()
                    {
                        for (member_name, old_sym_id) in target_members.iter() {
                            if member_name == "default" {
                                continue;
                            }
                            if let Some(&remapped_id) = id_remap.get(old_sym_id) {
                                exports.set(member_name.clone(), remapped_id);
                            }
                        }
                    }
                }

                // 2) Default export: add `"default"` entry when present.
                let mut default_export_old: Option<SymbolId> = None;
                if let Some(root_node) = result.arena.get(result.source_file)
                    && let Some(source) = result.arena.get_source_file(root_node)
                {
                    for &stmt_idx in &source.statements.nodes {
                        let Some(stmt_node) = result.arena.get(stmt_idx) else {
                            continue;
                        };
                        if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                            continue;
                        }
                        let Some(export_decl) = result.arena.get_export_decl(stmt_node) else {
                            continue;
                        };
                        if !export_decl.is_default_export {
                            continue;
                        }

                        // `export default <expr>;`
                        let Some(clause_node) = result.arena.get(export_decl.export_clause) else {
                            continue;
                        };

                        // Best-effort: if the default export is a reference to a named declaration
                        // (identifier/class/function), map `"default"` to that symbol.
                        //
                        // This matches the needs of `import X from "./mod"` and keeps the symbol ID
                        // stable across files without synthesizing a new symbol.
                        if clause_node.kind == crate::scanner::SyntaxKind::Identifier as u16 {
                            if let Some(ident) = result.arena.get_identifier(clause_node) {
                                default_export_old = result.file_locals.get(&ident.escaped_text);
                            }
                        } else if let Some(func) = result.arena.get_function(clause_node) {
                            if let Some(name_node) = result.arena.get(func.name)
                                && let Some(ident) = result.arena.get_identifier(name_node)
                            {
                                default_export_old = result.file_locals.get(&ident.escaped_text);
                            }
                        } else if let Some(class) = result.arena.get_class(clause_node)
                            && let Some(name_node) = result.arena.get(class.name)
                            && let Some(ident) = result.arena.get_identifier(name_node)
                        {
                            default_export_old = result.file_locals.get(&ident.escaped_text);
                        }

                        // Only one default export per module.
                        break;
                    }
                }

                if let Some(old_sym_id) = default_export_old
                    && let Some(&remapped_id) = id_remap.get(&old_sym_id)
                {
                    exports.set("default".to_string(), remapped_id);
                }

                if !exports.is_empty() {
                    self.module_exports
                        .insert(result.file_name.clone(), exports);
                }

                for (module_key, exports_table) in result.module_exports.iter() {
                    let remapped = remap_symbol_table_required(exports_table, &id_remap);
                    if !remapped.is_empty() {
                        merge_symbol_table_first_wins(
                            self.module_exports.entry(module_key.clone()).or_default(),
                            &remapped,
                        );
                    }
                }

                // Remap binder's per-file alias_partners to global SymbolIds
                for (&type_alias_id, &alias_id) in result.alias_partners.iter() {
                    if let (Some(&new_ta), Some(&new_alias)) =
                        (id_remap.get(&type_alias_id), id_remap.get(&alias_id))
                    {
                        self.alias_partners.insert(new_ta, new_alias);
                    }
                }

                // Remap binder's per-file semantic_defs to global SymbolIds (Phase 1 DefId-first).
                // Skip lib-originated symbols — they were already propagated in Phase 1.6.
                // Also collect per-file entries for BoundFile.semantic_defs (file-scoped identity).
                let mut file_semantic_defs: FxHashMap<SymbolId, crate::binder::SemanticDefEntry> =
                    FxHashMap::default();
                for (old_sym_id, entry) in result.semantic_defs.iter() {
                    if result.lib_symbol_ids.contains(old_sym_id) {
                        continue;
                    }
                    if let Some(&new_sym_id) = id_remap.get(old_sym_id) {
                        // Update file_id to use the global file index
                        let mut remapped_entry = entry.clone();
                        remapped_entry.file_id = file_idx as u32;
                        // Remap parent_namespace to global SymbolId
                        remapped_entry.parent_namespace = entry
                            .parent_namespace
                            .and_then(|old_parent| id_remap.get(&old_parent).copied());
                        // Collect per-file entry (always insert — no cross-file merging here)
                        file_semantic_defs.insert(new_sym_id, remapped_entry.clone());
                        // Insert the first occurrence, or accumulate heritage/metadata from
                        // later files via merge_cross_file (e.g., cross-file interface merging,
                        // class + interface merging).
                        self.semantic_defs
                            .entry(new_sym_id)
                            .and_modify(|existing| existing.merge_cross_file(&remapped_entry))
                            .or_insert(remapped_entry);
                    }
                }

                // Collect all nested merge pairs across all symbols in this file,
                // then process them AFTER all symbols have their data populated.
                // This is critical because HashMap iteration order is random — if a
                // parent symbol is processed before its children, the children won't
                // have their exports populated yet, making recursive merge ineffective.
                let mut all_nested_merges: Vec<(SymbolId, SymbolId)> = Vec::new();

                // Sort id_remap entries by old_id (ascending) so that symbol processing
                // order is deterministic regardless of FxHashMap iteration order. This
                // ensures declaration_arenas entries and nested merge pairs are always
                // collected in the same order across runs, producing identical merged
                // output for identical inputs.
                let mut sorted_remap: Vec<(SymbolId, SymbolId)> =
                    id_remap.iter().map(|(&old, &new)| (old, new)).collect();
                sorted_remap.sort_unstable_by_key(|(old, _)| old.0);

                for &(old_id, new_id) in &sorted_remap {
                    // Skip lib-originated symbols - they were already set up by Phase 1 + 1.5
                    if result.lib_symbol_ids.contains(&old_id) {
                        continue;
                    }
                    let Some(old_sym) = result.symbols.get(old_id) else {
                        continue;
                    };

                    // CRITICAL: Populate declaration_arenas for user symbols
                    for &decl_idx in &old_sym.declarations {
                        self.declaration_arenas
                            .entry((new_id, decl_idx))
                            .or_default()
                            .push(Arc::clone(&result.arena));
                    }

                    let mut nested_merges: Vec<(SymbolId, SymbolId)> = Vec::new();
                    if let Some(new_sym) = self.global_symbols.get_mut(new_id) {
                        // Check if this is a cross-file merge (same symbol already has data)
                        let is_cross_file_merge = !new_sym.declarations.is_empty();

                        if is_cross_file_merge {
                            // Cross-file merge: append declarations and merge flags
                            new_sym.flags |= old_sym.flags;
                            // Append new declarations from this file, but skip NodeIndex values
                            // that already exist from a DIFFERENT arena (cross-file NodeIndex
                            // collision). When two files produce the same NodeIndex for different
                            // declarations, adding duplicates causes the checker to look up the
                            // wrong arena and misidentify declaration kinds (e.g., treating a
                            // remote interface as a local class, triggering false TS2300).
                            // The declaration_arenas entry already contains both arenas for the
                            // colliding NodeIndex, so the checker can iterate all arenas there.
                            {
                                let mut filtered_decls: Vec<NodeIndex> = Vec::new();
                                for &decl_idx in &old_sym.declarations {
                                    if new_sym.declarations.contains(&decl_idx) {
                                        // NodeIndex collision: this index already exists in the
                                        // merged symbol from a previous file. Check if the
                                        // declaration_arenas entry has a different arena (meaning
                                        // it's from a different file, not a true duplicate).
                                        if let Some(arenas) =
                                            self.declaration_arenas.get(&(new_id, decl_idx))
                                        {
                                            let has_different_arena = arenas.iter().any(|a| {
                                                !std::ptr::eq(
                                                    Arc::as_ptr(a),
                                                    Arc::as_ptr(&result.arena),
                                                )
                                            });
                                            if has_different_arena {
                                                // Skip: this is a cross-file collision, not a
                                                // true duplicate declaration within the same file.
                                                continue;
                                            }
                                        }
                                    }
                                    filtered_decls.push(decl_idx);
                                }
                                append_unique_declarations(
                                    &mut new_sym.declarations,
                                    &filtered_decls,
                                );
                            }
                            // Update value_declaration if the old one was NONE
                            if new_sym.value_declaration.is_none()
                                && old_sym.value_declaration.is_some()
                            {
                                new_sym.value_declaration = old_sym.value_declaration;
                            }
                            // Merge exports (if both have exports)
                            // First pass: add missing exports, collect nested merge targets
                            if let (Some(old_exports), Some(new_exports)) =
                                (old_sym.exports.as_ref(), new_sym.exports.as_mut())
                            {
                                for (name, sym_id) in old_exports.iter() {
                                    if !new_exports.has(name) {
                                        // Remap the symbol ID and add to exports
                                        if let Some(&remapped_id) = id_remap.get(sym_id) {
                                            new_exports.set(name.clone(), remapped_id);
                                        }
                                    } else if let Some(&remapped_new_id) = id_remap.get(sym_id) {
                                        // Both files export the same name (e.g., nested namespace Utils).
                                        // Record for deferred merge outside the get_mut borrow scope.
                                        let existing_export_id = new_exports.get(name).expect(
                                            "else branch guarantees name exists in new_exports",
                                        );
                                        if existing_export_id != remapped_new_id {
                                            nested_merges
                                                .push((existing_export_id, remapped_new_id));
                                        }
                                    }
                                }
                            }
                            // Handle case where old symbol has exports but new doesn't yet
                            if old_sym.exports.is_some() && new_sym.exports.is_none() {
                                new_sym.exports = old_sym.exports.as_ref().map(|table| {
                                    Box::new(remap_symbol_table_required(table.as_ref(), &id_remap))
                                });
                            }
                            // Merge members (if both have members)
                            if let (Some(old_members), Some(new_members)) =
                                (old_sym.members.as_ref(), new_sym.members.as_mut())
                            {
                                for (name, sym_id) in old_members.iter() {
                                    if !new_members.has(name) {
                                        // Remap the symbol ID and add to members
                                        if let Some(&remapped_id) = id_remap.get(sym_id) {
                                            new_members.set(name.clone(), remapped_id);
                                        }
                                    }
                                }
                            }
                        } else {
                            // First time seeing this symbol - full update
                            let mut updated = old_sym.clone();
                            updated.id = new_id;
                            updated.parent = id_remap
                                .get(&old_sym.parent)
                                .copied()
                                .unwrap_or(SymbolId::NONE);
                            updated.value_declaration = old_sym.value_declaration;
                            updated.declarations = old_sym.declarations.clone();
                            updated.is_exported = old_sym.is_exported;
                            updated.is_umd_export = old_sym.is_umd_export;
                            // Track which file this symbol was declared in for TDZ cross-file detection
                            updated.decl_file_idx = file_idx as u32;
                            // Finalize file index on stable declaration locations that
                            // were recorded by per-file binders with `u32::MAX` (the
                            // parallel pipeline does not call `BinderState::set_file_idx`
                            // before binding). This keeps the Phase 1 stable-location
                            // invariants consistent with `decl_file_idx`.
                            let stamped = file_idx as u32;
                            for stable in &mut updated.stable_declarations {
                                stable.set_file_idx_if_unassigned(stamped);
                            }
                            updated
                                .stable_value_declaration
                                .set_file_idx_if_unassigned(stamped);
                            updated.exports = old_sym.exports.as_ref().map(|table| {
                                Box::new(remap_symbol_table_required(table.as_ref(), &id_remap))
                            });
                            updated.members = old_sym.members.as_ref().map(|table| {
                                Box::new(remap_symbol_table_required(table.as_ref(), &id_remap))
                            });
                            *new_sym = updated;
                        }
                    }

                    // Collect nested merges for processing AFTER all symbols are populated
                    all_nested_merges.extend(nested_merges);
                }

                // Process all nested merges now that every symbol has its data populated.
                // Uses a work queue to handle arbitrarily deep nesting (e.g.,
                // namespace A.B.C.D declared across files needs recursive merge).
                while let Some((existing_id, source_id)) = all_nested_merges.pop() {
                    // Collect data from source symbol first
                    let merge_data = self.global_symbols.get(source_id).map(|src| {
                        (
                            src.flags,
                            src.declarations.clone(),
                            src.value_declaration,
                            src.exports.as_ref().cloned(),
                            src.members.as_ref().cloned(),
                        )
                    });
                    if let Some((src_flags, src_decls, src_val_decl, src_exports, src_members)) =
                        merge_data
                        && let Some(dst) = self.global_symbols.get_mut(existing_id)
                    {
                        let can_merge = can_merge_symbols_cross_file(dst.flags, src_flags);
                        if !can_merge {
                            continue;
                        }
                        dst.flags |= src_flags;
                        // Propagate declaration_arenas from source to destination
                        // so the checker can find declarations from the merged file
                        for &decl_idx in &src_decls {
                            let cloned_arenas: Option<Vec<Arc<NodeArena>>> = self
                                .declaration_arenas
                                .get(&(source_id, decl_idx))
                                .map(|a| a.iter().cloned().collect());
                            if let Some(arenas) = cloned_arenas {
                                let target = self
                                    .declaration_arenas
                                    .entry((existing_id, decl_idx))
                                    .or_default();
                                for arena in arenas {
                                    target.push(arena);
                                }
                            }
                        }
                        // Also propagate symbol_arenas if source has one
                        let cloned_arena = self.symbol_arenas.get(&source_id).cloned();
                        if let Some(arena) = cloned_arena {
                            self.symbol_arenas.entry(existing_id).or_insert(arena);
                        }
                        append_unique_declarations(&mut dst.declarations, &src_decls);
                        if dst.value_declaration.is_none() && src_val_decl.is_some() {
                            dst.value_declaration = src_val_decl;
                        }
                        if let Some(src_exp) = src_exports {
                            let dst_exp = dst.exports.get_or_insert_with(|| {
                                Box::new(SymbolTable::with_capacity(src_exp.len()))
                            });
                            merge_symbol_table_collecting_conflicts(
                                dst_exp,
                                &src_exp,
                                &mut all_nested_merges,
                            );
                        }
                        if let Some(src_mem) = src_members {
                            let dst_mem = dst.members.get_or_insert_with(|| {
                                Box::new(SymbolTable::with_capacity(src_mem.len()))
                            });
                            merge_symbol_table_collecting_conflicts(
                                dst_mem,
                                &src_mem,
                                &mut all_nested_merges,
                            );
                        }
                    }
                }

                let mut remapped_node_symbols = FxHashMap::default();
                for (node_idx, old_sym_id) in result.node_symbols.iter() {
                    if let Some(&new_sym_id) = id_remap.get(old_sym_id) {
                        remapped_node_symbols.insert(*node_idx, new_sym_id);
                    }
                    // lib symbols are not in node_symbols: user-file binding accesses
                    // them by name lookup via file_locals, not by node mapping.
                }
                let mut remapped_file_locals = SymbolTable::with_capacity(result.file_locals.len());
                for (name, old_sym_id) in result.file_locals.iter() {
                    if let Some(&new_sym_id) = id_remap.get(old_sym_id) {
                        // User symbol - use remapped ID
                        remapped_file_locals.set(name.clone(), new_sym_id);
                        // Script-file top-levels are globally visible by default. For ordinary
                        // external modules, keep pure type-only top-level declarations file-scoped so
                        // unimported type aliases/interfaces do not leak across files. Value-bearing
                        // exports still stay visible because CommonJS/export-assignment and declaration
                        // emit paths rely on them being reachable cross-file. This shares the single
                        // `Symbol::is_cross_file_global` predicate with the checker's
                        // `global_file_locals_index` builders so both cross-file tables agree.
                        let sym_info = self.global_symbols.get(new_sym_id);
                        let is_umd = sym_info.is_some_and(|s| s.is_umd_export);
                        let is_declaration_file = result
                            .arena
                            .source_files
                            .first()
                            .is_some_and(|sf| sf.is_declaration_file);
                        let is_global_augmentation = result.global_augmentations.contains_key(name);
                        let is_truly_global = match sym_info {
                            Some(s) => s.is_cross_file_global(
                                result.is_external_module,
                                is_declaration_file,
                                is_global_augmentation,
                            ),
                            // When the merged symbol is unavailable, fall back to the
                            // flagless classification (equivalent to a symbol with no
                            // flags set), preserving the original behavior.
                            None => {
                                !result.is_external_module
                                    || is_declaration_file
                                    || is_global_augmentation
                            }
                        };
                        if is_truly_global {
                            // UMD namespace exports (`export as namespace Foo`) use
                            // "first in wins" semantics: when multiple modules declare
                            // the same UMD global name, the first one encountered is
                            // kept and subsequent ones are ignored. This matches tsc
                            // behavior. Non-UMD globals can safely overwrite because
                            // they are already merged to a single SymbolId by the
                            // merge phase.
                            if !is_umd || !self.globals.has(name) {
                                self.globals.set(name.clone(), new_sym_id);
                            }
                        }
                    } else {
                        let name_atom = self.name_interner.intern(name);
                        if let Some(&global_id) = self.lib_name_to_global.get(&name_atom) {
                            // Lib symbol - use the pre-remapped global ID
                            // Only add to file_locals, NOT to globals (lib symbols are accessed
                            // through lib_contexts in the checker, not through globals)
                            remapped_file_locals.set(name.clone(), global_id);
                        }
                    }
                }

                let mut remapped_scopes = Vec::with_capacity(result.scopes.len());
                for scope in result.scopes.iter() {
                    let mut table = SymbolTable::with_capacity(scope.table.len());
                    for (name, old_sym_id) in scope.table.iter() {
                        if let Some(&new_sym_id) = id_remap.get(old_sym_id) {
                            // User symbol - include in scope.
                            table.set(name.clone(), new_sym_id);
                        } else {
                            let name_atom = self.name_interner.intern(name);
                            if let Some(&global_id) = self.lib_name_to_global.get(&name_atom) {
                                // Preserve lib-backed scope entries exactly when they were present in
                                // the original binder. Dropping them during merge weakens same-file
                                // identifier resolution and forces later checker repair.
                                table.set(name.clone(), global_id);
                            }
                        }
                    }
                    remapped_scopes.push(Scope {
                        parent: scope.parent,
                        table,
                        kind: scope.kind,
                        container_node: scope.container_node,
                    });
                }

                patch_script_lib_interface_declaration_arenas_for_result(
                    result,
                    &remapped_file_locals,
                    &self.globals,
                    &self.global_lib_symbol_ids,
                    &mut self.declaration_arenas,
                );
                self.file_locals_list.push(remapped_file_locals);

                let mut remapped_declaration_arenas: DeclarationArenaMap = FxHashMap::default();
                for (&(old_sym_id, decl_idx), arenas) in result.declaration_arenas.iter() {
                    if result.lib_symbol_ids.contains(&old_sym_id) {
                        continue;
                    }
                    let has_non_local_arena = arenas
                        .iter()
                        .any(|arena| !Arc::ptr_eq(arena, &result.arena));
                    if !has_non_local_arena {
                        continue;
                    }
                    if let Some(&new_sym_id) = id_remap.get(&old_sym_id) {
                        remapped_declaration_arenas.insert((new_sym_id, decl_idx), arenas.clone());
                    }
                }
                let sym_to_decl_indices =
                    Arc::new(build_sym_to_decl_indices(&remapped_declaration_arenas));

                let symbols_with_non_local_declarations: FxHashSet<SymbolId> =
                    remapped_declaration_arenas
                        .keys()
                        .map(|&(sym_id, _)| sym_id)
                        .collect();

                let mut remapped_symbol_arenas: FxHashMap<SymbolId, Arc<NodeArena>> =
                    FxHashMap::default();
                for (&old_sym_id, arena) in result.symbol_arenas.iter() {
                    if let Some(&new_sym_id) = id_remap.get(&old_sym_id) {
                        let has_non_local_decl =
                            symbols_with_non_local_declarations.contains(&new_sym_id);
                        if has_non_local_decl || !Arc::ptr_eq(arena, &result.arena) {
                            remapped_symbol_arenas.insert(new_sym_id, Arc::clone(arena));
                        }
                    }
                }

                // Populate arena context for module augmentations
                let module_augmentations: FxHashMap<
                    String,
                    Vec<crate::binder::ModuleAugmentation>,
                > = result
                    .module_augmentations
                    .iter()
                    .map(|(spec, augs)| {
                        let arena = Arc::clone(&result.arena);
                        (
                            spec.clone(),
                            augs.iter()
                                .map(|aug| {
                                    crate::binder::ModuleAugmentation::with_arena(
                                        aug.name.clone(),
                                        aug.node,
                                        Arc::clone(&arena),
                                    )
                                })
                                .collect(),
                        )
                    })
                    .collect();

                self.files.push(BoundFile {
                    file_name: result.file_name.clone(),
                    source_file: result.source_file,
                    arena: Arc::clone(&result.arena),
                    // Wrap once here. `cross_file_node_symbols` (built later) takes
                    // an Arc::clone of `file.node_symbols`, so the underlying
                    // `FxHashMap<u32, SymbolId>` is shared via refcount instead of
                    // deep-cloned per consumer.
                    node_symbols: Arc::new(remapped_node_symbols),
                    symbol_arenas: Arc::new(remapped_symbol_arenas),
                    declaration_arenas: Arc::new(remapped_declaration_arenas),
                    sym_to_decl_indices,
                    module_declaration_exports_publicly: result
                        .module_declaration_exports_publicly
                        .clone(),
                    scopes: Arc::new(remapped_scopes),
                    node_scope_ids: result.node_scope_ids.clone(),
                    parse_diagnostics: result.parse_diagnostics.clone(),
                    global_augmentations: Arc::clone(&result.global_augmentations),
                    module_augmentations: Arc::new(module_augmentations),
                    augmentation_target_modules: Arc::new(
                        result
                            .augmentation_target_modules
                            .iter()
                            .map(|(&old_sym, name)| {
                                let new_sym = id_remap.get(&old_sym).copied().unwrap_or(old_sym);
                                (new_sym, name.clone())
                            })
                            .collect(),
                    ),
                    flow_nodes: result.flow_nodes.clone(),
                    // Arc::clone is O(1); per-file `BoundFile` shares the same
                    // `node_flow` map as later `cross_file_*` binder constructions.
                    node_flow: Arc::clone(&result.node_flow),
                    switch_clause_to_switch: result.switch_clause_to_switch.clone(),
                    is_external_module: result.is_external_module,
                    expando_properties: remap_expando_properties(
                        &result.expando_properties,
                        &id_remap,
                    ),
                    file_features: result.file_features,
                    lib_symbol_reverse_remap: Arc::new(
                        result
                            .lib_symbol_reverse_remap
                            .iter()
                            .filter_map(|(&old_sym, &(lib_idx, lib_local_sym))| {
                                id_remap
                                    .get(&old_sym)
                                    .copied()
                                    .map(|new_sym| (new_sym, (lib_idx, lib_local_sym)))
                            })
                            .collect(),
                    ),
                    semantic_defs: Arc::new(file_semantic_defs),
                });
            }
            results.release(file_idx);
        }

        // Build cross_file_node_symbols: map each arena pointer to its remapped node_symbols.
        // This enables the checker to resolve type references in cross-file interface declarations.
        // `file.node_symbols` is now `Arc<FxHashMap<...>>`, so cloning the Arc is an
        // O(1) refcount bump that shares the underlying map with the per-file
        // `BoundFile` instead of deep-cloning it.
        for file in &self.files {
            let arena_ptr = Arc::as_ptr(&file.arena) as usize;
            self.cross_file_node_symbols
                .insert(arena_ptr, Arc::clone(&file.node_symbols));
        }
    }

    fn finish(self, skeleton_index: SkeletonIndex, dep_graph: DepGraph) -> MergedProgram {
        // Validate skeleton data against legacy merge state before construction.
        // This runs only in debug builds and proves skeleton captures all
        // merge-relevant ambient module topology.
        {
            let user_file_names: FxHashSet<String> =
                self.files.iter().map(|f| f.file_name.clone()).collect();
            let module_export_keys: FxHashSet<String> =
                self.module_exports.keys().cloned().collect();
            skeleton_index.validate_against_merged(
                &self.declared_modules,
                &self.shorthand_ambient_modules,
                &module_export_keys,
                &user_file_names,
            );
        }

        // Pre-populate a shared DefinitionStore with DefIds for all semantic definitions.
        // This moves identity creation from the checker's per-file pre-population phase
        // (order-dependent, per-context) to merge time (single pass, deterministic).
        let type_interner = TypeInterner::new();
        let definition_store = std::sync::Arc::new(pre_populate_definition_store(
            &self.semantic_defs,
            &type_interner,
        ));

        // Build the secondary `sym_to_decl_indices` index over the program-wide
        // `declaration_arenas`. Checker paths that previously iterated every entry
        // filtering by `entry_sym_id == sym_id` use this to do a point lookup.
        let sym_to_decl_indices = build_sym_to_decl_indices(&self.declaration_arenas);

        MergedProgram {
            files: self.files,
            symbols: self.global_symbols,
            symbol_arenas: Arc::new(self.symbol_arenas),
            declaration_arenas: Arc::new(self.declaration_arenas),
            sym_to_decl_indices: Arc::new(sym_to_decl_indices),
            cross_file_node_symbols: Arc::new(self.cross_file_node_symbols),
            globals: self.globals,
            file_locals: self.file_locals_list,
            declared_modules: Arc::new(self.declared_modules),
            shorthand_ambient_modules: Arc::new(self.shorthand_ambient_modules),
            module_exports: Arc::new(self.module_exports),
            reexports: Arc::new(self.reexports),
            wildcard_reexports: Arc::new(self.wildcard_reexports),
            wildcard_reexports_type_only: Arc::new(self.wildcard_reexports_type_only),
            lib_binders: Arc::new(self.lib_binders),
            lib_symbol_ids: Arc::new(self.global_lib_symbol_ids),
            type_interner,
            alias_partners: Arc::new(self.alias_partners),
            semantic_defs: Arc::new(self.semantic_defs),
            definition_store,
            skeleton_index: Some(skeleton_index),
            dep_graph: Some(dep_graph),
            pre_merge_bind_total_bytes: self.pre_merge_bind_total_bytes,
        }
    }
}

fn merge_bind_results_from_source(results: &mut impl BindResultsSource) -> MergedProgram {
    let refs = results.refs();
    let skeletons = extract_skeletons_for_merge(&refs);
    drop(refs);
    let skeleton_index = reduce_skeletons(&skeletons);
    let dep_graph = DepGraph::build_simple(&skeletons);

    let mut reducer = BindResultReducer::new(results);
    reducer.merge_lib_binders();
    reducer.clear_unremapped_exports();
    reducer.remap_lib_references();
    reducer.propagate_lib_semantic_defs();
    reducer.merge_user_files(results);
    reducer.finish(skeleton_index, dep_graph)
}
