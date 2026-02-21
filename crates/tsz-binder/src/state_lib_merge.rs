//! Lib symbol merging and injection.
//!
//! This module handles merging lib.d.ts symbols into the binder's symbol arena,
//! including SymbolId remapping to avoid collisions across lib binders.

use crate::{SymbolId, SymbolTable, symbol_flags};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz_common::interner::{Atom, Interner};

use super::state::{BinderState, LibContext};

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
        self.resolved_identifier_cache.write().unwrap().clear();

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
                                if !existing.has(name) {
                                    existing.set(name.clone(), *id);
                                }
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
                                if !existing.has(name) {
                                    existing.set(name.clone(), *id);
                                }
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

        // Mark that lib symbols have been merged
        self.lib_symbols_merged = true;
    }

    /// Return the current length of the resolved identifier cache.
    ///
    /// # Panics
    ///
    /// Panics if the resolved identifier cache lock is poisoned.
    pub fn resolved_identifier_cache_len(&self) -> usize {
        self.resolved_identifier_cache.read().unwrap().len()
    }

    /// Inject lib file symbols into `file_locals` for global symbol resolution.
    ///
    /// This method now delegates to `merge_lib_contexts_into_binder` which properly
    /// remaps `SymbolIds` to avoid collisions across lib binders.
    ///
    /// # Arguments
    /// * `lib_contexts` - Vector of lib file contexts (arena + binder pairs)
    pub fn inject_lib_symbols(&mut self, lib_contexts: &[LibContext]) {
        self.merge_lib_contexts_into_binder(lib_contexts);
    }
}
