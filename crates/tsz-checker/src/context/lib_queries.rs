//! Library and global type availability queries for `CheckerContext`.
//!
//! These methods check whether specific types (Promise, Symbol, etc.) are
//! available in lib files or global scope.

use std::sync::Arc;

use rustc_hash::FxHashSet;
use tsz_binder::SymbolId;
use tsz_parser::parser::node::NodeAccess;

use super::CheckerContext;

fn is_builtin_lib_file_name(file_name: &str) -> bool {
    let basename = std::path::Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(file_name);

    if basename.starts_with("lib.") && basename.ends_with(".d.ts") {
        return true;
    }

    let stem = basename
        .strip_suffix(".generated.d.ts")
        .or_else(|| basename.strip_suffix(".d.ts"))
        .unwrap_or(basename);

    stem == "lib"
        || stem == "scripthost"
        || stem == "decorators"
        || stem == "decorators.legacy"
        || stem == "dom"
        || stem.starts_with("dom.")
        || stem == "webworker"
        || stem.starts_with("webworker.")
        || stem == "esnext"
        || stem.starts_with("esnext.")
        || (stem.starts_with("es") && stem.as_bytes().get(2).is_some_and(u8::is_ascii_digit))
}

fn arena_is_builtin_lib_file(arena: &tsz_parser::parser::NodeArena) -> bool {
    arena.source_files.first().is_some_and(|source_file| {
        source_file.is_declaration_file && is_builtin_lib_file_name(&source_file.file_name)
    })
}

impl<'a> CheckerContext<'a> {
    pub fn actual_lib_def_id_for_bare_name(&self, name: &str) -> Option<tsz_solver::DefId> {
        if name.contains('.') {
            return None;
        }
        // This lib alias is an option-dependent intrinsic: it lowers to
        // `undefined` under `strictBuiltinIteratorReturn` and `any` otherwise.
        // Returning a stable lib `DefId` here would bypass that policy.
        if name == "BuiltinIteratorReturn" {
            return None;
        }

        if !self.has_lib_loaded() {
            return None;
        }

        if let Some(cached) = self.actual_lib_def_id_cache.borrow().get(name).copied() {
            return cached;
        }

        let result = self.actual_lib_def_id_for_bare_name_uncached(name);
        self.actual_lib_def_id_cache
            .borrow_mut()
            .insert(name.to_string(), result);
        result
    }

    fn actual_lib_def_id_for_bare_name_uncached(&self, name: &str) -> Option<tsz_solver::DefId> {
        if let Some(sym_id) = self.actual_lib_symbol_id_for_bare_name(name) {
            return Some(self.get_canonical_lib_def_id(name, sym_id));
        }

        for lib_ctx in self.lib_contexts.iter().take(self.actual_lib_file_count) {
            if let Some(sym_id) = lib_ctx.binder.file_locals.get(name) {
                return Some(self.get_canonical_lib_def_id(name, sym_id));
            }
        }

        None
    }

    pub(crate) fn actual_lib_context_has_bare_name(&self, name: &str) -> bool {
        !name.contains('.')
            && name != "BuiltinIteratorReturn"
            && self
                .lib_contexts
                .iter()
                .take(self.actual_lib_file_count)
                .any(|lib_ctx| lib_ctx.binder.file_locals.has(name))
    }

    fn actual_lib_symbol_id_for_bare_name(&self, name: &str) -> Option<SymbolId> {
        if let Some(sym_id) = self.binder.file_locals.get(name)
            && self.symbol_is_from_actual_or_cloned_lib(sym_id)
            && !self.symbol_has_current_file_type_declaration(sym_id, name)
        {
            return Some(sym_id);
        }

        self.global_file_locals_index
            .as_ref()
            .and_then(|idx| idx.get(name))
            .and_then(|entries| {
                entries
                    .iter()
                    .map(|&(_, sym_id)| sym_id)
                    .filter(|&sym_id| self.symbol_is_from_actual_or_cloned_lib(sym_id))
                    .max_by_key(|sym_id| sym_id.0)
            })
    }

    pub fn file_local_type_shadow_for_lib_name(&self, name: &str) -> bool {
        use tsz_binder::symbol_flags;

        if !self.binder.is_external_module() {
            return false;
        }

        if self.current_file_type_shadow_for_name(name) {
            return true;
        }
        if self.same_file_type_declaration_exists(name) {
            return true;
        }

        self.binder.file_locals.get(name).is_some_and(|sym_id| {
            let is_actual_or_merged_lib = self.symbol_is_from_actual_lib(sym_id)
                || self.binder.lib_symbol_ids.contains(&sym_id);
            if is_actual_or_merged_lib {
                return self.symbol_has_current_file_type_declaration(sym_id, name);
            }
            !is_actual_or_merged_lib
                && self
                    .binder
                    .get_symbol(sym_id)
                    .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::TYPE))
        })
    }

    fn current_file_type_shadow_for_name(&self, name: &str) -> bool {
        use tsz_binder::symbol_flags;

        if !self.binder.is_external_module() {
            return false;
        }

        let Some(entries) = self
            .global_file_locals_index
            .as_ref()
            .and_then(|idx| idx.get(name))
        else {
            return false;
        };

        entries.iter().any(|&(file_idx, sym_id)| {
            if file_idx != self.current_file_idx || self.symbol_is_from_actual_or_cloned_lib(sym_id)
            {
                return false;
            }

            self.get_binder_for_file(file_idx)
                .or(Some(self.binder))
                .and_then(|binder| binder.get_symbol(sym_id))
                .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::TYPE))
        })
    }

    pub(crate) fn symbol_has_current_file_type_declaration(
        &self,
        sym_id: SymbolId,
        name: &str,
    ) -> bool {
        let Some(symbol) = self.binder.get_symbol(sym_id) else {
            return false;
        };
        symbol.declarations.iter().any(|&decl_idx| {
            if let Some(arenas) = self.binder.declaration_arenas.get(&(sym_id, decl_idx))
                && arenas.iter().any(|arena| {
                    if self.is_global_augmentation_declaration(name, arena.as_ref(), decl_idx) {
                        return false;
                    }
                    std::ptr::eq(arena.as_ref(), self.arena)
                        && self.type_declaration_name_matches(arena.as_ref(), decl_idx, name)
                })
            {
                return true;
            }

            if self.is_global_augmentation_declaration(name, self.arena, decl_idx) {
                return false;
            }
            self.type_declaration_name_matches(self.arena, decl_idx, name)
        })
    }

    fn is_global_augmentation_declaration(
        &self,
        name: &str,
        arena: &tsz_parser::parser::NodeArena,
        decl_idx: tsz_parser::parser::NodeIndex,
    ) -> bool {
        self.binder
            .global_augmentations
            .get(name)
            .is_some_and(|augmentations| {
                augmentations.iter().any(|augmentation| {
                    augmentation.node == decl_idx
                        && augmentation.arena.as_ref().map_or_else(
                            || std::ptr::eq(arena, self.arena),
                            |aug_arena| std::ptr::eq(arena, aug_arena.as_ref()),
                        )
                })
            })
    }

    pub(crate) fn same_file_type_declaration_symbol_for_name(
        &self,
        name: &str,
    ) -> Option<SymbolId> {
        if !self.binder.is_external_module() {
            return None;
        }

        self.arena.nodes.iter().enumerate().find_map(|(idx, _)| {
            let decl_idx = tsz_parser::NodeIndex(idx as u32);
            if self.is_global_augmentation_declaration(name, self.arena, decl_idx) {
                return None;
            }
            self.type_declaration_name_matches(self.arena, decl_idx, name)
                .then(|| self.binder.node_symbols.get(&decl_idx.0).copied())
                .flatten()
        })
    }

    pub(crate) fn same_file_type_declaration_exists(&self, name: &str) -> bool {
        if !self.binder.is_external_module() {
            return false;
        }

        if let Some(cached_names) = self
            .symbol_name_candidates_cache
            .borrow()
            .same_file_type_declaration_names
            .as_ref()
        {
            return cached_names.contains(name);
        }

        let names: FxHashSet<String> = self
            .arena
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(idx, _)| {
                let decl_idx = tsz_parser::NodeIndex(idx as u32);
                let decl_name = self.type_declaration_name_text(self.arena, decl_idx)?;
                (!self.is_global_augmentation_declaration(decl_name, self.arena, decl_idx))
                    .then(|| decl_name.to_string())
            })
            .collect();
        let exists = names.contains(name);

        self.symbol_name_candidates_cache
            .borrow_mut()
            .same_file_type_declaration_names = Some(names);
        exists
    }

    fn type_declaration_name_text<'arena>(
        &self,
        arena: &'arena tsz_parser::parser::NodeArena,
        decl_idx: tsz_parser::parser::NodeIndex,
    ) -> Option<&'arena str> {
        let node = arena.get(decl_idx)?;
        let name_node = arena
            .get_interface(node)
            .map(|decl| decl.name)
            .or_else(|| arena.get_type_alias(node).map(|decl| decl.name))
            .or_else(|| arena.get_class(node).map(|decl| decl.name))
            .or_else(|| arena.get_enum(node).map(|decl| decl.name))?;
        arena.get_identifier_text(name_node)
    }

    fn type_declaration_name_matches(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        decl_idx: tsz_parser::parser::NodeIndex,
        name: &str,
    ) -> bool {
        self.type_declaration_name_text(arena, decl_idx) == Some(name)
    }

    /// Check if the Promise constructor VALUE is available.
    /// The ES5 lib declares `interface Promise<T>` (type only) but NOT
    /// `declare var Promise: PromiseConstructor` (value). ES2015+ libs declare both.
    /// Used for TS2705: "An async function in ES5 requires the Promise constructor."
    pub fn has_promise_constructor_in_scope(&self) -> bool {
        use tsz_binder::symbol_flags;
        // Fast-path: if PromiseConstructor type is present in loaded libs/scope,
        // treat Promise constructor as available even if VALUE flags were not merged.
        if self.has_name_in_lib("PromiseConstructor") {
            return true;
        }
        // Check if Promise exists as a VALUE symbol (not just a TYPE)
        let check_symbol_has_value =
            |sym_id: tsz_binder::SymbolId, binder: &tsz_binder::BinderState| -> bool {
                if let Some(sym) = binder.symbols.get(sym_id) {
                    sym.has_any_flags(symbol_flags::VALUE)
                } else {
                    false
                }
            };

        // Check lib contexts
        for lib_ctx in self.lib_contexts.iter() {
            if let Some(sym_id) = lib_ctx.binder.file_locals.get("Promise")
                && check_symbol_has_value(sym_id, &lib_ctx.binder)
            {
                return true;
            }
        }

        // Check current scope
        if let Some(sym_id) = self.binder.current_scope.get("Promise")
            && check_symbol_has_value(sym_id, self.binder)
        {
            return true;
        }

        // Check file locals
        if let Some(sym_id) = self.binder.file_locals.get("Promise")
            && check_symbol_has_value(sym_id, self.binder)
        {
            return true;
        }

        false
    }

    /// Check whether Promise-constructor-based features should report missing-runtime diagnostics.
    ///
    /// This is intentionally based on the loaded libs / declarations, not on the
    /// `target` alone. Conformance cases like `@target: es2015` with `@lib: es5`
    /// still need TS2468/TS2705/TS2712 because the Promise value is absent.
    pub fn promise_constructor_diagnostics_required(&self) -> bool {
        !self.has_promise_constructor_in_scope()
    }

    /// Check if Symbol is available in lib files or global scope.
    /// Returns true if Symbol is declared in lib contexts, globals, or type declarations.
    pub fn has_symbol_in_lib(&self) -> bool {
        // Check lib contexts first
        for lib_ctx in self.lib_contexts.iter() {
            if lib_ctx.binder.file_locals.has("Symbol") {
                return true;
            }
        }

        // Check if Symbol is available in current scope/global context
        if self.binder.current_scope.has("Symbol") {
            return true;
        }

        // Check current file locals as fallback
        if self.binder.file_locals.has("Symbol") {
            return true;
        }

        false
    }

    /// Check if a named symbol is available in lib files or global scope.
    /// Returns true if the symbol is declared in lib contexts, globals, or current scope.
    /// This is a generalized version of `has_symbol_in_lib` for any symbol name.
    pub fn has_name_in_lib(&self, name: &str) -> bool {
        // Check lib contexts first
        for lib_ctx in self.lib_contexts.iter() {
            if lib_ctx.binder.file_locals.has(name) {
                return true;
            }
        }

        // Check if symbol is available in current scope/global context
        if self.binder.current_scope.has(name) {
            return true;
        }

        // Check current file locals as fallback
        if self.binder.file_locals.has(name) {
            return true;
        }

        false
    }

    /// Check if a symbol originates from a lib context.
    pub fn symbol_is_from_lib(&self, sym_id: SymbolId) -> bool {
        let Some(symbol_arena) = self.binder.symbol_arenas.get(&sym_id) else {
            return false;
        };

        self.lib_contexts
            .iter()
            .any(|lib_ctx| Arc::ptr_eq(&lib_ctx.arena, symbol_arena))
    }

    /// Check if a symbol originates from an actual standard lib file.
    ///
    /// `lib_contexts` can also contain user files for cross-file resolution, so
    /// callers that need standard-library behavior must only inspect the leading
    /// `actual_lib_file_count` contexts.
    pub fn symbol_is_from_actual_lib(&self, sym_id: SymbolId) -> bool {
        let Some(symbol_arena) = self.binder.symbol_arenas.get(&sym_id) else {
            return false;
        };

        self.lib_contexts
            .iter()
            .take(self.actual_lib_file_count)
            .any(|lib_ctx| Arc::ptr_eq(&lib_ctx.arena, symbol_arena))
    }

    /// Check if a symbol originates from an actual standard lib file, including
    /// driver paths where binding and checking use separately parsed lib arenas.
    pub fn symbol_is_from_actual_or_cloned_lib(&self, sym_id: SymbolId) -> bool {
        // Raw SymbolIds are per-binder. If this id has been pinned to a source
        // file, trust that file's arena before consulting the current binder's
        // same-number symbol or lib-id set.
        if let Some(file_idx) = self.resolve_symbol_file_index(sym_id) {
            return arena_is_builtin_lib_file(self.get_arena_for_file(file_idx as u32));
        }

        // `merge_lib_contexts_into_binder` remaps standard-lib symbols into the
        // file binder and records those new ids here. Arena-pointer checks below
        // can miss those local clones.
        if self.binder.lib_symbol_ids.contains(&sym_id) {
            return true;
        }

        if self.symbol_is_from_actual_lib(sym_id) {
            return true;
        }

        if !self.has_lib_loaded() || self.all_arenas.is_none() {
            return false;
        }

        let Some(symbol_arena) = self.binder.symbol_arenas.get(&sym_id) else {
            return self.binder.symbols.get(sym_id).is_some_and(|symbol| {
                let current_arena_is_builtin = arena_is_builtin_lib_file(self.arena);
                if symbol.decl_file_idx != u32::MAX
                    || self.binder.file_locals.get(symbol.escaped_name.as_str()) != Some(sym_id)
                {
                    return false;
                }

                // Unstamped source binders can leave decl_file_idx at u32::MAX.
                // A real declaration in a non-lib current file is still user
                // provenance, even when the name also exists in the standard libs.
                if !current_arena_is_builtin
                    && self.symbol_has_current_file_type_declaration(
                        sym_id,
                        symbol.escaped_name.as_str(),
                    )
                {
                    return false;
                }

                current_arena_is_builtin
                    || self.actual_lib_context_has_bare_name(symbol.escaped_name.as_str())
            });
        };

        let symbol_arena_ptr = Arc::as_ptr(symbol_arena) as usize;
        let current_arena_ptr = self.arena as *const _ as usize;
        if symbol_arena_ptr == current_arena_ptr {
            return false;
        }

        self.get_file_idx_for_arena(symbol_arena.as_ref()).is_none()
    }
}
