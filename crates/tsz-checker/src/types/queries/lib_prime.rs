use super::lib_resolution::{
    collect_lib_decls_with_arenas, lib_def_id_from_node, resolve_lib_fallback_arena,
    resolve_lib_node_in_arenas,
};
use crate::state::CheckerState;
use tsz_lowering::TypeLowering;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    pub(crate) fn prime_lib_type_params(&mut self, name: &str) {
        let Some(sym_id) = self.ctx.binder.file_locals.get(name) else {
            return;
        };
        // Use the stable `get_lib_def_id` helper: prefers pre-populated DefIds
        // and falls back to on-demand creation for symbols that semantic_defs
        // missed. This avoids silently skipping type param priming when
        // pre-population has gaps.
        let def_id = self.ctx.get_lib_def_id(sym_id);
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

        let binder = &self.ctx.binder;
        let resolver = |node_idx: NodeIndex| -> Option<u32> {
            resolve_lib_node_in_arenas(binder, node_idx, &decls_with_arenas, fallback_arena)
                .map(|sym_id| sym_id.0)
        };
        let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
            lib_def_id_from_node(
                &self.ctx,
                binder,
                node_idx,
                &decls_with_arenas,
                fallback_arena,
            )
        };
        let name_resolver = |type_name: &str| -> Option<tsz_solver::DefId> {
            self.resolve_entity_name_text_to_def_id_for_lowering(type_name)
        };

        let lazy_type_params_resolver =
            |def_id: tsz_solver::def::DefId| self.ctx.get_def_type_params(def_id);

        let lowering = TypeLowering::with_hybrid_resolver(
            fallback_arena,
            self.ctx.types,
            &resolver,
            &def_id_resolver,
            &|_| None,
        )
        .with_lazy_type_params_resolver(&lazy_type_params_resolver)
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
