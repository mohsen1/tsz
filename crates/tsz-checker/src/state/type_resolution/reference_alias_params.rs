//! Current-arena type-alias parameter lowering helpers.

use crate::query_boundaries::type_predicates::is_compiler_managed_type;
use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena, TypeAliasData};

impl CheckerState<'_> {
    pub(crate) fn collect_current_arena_type_alias_params_with_resolved_refs(
        &mut self,
        decl_arena: &NodeArena,
        type_alias: &TypeAliasData,
        effective_sym_id: SymbolId,
    ) -> Vec<tsz_solver::TypeParamInfo> {
        let referenced_defs = std::cell::RefCell::new(Vec::new());
        let params = {
            let resolve_current_type_symbol = |node_idx: NodeIndex| -> Option<(SymbolId, String)> {
                let name = decl_arena.get_identifier_text(node_idx)?.to_string();
                if let Some(raw_sym_id) = self.ctx.binder.resolve_identifier_with_filter(
                    decl_arena,
                    node_idx,
                    &[],
                    |candidate| {
                        let Some(symbol) = self.ctx.binder.get_symbol(candidate) else {
                            return false;
                        };
                        if symbol.escaped_name != name {
                            return false;
                        }
                        let typeish = symbol.has_any_flags(
                            symbol_flags::TYPE
                                | symbol_flags::ALIAS
                                | symbol_flags::REGULAR_ENUM
                                | symbol_flags::CONST_ENUM,
                        );
                        if !typeish {
                            return false;
                        }
                        let file_local =
                            self.ctx.binder.file_locals.get(name.as_str()) == Some(candidate);
                        let lib_like_file_local = file_local
                            && !symbol.has_any_flags(symbol_flags::ALIAS)
                            && (self.ctx.symbol_is_from_lib(candidate)
                                || symbol.decl_file_idx == u32::MAX);
                        !(is_compiler_managed_type(&name) && lib_like_file_local)
                    },
                ) {
                    let symbol = self.ctx.binder.get_symbol(raw_sym_id)?;
                    if self.reference_symbol_is_import_alias(symbol) {
                        let (target_sym_id, target_file_idx) =
                            self.reference_import_alias_export_target(symbol, &name)?;
                        let target_file_idx = target_file_idx.or_else(|| {
                            self.ctx
                                .binder
                                .get_symbol(target_sym_id)
                                .is_some_and(|target_symbol| target_symbol.escaped_name == name)
                                .then_some(self.ctx.current_file_idx)
                        });
                        if let Some(target_file_idx) = target_file_idx {
                            self.ctx
                                .register_symbol_file_target(target_sym_id, target_file_idx);
                        }
                        return Some((target_sym_id, name));
                    }
                    return Some((raw_sym_id, name));
                }

                self.resolve_type_symbol_for_lowering(node_idx)
                    .map(|sym| (SymbolId(sym), name))
            };
            let type_resolver = |node_idx: NodeIndex| {
                resolve_current_type_symbol(node_idx).map(|(sym_id, _)| sym_id.0)
            };
            let def_id_resolver = |node_idx: NodeIndex| {
                let (referenced_sym_id, referenced_name) = resolve_current_type_symbol(node_idx)?;
                let leaf_name = referenced_name
                    .rsplit('.')
                    .next()
                    .unwrap_or(&referenced_name);
                let def_id = self
                    .ctx
                    .get_or_create_def_id_for_symbol_name(referenced_sym_id, leaf_name);
                if referenced_sym_id != effective_sym_id {
                    referenced_defs
                        .borrow_mut()
                        .push((referenced_sym_id, def_id));
                }
                Some(def_id)
            };
            let value_resolver =
                |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
            let name_resolver =
                |type_name: &str| self.resolve_entity_name_text_to_def_id_for_lowering(type_name);
            tsz_lowering::TypeLowering::with_hybrid_resolver(
                decl_arena,
                self.ctx.types,
                &type_resolver,
                &def_id_resolver,
                &value_resolver,
            )
            .with_name_def_id_resolver(&name_resolver)
            .collect_type_alias_type_parameters(type_alias)
        };
        for (referenced_sym_id, def_id) in referenced_defs.into_inner() {
            crate::TypeNodeChecker::new(&mut self.ctx)
                .ensure_type_alias_resolved(referenced_sym_id, def_id);
        }
        params
    }
}
