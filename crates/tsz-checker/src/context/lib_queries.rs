//! Library and global type availability queries for `CheckerContext`.
//!
//! These methods check whether specific types (Promise, Symbol, etc.) are
//! available in lib files or global scope.

use std::sync::Arc;

use tsz_binder::SymbolId;
use tsz_parser::parser::node::NodeAccess;
use tsz_solver::TypeId;

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

    pub(crate) fn actual_lib_context_has_bare_name(&self, name: &str) -> bool {
        !name.contains('.')
            && name != "BuiltinIteratorReturn"
            && self
                .lib_contexts
                .iter()
                .take(self.actual_lib_file_count)
                .any(|lib_ctx| lib_ctx.binder.file_locals.has(name))
    }

    /// Resolve a name to the value-global symbol that contributes a property to
    /// the synthetic `typeof globalThis` surface (a `var`/`function`/`class`
    /// declaration in the current file or a lib). Block-scoped `let`/`const`
    /// bindings are not properties of `globalThis`, so they are excluded.
    fn global_this_surface_symbol(&self, name: &str) -> Option<SymbolId> {
        if let Some(sym_id) = self.binder.file_locals.get(name)
            && self
                .binder
                .get_symbol(sym_id)
                .is_some_and(is_global_this_surface_value)
        {
            return Some(sym_id);
        }

        for lib_ctx in self.lib_contexts.iter() {
            if let Some(sym_id) = lib_ctx.binder.file_locals.get(name)
                && lib_ctx
                    .binder
                    .get_symbol(sym_id)
                    .is_some_and(is_global_this_surface_value)
            {
                return Some(sym_id);
            }
        }

        None
    }

    /// `typeof globalThis` resolves to the synthetic surface object, never `any`.
    /// Returns it when `expr_name_idx` (resolved in `arena`) is the bare
    /// `globalThis` identifier. Shared by the `Fn`-typed `typeof` lowering
    /// overrides so the check lives in one place rather than at each call site.
    pub(crate) fn global_this_typeof_override(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        expr_name_idx: tsz_parser::parser::NodeIndex,
    ) -> Option<TypeId> {
        (arena.get_identifier_text(expr_name_idx) == Some("globalThis"))
            .then(|| self.global_this_surface_type())
    }

    /// Build (and memoize) the synthetic `typeof globalThis` object type for this
    /// checker file. `typeof globalThis` is a concrete object whose properties are
    /// the in-scope value globals (current-file `var`/`function`/`class` plus lib
    /// globals); it is **not** `any`. Representing it as `any` made
    /// `typeof globalThis extends X ? T : F` distribute to `T | F` and silenced
    /// element-access diagnostics, so the lowering and member-access paths route
    /// through this surface instead.
    ///
    /// The result is `&self`-buildable: it relies only on interior-mutable
    /// interning and the file-local cache, so it can be consulted from the
    /// `Fn`-typed `typeof` lowering overrides.
    pub(crate) fn global_this_surface_type(&self) -> TypeId {
        use tsz_solver::{PropertyInfo, SymbolRef};

        let cache = &self.type_reference_validation_caches.type_node_surface;
        if let Some(cached) = cache.global_this_type.get() {
            return cached;
        }
        // A nested `typeof globalThis` reached while the surface is being built
        // (a self-edge through one of the property types) resolves to the
        // `globalThis` self-property fallback rather than recursing forever.
        if cache.global_this_type_in_progress.get() {
            return TypeId::UNKNOWN;
        }
        cache.global_this_type_in_progress.set(true);

        let mut names: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
        for (name, _) in self.binder.file_locals.iter() {
            names.insert(name.clone());
        }
        for lib_ctx in self.lib_contexts.iter() {
            for (name, _) in lib_ctx.binder.file_locals.iter() {
                names.insert(name.clone());
            }
        }
        names.insert("globalThis".to_string());

        let mut properties = Vec::new();
        for name in names {
            // `globalThis.globalThis` is `typeof globalThis`; modeled as the
            // self-property fallback to avoid an infinite surface expansion.
            let (type_id, parent_id) = if name == "globalThis" {
                (TypeId::UNKNOWN, None)
            } else {
                let Some(sym_id) = self.global_this_surface_symbol(&name) else {
                    continue;
                };
                let type_id = self
                    .symbol_types
                    .get(&sym_id)
                    .filter(|&type_id| type_id != TypeId::ERROR)
                    .unwrap_or_else(|| self.types.factory().type_query(SymbolRef(sym_id.0)));
                (type_id, Some(sym_id))
            };

            let prop_name = self.types.intern_string(&name);
            let mut prop = PropertyInfo::new(prop_name, type_id);
            prop.write_type = type_id;
            prop.readonly = name == "globalThis";
            prop.parent_id = parent_id;
            prop.declaration_order = properties.len() as u32;
            properties.push(prop);
        }

        let global_this_type = self.types.factory().global_this_surface_object(properties);
        // Record the synthetic surface so diagnostics render it as
        // `typeof globalThis` rather than its full member body.
        self.types
            .mark_global_this_surface_display(global_this_type);
        cache.global_this_type.set(Some(global_this_type));
        cache.global_this_type_in_progress.set(false);
        global_this_type
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

    pub(crate) fn actual_lib_global_type_symbol_id(&self, name: &str) -> Option<SymbolId> {
        if name.contains('.') {
            return None;
        }

        for lib_ctx in self.lib_contexts.iter().take(self.actual_lib_file_count) {
            if let Some(sym_id) = lib_ctx.binder.file_locals.get(name) {
                return Some(sym_id);
            }
        }

        self.actual_lib_symbol_id_for_bare_name(name)
    }

    /// Resolve a synthetic `globalThis.<name>` *type* member to a `SymbolId`
    /// that is valid in **this file's** binder, so a consumer that re-reads the
    /// id locally (e.g. `type_reference_symbol_type`) resolves the intended
    /// symbol.
    ///
    /// Mirrors [`Self::actual_lib_def_id_for_bare_name`] on the `SymbolId` axis:
    /// prefer the merged-lib clone in `file_locals` (the production pipeline
    /// merges every lib symbol into each file binder), then fall back to the
    /// lib-context-local id only for non-merged setups (where the ids coincide
    /// with the file binder anyway). Unlike
    /// [`Self::actual_lib_global_type_symbol_id`] — whose lib-arena-canonical id
    /// is intended only for name-keyed identity comparison — this never hands a
    /// foreign binder's `SymbolId` to a local consumer, which would alias an
    /// unrelated symbol of the same numeric id (the `globalThis.Record` ->
    /// `CSSNestedDeclarations`, `globalThis.Array` -> `btoa` family, #14921).
    pub(crate) fn actual_lib_symbol_id_for_global_type(&self, name: &str) -> Option<SymbolId> {
        if name.contains('.') {
            return None;
        }

        if let Some(sym_id) = self.actual_lib_symbol_id_for_bare_name(name) {
            return Some(sym_id);
        }

        for lib_ctx in self.lib_contexts.iter().take(self.actual_lib_file_count) {
            if let Some(sym_id) = lib_ctx.binder.file_locals.get(name) {
                return Some(sym_id);
            }
        }

        None
    }

    pub fn file_local_type_shadow_for_lib_name(&self, name: &str) -> bool {
        use tsz_binder::symbol_flags;

        if !self.binder.is_external_module() {
            return false;
        }

        if self.current_file_type_shadow_for_name(name) {
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

        self.arena.nodes.iter().enumerate().any(|(idx, _)| {
            let decl_idx = tsz_parser::NodeIndex(idx as u32);
            !self.is_global_augmentation_declaration(name, self.arena, decl_idx)
                && self.type_declaration_name_matches(self.arena, decl_idx, name)
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

        for lib_ctx in self.lib_contexts.iter() {
            if let Some(sym_id) = lib_ctx.binder.file_locals.get("Promise")
                && check_symbol_has_value(sym_id, &lib_ctx.binder)
            {
                return true;
            }
        }

        if let Some(sym_id) = self.binder.current_scope().get("Promise")
            && check_symbol_has_value(sym_id, self.binder)
        {
            return true;
        }

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
        for lib_ctx in self.lib_contexts.iter() {
            if lib_ctx.binder.file_locals.has("Symbol") {
                return true;
            }
        }
        if self.binder.current_scope().has("Symbol") {
            return true;
        }
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

        if self.binder.current_scope().has(name) {
            return true;
        }
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

    /// `SymbolId` of the standard-library `Promise` declaration, if loaded.
    ///
    /// Looks only at actual lib contexts. Cloned lib symbols are handled by
    /// `sym_id_is_current_cloned_lib_promise`, which requires the symbol id to
    /// be resolved through this file's binder before matching the well-known
    /// global name.
    pub fn lib_promise_sym_id(&self) -> Option<SymbolId> {
        self.actual_lib_global_type_symbol_id("Promise")
    }

    /// `Lazy(DefId)` for the standard-library `Promise` declaration, if loaded.
    ///
    /// Use this when constructing `Promise<T>` types. It canonicalizes the
    /// per-lib symbol id through the well-known global name before creating the
    /// lazy reference, avoiding numeric `SymbolId` collisions across lib arenas.
    pub fn lib_promise_type_ref(&self) -> Option<tsz_solver::TypeId> {
        let sym_id = self.actual_lib_global_type_symbol_id("Promise")?;
        let def_id = self.get_canonical_lib_def_id("Promise", sym_id);
        Some(self.types.lazy(def_id))
    }

    /// True when `sym_id` is the standard-library `Promise` symbol.
    pub fn sym_id_is_lib_promise(&self, sym_id: SymbolId) -> bool {
        self.lib_promise_sym_id() == Some(sym_id)
    }

    /// True when `sym_id` is the current binder's cloned standard-library
    /// `Promise` symbol.
    pub fn sym_id_is_current_cloned_lib_promise(&self, sym_id: SymbolId) -> bool {
        self.binder.lib_symbol_ids.contains(&sym_id)
            && self
                .binder
                .get_symbol(sym_id)
                .is_some_and(|symbol| symbol.escaped_name.as_str() == "Promise")
    }

    /// True when `sym_id` is the standard-library `Function` symbol, including
    /// current-binder clones produced by lib merging.
    pub fn sym_id_is_lib_function(&self, sym_id: SymbolId) -> bool {
        self.actual_lib_global_type_symbol_id("Function") == Some(sym_id)
            || (self.binder.lib_symbol_ids.contains(&sym_id)
                && self
                    .binder
                    .get_symbol(sym_id)
                    .is_some_and(|symbol| symbol.escaped_name.as_str() == "Function"))
    }

    fn sym_id_is_lib_promise_like(&self, sym_id: SymbolId) -> bool {
        self.actual_lib_global_type_symbol_id("PromiseLike") == Some(sym_id)
    }

    fn sym_id_is_current_cloned_lib_promise_like(&self, sym_id: SymbolId) -> bool {
        self.binder.lib_symbol_ids.contains(&sym_id)
            && self
                .binder
                .get_symbol(sym_id)
                .is_some_and(|symbol| symbol.escaped_name.as_str() == "PromiseLike")
    }

    /// True when `sym_id` is the standard-library `Promise` or `PromiseLike` symbol.
    pub fn sym_id_is_lib_promise_or_promise_like(&self, sym_id: SymbolId) -> bool {
        self.sym_id_is_lib_promise(sym_id) || self.sym_id_is_lib_promise_like(sym_id)
    }

    /// True when `sym_id` is a current-binder clone of the standard-library
    /// `Promise` or `PromiseLike` symbol.
    pub fn sym_id_is_current_cloned_lib_promise_or_promise_like(&self, sym_id: SymbolId) -> bool {
        self.sym_id_is_current_cloned_lib_promise(sym_id)
            || self.sym_id_is_current_cloned_lib_promise_like(sym_id)
    }

    /// Structural predicate for suppressing the simple-object
    /// `RejectMissingInterfaceDecl` / declaration-provenance residue counter.
    ///
    /// Returns true when the symbol has no local interface declaration and
    /// is, by binder-recorded provenance, an actual or cloned-lib symbol.
    /// The predicate never inspects the symbol's name: §25 forbids a name
    /// allowlist here.
    pub fn simple_object_missing_interface_decl_residue_is_lib_provenance_case(
        &self,
        sym_id: SymbolId,
        has_local_interface_decl: bool,
    ) -> bool {
        !has_local_interface_decl && self.symbol_is_from_actual_or_cloned_lib(sym_id)
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

    /// True when `sym_id` is the actual or cloned standard-library global type
    /// symbol for `name`.
    pub fn sym_id_is_actual_or_cloned_lib_global_type_named(
        &self,
        sym_id: SymbolId,
        name: &str,
    ) -> bool {
        if name.contains('.') {
            return false;
        }

        if self.actual_lib_global_type_symbol_id(name) == Some(sym_id) {
            return true;
        }

        self.binder.lib_symbol_ids.contains(&sym_id)
            && self
                .binder
                .get_symbol(sym_id)
                .is_some_and(|symbol| symbol.escaped_name.as_str() == name)
    }
}

/// A symbol contributes a property to the `typeof globalThis` surface when it
/// has value meaning and is not a block-scoped `let`/`const` (those are not
/// properties of `globalThis`; only `var`/`function`/`class` are).
const fn is_global_this_surface_value(symbol: &tsz_binder::Symbol) -> bool {
    use tsz_binder::symbol_flags;
    symbol.has_any_flags(symbol_flags::VALUE)
        && (!symbol.has_any_flags(symbol_flags::BLOCK_SCOPED_VARIABLE)
            || symbol.has_any_flags(symbol_flags::FUNCTION_SCOPED_VARIABLE))
}
