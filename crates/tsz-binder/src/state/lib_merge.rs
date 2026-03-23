//! Lib symbol merging and injection.
//!
//! This module handles merging lib.d.ts symbols into the binder's symbol arena,
//! including SymbolId remapping to avoid collisions across lib binders.

use crate::{SymbolId, SymbolTable, symbol_flags};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz_common::interner::{Atom, Interner};

use super::{BinderState, LibContext};

impl BinderState {
    // =========================================================================
    // Lib Symbol Merging (SymbolId collision fix)
    // =========================================================================

    /// Check if two symbols can be merged across different lib files or files.
    ///
    /// TypeScript allows merging:
    /// - Interface + Interface (declaration merging)
    /// - Namespace + Namespace (declaration merging)
    /// - Class + Interface (merging for class declarations)
    /// - Namespace + Class/Function/Enum (augmentation)
    /// - Enum + Enum (declaration merging)
    pub(crate) const fn can_merge_symbols(existing_flags: u32, new_flags: u32) -> bool {
        // Interface can merge with interface
        if (existing_flags & symbol_flags::INTERFACE) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0
        {
            return true;
        }

        // Class can merge with interface
        if ((existing_flags & symbol_flags::CLASS) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0)
            || ((existing_flags & symbol_flags::INTERFACE) != 0
                && (new_flags & symbol_flags::CLASS) != 0)
        {
            return true;
        }

        // Namespace/module can merge with namespace/module
        if (existing_flags & symbol_flags::MODULE) != 0 && (new_flags & symbol_flags::MODULE) != 0 {
            return true;
        }

        // Namespace can merge with class, function, or enum
        if (existing_flags & symbol_flags::MODULE) != 0
            && (new_flags & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM))
                != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::MODULE) != 0
            && (existing_flags
                & (symbol_flags::CLASS | symbol_flags::FUNCTION | symbol_flags::ENUM))
                != 0
        {
            return true;
        }

        // Namespace/module can merge with interface
        if (existing_flags & symbol_flags::MODULE) != 0
            && (new_flags & symbol_flags::INTERFACE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::MODULE) != 0
            && (existing_flags & symbol_flags::INTERFACE) != 0
        {
            return true;
        }

        // Enum can merge with enum
        if (existing_flags & symbol_flags::ENUM) != 0 && (new_flags & symbol_flags::ENUM) != 0 {
            return true;
        }

        // Functions declared across lib files contribute overloads to the same symbol.
        if (existing_flags & symbol_flags::FUNCTION) != 0
            && (new_flags & symbol_flags::FUNCTION) != 0
        {
            return true;
        }

        // Methods declared across lib files also contribute overloads (e.g. Intl augmentations).
        if (existing_flags & symbol_flags::METHOD) != 0 && (new_flags & symbol_flags::METHOD) != 0 {
            return true;
        }

        // Interface can merge with VALUE symbols (e.g., `interface Promise<T>` + `declare var Promise`)
        // This enables global types like Object, Array, Promise to be used as both types and constructors
        if (existing_flags & symbol_flags::INTERFACE) != 0 && (new_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }
        if (new_flags & symbol_flags::INTERFACE) != 0 && (existing_flags & symbol_flags::VALUE) != 0
        {
            return true;
        }

        false
    }

    /// Merge lib contexts into this binder's symbol arena with remapped IDs.
    ///
    /// This is the core fix for `SymbolId` collisions across lib binders. Instead of
    /// storing raw lib `SymbolIds` (which collide), we:
    /// 1. Clone each lib symbol into our local symbol arena with a new unique ID
    /// 2. Remap internal references (parent, exports, members) to use new IDs
    /// 3. Update `file_locals` to use the new IDs
    /// 4. Track which arena each symbol's declarations belong to
    ///
    /// After this method, all symbol lookups can use our local arena directly,
    /// avoiding cross-binder ID collisions.
    ///
    /// # Panics
    ///
    /// Panics if the resolved identifier cache lock is poisoned.
    pub fn merge_lib_contexts_into_binder(&mut self, lib_contexts: &[LibContext]) {
        // Visible globals can change after merge; invalidate identifier resolutions.
        self.resolved_identifier_cache
            .write()
            .expect("RwLock not poisoned")
            .clear();

        if lib_contexts.is_empty() {
            return;
        }

        // Phase 1: Clone all lib symbols into local arena, building remap maps
        // Maps: (lib_binder_ptr, old_id) -> new_id
        let mut lib_symbol_remap: FxHashMap<(usize, SymbolId), SymbolId> = FxHashMap::default();
        // Maps: interned symbol name -> new_id (for merging same-name symbols)
        let mut name_interner = Interner::new();
        let mut merged_by_name: FxHashMap<Atom, SymbolId> = FxHashMap::default();

        for lib_ctx in lib_contexts {
            let lib_binder_ptr = Arc::as_ptr(&lib_ctx.binder) as usize;

            // Process all symbols in this lib binder
            for i in 0..lib_ctx.binder.symbols.len() {
                let Ok(local_idx) = u32::try_from(i) else {
                    continue;
                };
                let local_id = SymbolId(local_idx);
                let Some(lib_sym) = lib_ctx.binder.symbols.get(local_id) else {
                    continue;
                };

                // Check if a symbol with this name already exists (cross-lib merging)
                let name_atom = name_interner.intern(&lib_sym.escaped_name);
                let new_id = if let Some(&existing_id) = merged_by_name.get(&name_atom) {
                    // Symbol already exists - check if we can merge
                    if let Some(existing_sym) = self.symbols.get(existing_id) {
                        if Self::can_merge_symbols(existing_sym.flags, lib_sym.flags) {
                            // Merge: reuse existing symbol ID, merge declarations
                            if let Some(existing_mut) = self.symbols.get_mut(existing_id) {
                                existing_mut.flags |= lib_sym.flags;
                                for &decl in &lib_sym.declarations {
                                    if !existing_mut.declarations.contains(&decl) {
                                        existing_mut.declarations.push(decl);
                                    }
                                    // Always track the arena — multiple lib files may
                                    // reuse the same NodeIndex (cross-arena collision)
                                    self.declaration_arenas
                                        .entry((existing_id, decl))
                                        .or_default()
                                        .push(Arc::clone(&lib_ctx.arena));
                                }
                                // Update value_declaration if not set
                                if existing_mut.value_declaration.is_none()
                                    && lib_sym.value_declaration.is_some()
                                {
                                    existing_mut.value_declaration = lib_sym.value_declaration;
                                }
                            }
                            existing_id
                        } else {
                            // Cannot merge - allocate new (shadowing)
                            let new_id = self.symbols.alloc_from(lib_sym);
                            merged_by_name.insert(name_atom, new_id);
                            // Track declaration arenas for new symbol
                            for &decl in &lib_sym.declarations {
                                self.declaration_arenas
                                    .entry((new_id, decl))
                                    .or_default()
                                    .push(Arc::clone(&lib_ctx.arena));
                            }
                            new_id
                        }
                    } else {
                        // Shouldn't happen - allocate new
                        let new_id = self.symbols.alloc_from(lib_sym);
                        merged_by_name.insert(name_atom, new_id);
                        // Track declaration arenas for new symbol
                        for &decl in &lib_sym.declarations {
                            self.declaration_arenas
                                .entry((new_id, decl))
                                .or_default()
                                .push(Arc::clone(&lib_ctx.arena));
                        }
                        new_id
                    }
                } else {
                    // New symbol - allocate in local arena
                    let new_id = self.symbols.alloc_from(lib_sym);
                    merged_by_name.insert(name_atom, new_id);
                    // Track declaration arenas for new symbol
                    for &decl in &lib_sym.declarations {
                        self.declaration_arenas
                            .entry((new_id, decl))
                            .or_default()
                            .push(Arc::clone(&lib_ctx.arena));
                    }
                    new_id
                };

                // Store the remapping
                lib_symbol_remap.insert((lib_binder_ptr, local_id), new_id);

                // Store reverse mapping for Phase 2 of merge_bind_results
                self.lib_symbol_reverse_remap
                    .insert(new_id, (lib_binder_ptr, local_id));

                // Track which arena contains this symbol's declarations (legacy - stores last arena)
                self.symbol_arenas
                    .insert(new_id, Arc::clone(&lib_ctx.arena));
            }
        }

        // Phase 2: Remap internal references (parent, exports, members)
        // We need a second pass because parents/exports/members may reference symbols
        // that were processed later in the first pass.
        for lib_ctx in lib_contexts {
            let lib_binder_ptr = Arc::as_ptr(&lib_ctx.binder) as usize;

            for i in 0..lib_ctx.binder.symbols.len() {
                let Ok(local_idx) = u32::try_from(i) else {
                    continue;
                };
                let local_id = SymbolId(local_idx);
                let Some(&new_id) = lib_symbol_remap.get(&(lib_binder_ptr, local_id)) else {
                    continue;
                };
                let Some(lib_sym) = lib_ctx.binder.symbols.get(local_id) else {
                    continue;
                };

                // Remap parent
                if lib_sym.parent.is_some()
                    && let Some(&new_parent) =
                        lib_symbol_remap.get(&(lib_binder_ptr, lib_sym.parent))
                    && let Some(sym) = self.symbols.get_mut(new_id)
                {
                    sym.parent = new_parent;
                }

                // Remap exports
                if let Some(exports) = &lib_sym.exports {
                    let mut remapped_exports = SymbolTable::new();
                    for (name, &export_id) in exports.iter() {
                        if let Some(&new_export_id) =
                            lib_symbol_remap.get(&(lib_binder_ptr, export_id))
                        {
                            remapped_exports.set(name.clone(), new_export_id);
                        }
                    }
                    if !remapped_exports.is_empty()
                        && let Some(sym) = self.symbols.get_mut(new_id)
                    {
                        if sym.exports.is_none() {
                            sym.exports = Some(Box::new(remapped_exports));
                        } else if let Some(existing) = sym.exports.as_mut() {
                            for (name, id) in remapped_exports.iter() {
                                // Always overwrite: Phase 1's alloc_from copies exports
                                // with un-remapped SymbolIds. We must replace them with
                                // the remapped IDs from lib_symbol_remap.
                                existing.set(name.clone(), *id);
                            }
                        }
                    }
                }

                // Remap members
                if let Some(members) = &lib_sym.members {
                    let mut remapped_members = SymbolTable::new();
                    for (name, &member_id) in members.iter() {
                        if let Some(&new_member_id) =
                            lib_symbol_remap.get(&(lib_binder_ptr, member_id))
                        {
                            remapped_members.set(name.clone(), new_member_id);
                        }
                    }
                    if !remapped_members.is_empty()
                        && let Some(sym) = self.symbols.get_mut(new_id)
                    {
                        if sym.members.is_none() {
                            sym.members = Some(Box::new(remapped_members));
                        } else if let Some(existing) = sym.members.as_mut() {
                            for (name, id) in remapped_members.iter() {
                                // Always overwrite: same reasoning as exports above.
                                existing.set(name.clone(), *id);
                            }
                        }
                    }
                }
            }
        }

        // Phase 3: Update file_locals with remapped IDs and track lib symbol IDs
        for lib_ctx in lib_contexts {
            let lib_binder_ptr = Arc::as_ptr(&lib_ctx.binder) as usize;

            for (name, &local_id) in lib_ctx.binder.file_locals.iter() {
                if let Some(&new_id) = lib_symbol_remap.get(&(lib_binder_ptr, local_id)) {
                    // Only add if not already present (user symbols take precedence)
                    if !self.file_locals.has(name) {
                        self.file_locals.set(name.clone(), new_id);
                    }
                    // Track all lib-originating symbols for unused checking exclusion
                    self.lib_symbol_ids.insert(new_id);
                }
            }
        }

        // Phase 4: Propagate semantic_defs from lib binders with remapped SymbolIds.
        //
        // Lib binders record `semantic_defs` for their top-level declarations during
        // binding (TypeAlias, Interface, Class, Enum, Namespace). After Phase 1
        // remaps SymbolIds, the main binder's `semantic_defs` doesn't know about
        // these merged lib symbols. Without this, `pre_populate_def_ids_from_binder`
        // only covers user-declared types, and lib symbols fall through to the
        // `get_or_create_def_id` repair path in the checker.
        //
        // By propagating here, the main binder's `semantic_defs` includes all
        // merged lib symbols, so the checker's `pre_populate_def_ids_from_binder`
        // creates stable DefIds for them at construction time.
        for lib_ctx in lib_contexts {
            let lib_binder_ptr = Arc::as_ptr(&lib_ctx.binder) as usize;

            for (&old_sym_id, entry) in &lib_ctx.binder.semantic_defs {
                if let Some(&new_id) = lib_symbol_remap.get(&(lib_binder_ptr, old_sym_id)) {
                    // Update file_id to match the remapped symbol's decl_file_idx
                    // so that DefinitionStore composite key lookups stay consistent.
                    let file_id = self
                        .symbols
                        .get(new_id)
                        .map_or(entry.file_id, |s| s.decl_file_idx);
                    // Remap parent_namespace through the lib symbol remap.
                    let remapped_parent = entry.parent_namespace.and_then(|old_parent| {
                        lib_symbol_remap.get(&(lib_binder_ptr, old_parent)).copied()
                    });
                    let remapped = super::SemanticDefEntry {
                        kind: entry.kind,
                        name: entry.name.clone(),
                        file_id,
                        span_start: entry.span_start,
                        type_param_count: entry.type_param_count,
                        type_param_names: entry.type_param_names.clone(),
                        is_exported: entry.is_exported,
                        enum_member_names: entry.enum_member_names.clone(),
                        is_const: entry.is_const,
                        is_abstract: entry.is_abstract,
                        extends_names: entry.extends_names.clone(),
                        implements_names: entry.implements_names.clone(),
                        parent_namespace: remapped_parent,
                        is_global_augmentation: entry.is_global_augmentation,
                    };
                    if let Some(existing) = self.semantic_defs.get_mut(&new_id) {
                        // User-declared entries take precedence for core identity,
                        // but accumulate heritage/members/exports from lib declarations.
                        existing.merge_cross_file(&remapped);
                    } else {
                        self.semantic_defs.insert(new_id, remapped);
                    }
                }
            }
        }

        // Mark that lib symbols have been merged
        self.lib_symbols_merged = true;
    }

    /// Return the current length of the resolved identifier cache.
    ///
    /// Test-only introspection method for validating caching behavior.
    pub fn resolved_identifier_cache_len(&self) -> usize {
        self.resolved_identifier_cache
            .read()
            .expect("RwLock not poisoned")
            .len()
    }
}
