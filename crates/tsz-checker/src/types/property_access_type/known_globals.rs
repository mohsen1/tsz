//! Helpers for distinguishing built-in globals from same-named local values.

use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::node::NodeArena;

impl<'a> CheckerState<'a> {
    pub(crate) fn known_global_identifier_resolves_to_lib_value(
        &self,
        idx: NodeIndex,
        name: &str,
    ) -> bool {
        if let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(idx) {
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                return false;
            };
            return symbol.escaped_name == name
                && (self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                    || self.ctx.symbol_is_from_lib(sym_id));
        }

        !self.known_global_value_has_local_shadow(idx, name)
    }

    pub(crate) fn known_global_identifier_resolves_to_lib_value_in_arena(
        &self,
        arena: &NodeArena,
        binder: &BinderState,
        idx: NodeIndex,
        name: &str,
    ) -> bool {
        if let Some(sym_id) = binder.resolve_identifier(arena, idx) {
            let Some(symbol) = binder
                .get_symbol(sym_id)
                .or_else(|| self.ctx.binder.get_symbol(sym_id))
            else {
                return false;
            };
            if symbol.escaped_name == name
                && (self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                    || binder.lib_symbol_ids.contains(&sym_id)
                    || self.ctx.symbol_is_from_lib(sym_id))
            {
                return true;
            }

            return !self.known_global_value_has_local_shadow_in_arena(arena, binder, idx, name);
        }

        !self.known_global_value_has_local_shadow_in_arena(arena, binder, idx, name)
    }

    /// Returns `true` if the node at `idx` is an identifier spelled `name` and
    /// that identifier resolves to the built-in global value, not a same-named
    /// local declaration.
    pub(crate) fn identifier_resolves_to_unshadowed_global(
        &self,
        idx: NodeIndex,
        name: &str,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(node) else {
            return false;
        };
        if ident.escaped_text.as_str() != name {
            return false;
        }
        self.known_global_identifier_resolves_to_lib_value(idx, name)
    }

    /// Arena-aware form of [`Self::identifier_resolves_to_unshadowed_global`]
    /// for cross-file export-surface scans.
    pub(crate) fn identifier_resolves_to_unshadowed_global_in_arena(
        &self,
        arena: &NodeArena,
        binder: &BinderState,
        idx: NodeIndex,
        name: &str,
    ) -> bool {
        let Some(node) = arena.get(idx) else {
            return false;
        };
        let Some(ident) = arena.get_identifier(node) else {
            return false;
        };
        if ident.escaped_text.as_str() != name {
            return false;
        }
        self.known_global_identifier_resolves_to_lib_value_in_arena(arena, binder, idx, name)
    }

    /// Returns `true` if the node at `idx` is an identifier spelled `name` and
    /// the binder resolves it to a proven lib/global value.
    ///
    /// Unlike `identifier_resolves_to_unshadowed_global`, this intentionally
    /// does not fall back to "unresolved but unshadowed" when symbol resolution
    /// misses. Use it for paths whose existing behavior requires concrete lib
    /// identity evidence.
    pub(crate) fn identifier_resolves_to_proven_lib_global(
        &self,
        idx: NodeIndex,
        name: &str,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(node) else {
            return false;
        };
        if ident.escaped_text.as_str() != name {
            return false;
        }
        let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(idx) else {
            return false;
        };
        if self.known_global_value_has_local_shadow(idx, name) {
            return false;
        }
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        symbol.escaped_name == name
            && (self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                || self.ctx.symbol_is_from_lib(sym_id))
    }

    pub(crate) fn known_global_value_has_local_shadow(&self, idx: NodeIndex, name: &str) -> bool {
        if let Some(mut scope_id) = self.ctx.binder.find_enclosing_scope(self.ctx.arena, idx) {
            let mut iterations = 0;
            while scope_id.is_some() {
                iterations += 1;
                if iterations > crate::state::MAX_TREE_WALK_ITERATIONS {
                    break;
                }
                let Some(scope) = self.ctx.binder.scopes.get(scope_id.0 as usize) else {
                    break;
                };
                if let Some(sym_id) = scope.table.get(name)
                    && !self.ctx.symbol_is_from_actual_lib(sym_id)
                    && !self.ctx.symbol_is_from_lib(sym_id)
                {
                    return true;
                }
                scope_id = scope.parent;
            }
        }

        let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(idx) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if symbol.escaped_name != name {
            return false;
        }
        if !self.ctx.symbol_is_from_actual_lib(sym_id) && !self.ctx.symbol_is_from_lib(sym_id) {
            return true;
        }

        let mut declarations = symbol.declarations.clone();
        if symbol.value_declaration.is_some() && !declarations.contains(&symbol.value_declaration) {
            declarations.push(symbol.value_declaration);
        }

        declarations
            .into_iter()
            .filter(|decl| decl.is_some())
            .any(|decl| self.current_arena_value_declares_name(sym_id, decl, name))
    }

    pub(crate) fn known_global_value_has_local_shadow_in_arena(
        &self,
        arena: &NodeArena,
        binder: &BinderState,
        idx: NodeIndex,
        name: &str,
    ) -> bool {
        if let Some(mut scope_id) = binder.find_enclosing_scope(arena, idx) {
            let mut iterations = 0;
            while scope_id.is_some() {
                iterations += 1;
                if iterations > crate::state::MAX_TREE_WALK_ITERATIONS {
                    break;
                }
                let Some(scope) = binder.scopes.get(scope_id.0 as usize) else {
                    break;
                };
                if let Some(sym_id) = scope.table.get(name) {
                    if binder.lib_symbol_ids.contains(&sym_id)
                        || self.ctx.symbol_is_from_actual_lib(sym_id)
                        || self.ctx.symbol_is_from_lib(sym_id)
                    {
                        scope_id = scope.parent;
                        continue;
                    }
                    let Some(symbol) = binder
                        .get_symbol(sym_id)
                        .or_else(|| self.ctx.binder.get_symbol(sym_id))
                    else {
                        return true;
                    };
                    if symbol.escaped_name == name
                        && symbol.declarations.iter().copied().any(|decl_idx| {
                            self.arena_value_declares_name(arena, binder, sym_id, decl_idx, name)
                        })
                    {
                        return true;
                    }
                }
                scope_id = scope.parent;
            }
        }

        let Some(sym_id) = binder.resolve_identifier(arena, idx) else {
            return false;
        };
        let Some(symbol) = binder
            .get_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol(sym_id))
        else {
            return false;
        };
        if symbol.escaped_name != name {
            return false;
        }
        let mut declarations = symbol.declarations.clone();
        if symbol.value_declaration.is_some() && !declarations.contains(&symbol.value_declaration) {
            declarations.push(symbol.value_declaration);
        }

        if !binder.lib_symbol_ids.contains(&sym_id)
            && !self.ctx.symbol_is_from_actual_lib(sym_id)
            && !self.ctx.symbol_is_from_lib(sym_id)
        {
            return declarations.into_iter().any(|decl_idx| {
                self.arena_value_declares_name(arena, binder, sym_id, decl_idx, name)
            });
        }

        declarations
            .into_iter()
            .any(|decl_idx| self.arena_value_declares_name(arena, binder, sym_id, decl_idx, name))
    }

    fn current_arena_value_declares_name(
        &self,
        sym_id: tsz_binder::SymbolId,
        decl_idx: NodeIndex,
        name: &str,
    ) -> bool {
        self.arena_value_declares_name(self.ctx.arena, self.ctx.binder, sym_id, decl_idx, name)
    }

    fn arena_value_declares_name(
        &self,
        arena: &NodeArena,
        binder: &BinderState,
        sym_id: tsz_binder::SymbolId,
        decl_idx: NodeIndex,
        name: &str,
    ) -> bool {
        if !decl_idx.is_some() {
            return false;
        }

        if let Some(arenas) = binder.declaration_arenas.get(&(sym_id, decl_idx)) {
            if !arenas
                .iter()
                .any(|known| std::ptr::eq(known.as_ref(), arena))
            {
                return false;
            }
        } else if binder.symbol_arenas.contains_key(&sym_id) {
            return false;
        }

        let Some(node) = arena.get(decl_idx) else {
            return false;
        };

        if let Some(var_decl) = arena.get_variable_declaration(node) {
            return arena
                .get_identifier_text(var_decl.name)
                .is_some_and(|decl_name| decl_name == name);
        }
        if let Some(class_decl) = arena.get_class(node) {
            return arena
                .get_identifier_text(class_decl.name)
                .is_some_and(|decl_name| decl_name == name);
        }
        if let Some(function_decl) = arena.get_function(node) {
            return arena
                .get_identifier_text(function_decl.name)
                .is_some_and(|decl_name| decl_name == name);
        }
        if let Some(enum_decl) = arena.get_enum(node) {
            return arena
                .get_identifier_text(enum_decl.name)
                .is_some_and(|decl_name| decl_name == name);
        }

        false
    }
}
