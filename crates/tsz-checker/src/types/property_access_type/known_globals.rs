//! Helpers for distinguishing built-in globals from same-named local values.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;

impl<'a> CheckerState<'a> {
    pub(crate) fn known_global_value_has_local_shadow(&self, idx: NodeIndex, name: &str) -> bool {
        let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(idx) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if symbol.escaped_name != name {
            return false;
        }

        let mut declarations = symbol.declarations.clone();
        if symbol.value_declaration.is_some() && !declarations.contains(&symbol.value_declaration) {
            declarations.push(symbol.value_declaration);
        }

        declarations
            .into_iter()
            .any(|decl_idx| self.current_arena_value_declares_name(sym_id, decl_idx, name))
    }

    fn current_arena_value_declares_name(
        &self,
        sym_id: tsz_binder::SymbolId,
        decl_idx: NodeIndex,
        name: &str,
    ) -> bool {
        if !decl_idx.is_some() {
            return false;
        }

        if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
            if !arenas
                .iter()
                .any(|arena| std::ptr::eq(arena.as_ref(), self.ctx.arena))
            {
                return false;
            }
        } else if self.ctx.binder.symbol_arenas.contains_key(&sym_id) {
            return false;
        }

        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };

        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) {
            return self
                .ctx
                .arena
                .get_identifier_text(var_decl.name)
                .is_some_and(|decl_name| decl_name == name);
        }
        if let Some(class_decl) = self.ctx.arena.get_class(node) {
            return self
                .ctx
                .arena
                .get_identifier_text(class_decl.name)
                .is_some_and(|decl_name| decl_name == name);
        }
        if let Some(function_decl) = self.ctx.arena.get_function(node) {
            return self
                .ctx
                .arena
                .get_identifier_text(function_decl.name)
                .is_some_and(|decl_name| decl_name == name);
        }
        if let Some(enum_decl) = self.ctx.arena.get_enum(node) {
            return self
                .ctx
                .arena
                .get_identifier_text(enum_decl.name)
                .is_some_and(|decl_name| decl_name == name);
        }

        false
    }
}
