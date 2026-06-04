impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    /// Resolve a DefId with support for qualified names (e.g., `AnimalType.cat`).
    ///
    /// Used by the `compute_type` fallback path where template literal types may
    /// reference enum members via qualified names inside `${...}`.
    pub(crate) fn resolve_def_id_with_qualified_names(
        &self,
        node_idx: NodeIndex,
    ) -> Option<tsz_solver::def::DefId> {
        use tsz_parser::parser::syntax_kind_ext;

        if let Some(name) = self.entity_name_text(node_idx)
            && !name.contains('.')
            && self.ctx.type_parameter_scope.contains_key(&name)
        {
            return None;
        }

        if let Some(name) = self.entity_name_text(node_idx)
            && !name.contains('.')
            && let Some(sym_id) = self.resolve_type_symbol(node_idx)
        {
            let sym_id = tsz_binder::SymbolId(sym_id);
            let def_id = if self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                || self.ctx.symbol_is_from_lib(sym_id)
            {
                self.ctx.get_canonical_lib_def_id(name.as_str(), sym_id)
            } else {
                self.ctx
                    .get_or_create_def_id_for_symbol_name(sym_id, name.as_str())
            };
            self.ensure_type_alias_resolved(sym_id, def_id);
            return Some(def_id);
        }

        if let Some(name) = self.entity_name_text(node_idx)
            && let Some(sym_id) = self.resolve_entity_name_text_symbol(&name)
        {
            let expected_name = name.rsplit('.').next().unwrap_or(name.as_str());
            let def_id = if self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                || self.ctx.symbol_is_from_lib(sym_id)
            {
                self.ctx.get_canonical_lib_def_id(expected_name, sym_id)
            } else {
                self.ctx
                    .get_or_create_def_id_for_symbol_name(sym_id, expected_name)
            };
            self.ensure_type_alias_resolved(sym_id, def_id);
            return Some(def_id);
        }

        if let Some(sym_id) = self.resolve_type_symbol(node_idx) {
            let sym_id = tsz_binder::SymbolId(sym_id);
            let def_id = if let Some(name) = self.entity_name_text(node_idx) {
                let expected_name = name.rsplit('.').next().unwrap_or(name.as_str());
                if self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                    || self.ctx.symbol_is_from_lib(sym_id)
                {
                    self.ctx.get_canonical_lib_def_id(expected_name, sym_id)
                } else {
                    self.ctx
                        .get_or_create_def_id_for_symbol_name(sym_id, expected_name)
                }
            } else {
                self.ensure_def_id_with_alias(sym_id)
            };
            self.ensure_type_alias_resolved(sym_id, def_id);
            return Some(def_id);
        }

        let node = self.ctx.arena.get(node_idx)?;
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.ctx.arena.get_qualified_name(node)?;
            let lib_binders: Vec<_> = self
                .ctx
                .lib_contexts
                .iter()
                .map(|ctx| std::sync::Arc::clone(&ctx.binder))
                .collect();
            // For the left part of a qualified name (e.g., `Lib` in `Lib.Base`),
            // we need to also consider ALIAS symbols because import declarations
            // like `import Lib = require('./helper')` create ALIAS-flagged symbols.
            // resolve_type_symbol only checks TYPE | ENUM flags, so try it first,
            // then fall back to resolve_type_or_alias_symbol for the namespace part.
            let left_sym_raw = self
                .resolve_type_symbol(qn.left)
                .or_else(|| self.resolve_type_or_alias_symbol(qn.left))?;
            let mut left_sym_id = tsz_binder::SymbolId(left_sym_raw);
            if let Some(left_name) = self
                .ctx
                .arena
                .get_identifier_at(qn.left)
                .map(|ident| ident.escaped_text.as_str())
                && let Some(local_namespace_sym_id) = self
                    .ctx
                    .local_namespace_symbol_for_conflicted_namespace_import(
                        qn.left,
                        left_name,
                        left_sym_id,
                        &lib_binders,
                    )
            {
                left_sym_id = local_namespace_sym_id;
            }

            // If the left symbol is an import alias (e.g., `import Lib = require('./helper')`),
            // follow the import to the target module symbol which holds the actual exports.
            let left_symbol_has_local_namespace_conflict = self
                .ctx
                .binder
                .get_symbol_with_libs(left_sym_id, &lib_binders)
                .is_some_and(|symbol| {
                    self.ctx
                        .namespace_import_alias_has_local_namespace_conflict(symbol)
                });
            let resolved_sym_id = if left_symbol_has_local_namespace_conflict {
                left_sym_id
            } else {
                self.ctx
                    .binder
                    .resolve_import_symbol(left_sym_id)
                    .unwrap_or(left_sym_id)
            };
            let resolved_symbol = self
                .ctx
                .binder
                .get_symbol_with_libs(resolved_sym_id, &lib_binders)?;

            let right_node = self.ctx.arena.get(qn.right)?;
            let right_ident = self.ctx.arena.get_identifier(right_node)?;
            let right_name = right_ident.escaped_text.as_str();

            // Look up the member in the resolved symbol's exports
            if let Some(exports) = resolved_symbol.exports.as_ref()
                && let Some(member_sym_id) = exports.get(right_name)
            {
                return Some(self.ensure_def_id_with_alias(member_sym_id));
            }

            // TYPE_ALIAS+ALIAS merge: resolve member through ALIAS partner
            if let Some(alias_id) = self.ctx.alias_partner_for(self.ctx.binder, resolved_sym_id)
                && let Some(alias_sym) =
                    self.ctx.binder.get_symbol_with_libs(alias_id, &lib_binders)
            {
                // Check direct exports first
                if let Some(exports) = alias_sym.exports.as_ref()
                    && let Some(member_sym_id) = exports.get(right_name)
                {
                    return Some(self.ensure_def_id_with_alias(member_sym_id));
                }
                // Follow the ALIAS's import_module, resolving from the
                // ALIAS's source file perspective (cross-file), then
                // falling back to the merged binder (same-file).
                if !self
                    .ctx
                    .namespace_import_alias_has_local_namespace_conflict(alias_sym)
                    && let Some(module_name) = alias_sym.import_module.as_ref()
                {
                    let member = self
                        .ctx
                        .resolve_alias_import_member(alias_id, module_name, right_name)
                        .or_else(|| {
                            self.ctx
                                .binder
                                .resolve_import_with_reexports_type_only(module_name, right_name)
                                .map(|(sym_id, _)| sym_id)
                        });
                    if let Some(member_sym_id) = member {
                        return Some(self.ensure_def_id_with_alias(member_sym_id));
                    }
                }
            }

            // Namespace import fallback: `import X = require('./mod')` where the target module
            // uses ES-style exports (no `export=`). `resolve_import_symbol` returns None in that
            // case so `resolved_sym_id == left_sym_id`. Look up the member directly in the
            // imported module's ES exports.
            if resolved_sym_id == left_sym_id
                && let Some(left_sym) = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(left_sym_id, &lib_binders)
                && !self
                    .ctx
                    .namespace_import_alias_has_local_namespace_conflict(left_sym)
                && let Some(module_name) = left_sym.import_module.as_ref()
            {
                // Use the current file's index to resolve the import target, since `left_sym`
                // is a local alias declared in the current file.
                let member = self
                    .ctx
                    .resolve_import_target_from_file(self.ctx.current_file_idx, module_name)
                    .and_then(|target_idx| {
                        let target_binder = self.ctx.get_binder_for_file(target_idx)?;
                        let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
                        let file_name = &target_arena.source_files.first()?.file_name;
                        target_binder
                            .resolve_import_with_reexports_type_only(file_name, right_name)
                            .map(|(sym_id, _)| {
                                self.ctx.register_symbol_file_target(sym_id, target_idx);
                                sym_id
                            })
                    })
                    .or_else(|| {
                        self.ctx
                            .binder
                            .resolve_import_with_reexports_type_only(module_name, right_name)
                            .map(|(sym_id, _)| sym_id)
                    });
                if let Some(member_sym_id) = member {
                    return Some(self.ensure_def_id_with_alias(member_sym_id));
                }
            }

            // Also check lib contexts for the member (e.g., global namespace types)
            for lib_ctx in self.ctx.lib_contexts.iter() {
                if let Some(lib_resolved) = lib_ctx.binder.resolve_import_symbol(left_sym_id)
                    && let Some(lib_symbol) = lib_ctx.binder.get_symbol(lib_resolved)
                    && let Some(exports) = lib_symbol.exports.as_ref()
                    && let Some(member_sym_id) = exports.get(right_name)
                {
                    return Some(self.ctx.get_or_create_def_id(member_sym_id));
                }
            }
        }

        None
    }

    /// Resolve a type-or-alias-or-namespace symbol from a node index.
    ///
    /// Like `resolve_type_symbol` but also matches ALIAS and NAMESPACE-flagged
    /// symbols, needed for:
    /// - Import declarations used as namespace qualifiers
    ///   (e.g., `import Lib = require('./helper')` then `Lib.Type`)
    /// - Namespace declarations used as qualified name prefixes
    ///   (e.g., `declare namespace NS { class C {} }` then `NS.C`)
    fn resolve_type_or_alias_symbol(&self, node_idx: NodeIndex) -> Option<u32> {
        use tsz_binder::symbol_flags;

        let ident = self.ctx.arena.get_identifier_at(node_idx)?;
        let name = ident.escaped_text.as_str();

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if (symbol.flags
                & (symbol_flags::TYPE
                    | symbol_flags::ALIAS
                    | symbol_flags::REGULAR_ENUM
                    | symbol_flags::CONST_ENUM
                    | symbol_flags::VALUE_MODULE
                    | symbol_flags::NAMESPACE_MODULE))
                != 0
            {
                return Some(sym_id.0);
            }
        }

        for lib_ctx in self.ctx.lib_contexts.iter() {
            if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name) {
                let symbol = lib_ctx.binder.get_symbol(lib_sym_id)?;
                if (symbol.flags
                    & (symbol_flags::TYPE
                        | symbol_flags::ALIAS
                        | symbol_flags::REGULAR_ENUM
                        | symbol_flags::CONST_ENUM
                        | symbol_flags::VALUE_MODULE
                        | symbol_flags::NAMESPACE_MODULE))
                    != 0
                {
                    let file_sym_id = self.ctx.binder.file_locals.get(name).unwrap_or(lib_sym_id);
                    return Some(file_sym_id.0);
                }
            }
        }

        None
    }
}
