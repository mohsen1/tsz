use super::lib_resolution::resolve_lib_node_in_arenas;
use crate::state::CheckerState;
use tsz_lowering::TypeLowering;
use tsz_parser::parser::{NodeArena, NodeIndex};

impl<'a> CheckerState<'a> {
    pub(crate) fn prime_lib_type_params(&mut self, name: &str) {
        let Some(sym_id) = self.ctx.binder.file_locals.get(name) else {
            return;
        };
        let def_id = self.ctx.get_or_create_def_id(sym_id);
        if self.ctx.get_def_type_params(def_id).is_some() {
            return;
        }

        let lib_contexts = &self.ctx.lib_contexts;
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return;
        };
        let fallback_arena: &NodeArena = self
            .ctx
            .binder
            .symbol_arenas
            .get(&sym_id)
            .map(std::convert::AsRef::as_ref)
            .or_else(|| lib_contexts.first().map(|ctx| ctx.arena.as_ref()))
            .unwrap_or(self.ctx.arena);

        let decls_with_arenas: Vec<(NodeIndex, &NodeArena)> = symbol
            .declarations
            .iter()
            .flat_map(|&decl_idx| {
                if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                    arenas
                        .iter()
                        .map(|arc| (decl_idx, arc.as_ref()))
                        .collect::<Vec<_>>()
                } else {
                    vec![(decl_idx, fallback_arena)]
                }
            })
            .collect();
        if decls_with_arenas.is_empty() {
            return;
        }

        // Use the stable identity helper instead of a local resolver closure.
        let binder = &self.ctx.binder;
        let resolver = |node_idx: NodeIndex| -> Option<u32> {
            resolve_lib_node_in_arenas(binder, node_idx, &decls_with_arenas, fallback_arena)
        };
        let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
            resolver(node_idx)
                .map(|found| self.ctx.get_or_create_def_id(tsz_binder::SymbolId(found)))
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
