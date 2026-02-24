use crate::state::CheckerState;
use tsz_lowering::TypeLowering;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_solver::is_compiler_managed_type;

impl<'a> CheckerState<'a> {
    pub(crate) fn prime_lib_type_params(&mut self, name: &str) {
        use tsz_parser::parser::node::NodeAccess;

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

        let binder = &self.ctx.binder;
        let resolver = |node_idx: NodeIndex| -> Option<u32> {
            for (_, arena) in &decls_with_arenas {
                if let Some(ident_name) = arena.get_identifier_text(node_idx) {
                    if is_compiler_managed_type(ident_name) {
                        continue;
                    }
                    if let Some(found_sym) = binder.file_locals.get(ident_name) {
                        return Some(found_sym.0);
                    }
                }
            }
            if let Some(ident_name) = fallback_arena.get_identifier_text(node_idx) {
                if is_compiler_managed_type(ident_name) {
                    return None;
                }
                if let Some(found_sym) = binder.file_locals.get(ident_name) {
                    return Some(found_sym.0);
                }
            }
            None
        };
        let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::DefId> {
            resolver(node_idx)
                .map(|found| self.ctx.get_or_create_def_id(tsz_binder::SymbolId(found)))
        };
        let name_resolver = |ident_name: &str| -> Option<tsz_solver::DefId> {
            if is_compiler_managed_type(ident_name) {
                return None;
            }
            binder
                .file_locals
                .get(ident_name)
                .map(|found| self.ctx.get_or_create_def_id(found))
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
