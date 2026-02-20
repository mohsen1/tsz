//! Cross-file symbol resolution: resolving symbols across multiple files,
//! delegating type resolution to child checkers, and tracking cross-file targets.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
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
        // 3. Check other files in the project (multi-file mode)
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
        let file_idx = self
            .ctx
            .cross_file_symbol_targets
            .borrow()
            .get(&sym_id)
            .copied();
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
                let symbol = sym_found.unwrap();
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
        let is_known_cross_file = self
            .ctx
            .cross_file_symbol_targets
            .borrow()
            .contains_key(&sym_id);

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
                if !symbol.value_declaration.is_none() {
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
                .cross_file_symbol_targets
                .borrow()
                .get(&sym_id)
                .is_some_and(|&file_idx| {
                    let target_arena = self.ctx.get_arena_for_file(file_idx as u32);
                    !std::ptr::eq(target_arena, self.ctx.arena)
                });

        if needs_cross_file_delegation {
            let file_idx = *self
                .ctx
                .cross_file_symbol_targets
                .borrow()
                .get(&sym_id)
                .unwrap();
            cross_file_idx = Some(file_idx);
        }

        // Check if we have a valid delegate arena (either from symbol_arenas/declaration_arenas
        // or from cross_file_symbol_targets).
        let should_delegate = if needs_cross_file_delegation {
            true
        } else {
            delegate_arena.is_some_and(|arena| !std::ptr::eq(arena, self.ctx.arena))
        };

        if should_delegate {
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
            let (symbol_arena, delegate_binder) = if let Some(file_idx) = cross_file_idx {
                let arena = self.ctx.get_arena_for_file(file_idx as u32);
                let binder = self
                    .ctx
                    .get_binder_for_file(file_idx)
                    .unwrap_or(self.ctx.binder);
                (arena, binder)
            } else {
                // Non-cross-file delegation: use the already-computed arena.
                // Safe to re-fetch since the data hasn't changed.
                let arena = delegate_arena.unwrap_or(self.ctx.arena);
                (arena, self.ctx.binder)
            };

            // Box the child checker to keep it on the heap — nested delegations for
            // interdependent lib types (Array → ReadonlyArray → Iterator → ...) can
            // create deep call stacks, and CheckerState is too large to stack-allocate
            // at every level without risking stack overflow.
            let mut checker = Box::new(CheckerState::with_parent_cache(
                symbol_arena,
                delegate_binder,
                self.ctx.types,
                self.ctx.file_name.clone(),
                self.ctx.compiler_options.clone(),
                self, // Share parent's cache to fix Cache Isolation Bug
            ));
            // Copy lib contexts for global symbol resolution (Array, Promise, etc.)
            checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
            // Copy cross-file symbol targets so nested resolutions work
            if !self.ctx.cross_file_symbol_targets.borrow().is_empty() {
                *checker.ctx.cross_file_symbol_targets.borrow_mut() =
                    self.ctx.cross_file_symbol_targets.borrow().clone();
            }
            // Copy all_arenas and all_binders for nested cross-file resolution
            checker.ctx.all_arenas = self.ctx.all_arenas.clone();
            checker.ctx.all_binders = self.ctx.all_binders.clone();
            checker.ctx.resolved_module_paths = self.ctx.resolved_module_paths.clone();
            checker.ctx.current_file_idx = cross_file_idx.unwrap_or(self.ctx.current_file_idx);
            // Copy symbol resolution state to detect cross-file cycles, but exclude
            // the current symbol (which the parent added) since this checker will
            // add it again during get_type_of_symbol
            for &id in &self.ctx.symbol_resolution_set {
                if id != sym_id {
                    checker.ctx.symbol_resolution_set.insert(id);
                }
            }
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

            // DO NOT merge child's symbol_types back to the parent.
            // Cross-arena child checkers share the parent's binder (including node_symbols
            // which maps the parent file's node indices to symbol IDs) but operate on a
            // different arena. This causes node index collisions: a lib node at index N
            // can be confused with the parent file's node at index N, contaminating the
            // symbol cache (e.g., setting an ALIAS import symbol to STRING because the
            // same node index maps to a StringKeyword in the lib arena).
            // The delegated symbol's result is returned directly and cached by the caller
            // in get_type_of_symbol, so no merge-back is needed for correctness.

            // Merge child's DefId→SymbolId mappings to parent.
            // The child creates DefIds (in the shared DefinitionStore) for enum/class/etc.
            // symbols. These DefIds are embedded in TypeKeys in the shared TypeStore.
            // The parent needs DefId→SymbolId mappings to resolve these types
            // (e.g., for enum property access via resolve_namespace_value_member).
            // NOTE: symbol_to_def is NOT merged because SymbolIds are binder-local;
            // the same SymbolId maps to different symbols in different binders.
            {
                let child_d2s = checker.ctx.def_to_symbol.borrow();
                let mut parent_d2s = self.ctx.def_to_symbol.borrow_mut();
                for (&def_id, &sym_id) in child_d2s.iter() {
                    parent_d2s.entry(def_id).or_insert(sym_id);
                }
            }

            // Merge child's def_type_params to parent.
            // Generic type aliases (e.g., `type Constructor<T = {}> = new (...args: any[]) => T`)
            // register their type parameters in def_type_params. When the parent later tries to
            // expand Application(Lazy(DefId), Args) via CompatChecker, it needs these type params.
            {
                let child_params = checker.ctx.def_type_params.borrow();
                let mut parent_params = self.ctx.def_type_params.borrow_mut();
                for (def_id, params) in child_params.iter() {
                    parent_params
                        .entry(*def_id)
                        .or_insert_with(|| params.clone());
                }
            }

            // Merge child's type_env def entries (type alias bodies and params) to parent.
            // The child registers type alias bodies via type_env.insert_def_with_params.
            // The parent's CompatChecker needs these to expand Application types like
            // Constructor<{}> → new (...args: any[]) => {}.
            {
                let child_env = checker.ctx.type_env.borrow();
                let mut parent_env = self.ctx.type_env.borrow_mut();
                child_env.merge_defs_into(&mut parent_env);
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

        let mut checker = Box::new(CheckerState::with_parent_cache(
            symbol_arena,
            self.ctx.binder,
            self.ctx.types,
            self.ctx.file_name.clone(),
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
        for &id in &self.ctx.class_constructor_resolution_set {
            checker.ctx.class_constructor_resolution_set.insert(id);
        }

        let result = checker.class_instance_type_with_params_from_symbol(sym_id);

        self.ctx.leave_recursion();
        Self::leave_cross_arena_delegation();

        result
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
        if self
            .ctx
            .cross_file_symbol_targets
            .borrow()
            .contains_key(&sym_id)
        {
            return;
        }

        // Check if the SymbolId maps to the expected name in the current binder.
        // If it does, this is a local symbol and no cross-file tracking needed.
        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.escaped_name.as_str() == expected_name
        {
            return;
        }

        // The SymbolId doesn't match in the current binder — it's cross-file.
        // Try resolve_import_target first (most reliable).
        if let Some(target_file_idx) = self.ctx.resolve_import_target(module_name) {
            self.ctx
                .cross_file_symbol_targets
                .borrow_mut()
                .insert(sym_id, target_file_idx);
            return;
        }

        // Fallback: search all binders for one where this SymbolId has the expected name.
        if let Some(binders) = &self.ctx.all_binders {
            for (idx, binder) in binders.iter().enumerate() {
                if let Some(symbol) = binder.get_symbol(sym_id)
                    && symbol.escaped_name.as_str() == expected_name
                {
                    self.ctx
                        .cross_file_symbol_targets
                        .borrow_mut()
                        .insert(sym_id, idx);
                    return;
                }
            }
        }
    }
}
