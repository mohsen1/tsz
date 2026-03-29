//! Cross-file symbol resolution: resolving symbols across multiple files,
//! delegating type resolution to child checkers, tracking cross-file targets,
//! and cross-file interface declaration merging.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get a symbol from the current binder, lib binders, or other file binders.
    /// This ensures we can resolve symbols from lib.d.ts and other files.
    pub(crate) fn get_symbol_globally(&self, sym_id: SymbolId) -> Option<&tsz_binder::Symbol> {
        // 1. Check current file
        if let Some(sym) = self.ctx.binder.get_symbol(sym_id) {
            return Some(sym);
        }
        // 2. Check lib files (lib.d.ts, etc.)
        for lib in &self.ctx.lib_contexts {
            if let Some(sym) = lib.binder.get_symbol(sym_id) {
                return Some(sym);
            }
        }
        // 3. O(1) fast-path: if this SymbolId was already resolved to a specific
        //    file via resolve_symbol_file_index, go directly to that binder.
        {
            let file_idx = self.ctx.resolve_symbol_file_index(sym_id);
            if let Some(file_idx) = file_idx
                && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
                && let Some(sym) = binder.get_symbol(sym_id)
            {
                return Some(sym);
            }
        }
        // 4. Fallback: O(N) scan over all binders
        if let Some(binders) = &self.ctx.all_binders {
            for binder in binders.iter() {
                if let Some(sym) = binder.get_symbol(sym_id) {
                    return Some(sym);
                }
            }
        }
        None
    }

    /// Get a symbol, preferring the cross-file binder for known cross-file `SymbolIds`.
    ///
    /// Unlike `get_symbol_globally` (which checks the local binder first and may find
    /// a WRONG symbol due to `SymbolId` collisions), this method checks
    /// `cross_file_symbol_targets` FIRST. If the `SymbolId` is known to belong to another
    /// file, the target file's binder is used directly, avoiding the collision.
    ///
    /// Falls back to `get_symbol_globally` for non-cross-file symbols.
    pub(crate) fn get_cross_file_symbol(&self, sym_id: SymbolId) -> Option<&tsz_binder::Symbol> {
        // Check if this is a known cross-file symbol
        let file_idx = self.ctx.resolve_symbol_file_index(sym_id);
        if let Some(file_idx) = file_idx
            && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
            && let Some(sym) = binder.get_symbol(sym_id)
        {
            return Some(sym);
        }
        // Fall back to global search
        self.get_symbol_globally(sym_id)
    }

    /// Delegate symbol resolution to a checker using the correct arena.
    ///
    /// When a symbol's arena differs from the current arena (cross-file symbol),
    /// we create a child checker with the correct arena and delegate the resolution.
    /// This ensures symbols are resolved in their original context.
    ///
    /// ## Returns:
    /// - `Some((type_id, params))`: Delegation occurred, use this result
    /// - `None`: Symbol is in the local arena, proceed with local computation
    ///
    /// ## Critical Behavior:
    /// - Removes the "in-progress" ERROR marker from cache before delegation
    /// - Shares the parent's cache via `with_parent_cache` (fixes Cache Isolation Bug)
    /// - Copies `lib_contexts` for global symbol resolution (Array, Promise, etc.)
    /// - Copies resolution sets for cross-file cycle detection
    pub(crate) fn delegate_cross_arena_symbol_resolution(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        // Fast path: if this is a known cross-file symbol, skip the namespace guard
        // (which would check the wrong symbol in the current binder) and go straight
        // to cross-file delegation.
        //
        // TYPE_ALIAS + value merge fix: When a user-defined type alias (e.g., `type Proxy<T>`)
        // has the same name as a global value (`declare var Proxy: ProxyConstructor`), the
        // merged symbol has both TYPE_ALIAS and value flags, and symbol_arenas may point to
        // the lib arena. Delegating to the lib arena loses the type alias declaration (which
        // lives in the user arena), causing property access on the instantiated type to fail.
        // If the type alias declaration exists in the current arena, handle it locally.
        {
            let sym_found = self.get_symbol_globally(sym_id);
            let has_type_alias = sym_found.is_some_and(|s| s.flags & symbol_flags::TYPE_ALIAS != 0);
            if has_type_alias {
                let symbol = sym_found.expect("has_type_alias guard ensures sym_found is Some");
                tracing::debug!(
                    sym_id = sym_id.0,
                    name = %symbol.escaped_name,
                    num_decls = symbol.declarations.len(),
                    arena_len = self.ctx.arena.len(),
                    "delegate_cross_arena: checking TYPE_ALIAS in current arena"
                );
                let has_type_alias_in_current_arena = symbol.declarations.iter().any(|&d| {
                    self.ctx
                        .arena
                        .get(d)
                        .and_then(|n| {
                            if n.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                                // Verify the name matches to prevent NodeIndex collisions:
                                // A lib NodeIndex may accidentally map to a different
                                // TYPE_ALIAS_DECLARATION in the user arena.
                                let type_alias = self.ctx.arena.get_type_alias(n)?;
                                let name_node = self.ctx.arena.get(type_alias.name)?;
                                let ident = self.ctx.arena.get_identifier(name_node)?;
                                let name = self.ctx.arena.resolve_identifier_text(ident);
                                Some(name == symbol.escaped_name.as_str())
                            } else {
                                Some(false)
                            }
                        })
                        .unwrap_or(false)
                });
                tracing::debug!(
                    sym_id = sym_id.0,
                    name = %symbol.escaped_name,
                    has_type_alias_in_current_arena,
                    "delegate_cross_arena: TYPE_ALIAS check result"
                );
                if has_type_alias_in_current_arena {
                    return None; // Handle locally, don't delegate to lib arena
                }
            }
        }
        let is_known_cross_file = self.ctx.has_symbol_file_index(sym_id);

        if !is_known_cross_file
            && let Some(symbol) = self.get_symbol_globally(sym_id)
            && (symbol.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)) != 0
        {
            return None;
        }

        let mut delegate_arena: Option<&tsz_parser::NodeArena> = self
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref);

        // For INTERFACE symbols that have local (user) interface declarations in the
        // current arena, do NOT delegate to the lib arena. The user's interface body
        // must be merged with the lib type, and delegating would lose the user's
        // members (e.g., `interface Node { forEachChild(...) }` augments lib Node).
        // The INTERFACE block in compute_type_of_symbol handles multi-arena merging.
        if delegate_arena.is_some_and(|arena| !std::ptr::eq(arena, self.ctx.arena))
            && let Some(symbol) = self.get_symbol_globally(sym_id)
            && (symbol.flags & symbol_flags::INTERFACE) != 0
        {
            let has_local_interface = symbol.declarations.iter().any(|&d| {
                self.ctx
                    .arena
                    .get(d)
                    .and_then(|n| self.ctx.arena.get_interface(n))
                    .is_some()
            });
            if has_local_interface {
                delegate_arena = None; // Handle locally with merge
            }
        }

        if delegate_arena.is_none_or(|arena| std::ptr::eq(arena, self.ctx.arena))
            && let Some(symbol) = self.get_symbol_globally(sym_id)
        {
            // For INTERFACE symbols whose primary arena is already the current arena,
            // do NOT scan per-declaration arenas for delegation. Interfaces split across
            // multiple lib files (e.g., RegExp in es5 + es2015.symbol.wellknown) cause
            // ping-pong between arenas until the depth limit, resulting in ERROR.
            // The INTERFACE block in compute_type_of_symbol handles multi-arena merging
            // correctly via resolve_lib_type_by_name.
            if symbol.flags & symbol_flags::INTERFACE == 0 {
                let mut decl_candidates = symbol.declarations.clone();
                if symbol.value_declaration.is_some() {
                    decl_candidates.push(symbol.value_declaration);
                }

                for decl_idx in decl_candidates {
                    if decl_idx.is_none() {
                        continue;
                    }
                    if let Some(arena) = self
                        .ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                        .and_then(|v| v.first())
                        && !std::ptr::eq(arena.as_ref(), self.ctx.arena)
                    {
                        delegate_arena = Some(arena.as_ref());
                        break;
                    }
                }
            }
        }

        // Check cross-file symbol target mapping as fallback.
        // When resolve_cross_file_export returns a SymbolId from another file's binder,
        // it records the target file index. Use that to find the correct arena AND binder.
        let mut cross_file_idx: Option<usize> = None;
        let needs_cross_file_delegation = delegate_arena
            .is_none_or(|arena| std::ptr::eq(arena, self.ctx.arena))
            && self
                .ctx
                .resolve_symbol_file_index(sym_id)
                .is_some_and(|file_idx| {
                    let target_arena = self.ctx.get_arena_for_file(file_idx as u32);
                    !std::ptr::eq(target_arena, self.ctx.arena)
                });

        if needs_cross_file_delegation {
            let file_idx = self.ctx.resolve_symbol_file_index(sym_id).expect(
                "needs_cross_file_delegation derived from has_symbol_file_index returning true",
            );
            cross_file_idx = Some(file_idx);
        }

        // Check if we have a valid delegate arena (either from symbol_arenas/declaration_arenas
        // or from resolve_symbol_file_index).
        let should_delegate = if needs_cross_file_delegation {
            true
        } else {
            delegate_arena.is_some_and(|arena| !std::ptr::eq(arena, self.ctx.arena))
        };

        if should_delegate {
            // Fast path: check lib delegation cache by SymbolId.
            // Each lib SymbolId is delegated at most once; subsequent lookups
            // return the cached result directly.
            if !needs_cross_file_delegation
                && let Some(&cached_type) = self.ctx.lib_delegation_cache.get(&sym_id)
            {
                self.ctx.symbol_types.insert(sym_id, cached_type);
                return Some((cached_type, Vec::new()));
            }

            // Guard against deep cross-arena recursion to prevent stack overflow.
            // Uses shared thread-local counter across all delegation points.
            if !Self::enter_cross_arena_delegation() {
                self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
                return Some((TypeId::ERROR, Vec::new()));
            }

            // Also check the per-checker recursion guard
            if !self.ctx.enter_recursion() {
                Self::leave_cross_arena_delegation();
                self.ctx.symbol_types.insert(sym_id, TypeId::ERROR);
                return Some((TypeId::ERROR, Vec::new()));
            }

            // Remove the in-progress ERROR marker before delegating to child checker.
            // The parent pre-caches ERROR as a cycle-detection marker and we don't
            // want the child checker to observe that placeholder.
            self.ctx.symbol_types.remove(&sym_id);

            // Re-fetch the arena reference after mutable operations above.
            // For cross-file symbols, use the target file's arena and binder.
            let (symbol_arena, delegate_binder, delegate_file_idx) =
                if let Some(file_idx) = cross_file_idx {
                    let arena = self.ctx.get_arena_for_file(file_idx as u32);
                    let binder = self
                        .ctx
                        .get_binder_for_file(file_idx)
                        .unwrap_or(self.ctx.binder);
                    (arena, binder, Some(file_idx))
                } else {
                    // Non-cross-file delegation: use the already-computed arena.
                    let arena = delegate_arena.unwrap_or(self.ctx.arena);
                    let binder = if std::ptr::eq(arena, self.ctx.arena) {
                        self.ctx.binder
                    } else {
                        self.ctx
                            .get_binder_for_arena(arena)
                            .unwrap_or(self.ctx.binder)
                    };
                    let file_idx = if std::ptr::eq(arena, self.ctx.arena) {
                        Some(self.ctx.current_file_idx)
                    } else {
                        self.ctx.get_file_idx_for_arena(arena)
                    };
                    (arena, binder, file_idx)
                };

            // Use the target file's name so that file-type-sensitive checks
            // (e.g. is_js_file() for optional JS parameters) use the declaring
            // file's context rather than the calling file's context.
            let delegate_file_name = symbol_arena
                .source_files
                .first()
                .map(|sf| sf.file_name.clone())
                .unwrap_or_else(|| self.ctx.file_name.clone());

            // Box the child checker to keep it on the heap — nested delegations for
            // interdependent lib types (Array → ReadonlyArray → Iterator → ...) can
            // create deep call stacks, and CheckerState is too large to stack-allocate
            // at every level without risking stack overflow.
            let mut checker = Box::new(CheckerState::with_parent_cache(
                symbol_arena,
                delegate_binder,
                self.ctx.types,
                delegate_file_name,
                self.ctx.compiler_options.clone(),
                self, // Share parent's cache to fix Cache Isolation Bug
            ));
            // Copy lib contexts for global symbol resolution (Array, Promise, etc.)
            checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
            // Copy all cross-file state: arenas, binders, all 6 global indices,
            // resolved_module_paths, and module_specifiers.
            checker.ctx.copy_cross_file_state_from(&self.ctx);
            // Copy cross-file symbol targets (local overlay only; global index
            // is already shared via copy_cross_file_state_from)
            self.ctx.copy_symbol_file_targets_to(&mut checker.ctx);
            checker.ctx.current_file_idx = delegate_file_idx.unwrap_or(self.ctx.current_file_idx);
            // Copy symbol resolution state to detect cross-file cycles, but exclude
            // the current symbol (which the parent added) since this checker will
            // add it again during get_type_of_symbol
            for &id in &self.ctx.symbol_resolution_set {
                if id != sym_id {
                    checker.ctx.symbol_resolution_set.insert(id);
                }
            }
            // DefId ↔ SymbolId mappings are no longer copied from parent to child.
            // The child's `def_to_symbol_id()` and `get_existing_def_id()` methods
            // fall back to the shared `DefinitionStore` on local cache miss, which
            // contains all mappings registered by any checker context. This enables
            // cross-file circular reference detection (e.g., `is_direct_circular_reference`)
            // without the O(N) copy overhead.

            // Copy class_instance_resolution_set to detect circular class inheritance
            for &id in &self.ctx.class_instance_resolution_set {
                checker.ctx.class_instance_resolution_set.insert(id);
            }
            // Copy class_constructor_resolution_set to detect circular constructor resolution
            for &id in &self.ctx.class_constructor_resolution_set {
                checker.ctx.class_constructor_resolution_set.insert(id);
            }
            // Use get_type_of_symbol to ensure proper cycle detection.
            let result = checker.get_type_of_symbol(sym_id);

            // Collect child data before dropping (child borrows from self.ctx.types).

            // Merge child's symbol_types back to parent to avoid re-resolving the
            // same types across delegations.  Without this, multi-file tests with
            // complex type libraries (react.d.ts) hang due to O(K×N) rework.
            //
            // For cross-file delegations (correct binder+arena pairing), ALL entries
            // are safe to merge.  For lib delegations, the child uses the parent's
            // binder with a lib arena, so entries for SymbolIds that belong to the
            // parent's binder may be corrupt (node index collision).  We filter those
            // out by only merging SymbolIds that the parent's binder doesn't own.
            let child_symbol_types: Vec<(SymbolId, TypeId)> = if needs_cross_file_delegation {
                // Cross-file: safe to merge everything
                checker.ctx.symbol_types.iter().collect()
            } else {
                // Lib delegation: only merge entries for MERGED lib SymbolIds.
                // During lib merge, symbols get new IDs tracked in
                // `lib_symbol_reverse_remap`. Entries for SymbolIds NOT in that
                // map belong to the parent binder's own symbols — they collide
                // with lib arena indices and may carry wrong types.
                checker
                    .ctx
                    .symbol_types
                    .iter()
                    .filter(|(k, _)| self.ctx.binder.lib_symbol_reverse_remap.contains_key(k))
                    .collect()
            };

            // def_to_symbol and def_type_params are no longer collected from the
            // child for merge-back. The child's `get_or_create_def_id()` and
            // `insert_def_type_params()` write through to the shared
            // `DefinitionStore`, so the parent can read them on next access via
            // the fallback path in `def_to_symbol_id()` and `get_def_type_params()`.

            let child_type_env = checker.ctx.type_env.borrow().clone();

            let child_namespace_names: rustc_hash::FxHashMap<TypeId, String> =
                std::mem::take(&mut checker.ctx.namespace_module_names);

            let child_lib_delegation_cache: Vec<(SymbolId, TypeId)> =
                std::mem::take(&mut checker.ctx.lib_delegation_cache)
                    .into_iter()
                    .collect();

            // Propagate lib type resolution cache from child to parent.
            // Without this, child contexts that resolve lib types (Array, Promise, etc.)
            // lose those cached results, forcing the parent to re-resolve them.
            let child_lib_type_cache: Vec<(String, Option<TypeId>)> =
                std::mem::take(&mut checker.ctx.lib_type_resolution_cache)
                    .into_iter()
                    .collect();

            // Collect circular type alias markers so the parent can detect
            // cross-file cycles.  When the child resolves `type B = A` and
            // finds A in the resolution set (from the parent), it marks A as
            // circular.  Propagating this back lets the parent's TS2456 check
            // for A fire correctly.
            let child_circular_aliases: Vec<SymbolId> =
                checker.ctx.circular_type_aliases.iter().copied().collect();

            // Propagate class instance types so that type-position references
            // (e.g., `foo(): Cls`) can resolve the instance type without
            // re-computing it from the class declaration (which lives in a
            // different arena and would fail).
            let child_instance_types: Vec<(SymbolId, TypeId)> =
                checker.ctx.symbol_instance_types.iter().collect();

            // Drop child checker to release borrow on self.ctx.types.
            drop(checker);

            // Merge collected data into the parent.
            // Note: def_to_symbol and def_type_params are NOT merged back here.
            // The child already wrote through to the shared DefinitionStore, and
            // the parent reads from DefinitionStore on local cache miss.
            for (sym_id, type_id) in child_symbol_types {
                self.ctx.symbol_types.entry_or_insert(sym_id, type_id);
            }
            {
                let mut parent_env = self.ctx.type_env.borrow_mut();
                child_type_env.merge_defs_into(&mut parent_env);
            }
            self.ctx
                .namespace_module_names
                .extend(child_namespace_names);
            for (name, type_id) in child_lib_delegation_cache {
                self.ctx.lib_delegation_cache.entry(name).or_insert(type_id);
            }
            for (name, type_id) in child_lib_type_cache {
                self.ctx
                    .lib_type_resolution_cache
                    .entry(name)
                    .or_insert(type_id);
            }
            for sym in child_circular_aliases {
                self.ctx.circular_type_aliases.insert(sym);
            }
            for (sym_id, inst_type) in child_instance_types {
                self.ctx
                    .symbol_instance_types
                    .entry_or_insert(sym_id, inst_type);
            }

            // Cache the result for lib delegations by SymbolId.
            // This prevents redundant child checker creation for the same lib symbol.
            if !needs_cross_file_delegation {
                self.ctx.lib_delegation_cache.insert(sym_id, result);
            }

            self.ctx.leave_recursion();
            Self::leave_cross_arena_delegation();
            return Some((result, Vec::new()));
        }

        None
    }

    /// Delegate class instance type resolution to a child checker with the correct arena.
    ///
    /// When a class symbol's declaration is not in the current file's arena (cross-file case),
    /// this creates a child checker using the symbol's home arena and computes the instance
    /// type there, where the class declaration node is accessible.
    pub(crate) fn delegate_cross_arena_class_instance_type(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        // Find the symbol's home arena
        let delegate_arena: Option<&tsz_parser::NodeArena> = self
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref);

        let symbol_arena = delegate_arena.filter(|arena| !std::ptr::eq(*arena, self.ctx.arena))?;

        // Guard against deep cross-arena recursion
        if !Self::enter_cross_arena_delegation() {
            return None;
        }

        if !self.ctx.enter_recursion() {
            Self::leave_cross_arena_delegation();
            return None;
        }

        // Use the target arena's file name for correct is_js_file() detection.
        let delegate_file_name = symbol_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
            .unwrap_or_else(|| self.ctx.file_name.clone());

        // Use the target file's binder when available so that node→symbol
        // lookups (e.g. `get_node_symbol` for private member `parent_id`)
        // resolve correctly instead of returning `None`.
        let delegate_binder = self
            .ctx
            .get_binder_for_arena(symbol_arena)
            .unwrap_or(self.ctx.binder);

        let mut checker = Box::new(CheckerState::with_parent_cache(
            symbol_arena,
            delegate_binder,
            self.ctx.types,
            delegate_file_name,
            self.ctx.compiler_options.clone(),
            self,
        ));
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        for &id in &self.ctx.class_instance_resolution_set {
            checker.ctx.class_instance_resolution_set.insert(id);
        }
        for &id in &self.ctx.symbol_resolution_set {
            if id != sym_id {
                checker.ctx.symbol_resolution_set.insert(id);
            }
        }
        // DefId ↔ SymbolId mappings are resolved via DefinitionStore fallback
        // on cache miss — no parent-to-child copy needed.
        for &id in &self.ctx.class_constructor_resolution_set {
            checker.ctx.class_constructor_resolution_set.insert(id);
        }

        let result = checker.class_instance_type_with_params_from_symbol(sym_id);

        self.ctx.leave_recursion();
        Self::leave_cross_arena_delegation();

        result
    }

    /// Delegate interface type resolution to a child checker with the symbol's home arena.
    ///
    /// When `type_reference_symbol_type` encounters a cross-file INTERFACE symbol
    /// whose declarations are in a different arena, `get_type_of_symbol` returns UNKNOWN.
    /// This function creates a child checker with the correct arena and resolves the
    /// interface type there.
    pub(crate) fn delegate_cross_arena_interface_type(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<TypeId> {
        // Prefer the symbol's declared arena, but fall back to explicit
        // cross-file ownership when the current binder does not know it.
        let mut delegate_arena: Option<&tsz_parser::NodeArena> = self
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref);
        let mut delegate_file_idx = None;

        let needs_cross_file_delegation = delegate_arena
            .is_none_or(|arena| std::ptr::eq(arena, self.ctx.arena))
            && self
                .ctx
                .resolve_symbol_file_index(sym_id)
                .is_some_and(|file_idx| {
                    let target_arena = self.ctx.get_arena_for_file(file_idx as u32);
                    !std::ptr::eq(target_arena, self.ctx.arena)
                });

        if needs_cross_file_delegation {
            let file_idx = self.ctx.resolve_symbol_file_index(sym_id).expect(
                "needs_cross_file_delegation derived from has_symbol_file_index returning true",
            );
            delegate_arena = Some(self.ctx.get_arena_for_file(file_idx as u32));
            delegate_file_idx = Some(file_idx);
        }

        let symbol_arena = delegate_arena.filter(|arena| !std::ptr::eq(*arena, self.ctx.arena))?;
        let delegate_binder = if let Some(file_idx) = delegate_file_idx {
            self.ctx
                .get_binder_for_file(file_idx)
                .unwrap_or(self.ctx.binder)
        } else {
            // Use the target arena's binder so that node→symbol lookups
            // (e.g. `get_node_symbol` for private member `parent_id`)
            // resolve correctly instead of returning `None`.
            self.ctx
                .get_binder_for_arena(symbol_arena)
                .unwrap_or(self.ctx.binder)
        };

        // Guard against deep cross-arena recursion
        if !Self::enter_cross_arena_delegation() {
            return None;
        }

        if !self.ctx.enter_recursion() {
            Self::leave_cross_arena_delegation();
            return None;
        }

        let delegate_file_name = symbol_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
            .unwrap_or_else(|| self.ctx.file_name.clone());

        let mut checker = Box::new(CheckerState::with_parent_cache(
            symbol_arena,
            delegate_binder,
            self.ctx.types,
            delegate_file_name,
            self.ctx.compiler_options.clone(),
            self,
        ));
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        checker.ctx.copy_cross_file_state_from(&self.ctx);
        self.ctx.copy_symbol_file_targets_to(&mut checker.ctx);
        checker.ctx.current_file_idx = delegate_file_idx.unwrap_or(self.ctx.current_file_idx);
        for &id in &self.ctx.symbol_resolution_set {
            if id != sym_id {
                checker.ctx.symbol_resolution_set.insert(id);
            }
        }
        // DefId ↔ SymbolId mappings are resolved via DefinitionStore fallback
        // on cache miss — no parent-to-child copy needed.

        // Try compute_interface_type_from_declarations first (more direct),
        // fall back to get_type_of_symbol for non-pure-interface symbols.
        let mut result = checker.compute_interface_type_from_declarations(sym_id);
        if result == TypeId::ERROR {
            result = checker.get_type_of_symbol(sym_id);
        }
        self.ctx.leave_recursion();
        Self::leave_cross_arena_delegation();

        if result != TypeId::UNKNOWN && result != TypeId::ERROR {
            Some(result)
        } else {
            None
        }
    }

    pub(crate) fn delegate_cross_arena_interface_member_simple_type(
        &mut self,
        interface_idx: NodeIndex,
        member_idx: NodeIndex,
        interface_arena: &tsz_parser::NodeArena,
        type_args: Option<&[TypeId]>,
    ) -> Option<TypeId> {
        if std::ptr::eq(interface_arena, self.ctx.arena) {
            return None;
        }

        // O(1) via global_arena_index (replaces O(N) position scan)
        let delegate_file_idx = self.ctx.get_file_idx_for_arena(interface_arena);
        let delegate_binder = delegate_file_idx
            .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            .unwrap_or(self.ctx.binder);

        if !Self::enter_cross_arena_delegation() {
            return None;
        }
        if !self.ctx.enter_recursion() {
            Self::leave_cross_arena_delegation();
            return None;
        }

        let delegate_file_name = interface_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
            .unwrap_or_else(|| self.ctx.file_name.clone());

        let mut checker = Box::new(CheckerState::with_parent_cache(
            interface_arena,
            delegate_binder,
            self.ctx.types,
            delegate_file_name,
            self.ctx.compiler_options.clone(),
            self,
        ));
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        checker.ctx.copy_cross_file_state_from(&self.ctx);
        self.ctx.copy_symbol_file_targets_to(&mut checker.ctx);
        checker.ctx.current_file_idx = delegate_file_idx.unwrap_or(self.ctx.current_file_idx);
        // DefId ↔ SymbolId mappings are resolved via DefinitionStore fallback
        // on cache miss — no parent-to-child copy needed.

        let interface_type_params = checker
            .ctx
            .arena
            .get(interface_idx)
            .and_then(|node| checker.ctx.arena.get_interface(node))
            .and_then(|iface| iface.type_parameters.clone());
        let (interface_params, interface_updates) = interface_type_params
            .as_ref()
            .map(|type_parameters| checker.push_type_parameters(&Some(type_parameters.clone())))
            .unwrap_or_default();
        let mut result = checker.get_type_of_interface_member_simple(member_idx);
        if let Some(type_args) = type_args
            && !interface_params.is_empty()
            && interface_params.len() == type_args.len()
        {
            let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
                checker.ctx.types,
                &interface_params,
                type_args,
            );
            result = crate::query_boundaries::common::instantiate_type(
                checker.ctx.types,
                result,
                &substitution,
            );
        }
        checker.pop_type_parameters(interface_updates);

        self.ctx.leave_recursion();
        Self::leave_cross_arena_delegation();

        if result != TypeId::UNKNOWN && result != TypeId::ERROR {
            Some(result)
        } else {
            None
        }
    }

    /// Detect and record cross-file `SymbolIds`.
    ///
    /// In multi-file mode, the driver copies target file's `module_exports` into
    /// the local binder, so `SymbolIds` may be from another file's binder. We
    /// detect this by checking if the `SymbolId` maps to a symbol with the expected
    /// name in the current binder. If not, we search `all_binders` to find the
    /// correct source file.
    pub(crate) fn record_cross_file_symbol_if_needed(
        &self,
        sym_id: SymbolId,
        expected_name: &str,
        module_name: &str,
    ) {
        // Skip if already recorded
        if self.ctx.has_symbol_file_index(sym_id) {
            return;
        }

        // Try resolve_import_target first (most reliable). This avoids SymbolId
        // collision issues: after lib_symbols_merged, different files' binders share
        // the same base_offset, so binder.get_symbol(sym_id) can return the WRONG
        // symbol from the current file that happens to share the same index offset.
        if let Some(target_file_idx) = self.ctx.resolve_import_target(module_name) {
            if target_file_idx != self.ctx.current_file_idx {
                self.ctx
                    .register_symbol_file_target(sym_id, target_file_idx);
            }
            return;
        }

        // resolve_import_target didn't work (the module specifier may be relative
        // to a different file). Fall back to the binder locality check.
        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.escaped_name.as_str() == expected_name
        {
            return;
        }

        // Fast-path: use global_file_locals_index for O(1) name→binder lookup.
        // Only covers top-level file_locals symbols; nested symbols (class members,
        // namespace exports) fall through to the O(N) scan below.
        if let Some(entries) = self
            .ctx
            .global_file_locals_index
            .as_ref()
            .and_then(|idx| idx.get(expected_name))
            && let Some(binders) = &self.ctx.all_binders
        {
            for &(file_idx, _) in entries {
                if let Some(binder) = binders.get(file_idx)
                    && let Some(symbol) = binder.get_symbol(sym_id)
                    && symbol.escaped_name.as_str() == expected_name
                {
                    self.ctx.register_symbol_file_target(sym_id, file_idx);
                    return;
                }
            }
        }
        // Full fallback: the symbol may be nested (not in file_locals).
        if let Some(binders) = &self.ctx.all_binders {
            for (idx, binder) in binders.iter().enumerate() {
                if let Some(symbol) = binder.get_symbol(sym_id)
                    && symbol.escaped_name.as_str() == expected_name
                {
                    self.ctx.register_symbol_file_target(sym_id, idx);
                    return;
                }
            }
        }
    }

    /// Lower a single interface declaration from a cross-file arena.
    ///
    /// When an interface is declared across multiple files (e.g., global script
    /// interface merging), each cross-file declaration lives in a different
    /// `NodeArena`. This method creates a `TypeLowering` bound to the source arena
    /// and uses name-based resolution via `file_locals` to resolve type references.
    pub(crate) fn lower_cross_file_interface_decl(
        &self,
        arena: &std::sync::Arc<tsz_parser::parser::node::NodeArena>,
        decl_idx: NodeIndex,
        sym_id: SymbolId,
    ) -> TypeId {
        use tsz_lowering::TypeLowering;
        use tsz_solver::is_compiler_managed_type;

        let arena_ref: &tsz_parser::parser::node::NodeArena = arena.as_ref();
        let lib_binders = self.get_lib_binders();

        // Cross-file type resolver: reads identifier text from the cross-file
        // arena, then resolves by name in the current binder's file_locals
        // (which includes merged global symbols from all files).
        let cross_type_resolver = |node_idx: NodeIndex| -> Option<u32> {
            let node = arena_ref.get(node_idx)?;
            let ident = arena_ref.get_identifier(node)?;
            let name = ident.escaped_text.as_str();
            if is_compiler_managed_type(name) {
                return None;
            }
            let sym = self.ctx.binder.file_locals.get(name)?;
            let symbol = self.ctx.binder.get_symbol_with_libs(sym, &lib_binders)?;
            if (symbol.flags & symbol_flags::TYPE) != 0 {
                return Some(sym.0);
            }
            None
        };

        let cross_def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
            let node = arena_ref.get(node_idx)?;
            let ident = arena_ref.get_identifier(node)?;
            let name = ident.escaped_text.as_str();
            if is_compiler_managed_type(name) {
                return None;
            }
            let sym = self.ctx.binder.file_locals.get(name)?;
            let symbol = self.ctx.binder.get_symbol_with_libs(sym, &lib_binders)?;
            if (symbol.flags & symbol_flags::TYPE) != 0 {
                Some(self.ctx.get_or_create_def_id(sym))
            } else {
                None
            }
        };

        let cross_value_resolver = |node_idx: NodeIndex| -> Option<u32> {
            let node = arena_ref.get(node_idx)?;
            let ident = arena_ref.get_identifier(node)?;
            let name = ident.escaped_text.as_str();
            let sym = self.ctx.binder.file_locals.get(name)?;
            let symbol = self.ctx.binder.get_symbol_with_libs(sym, &lib_binders)?;
            if (symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0 {
                Some(sym.0)
            } else {
                None
            }
        };

        let type_param_bindings = self.get_type_param_bindings();
        let lowering = TypeLowering::with_hybrid_resolver(
            arena_ref,
            self.ctx.types,
            &cross_type_resolver,
            &cross_def_id_resolver,
            &cross_value_resolver,
        )
        .with_type_param_bindings(type_param_bindings);

        lowering.lower_interface_declarations_with_symbol(&[decl_idx], sym_id)
    }

    /// Merge heritage types from cross-file interface declarations.
    ///
    /// `merge_interface_heritage_types` uses `self.ctx.arena` to read heritage
    /// clauses, so it silently skips cross-file declarations. This method handles
    /// those skipped declarations by reading from the source arena and resolving
    /// base types via `file_locals` name lookup.
    pub(crate) fn merge_cross_file_heritage(
        &mut self,
        declarations: &[NodeIndex],
        sym_id: SymbolId,
        mut derived_type: TypeId,
    ) -> TypeId {
        use tsz_scanner::SyntaxKind;

        for &decl_idx in declarations {
            let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) else {
                continue;
            };
            for arena in arenas.iter() {
                // Skip the local arena (already processed by merge_interface_heritage_types)
                if std::ptr::eq(arena.as_ref(), self.ctx.arena) {
                    continue;
                }
                let Some(node) = arena.get(decl_idx) else {
                    continue;
                };
                let Some(interface) = arena.get_interface(node) else {
                    continue;
                };
                let Some(ref heritage_clauses) = interface.heritage_clauses else {
                    continue;
                };

                for &clause_idx in &heritage_clauses.nodes {
                    let Some(clause_node) = arena.get(clause_idx) else {
                        continue;
                    };
                    let Some(heritage) = arena.get_heritage_clause(clause_node) else {
                        continue;
                    };
                    if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                        continue;
                    }

                    for &type_idx in &heritage.types.nodes {
                        let Some(type_node) = arena.get(type_idx) else {
                            continue;
                        };

                        // Extract the expression (base type name) from the heritage type
                        let expr_idx = if let Some(expr) = arena.get_expr_type_args(type_node) {
                            expr.expression
                        } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                            if let Some(type_ref) = arena.get_type_ref(type_node) {
                                type_ref.type_name
                            } else {
                                type_idx
                            }
                        } else {
                            type_idx
                        };

                        // Resolve the base type by name via file_locals
                        let Some(base_node) = arena.get(expr_idx) else {
                            continue;
                        };
                        let Some(ident) = arena.get_identifier(base_node) else {
                            continue;
                        };
                        let name = ident.escaped_text.as_str();
                        let Some(base_sym_id) = self.ctx.binder.file_locals.get(name) else {
                            continue;
                        };

                        let base_type = self.get_type_of_symbol(base_sym_id);
                        if base_type == TypeId::ERROR || base_type == TypeId::UNKNOWN {
                            continue;
                        }

                        derived_type = self.merge_interface_types(derived_type, base_type);
                    }
                }
            }
        }

        derived_type
    }
}
