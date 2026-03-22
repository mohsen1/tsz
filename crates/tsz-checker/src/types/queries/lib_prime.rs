use super::lib_resolution::{
    collect_lib_decls_with_arenas, resolve_lib_fallback_arena, resolve_lib_node_in_arenas,
};
use crate::state::CheckerState;
use tsz_lowering::TypeLowering;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    pub(crate) fn prime_lib_type_params(&mut self, name: &str) {
        let Some(sym_id) = self.ctx.binder.file_locals.get(name) else {
            return;
        };
        // Lib symbols are pre-populated at checker construction
        // (pre_populate_def_ids_from_lib_binders); no on-demand creation needed.
        let Some(def_id) = self.ctx.get_existing_def_id(sym_id) else {
            return;
        };
        if self.ctx.get_def_type_params(def_id).is_some() {
            return;
        }

        let lib_contexts = &self.ctx.lib_contexts;
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return;
        };
        let fallback_arena =
            resolve_lib_fallback_arena(self.ctx.binder, sym_id, lib_contexts, self.ctx.arena);

        // prime_lib_type_params has no user-arena context (no local augmentations),
        // so pass None for user_arena.
        let decls_with_arenas = collect_lib_decls_with_arenas(
            self.ctx.binder,
            sym_id,
            &symbol.declarations,
            fallback_arena,
            None,
        );
        if decls_with_arenas.is_empty() {
            return;
        }

        // Use the stable identity helper instead of a local resolver closure.
        let binder = &self.ctx.binder;
        let resolver = |node_idx: NodeIndex| -> Option<u32> {
            resolve_lib_node_in_arenas(binder, node_idx, &decls_with_arenas, fallback_arena)
        };
        let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
            resolver(node_idx).map(|raw| self.ctx.get_lib_def_id(tsz_binder::SymbolId(raw)))
        };
        let name_resolver = |type_name: &str| -> Option<tsz_solver::DefId> {
            self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
        };

        let lowering = TypeLowering::with_hybrid_resolver(
            fallback_arena,
            self.ctx.types,
            &resolver,
            &def_id_resolver,
            &|_| None,
        )
        .with_name_def_id_resolver(&name_resolver);

        let mut params = lowering.collect_merged_interface_type_parameters(&decls_with_arenas);
        if params.is_empty() {
            for (decl_idx, decl_arena) in &decls_with_arenas {
                if let Some(node) = decl_arena.get(*decl_idx)
                    && let Some(alias) = decl_arena.get_type_alias(node)
                {
                    let alias_lowering = lowering.with_arena(decl_arena);
                    params = alias_lowering.collect_type_alias_type_parameters(alias);
                    if !params.is_empty() {
                        break;
                    }
                }
            }
        }

        if !params.is_empty() {
            self.ctx.insert_def_type_params(def_id, params);
        }
    }
}
