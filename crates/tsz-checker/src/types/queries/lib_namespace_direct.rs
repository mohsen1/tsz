//! Narrow direct lowering helpers for namespace-qualified bundled-lib symbols.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_lowering::TypeLowering;
use tsz_parser::NodeIndex;
use tsz_solver::TypeId;

use super::lib_resolution::{
    collect_lib_decls_with_arenas_in_contexts, dedup_decl_arenas, lib_def_id_from_node,
    resolve_lib_fallback_arena, resolve_lib_node_in_arenas,
};

fn allow_direct_lib_interface_heritage(cache_name: &str) -> bool {
    matches!(cache_name, "Iterator" | "Intl.Locale")
}

impl<'a> CheckerState<'a> {
    pub(crate) fn resolve_lib_namespace_export_symbol(
        &self,
        namespace: &str,
        export_name: &str,
    ) -> Option<SymbolId> {
        let lib_binders = self.get_lib_binders();
        let namespace_sym_id = self.resolve_lib_symbol_by_name(namespace)?;
        if !self
            .ctx
            .symbol_is_from_actual_or_cloned_lib(namespace_sym_id)
        {
            return None;
        }
        let namespace_symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(namespace_sym_id, &lib_binders)?;
        namespace_symbol
            .exports
            .as_ref()
            .and_then(|exports| exports.get(export_name))
    }

    pub(crate) fn resolve_lib_interface_type_by_symbol(
        &mut self,
        cache_name: &str,
        sym_id: SymbolId,
    ) -> Option<TypeId> {
        if self.ctx.skip_lib_type_resolution {
            return None;
        }

        if let Some(cached) = self.ctx.lib_type_resolution_cache.get(cache_name)
            && self.cached_lib_type_is_usable(cache_name, *cached)
        {
            return *cached;
        }

        if !self.lib_name_locally_augmented(cache_name)
            && let Some(ref shared) = self.ctx.shared_lib_type_cache
            && let Some(entry) = shared.get(cache_name)
        {
            let cached = *entry;
            if self.cached_lib_type_is_usable(cache_name, cached) {
                self.ctx
                    .lib_type_resolution_cache
                    .insert(cache_name.to_string(), cached);
                return cached;
            }
        }

        self.ctx
            .lib_type_resolution_cache
            .insert(cache_name.to_string(), None);

        let lib_contexts = self.ctx.lib_contexts.clone();
        let lib_binders = self.get_lib_binders();
        let (mut declarations, has_interface, has_type_alias) = {
            let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
            (
                symbol.declarations.clone(),
                symbol.has_any_flags(symbol_flags::INTERFACE),
                symbol.has_any_flags(symbol_flags::TYPE_ALIAS),
            )
        };
        if !has_interface || has_type_alias || declarations.is_empty() {
            self.ctx
                .lib_type_resolution_cache
                .insert(cache_name.to_string(), None);
            return None;
        }
        if let Some((namespace, export_name)) = cache_name.split_once('.') {
            for lib_ctx in lib_contexts.iter().take(self.ctx.actual_lib_file_count) {
                let Some(namespace_sym_id) = lib_ctx.binder.file_locals.get(namespace) else {
                    continue;
                };
                let Some(namespace_symbol) = lib_ctx.binder.get_symbol(namespace_sym_id) else {
                    continue;
                };
                let Some(export_sym_id) = namespace_symbol
                    .exports
                    .as_ref()
                    .and_then(|exports| exports.get(export_name))
                else {
                    continue;
                };
                if export_sym_id == sym_id {
                    continue;
                }
                let Some(export_symbol) = lib_ctx.binder.get_symbol(export_sym_id) else {
                    continue;
                };
                if export_symbol.has_any_flags(symbol_flags::INTERFACE)
                    && !export_symbol.has_any_flags(symbol_flags::TYPE_ALIAS)
                {
                    declarations.extend(export_symbol.declarations.iter().copied());
                }
            }
        }

        let fallback_arena =
            resolve_lib_fallback_arena(self.ctx.binder, sym_id, &lib_contexts, self.ctx.arena);
        let decls_with_arenas = collect_lib_decls_with_arenas_in_contexts(
            self.ctx.binder,
            sym_id,
            &declarations,
            fallback_arena,
            &lib_contexts,
            Some(self.ctx.arena),
        );
        if decls_with_arenas.is_empty() {
            self.ctx
                .lib_type_resolution_cache
                .insert(cache_name.to_string(), None);
            return None;
        }
        if decls_with_arenas.iter().any(|&(decl_idx, arena)| {
            arena
                .get(decl_idx)
                .and_then(|node| arena.get_interface(node))
                .and_then(|interface| interface.heritage_clauses.as_ref())
                .is_some_and(|clauses| !clauses.nodes.is_empty())
        }) && !allow_direct_lib_interface_heritage(cache_name)
        {
            self.ctx
                .lib_type_resolution_cache
                .insert(cache_name.to_string(), None);
            return None;
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
        let namespace = cache_name.split_once('.').map(|(namespace, _)| namespace);
        let name_resolver = |type_name: &str| -> Option<tsz_solver::DefId> {
            if let Some(namespace) = namespace
                && let Some(sym_id) = self.resolve_lib_namespace_export_symbol(namespace, type_name)
            {
                return Some(self.ctx.get_lib_def_id(sym_id));
            }
            self.resolve_actual_lib_name_to_def_id_for_lowering(type_name)
                .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(type_name))
        };

        let lowering = TypeLowering::with_hybrid_resolver(
            fallback_arena,
            self.ctx.types,
            &resolver,
            &def_id_resolver,
            &resolver,
        )
        .with_builtin_iterator_return_type(self.builtin_iterator_return_intrinsic_type())
        .with_name_def_id_resolver(&name_resolver);
        let lowering =
            if self.ctx.all_binders.is_some() || self.ctx.global_file_locals_index.is_some() {
                lowering.prefer_name_def_id_resolution()
            } else {
                lowering
            };

        let deduped = dedup_decl_arenas(&decls_with_arenas);
        let (ty, params) =
            lowering.lower_merged_interface_declarations_with_symbol(&deduped, Some(sym_id));
        if ty == TypeId::ERROR || ty == TypeId::UNKNOWN {
            self.ctx
                .lib_type_resolution_cache
                .insert(cache_name.to_string(), None);
            return None;
        }

        self.ctx.register_lib_def_resolved(sym_id, ty, params);
        self.ensure_relation_input_ready(ty);
        self.ctx
            .lib_type_resolution_cache
            .insert(cache_name.to_string(), Some(ty));
        if !self.lib_name_locally_augmented(cache_name)
            && let Some(ref shared) = self.ctx.shared_lib_type_cache
        {
            shared.insert(cache_name.to_string(), Some(ty));
        }

        Some(ty)
    }
}
