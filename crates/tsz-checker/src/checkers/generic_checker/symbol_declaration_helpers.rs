//! Symbol declaration introspection helpers used by generic constraint
//! validation. Split from `constraint_validation.rs` to keep that file under
//! the architecture LOC guard; behavior is unchanged.

use crate::state::CheckerState;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    /// Check if a symbol's declaration has type parameters, even if they couldn't be
    /// resolved via `get_type_params_for_symbol` (e.g., cross-arena lib types).
    pub(crate) fn symbol_declaration_has_type_parameters(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        let lib_binders = self.get_lib_binders();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders);
        let Some(symbol) = symbol else {
            return false;
        };

        // Check the value declaration and all declarations for type parameters
        for decl_idx in symbol.all_declarations() {
            // Try current arena first
            if let Some(node) = self.ctx.arena.get(decl_idx) {
                if let Some(ta) = self.ctx.arena.get_type_alias(node) {
                    if ta.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(iface) = self.ctx.arena.get_interface(node) {
                    if iface.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(class) = self.ctx.arena.get_class(node) {
                    if class.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
            }

            // Try cross-arena (lib files)
            if let Some(decl_arena) = self.ctx.binder.symbol_arenas.get(&sym_id)
                && let Some(node) = decl_arena.get(decl_idx)
            {
                if let Some(ta) = decl_arena.get_type_alias(node) {
                    if ta.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(iface) = decl_arena.get_interface(node) {
                    if iface.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(class) = decl_arena.get_class(node) {
                    if class.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
            }

            // Try declaration_arenas
            if let Some(decl_arena) = self
                .ctx
                .binder
                .declaration_arenas
                .get(&(sym_id, decl_idx))
                .and_then(|v| v.first())
                && let Some(node) = decl_arena.get(decl_idx)
            {
                if let Some(ta) = decl_arena.get_type_alias(node) {
                    if ta.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(iface) = decl_arena.get_interface(node) {
                    if iface.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
                if let Some(class) = decl_arena.get_class(node) {
                    if class.type_parameters.is_some() {
                        return true;
                    }
                    continue;
                }
            }
        }

        false
    }

    /// Return `true` when `sym_id` resolves to a non-generic type alias
    /// declaration whose body is the `any` keyword written explicitly.
    ///
    /// The TS2315 ("Type X is not generic") emission path skips emitting
    /// when the resolved symbol type is `any`, because cross-arena lib
    /// symbols whose declarations couldn't be located also surface as
    /// `any`. That guard over-suppresses for explicit non-generic alias
    /// declarations like `type Chunk = any`. tsc 6.0.3 emits TS2315 for
    /// those (e.g. `Chunk<X>`), and so should we.
    pub(crate) fn symbol_declaration_body_is_explicit_any(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        let lib_binders = self.get_lib_binders();
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
            return false;
        };
        if !symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS) {
            return false;
        }
        for decl_idx in symbol.all_declarations() {
            let decl_node = self
                .ctx
                .arena
                .get(decl_idx)
                .or_else(|| {
                    self.ctx
                        .binder
                        .symbol_arenas
                        .get(&sym_id)
                        .and_then(|a| a.get(decl_idx))
                })
                .or_else(|| {
                    self.ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                        .and_then(|v| v.first())
                        .and_then(|a| a.get(decl_idx))
                });
            let Some(decl_node) = decl_node else { continue };
            let alias = self
                .ctx
                .arena
                .get_type_alias(decl_node)
                .or_else(|| {
                    self.ctx
                        .binder
                        .symbol_arenas
                        .get(&sym_id)
                        .and_then(|a| a.get_type_alias(decl_node))
                })
                .or_else(|| {
                    self.ctx
                        .binder
                        .declaration_arenas
                        .get(&(sym_id, decl_idx))
                        .and_then(|v| v.first())
                        .and_then(|a| a.get_type_alias(decl_node))
                });
            let Some(alias) = alias else { continue };
            if alias.type_parameters.is_some() {
                return false;
            }
            // Resolve the arena that holds `alias.type_node`. The body may
            // live in the current arena (most common), in `symbol_arenas`
            // for cross-arena lib symbols, or in `declaration_arenas` for
            // multi-file declaration merges. Borrow as `&NodeArena` so the
            // lookup methods below see the same concrete type regardless
            // of which container the arena was stored in.
            let arena_ref: &tsz_parser::parser::NodeArena =
                if self.ctx.arena.get(alias.type_node).is_some() {
                    self.ctx.arena
                } else if let Some(a) = self.ctx.binder.symbol_arenas.get(&sym_id) {
                    a
                } else if let Some(v) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx))
                    && let Some(a) = v.first()
                {
                    a
                } else {
                    continue;
                };
            let Some(body_node) = arena_ref.get(alias.type_node) else {
                continue;
            };
            // Direct `any` keyword.
            if body_node.kind == SyntaxKind::AnyKeyword as u16 {
                return true;
            }
            // Bare TypeReference whose name is the identifier `any`. The
            // parser models `type X = any` as a TypeReference rather than
            // a plain AnyKeyword token, so this is the actual surface form.
            if body_node.kind == syntax_kind_ext::TYPE_REFERENCE
                && let Some(type_ref) = arena_ref.get_type_ref(body_node)
                && type_ref.type_arguments.is_none()
                && let Some(name_node) = arena_ref.get(type_ref.type_name)
                && let Some(ident) = arena_ref.get_identifier(name_node)
                && ident.escaped_text == "any"
            {
                return true;
            }
        }
        false
    }
}
