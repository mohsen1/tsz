//! Lazy identity preservation for simple actual-lib interface references.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn try_lazy_actual_lib_interface_reference(
        &mut self,
        sym_id: SymbolId,
        escaped_name: &str,
        flags: u32,
        declarations: &[NodeIndex],
        is_merged_with_namespace: bool,
        should_force_interface_decl_path: bool,
    ) -> Option<TypeId> {
        if is_merged_with_namespace
            || should_force_interface_decl_path
            || (flags & symbol_flags::TYPE_ALIAS) != 0
            || (flags & symbol_flags::CLASS) != 0
            || !self.ctx.has_lib_loaded()
            || !self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
            || self.ctx.file_local_type_shadow_for_lib_name(escaped_name)
            || self.lib_name_locally_augmented(escaped_name)
            || self.interface_declarations_have_index_signature(sym_id, declarations)
        {
            return None;
        }

        let def_id = self
            .ctx
            .get_or_create_def_id_for_symbol_name(sym_id, escaped_name);
        if self.ctx.get_def_type_params(def_id).is_none() {
            let params =
                self.extract_declared_type_params_for_reference_symbol(sym_id, escaped_name);
            if !params.is_empty() {
                self.ctx.insert_def_type_params(def_id, params);
            }
        }

        if self
            .ctx
            .get_def_type_params(def_id)
            .is_some_and(|params| !params.is_empty())
        {
            return None;
        }

        Some(self.ctx.types.lazy(def_id))
    }

    fn interface_declarations_have_index_signature(
        &self,
        sym_id: SymbolId,
        declarations: &[NodeIndex],
    ) -> bool {
        declarations.iter().copied().any(|decl_idx| {
            let declaration_arenas = self
                .ctx
                .binder
                .declaration_arenas
                .get(&(sym_id, decl_idx))
                .cloned();

            if let Some(declaration_arenas) = declaration_arenas {
                return declaration_arenas.iter().any(|arena| {
                    Self::interface_declaration_has_index_signature(decl_idx, arena.as_ref())
                });
            }

            let arena = self
                .ctx
                .binder
                .arena_for_declaration_or(sym_id, decl_idx, self.ctx.arena);
            Self::interface_declaration_has_index_signature(decl_idx, arena)
        })
    }

    fn interface_declaration_has_index_signature(decl_idx: NodeIndex, arena: &NodeArena) -> bool {
        arena
            .get(decl_idx)
            .and_then(|node| arena.get_interface(node))
            .is_some_and(|interface| {
                interface.members.nodes.iter().copied().any(|member_idx| {
                    arena
                        .get(member_idx)
                        .is_some_and(|member| member.kind == syntax_kind_ext::INDEX_SIGNATURE)
                })
            })
    }
}
