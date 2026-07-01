//! Declared type-parameter caching for `TypeNodeChecker`.

use super::type_node::TypeNodeChecker;
use crate::query_boundaries::type_predicates::is_compiler_managed_type;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    pub(super) fn ensure_declared_type_params_cached(
        &self,
        sym_id: tsz_binder::SymbolId,
        def_id: tsz_solver::def::DefId,
    ) {
        if self.ctx.get_def_type_params(def_id).is_some() {
            return;
        }
        let Some(symbol) = self.get_symbol_from_any_context(sym_id) else {
            return;
        };
        if !symbol.has_any_flags(
            tsz_binder::symbol_flags::TYPE_ALIAS
                | tsz_binder::symbol_flags::INTERFACE
                | tsz_binder::symbol_flags::CLASS,
        ) {
            return;
        }

        let auth_binder: &tsz_binder::BinderState = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .filter(|&f| f != self.ctx.current_file_idx)
            .and_then(|f| self.ctx.get_binder_for_file(f))
            .unwrap_or(self.ctx.binder);
        let mut decls_with_arenas = Vec::new();
        for &decl_idx in &symbol.declarations {
            if let Some(arenas) = auth_binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                decls_with_arenas.extend(arenas.iter().map(|arena| (decl_idx, arena.as_ref())));
            } else if let Some(arena) = auth_binder.symbol_arenas.get(&sym_id) {
                decls_with_arenas.push((decl_idx, arena.as_ref()));
            } else if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx))
            {
                decls_with_arenas.extend(arenas.iter().map(|arena| (decl_idx, arena.as_ref())));
            } else if let Some(arena) = self.ctx.binder.symbol_arenas.get(&sym_id) {
                decls_with_arenas.push((decl_idx, arena.as_ref()));
            } else {
                decls_with_arenas.push((decl_idx, self.ctx.arena));
            }
        }

        let def_id_for_symbol =
            |referenced_sym_id: tsz_binder::SymbolId, name: &str| -> tsz_solver::def::DefId {
                let leaf_name = name.rsplit('.').next().unwrap_or(name);
                let lib_binders: Vec<_> = self
                    .ctx
                    .lib_contexts
                    .iter()
                    .map(|ctx| std::sync::Arc::clone(&ctx.binder))
                    .collect();
                let is_lib_global = self
                    .ctx
                    .binder
                    .get_global_type_with_libs(leaf_name, &lib_binders)
                    .is_some_and(|sym_id| sym_id == referenced_sym_id)
                    || lib_binders
                        .iter()
                        .any(|lib| lib.file_locals.get(leaf_name) == Some(referenced_sym_id));
                let authoritative_symbol_exists = self
                    .ctx
                    .resolve_symbol_file_index(referenced_sym_id)
                    .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
                    .and_then(|binder| binder.get_symbol(referenced_sym_id))
                    .is_some_and(|symbol| symbol.escaped_name == leaf_name);

                if is_lib_global && !authoritative_symbol_exists {
                    self.ctx
                        .get_canonical_lib_def_id(leaf_name, referenced_sym_id)
                } else {
                    self.ctx
                        .get_or_create_def_id_for_symbol_name(referenced_sym_id, leaf_name)
                }
            };

        for &(decl_idx, decl_arena) in &decls_with_arenas {
            let Some(node) = decl_arena.get(decl_idx) else {
                continue;
            };
            let decl_binder = self
                .ctx
                .get_binder_for_arena(decl_arena)
                .unwrap_or(auth_binder);
            let namespace_prefix = self.declaration_namespace_prefix(decl_arena, decl_idx);
            let resolve_text_symbol = |name: &str| -> Option<tsz_binder::SymbolId> {
                namespace_prefix
                    .as_ref()
                    .and_then(|prefix| {
                        let mut scoped = String::with_capacity(prefix.len() + 1 + name.len());
                        scoped.push_str(prefix);
                        scoped.push('.');
                        scoped.push_str(name);
                        self.resolve_entity_name_text_symbol_in_binder(decl_binder, &scoped)
                    })
                    .or_else(|| self.resolve_entity_name_text_symbol_in_binder(decl_binder, name))
            };
            let resolve_decl_type_symbol =
                |node_idx: NodeIndex| -> Option<(tsz_binder::SymbolId, String)> {
                    let name = if std::ptr::eq(decl_arena, self.ctx.arena) {
                        self.entity_name_text(node_idx).or_else(|| {
                            decl_arena.get_identifier_text(node_idx).map(str::to_owned)
                        })?
                    } else {
                        decl_arena.get_identifier_text(node_idx)?.to_string()
                    };

                    if let Some(raw_sym_id) = decl_binder.resolve_identifier_with_filter(
                        decl_arena,
                        node_idx,
                        &[],
                        |candidate| {
                            let Some(symbol) = decl_binder
                                .get_symbol(candidate)
                                .or_else(|| self.ctx.binder.get_symbol(candidate))
                            else {
                                return false;
                            };
                            if symbol.escaped_name != name {
                                return false;
                            }
                            let typeish = symbol.has_any_flags(
                                tsz_binder::symbol_flags::TYPE
                                    | tsz_binder::symbol_flags::ALIAS
                                    | tsz_binder::symbol_flags::REGULAR_ENUM
                                    | tsz_binder::symbol_flags::CONST_ENUM,
                            );
                            if !typeish {
                                return false;
                            }
                            let file_local =
                                decl_binder.file_locals.get(name.as_str()) == Some(candidate);
                            let lib_like_file_local = file_local
                                && !symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
                                && (self.ctx.symbol_is_from_lib(candidate)
                                    || symbol.decl_file_idx == u32::MAX);
                            !(is_compiler_managed_type(&name) && lib_like_file_local)
                        },
                    ) {
                        let raw_symbol = decl_binder
                            .get_symbol(raw_sym_id)
                            .or_else(|| self.ctx.binder.get_symbol(raw_sym_id))?;
                        let effective_sym_id =
                            if raw_symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS) {
                                self.resolve_import_alias_in_decl_binder(
                                    decl_binder,
                                    decl_arena,
                                    raw_sym_id,
                                )?
                            } else {
                                raw_sym_id
                            };
                        return Some((effective_sym_id, name));
                    }

                    if std::ptr::eq(decl_arena, self.ctx.arena) {
                        return self
                            .resolve_type_symbol(node_idx)
                            .map(|sym| (tsz_binder::SymbolId(sym), name));
                    }

                    if is_compiler_managed_type(&name) {
                        return None;
                    }

                    let raw_sym_id = decl_binder
                        .resolve_identifier(decl_arena, node_idx)
                        .or_else(|| resolve_text_symbol(&name))?;
                    let effective_sym_id = self
                        .resolve_import_alias_in_decl_binder(decl_binder, decl_arena, raw_sym_id)
                        .unwrap_or(raw_sym_id);
                    Some((effective_sym_id, name))
                };
            let type_resolver = |node_idx: NodeIndex| -> Option<u32> {
                resolve_decl_type_symbol(node_idx).map(|(sym_id, _)| sym_id.0)
            };
            let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
                let (referenced_sym_id, referenced_name) = resolve_decl_type_symbol(node_idx)?;
                let resolved = def_id_for_symbol(referenced_sym_id, &referenced_name);
                if referenced_sym_id != sym_id && resolved != def_id {
                    self.ensure_type_alias_resolved(referenced_sym_id, resolved);
                }
                Some(resolved)
            };
            let value_resolver = |_node_idx: NodeIndex| -> Option<u32> { None };
            let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
                let raw_sym_id = resolve_text_symbol(type_name)?;
                let referenced_sym_id = self
                    .resolve_import_alias_in_decl_binder(decl_binder, decl_arena, raw_sym_id)
                    .unwrap_or(raw_sym_id);
                let resolved = def_id_for_symbol(referenced_sym_id, type_name);
                if referenced_sym_id != sym_id && resolved != def_id {
                    self.ensure_type_alias_resolved(referenced_sym_id, resolved);
                }
                Some(resolved)
            };
            let lowering = tsz_lowering::TypeLowering::with_hybrid_resolver(
                decl_arena,
                self.ctx.types,
                &type_resolver,
                &def_id_resolver,
                &value_resolver,
            )
            .with_name_def_id_resolver(&name_resolver);

            let params = if let Some(alias) = decl_arena.get_type_alias(node) {
                lowering.collect_type_alias_type_parameters(alias)
            } else if decl_arena.get_interface(node).is_some() {
                lowering.collect_merged_interface_type_parameters(&[(decl_idx, decl_arena)])
            } else {
                Vec::new()
            };
            if !params.is_empty() {
                self.ctx.insert_def_type_params(def_id, params);
                return;
            }
        }
    }
}
