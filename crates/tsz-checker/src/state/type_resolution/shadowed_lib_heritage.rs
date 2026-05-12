//! Helpers for class heritage expressions that shadow lib value declarations.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn heritage_expression_shadows_nonconstructable_lib_value(
        &mut self,
        expr_idx: NodeIndex,
        heritage_sym: tsz_binder::SymbolId,
    ) -> bool {
        use tsz_binder::symbol_flags;

        let Some(symbol) = self.get_symbol_globally(heritage_sym) else {
            return false;
        };
        if !symbol.has_any_flags(symbol_flags::CLASS) {
            return false;
        }
        let name = symbol.escaped_name.clone();
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        let Some(expr_ident) = self.ctx.arena.get_identifier(expr_node) else {
            return false;
        };
        if expr_ident.escaped_text.as_str() != name {
            return false;
        }

        let shadowed_lib_id = if symbol.has_any_flags(symbol_flags::VARIABLE)
            && self.ctx.binder.lib_symbol_ids.contains(&heritage_sym)
        {
            Some(heritage_sym)
        } else {
            self.ctx.binder.lib_symbol_ids.iter().find_map(|&lib_id| {
                if lib_id == heritage_sym {
                    return None;
                }
                self.ctx.binder.get_symbol(lib_id).and_then(|lib_symbol| {
                    (lib_symbol.escaped_name == name
                        && lib_symbol.has_any_flags(symbol_flags::VARIABLE))
                    .then_some(lib_id)
                })
            })
        };
        let Some(shadowed_lib_id) = shadowed_lib_id else {
            return false;
        };
        let Some(lib_sym) = self.ctx.binder.get_symbol(shadowed_lib_id) else {
            return false;
        };
        let declarations = lib_sym.declarations.clone();

        for decl_idx in declarations {
            let lib_arena = self
                .ctx
                .binder
                .declaration_arenas
                .get(&(shadowed_lib_id, decl_idx))
                .and_then(|arenas| arenas.first())
                .filter(|arena| !std::ptr::eq(arena.as_ref(), self.ctx.arena))
                .map(std::sync::Arc::clone)
                .or_else(|| {
                    if self.ctx.arena.get(decl_idx).is_none() {
                        self.ctx.binder.symbol_arenas.get(&shadowed_lib_id).cloned()
                    } else {
                        None
                    }
                });
            let Some(lib_arena) = lib_arena else {
                continue;
            };
            let Some(node) = lib_arena.get(decl_idx) else {
                continue;
            };
            let Some(var_decl) = lib_arena.get_variable_declaration(node) else {
                continue;
            };
            let Some(type_node) = lib_arena.get(var_decl.type_annotation) else {
                continue;
            };
            let Some(type_ref) = lib_arena.get_type_ref(type_node) else {
                continue;
            };
            let Some(type_name_node) = lib_arena.get(type_ref.type_name) else {
                continue;
            };
            let Some(type_name) = lib_arena.get_identifier(type_name_node) else {
                continue;
            };
            let Some(lib_type) = self.resolve_lib_type_by_name(type_name.escaped_text.as_str())
            else {
                continue;
            };
            if lib_type == TypeId::ERROR {
                continue;
            }
            let has_construct_sigs =
                crate::query_boundaries::common::construct_signatures_for_type(
                    self.ctx.types,
                    lib_type,
                )
                .is_some_and(|sigs| !sigs.is_empty());
            if !has_construct_sigs {
                return true;
            }
        }

        false
    }
}
