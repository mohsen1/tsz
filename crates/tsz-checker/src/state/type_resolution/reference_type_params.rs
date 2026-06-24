//! Type-reference type-parameter lookup and import-alias target helpers.

use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::{NodeIndex, NodeList, syntax_kind_ext};

impl CheckerState<'_> {
    pub(super) fn same_file_type_alias_parts_for_name(
        &self,
        name: &str,
    ) -> Option<(Option<NodeList>, NodeIndex, Option<SymbolId>)> {
        self.ctx
            .arena
            .nodes
            .iter()
            .enumerate()
            .find_map(|(idx, node)| {
                let type_alias = self.ctx.arena.get_type_alias(node)?;
                let alias_name = self.ctx.arena.get_identifier_text(type_alias.name)?;
                (alias_name == name).then(|| {
                    (
                        type_alias.type_parameters.clone(),
                        type_alias.type_node,
                        self.ctx.binder.node_symbols.get(&(idx as u32)).copied(),
                    )
                })
            })
    }

    pub(super) fn type_node_is_outside_symbol_declarations(
        &self,
        node_idx: NodeIndex,
        sym_id: SymbolId,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return true;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return true;
        };

        !symbol.declarations.iter().any(|&decl_idx| {
            self.ctx
                .arena
                .get(decl_idx)
                .is_some_and(|decl| node.pos >= decl.pos && node.end <= decl.end)
        })
    }

    pub(super) fn def_body_involves_depth_poisoned_def(&self, def_id: tsz_solver::DefId) -> bool {
        if !self.ctx.definition_store.has_any_depth_poisoned() {
            return false;
        }

        self.ctx
            .type_env
            .try_borrow()
            .ok()
            .and_then(|env| env.get_def(def_id))
            .is_some_and(|body| self.ctx.type_involves_depth_poisoned_def(body))
    }

    pub(super) fn def_body_can_own_ambient_depth(&self, def_id: tsz_solver::DefId) -> bool {
        let Some(body) = self
            .ctx
            .type_env
            .try_borrow()
            .ok()
            .and_then(|env| env.get_def(def_id))
            .or_else(|| self.ctx.definition_store.get_body(def_id))
        else {
            return false;
        };

        let db = self.ctx.types.as_type_database();
        crate::query_boundaries::common::contains_conditional_type(db, body)
            || crate::query_boundaries::common::contains_keyof_type(db, body)
            || crate::query_boundaries::common::contains_index_access_type(db, body)
            || crate::query_boundaries::common::is_mapped_type(db, body)
    }

    pub(crate) fn declaration_file_type_shadow_for_lib_name(
        &self,
        name: &str,
        file_idx: Option<usize>,
    ) -> bool {
        use tsz_binder::symbol_flags;

        let Some(file_idx) = file_idx else {
            return self.ctx.file_local_type_shadow_for_lib_name(name);
        };
        let Some(binder) = self.ctx.get_binder_for_file(file_idx) else {
            return false;
        };
        if !binder.is_external_module() {
            return false;
        }
        binder.file_locals.get(name).is_some_and(|sym_id| {
            let is_actual_or_merged_lib = self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                || binder.lib_symbol_ids.contains(&sym_id);
            !is_actual_or_merged_lib
                && binder
                    .get_symbol(sym_id)
                    .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::TYPE))
        })
    }

    pub(crate) fn resolve_declaration_file_type_symbol_for_lowering(
        &self,
        name: &str,
        file_idx: Option<usize>,
    ) -> Option<SymbolId> {
        let file_idx = file_idx?;
        let binder = self.ctx.get_binder_for_file(file_idx)?;
        let sym_id = binder.file_locals.get(name)?;
        let symbol = binder.get_symbol(sym_id)?;
        let is_actual_or_merged_lib = self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
            || binder.lib_symbol_ids.contains(&sym_id);
        if is_actual_or_merged_lib {
            return None;
        }
        if symbol.has_any_flags(symbol_flags::ALIAS)
            && let Some(module_name) = symbol.import_module()
        {
            let import_name = symbol.import_name().unwrap_or(name);
            let target_sym_id =
                self.resolve_cross_file_export_from_file(module_name, import_name, Some(file_idx))?;
            if let Some(target_file_idx) = self
                .ctx
                .resolve_symbol_file_index_stable(target_sym_id)
                .or_else(|| self.ctx.resolve_symbol_file_index(target_sym_id))
            {
                self.ctx
                    .register_symbol_file_target(target_sym_id, target_file_idx);
            }
            return Some(target_sym_id);
        }
        if symbol.has_any_flags(symbol_flags::TYPE) {
            self.ctx.register_symbol_file_target(sym_id, file_idx);
            return Some(sym_id);
        }
        None
    }

    pub(crate) fn resolve_declaration_file_type_def_id_for_lowering(
        &self,
        name: &str,
        file_idx: Option<usize>,
    ) -> Option<tsz_solver::def::DefId> {
        self.resolve_declaration_file_type_symbol_for_lowering(name, file_idx)
            .map(|sym_id| self.ctx.get_or_create_def_id_for_symbol_name(sym_id, name))
    }

    pub(crate) fn get_reference_type_params_for_symbol(
        &mut self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> Vec<tsz_solver::TypeParamInfo> {
        let cache_key = self.reference_type_params_cache_key(sym_id, expected_name);
        if let Some(cached) = self
            .ctx
            .type_reference_validation_caches
            .ref_type_params
            .get(&cache_key)
        {
            return cached.clone();
        }
        // Only fall back to the raw-`SymbolId`-keyed display path when no
        // matching declaration was found. When `extract_*` resolved a matching
        // declaration (even to zero params, i.e. a non-generic symbol), trust it:
        // the display fallback re-derives the owner from the raw `SymbolId`, which
        // collides across binders and can read a sibling generic's parameters.
        let result = match self
            .extract_declared_type_params_for_reference_symbol(cache_key.0, expected_name)
        {
            Some(declared) => declared,
            None => self.get_display_type_params_for_symbol(cache_key.0),
        };
        self.ctx
            .type_reference_validation_caches
            .ref_type_params
            .insert(cache_key, result.clone());
        result
    }

    pub(crate) fn count_required_reference_type_params(
        &mut self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> usize {
        let cache_key = self.reference_type_params_cache_key(sym_id, expected_name);
        if let Some(required) = self.count_required_reference_type_params_from_syntax(
            cache_key.0,
            cache_key.1,
            expected_name,
        ) {
            return required;
        }
        if let Some(cached) = self
            .ctx
            .type_reference_validation_caches
            .ref_type_params
            .get(&cache_key)
        {
            return cached.iter().filter(|p| p.default.is_none()).count();
        }
        // A resolved declaration (even one with zero params) is authoritative.
        // Only when no matching declaration exists do we consult the
        // raw-`SymbolId`-keyed `count_required_type_params`, which can otherwise
        // read a colliding same-file symbol's parameter count.
        if let Some(declared) =
            self.extract_declared_type_params_for_reference_symbol(cache_key.0, expected_name)
        {
            let count = declared
                .iter()
                .filter(|param| param.default.is_none())
                .count();
            self.ctx
                .type_reference_validation_caches
                .ref_type_params
                .insert(cache_key, declared);
            return count;
        }
        self.count_required_type_params(cache_key.0)
    }

    fn count_required_reference_type_params_from_syntax(
        &self,
        sym_id: SymbolId,
        file_idx: Option<usize>,
        expected_name: &str,
    ) -> Option<usize> {
        let symbol = if file_idx.is_some() {
            self.get_symbol_from_registered_file_target(sym_id)
                .or_else(|| self.get_cross_file_symbol(sym_id))
        } else {
            self.ctx
                .binder
                .get_symbol(sym_id)
                .or_else(|| self.get_symbol_from_registered_file_target(sym_id))
                .or_else(|| self.get_cross_file_symbol(sym_id))
        }?;
        let flags = symbol.flags;
        let expected_leaf_name = expected_name.rsplit('.').next().unwrap_or(expected_name);
        let mut best_required: Option<usize> = None;

        for decl_idx in symbol.all_declarations() {
            if let Some(file_idx) = file_idx {
                let arena = self.ctx.get_arena_for_file(file_idx as u32);
                if let Some(required) = Self::count_required_reference_params_in_arena(
                    arena,
                    flags,
                    decl_idx,
                    expected_name,
                    expected_leaf_name,
                ) {
                    best_required = Some(best_required.map_or(required, |prev| prev.min(required)));
                }
                continue;
            }

            if let Some(required) = Self::count_required_reference_params_in_arena(
                self.ctx.arena,
                flags,
                decl_idx,
                expected_name,
                expected_leaf_name,
            ) {
                best_required = Some(best_required.map_or(required, |prev| prev.min(required)));
                continue;
            }
            if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for arena in arenas {
                    if let Some(required) = Self::count_required_reference_params_in_arena(
                        arena.as_ref(),
                        flags,
                        decl_idx,
                        expected_name,
                        expected_leaf_name,
                    ) {
                        best_required =
                            Some(best_required.map_or(required, |prev| prev.min(required)));
                    }
                }
            }
            if let Some(required) = self
                .ctx
                .binder
                .symbol_arenas
                .get(&sym_id)
                .and_then(|arena| {
                    Self::count_required_reference_params_in_arena(
                        arena.as_ref(),
                        flags,
                        decl_idx,
                        expected_name,
                        expected_leaf_name,
                    )
                })
            {
                best_required = Some(best_required.map_or(required, |prev| prev.min(required)));
            }
        }

        best_required
    }

    fn count_required_reference_params_in_arena(
        arena: &NodeArena,
        flags: u32,
        decl_idx: NodeIndex,
        expected_name: &str,
        expected_leaf_name: &str,
    ) -> Option<usize> {
        let node = arena.get(decl_idx)?;
        let type_params = if flags & symbol_flags::INTERFACE != 0 {
            let iface = arena.get_interface(node)?;
            Self::reference_decl_name_matches(arena, iface.name, expected_name, expected_leaf_name)
                .then_some(iface.type_parameters.as_ref())?
        } else if flags & symbol_flags::TYPE_ALIAS != 0 {
            let alias = arena.get_type_alias(node)?;
            Self::reference_decl_name_matches(arena, alias.name, expected_name, expected_leaf_name)
                .then_some(alias.type_parameters.as_ref())?
        } else if flags & symbol_flags::CLASS != 0 {
            let class = arena.get_class(node)?;
            Self::reference_decl_name_matches(arena, class.name, expected_name, expected_leaf_name)
                .then_some(class.type_parameters.as_ref())?
        } else {
            return None;
        }?;

        Some(
            type_params
                .nodes
                .iter()
                .filter(|&&param_idx| {
                    arena
                        .get(param_idx)
                        .and_then(|node| arena.get_type_parameter(node))
                        .is_some_and(|param| param.default == NodeIndex::NONE)
                })
                .count(),
        )
    }

    fn reference_decl_name_matches(
        arena: &NodeArena,
        name_idx: NodeIndex,
        expected_name: &str,
        expected_leaf_name: &str,
    ) -> bool {
        arena
            .get(name_idx)
            .and_then(|node| arena.get_identifier(node))
            .is_some_and(|ident| {
                ident.escaped_text == expected_name || ident.escaped_text == expected_leaf_name
            })
    }

    fn reference_type_params_cache_key(
        &self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> (SymbolId, Option<usize>, String) {
        if let Some(local_sym_id) =
            self.current_non_import_reference_symbol_id(sym_id, expected_name)
        {
            return (local_sym_id, None, expected_name.to_owned());
        }
        let (sym_id, file_idx) = self
            .reference_type_params_import_target(sym_id, expected_name)
            .unwrap_or_else(|| (sym_id, self.ctx.resolve_symbol_file_index(sym_id)));
        (sym_id, file_idx, expected_name.to_owned())
    }

    pub(crate) fn current_non_import_reference_symbol_id(
        &self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> Option<SymbolId> {
        if self.reference_symbol_is_current_non_import(sym_id, expected_name) {
            return Some(sym_id);
        }
        if let Some(local_sym_id) = self.ctx.binder.file_locals.get(expected_name)
            && self.reference_symbol_is_current_non_import(local_sym_id, expected_name)
        {
            return Some(local_sym_id);
        }
        None
    }

    fn reference_symbol_is_current_non_import(
        &self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> bool {
        self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
            symbol.escaped_name == expected_name && !self.reference_symbol_is_import_alias(symbol)
        })
    }

    fn reference_type_params_import_target(
        &self,
        sym_id: SymbolId,
        expected_name: &str,
    ) -> Option<(SymbolId, Option<usize>)> {
        let alias_symbol = self
            .ctx
            .binder
            .file_locals
            .get(expected_name)
            .and_then(|alias_sym_id| self.ctx.binder.get_symbol(alias_sym_id))
            .or_else(|| self.ctx.binder.get_symbol(sym_id))?;
        if !self.reference_symbol_is_import_alias(alias_symbol) {
            return None;
        }
        self.reference_import_alias_export_target(alias_symbol, expected_name)
    }

    /// Canonical `DefId` for a bare type reference whose local symbol is an
    /// import alias reaching its declaration through a barrel re-export
    /// (`export { X } from` / `export type { X } from`).
    ///
    /// Follows the re-export chain to the declaring `(SymbolId, file)` and keys
    /// the `DefId` to that declaration via
    /// [`crate::context::CheckerContext::def_id_for_declaration_in_file`].
    /// Returns `None` when the local symbol is not an import alias, the chain
    /// has no resolvable file, or the declaration is a lib symbol (lib types
    /// keep their canonical-lib `DefId` path).
    ///
    /// Without this, lowering keys the application's base `DefId` to an
    /// intermediate re-export file (raw `SymbolId`s collide across binders),
    /// minting a non-canonical def whose generic body is never registered — so
    /// `Base<number>` stays an opaque `Application` with its type parameter
    /// never substituted, producing a false `TS2322`. Scoped to interface/class
    /// declarations (see the gate below). Mirrors the reference-aware heritage
    /// path (#13803); refs #13212 / #10663.
    pub(crate) fn reexported_declaration_def_id_for_lowering(
        &self,
        local_sym: SymbolId,
        expected_name: &str,
    ) -> Option<tsz_solver::def::DefId> {
        let leaf_name = expected_name.rsplit('.').next().unwrap_or(expected_name);
        let alias_symbol = self.ctx.binder.get_symbol(local_sym)?;
        if !self.reference_symbol_is_import_alias(alias_symbol) {
            return None;
        }
        let (decl_sym, Some(decl_file_idx)) =
            self.reference_import_alias_export_target(alias_symbol, leaf_name)?
        else {
            return None;
        };
        if self.ctx.has_lib_loaded() && self.ctx.symbol_is_from_actual_or_cloned_lib(decl_sym) {
            return None;
        }
        // Restrict to interface/class declarations. Their bodies resolve on
        // demand at relation time once the `DefId` is keyed to the declaration,
        // so canonicalization alone fixes the substitution. Re-exported generic
        // *type aliases* need their body eagerly registered under the canonical
        // `DefId` as well (a distinct path); leave them on the existing
        // resolution so this change does not alter the alias path.
        //
        // Key the `DefId` to the declaration's *own* declared name, not the
        // local `leaf_name`: a renamed re-export
        // (`export type { Original as Renamed } from`) resolves the chain to a
        // declaration whose name differs from the name used at the import site.
        // `def_id_for_declaration_in_file` requires the declaration's
        // `escaped_name` to match, so passing `leaf_name` ("Renamed") returns
        // `None` for the renamed case and the application is left opaque with
        // its type argument unsubstituted (false TS2322). The declaration name
        // also keys the body that resolves on demand, so the two agree.
        let decl_name = self
            .ctx
            .get_binder_for_file(decl_file_idx)
            .and_then(|binder| binder.get_symbol(decl_sym))
            .filter(|symbol| symbol.has_any_flags(symbol_flags::INTERFACE | symbol_flags::CLASS))
            .map(|symbol| symbol.escaped_name.clone())?;
        self.ctx
            .def_id_for_declaration_in_file(decl_sym, decl_file_idx, &decl_name)
    }

    pub(crate) fn reference_import_alias_export_target(
        &self,
        alias_symbol: &tsz_binder::Symbol,
        expected_name: &str,
    ) -> Option<(SymbolId, Option<usize>)> {
        let module_specifier = alias_symbol.import_module()?;
        let import_name = alias_symbol.import_name().unwrap_or(expected_name);
        let source_file_idx = if alias_symbol.decl_file_idx == u32::MAX {
            self.ctx.current_file_idx
        } else {
            alias_symbol.decl_file_idx as usize
        };
        if let Some(target_file_idx) = self
            .ctx
            .resolve_import_target_from_file(source_file_idx, module_specifier)
            && let Some((target_sym_id, actual_file_idx)) =
                self.resolve_reexport_chain_to_declaration(target_file_idx, import_name)
        {
            self.ctx
                .register_symbol_file_target(target_sym_id, actual_file_idx);
            return Some((target_sym_id, Some(actual_file_idx)));
        }

        let target_sym_id = self.resolve_cross_file_export_from_file(
            module_specifier,
            import_name,
            Some(source_file_idx),
        )?;
        let target_file_idx = self
            .ctx
            .resolve_symbol_file_index_stable(target_sym_id)
            .or_else(|| self.ctx.resolve_symbol_file_index(target_sym_id));
        if let Some(file_idx) = target_file_idx {
            self.ctx
                .register_symbol_file_target(target_sym_id, file_idx);
        }
        Some((target_sym_id, target_file_idx))
    }

    pub(crate) fn reference_symbol_is_import_alias(&self, symbol: &tsz_binder::Symbol) -> bool {
        let arena = if symbol.decl_file_idx == u32::MAX {
            self.ctx.arena
        } else {
            self.ctx.get_arena_for_file(symbol.decl_file_idx)
        };
        symbol.has_any_flags(symbol_flags::ALIAS)
            && symbol.import_module().is_some()
            && symbol
                .declarations
                .iter()
                .copied()
                .any(|decl_idx| self.reference_decl_is_import_alias_syntax(arena, decl_idx))
    }

    fn reference_decl_is_import_alias_syntax(
        &self,
        arena: &NodeArena,
        mut decl_idx: NodeIndex,
    ) -> bool {
        for _ in 0..4 {
            let Some(node) = arena.get(decl_idx) else {
                return false;
            };
            if node.kind == syntax_kind_ext::IMPORT_SPECIFIER
                || node.kind == syntax_kind_ext::IMPORT_CLAUSE
                || node.kind == syntax_kind_ext::NAMESPACE_IMPORT
                || node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                return true;
            }
            let Some(extended) = arena.get_extended(decl_idx) else {
                return false;
            };
            let parent_idx = extended.parent;
            if parent_idx == NodeIndex::NONE {
                return false;
            }
            decl_idx = parent_idx;
        }
        false
    }
}
