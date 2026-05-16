//! Library and global type availability queries for `CheckerContext`.
//!
//! These methods check whether specific types (Promise, Symbol, etc.) are
//! available in lib files or global scope.

use std::sync::Arc;

use tsz_binder::SymbolId;
use tsz_parser::parser::node::NodeAccess;

use super::CheckerContext;

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

    pub(crate) fn symbol_has_current_file_type_declaration(
        &self,
        sym_id: SymbolId,
        name: &str,
    ) -> bool {
        if self
            .binder
            .global_augmentations
            .get(name)
            .is_some_and(|augmentations| !augmentations.is_empty())
        {
            return false;
        }

        let Some(symbol) = self.binder.get_symbol(sym_id) else {
            return false;
        };
        symbol.declarations.iter().any(|&decl_idx| {
            if let Some(arenas) = self.binder.declaration_arenas.get(&(sym_id, decl_idx))
                && arenas.iter().any(|arena| {
                    std::ptr::eq(arena.as_ref(), self.arena)
                        && self.type_declaration_name_matches(arena.as_ref(), decl_idx, name)
                })
            {
                return true;
            }

            self.type_declaration_name_matches(self.arena, decl_idx, name)
        })
    }

    fn type_declaration_name_matches(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        decl_idx: tsz_parser::parser::NodeIndex,
        name: &str,
    ) -> bool {
        let Some(node) = arena.get(decl_idx) else {
            return false;
        };
        let name_node = arena
            .get_interface(node)
            .map(|decl| decl.name)
            .or_else(|| arena.get_type_alias(node).map(|decl| decl.name))
            .or_else(|| arena.get_class(node).map(|decl| decl.name))
            .or_else(|| arena.get_enum(node).map(|decl| decl.name));
        name_node.is_some_and(|name_node| arena.get_identifier_text(name_node) == Some(name))
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
                symbol.decl_file_idx == u32::MAX
                    && self.binder.file_locals.get(symbol.escaped_name.as_str()) == Some(sym_id)
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
