impl<'a> CheckerState<'a> {
    /// Check if a namespace with the given name exists in any outer scope.
    /// Used to determine if a type-only or value-only local declaration is
    /// shadowing a namespace that import-equals should resolve through.
    fn namespace_exists_in_outer_scope(&self, name: &str, node: NodeIndex) -> bool {
        let Some(scope_id) = self.ctx.binder.find_enclosing_scope(self.ctx.arena, node) else {
            return false;
        };
        let Some(current_scope) = self.ctx.binder.scopes.get(scope_id.0 as usize) else {
            return false;
        };
        let mut walk_id = current_scope.parent;
        let lib_binders = self.get_lib_binders();

        while let Some(scope) = self.ctx.binder.scopes.get(walk_id.0 as usize) {
            if let Some(sym_id) = scope.table.get(name) {
                let sym_flags = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(sym_id, &lib_binders)
                    .map_or(0, |s| s.flags);
                if (sym_flags & tsz_binder::symbol_flags::NAMESPACE) != 0 {
                    return true;
                }
            }
            if walk_id == scope.parent {
                break;
            }
            walk_id = scope.parent;
        }
        false
    }

    /// Check if a namespace with the given name exists in an outer scope and has
    /// value semantics (is instantiated). Used for TS2437 to determine if a local
    /// non-namespace declaration is truly shadowing an instantiated module.
    fn check_namespace_has_value_in_outer_scope(&self, name: &str, node: NodeIndex) -> bool {
        // Walk up from the enclosing scope's parent to find a NAMESPACE symbol
        let Some(scope_id) = self.ctx.binder.find_enclosing_scope(self.ctx.arena, node) else {
            return false;
        };
        // Start from the parent of the current scope (skip the scope where the var is)
        let Some(current_scope) = self.ctx.binder.scopes.get(scope_id.0 as usize) else {
            return false;
        };
        let mut walk_id = current_scope.parent;
        let lib_binders = self.get_lib_binders();

        while let Some(scope) = self.ctx.binder.scopes.get(walk_id.0 as usize) {
            if let Some(sym_id) = scope.table.get(name) {
                let sym_flags = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(sym_id, &lib_binders)
                    .map_or(0, |s| s.flags);
                if (sym_flags & tsz_binder::symbol_flags::NAMESPACE) != 0 {
                    // Found a namespace — check if it has value (is instantiated)
                    let has_value = (sym_flags & tsz_binder::symbol_flags::VALUE) != 0;
                    if has_value
                        && (sym_flags & tsz_binder::symbol_flags::VALUE_MODULE) != 0
                        && (sym_flags
                            & (tsz_binder::symbol_flags::VALUE
                                & !tsz_binder::symbol_flags::VALUE_MODULE))
                            == 0
                    {
                        // Only VALUE_MODULE — check if any declaration is instantiated
                        if let Some(sym) =
                            self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                        {
                            return sym.declarations.iter().any(|&decl_idx| {
                                self.ctx.arena.get(decl_idx).is_some_and(|decl_node| {
                                    decl_node.kind
                                        != tsz_parser::parser::syntax_kind_ext::MODULE_DECLARATION
                                        || self.is_namespace_declaration_instantiated(decl_idx)
                                })
                            });
                        }
                        return false;
                    }
                    return has_value;
                }
            }
            // Move to parent scope
            if walk_id == scope.parent {
                break; // At root scope
            }
            walk_id = scope.parent;
        }
        false
    }

    /// Whether a declaration introduces a runtime value binding in the current file.
    ///
    /// Used by TS2440 conflict checks to avoid reporting conflicts against purely
    /// type-space declarations (e.g. interfaces/type aliases).
    fn declaration_introduces_runtime_value(&self, decl_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        match node.kind {
            syntax_kind_ext::FUNCTION_DECLARATION
            | syntax_kind_ext::CLASS_DECLARATION
            | syntax_kind_ext::ENUM_DECLARATION
            | syntax_kind_ext::VARIABLE_DECLARATION
            | syntax_kind_ext::VARIABLE_STATEMENT
            // Type aliases occupy the type namespace and conflict with import-equals.
            // tsc TS2440 fires for type aliases even though they have no runtime value.
            | syntax_kind_ext::TYPE_ALIAS_DECLARATION => true,
            syntax_kind_ext::MODULE_DECLARATION => {
                self.is_namespace_declaration_instantiated(decl_idx)
            }
            _ => false,
        }
    }

    /// Whether an import-equals RHS has value semantics through an exported namespace
    /// member, even when a later type-space export with the same name occupies the
    /// namespace export-table slot.
    fn import_equals_target_has_exported_value(&self, module_ref: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(module_ref) else {
            return false;
        };
        if node.kind != syntax_kind_ext::QUALIFIED_NAME {
            return false;
        }

        let Some(qn) = self.ctx.arena.get_qualified_name(node) else {
            return false;
        };
        let Some(right_name) = self
            .ctx
            .arena
            .get(qn.right)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())
        else {
            return false;
        };

        let mut visited = AliasCycleTracker::new();
        let Some(left_sym_id) = self.resolve_qualified_symbol_inner(qn.left, &mut visited, 0)
        else {
            return false;
        };
        let left_sym_id = self
            .resolve_alias_symbol(left_sym_id, &mut visited)
            .unwrap_or(left_sym_id);

        let lib_binders = self.get_lib_binders();
        let Some(left_symbol) = self
            .ctx
            .binder
            .get_symbol_with_libs(left_sym_id, &lib_binders)
        else {
            return false;
        };

        if let Some(exports) = left_symbol.exports.as_ref()
            && let Some(member_sym_id) = exports.get(right_name)
            && self.symbol_has_import_equals_value_semantics(member_sym_id)
        {
            return true;
        }

        let namespace_decls = left_symbol.declarations.clone();
        for &candidate_id in self.ctx.binder.symbols.find_all_by_name(right_name) {
            let Some(candidate) = self.ctx.binder.symbols.get(candidate_id) else {
                continue;
            };
            if !candidate.is_exported {
                continue;
            }
            let declared_in_namespace = candidate.declarations.iter().any(|&decl_idx| {
                namespace_decls
                    .iter()
                    .any(|&ns_decl_idx| self.node_has_ancestor(decl_idx, ns_decl_idx))
            });
            if declared_in_namespace && self.symbol_has_import_equals_value_semantics(candidate_id)
            {
                return true;
            }
        }

        false
    }

    fn symbol_has_import_equals_value_semantics(&self, sym_id: tsz_binder::SymbolId) -> bool {
        let mut visited = AliasCycleTracker::new();
        let resolved_id = self
            .resolve_alias_symbol(sym_id, &mut visited)
            .unwrap_or(sym_id);
        let lib_binders = self.get_lib_binders();
        let Some(symbol) = self
            .ctx
            .binder
            .get_symbol_with_libs(resolved_id, &lib_binders)
        else {
            return false;
        };

        let mut has_value = symbol.has_any_flags(tsz_binder::symbol_flags::VALUE);
        if has_value
            && symbol.has_any_flags(tsz_binder::symbol_flags::VALUE_MODULE)
            && !symbol.has_any_flags(
                tsz_binder::symbol_flags::VALUE & !tsz_binder::symbol_flags::VALUE_MODULE,
            )
        {
            has_value = symbol.declarations.iter().any(|&decl_idx| {
                self.ctx.arena.get(decl_idx).is_some_and(|decl_node| {
                    decl_node.kind != syntax_kind_ext::MODULE_DECLARATION
                        || self.is_namespace_declaration_instantiated(decl_idx)
                })
            });
        }
        has_value
    }

    fn declaration_is_enclosing_namespace_of_node(
        &self,
        decl_idx: NodeIndex,
        node_idx: NodeIndex,
    ) -> bool {
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if decl_node.kind != syntax_kind_ext::MODULE_DECLARATION {
            return false;
        }
        self.node_has_ancestor(node_idx, decl_idx)
    }

    fn node_has_ancestor(&self, mut node_idx: NodeIndex, ancestor_idx: NodeIndex) -> bool {
        let mut guard = 0u32;
        loop {
            if node_idx == ancestor_idx {
                return true;
            }
            guard += 1;
            if guard > 4096 {
                return false;
            }
            let Some(ext) = self.ctx.arena.get_extended(node_idx) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            node_idx = ext.parent;
        }
    }

    fn namespace_import_alias_is_referenced(
        &self,
        containing_module_node: Option<NodeIndex>,
        import_decl_node: NodeIndex,
        import_alias_sym_id: Option<tsz_binder::SymbolId>,
    ) -> bool {
        let Some(import_alias_sym_id) = import_alias_sym_id else {
            return true;
        };

        let Some(containing_module_node) = containing_module_node else {
            return true;
        };
        let Some(module_node) = self.ctx.arena.get(containing_module_node) else {
            return true;
        };
        if module_node.kind != syntax_kind_ext::MODULE_DECLARATION {
            return true;
        }
        let Some(module_decl) = self.ctx.arena.get_module(module_node) else {
            return true;
        };
        let Some(module_body_node) = self.ctx.arena.get(module_decl.body) else {
            return true;
        };
        if module_body_node.kind != syntax_kind_ext::MODULE_BLOCK {
            return true;
        }

        let mut stack: Vec<NodeIndex> = self
            .ctx
            .arena
            .get_children(module_decl.body)
            .into_iter()
            .collect();
        while let Some(current_idx) = stack.pop() {
            if current_idx == import_decl_node {
                continue;
            }
            let Some(current_node) = self.ctx.arena.get(current_idx) else {
                continue;
            };
            if current_node.kind == SyntaxKind::Identifier as u16
                && self
                    .resolve_identifier_symbol(current_idx)
                    .is_some_and(|sym_id| sym_id == import_alias_sym_id)
            {
                return true;
            }
            stack.extend(self.ctx.arena.get_children(current_idx));
        }
        false
    }
}
