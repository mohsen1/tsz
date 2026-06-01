//! Type-reference helpers for local import aliases whose raw `SymbolId`s collide with targets.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::{NodeArena, NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn extract_declared_type_params_for_local_import_alias(
        &mut self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> Vec<tsz_solver::TypeParamInfo> {
        let Some(alias) = self.local_import_alias(sym_id) else {
            return Vec::new();
        };
        if self.local_import_alias_is_import_equals(alias) {
            return Vec::new();
        }
        let Some(module_specifier) = alias.import_module.clone() else {
            return Vec::new();
        };
        let import_name = alias
            .import_name
            .as_deref()
            .unwrap_or(&alias.escaped_name)
            .to_owned();
        if alias.escaped_name != expected_name {
            return Vec::new();
        }

        let Some(target_file_idx) = self
            .ctx
            .resolve_import_target_from_file(self.ctx.current_file_idx, &module_specifier)
        else {
            return Vec::new();
        };
        let Some(target_binder) = self.ctx.get_binder_for_file(target_file_idx) else {
            return Vec::new();
        };
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let Some(file_name) = target_arena
            .source_files
            .first()
            .map(|source_file| source_file.file_name.as_str())
        else {
            return Vec::new();
        };
        let Some(target_sym_id) = target_binder
            .module_exports
            .get(file_name)
            .and_then(|exports| exports.get(&import_name))
            .or_else(|| target_binder.file_locals.get(&import_name))
        else {
            return Vec::new();
        };
        let Some(target_symbol) = target_binder.get_symbol(target_sym_id) else {
            return Vec::new();
        };

        self.ctx
            .register_symbol_file_target(target_sym_id, target_file_idx);
        for decl_idx in target_symbol.all_declarations() {
            let decl_arenas: Vec<&NodeArena> = target_binder
                .declaration_arenas
                .get(&(target_sym_id, decl_idx))
                .map(|arenas| arenas.iter().map(std::convert::AsRef::as_ref).collect())
                .or_else(|| {
                    target_binder
                        .symbol_arenas
                        .get(&target_sym_id)
                        .map(|arena| vec![arena.as_ref()])
                })
                .unwrap_or_else(|| vec![target_arena]);

            for decl_arena in decl_arenas {
                let Some(node) = decl_arena.get(decl_idx) else {
                    continue;
                };
                if let Some(type_alias) = decl_arena.get_type_alias(node) {
                    return self
                        .collect_import_alias_type_params_from_alias(decl_arena, type_alias);
                }
                if decl_arena.get_interface(node).is_some() {
                    return self
                        .collect_import_alias_type_params_from_interface(decl_arena, decl_idx);
                }
                if let Some(class) = decl_arena.get_class(node)
                    && let Some(type_parameters) = class.type_parameters.as_ref()
                {
                    return self.with_import_alias_lowering(decl_arena, |lowering| {
                        lowering.collect_type_parameters(type_parameters)
                    });
                }
            }
        }

        Vec::new()
    }

    fn collect_import_alias_type_params_from_alias(
        &self,
        decl_arena: &NodeArena,
        type_alias: &tsz_parser::parser::node::TypeAliasData,
    ) -> Vec<tsz_solver::TypeParamInfo> {
        self.with_import_alias_lowering(decl_arena, |lowering| {
            lowering.collect_type_alias_type_parameters(type_alias)
        })
    }

    fn collect_import_alias_type_params_from_interface(
        &self,
        decl_arena: &NodeArena,
        decl_idx: NodeIndex,
    ) -> Vec<tsz_solver::TypeParamInfo> {
        self.with_import_alias_lowering(decl_arena, |lowering| {
            lowering.collect_merged_interface_type_parameters(&[(decl_idx, decl_arena)])
        })
    }

    fn with_import_alias_lowering<R>(
        &self,
        decl_arena: &NodeArena,
        f: impl FnOnce(tsz_lowering::TypeLowering<'_>) -> R,
    ) -> R {
        let type_resolver = move |node_idx: NodeIndex| {
            decl_arena.get_identifier_text(node_idx).and_then(|name| {
                (!self.ctx.file_local_type_shadow_for_lib_name(name))
                    .then(|| self.resolve_actual_lib_name_to_def_id_for_lowering(name))
                    .flatten()
                    .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(name))
                    .and_then(|def_id| self.ctx.def_to_symbol_id_with_fallback(def_id))
                    .map(|sym_id| sym_id.0)
            })
        };
        let def_id_resolver = move |node_idx: NodeIndex| {
            decl_arena.get_identifier_text(node_idx).and_then(|name| {
                (!self.ctx.file_local_type_shadow_for_lib_name(name))
                    .then(|| self.resolve_actual_lib_name_to_def_id_for_lowering(name))
                    .flatten()
                    .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(name))
            })
        };
        let value_resolver =
            move |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
        let name_resolver = move |type_name: &str| {
            (!self.ctx.file_local_type_shadow_for_lib_name(type_name))
                .then(|| self.resolve_actual_lib_name_to_def_id_for_lowering(type_name))
                .flatten()
                .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(type_name))
        };

        let lowering = tsz_lowering::TypeLowering::with_hybrid_resolver(
            decl_arena,
            self.ctx.types,
            &type_resolver,
            &def_id_resolver,
            &value_resolver,
        )
        .with_name_def_id_resolver(&name_resolver)
        .prefer_name_def_id_resolution();
        f(lowering)
    }

    pub(crate) fn class_instance_type_with_params_from_local_import_alias(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        let alias = self.local_import_alias(sym_id)?;
        if self.local_import_alias_is_import_equals(alias) {
            return None;
        }
        let module_specifier = alias.import_module.clone()?;
        let import_name = alias
            .import_name
            .as_deref()
            .unwrap_or(&alias.escaped_name)
            .to_owned();
        let target_file_idx = self
            .ctx
            .resolve_import_target_from_file(self.ctx.current_file_idx, &module_specifier)?;
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let target_binder = self.ctx.get_binder_for_file(target_file_idx)?;
        let file_name = target_arena.source_files.first()?.file_name.as_str();
        let target_sym_id = target_binder
            .module_exports
            .get(file_name)
            .and_then(|exports| exports.get(&import_name))
            .or_else(|| target_binder.file_locals.get(&import_name))?;
        let target_symbol = target_binder.get_symbol(target_sym_id)?;
        if !target_symbol.has_any_flags(symbol_flags::CLASS) {
            return None;
        }

        self.ctx
            .register_symbol_file_target(target_sym_id, target_file_idx);
        let result = self.delegate_cross_arena_class_instance_type(target_sym_id);
        if let Some((instance_type, _)) = result.as_ref()
            && *instance_type != TypeId::ERROR
            && *instance_type != TypeId::UNKNOWN
        {
            self.ctx
                .symbol_instance_types
                .insert(sym_id, *instance_type);
            self.ctx
                .symbol_instance_types
                .insert(target_sym_id, *instance_type);
        }

        result
    }

    pub(crate) fn local_import_alias_is_import_equals(&self, alias: &tsz_binder::Symbol) -> bool {
        alias.all_declarations().iter().any(|&decl_idx| {
            self.ctx
                .arena
                .get(decl_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
        })
    }
}
