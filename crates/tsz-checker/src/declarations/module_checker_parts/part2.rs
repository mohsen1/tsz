impl<'a> CheckerState<'a> {
    // =========================================================================
    // Export Module Specifier Validation
    // =========================================================================

    /// Eagerly checks all alias symbols in the current file for circular definitions.
    /// Emits TS2303 for any alias that circularly references itself.
    pub(crate) fn check_circular_import_aliases(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        let mut reported_cycle_symbols = rustc_hash::FxHashSet::default();

        let is_js_file = self.ctx.is_js_file();

        // Collect ALIAS symbols only from scope tables, not from the full symbol arena.
        // After multi-file merge, the global symbol arena contains symbols from ALL files.
        // Iterating symbols.iter() would cause each file to check every file's symbols,
        // leading to duplicate TS2303 emissions. Scope tables contain only this file's symbols.
        let mut local_alias_ids: Vec<tsz_binder::SymbolId> = Vec::new();
        for scope in self.ctx.binder.scopes.iter() {
            for (_, &sym_id) in scope.table.iter() {
                if let Some(s) = self.ctx.binder.symbols.get(sym_id)
                    && s.has_any_flags(symbol_flags::ALIAS)
                    && !s.is_umd_export
                {
                    local_alias_ids.push(sym_id);
                }
            }
        }
        local_alias_ids.sort_unstable_by_key(|s| s.0);
        local_alias_ids.dedup();

        for sym_id in local_alias_ids {
            let sym = match self.ctx.binder.symbols.get(sym_id) {
                Some(s) => s,
                None => continue,
            };

            if reported_cycle_symbols.contains(&sym_id) {
                continue;
            }

            // In JS files, `import x = require(...)` is TS-only syntax (TS8002).
            // tsc skips semantic analysis for such statements — skip circular check.
            if is_js_file {
                let decl_idx = sym.primary_declaration().unwrap_or(NodeIndex::NONE);
                if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                    && decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                {
                    continue;
                }
            }

            let mut current_binder = self.ctx.binder;
            let mut current_file_idx = self.ctx.current_file_idx;
            let mut current_sym_id = sym_id;
            let mut visited = Vec::new();
            let mut visited_sym_ids = Vec::new();
            let mut cycle_detected = false;

            for _ in 0..128 {
                let key = (current_file_idx, current_sym_id.0 as usize);
                if visited.contains(&key) {
                    if key.0 == self.ctx.current_file_idx && key.1 == sym_id.0 as usize {
                        // When we get an immediate self-reference (one-step cycle),
                        // it may be a self-import pattern:
                        //   export { f as g } from "./a";  // re-export
                        //   import { g } from "./b";       // self-import
                        // The binder merges both into one symbol. The self-import
                        // resolves to the merged symbol → appears circular.
                        // Don't flag it as circular if the symbol has a re-export
                        // declaration (EXPORT_SPECIFIER with a `from` clause) that
                        // points to a different module, providing a real resolution.
                        if visited.len() == 1 {
                            let has_reexport_from = sym.declarations.iter().any(|&decl_idx| {
                                if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                                    && decl_node.kind == syntax_kind_ext::EXPORT_SPECIFIER
                                {
                                    // Check if the parent export declaration has a module
                                    // specifier (`from "..."` clause).
                                    if let Some(ext) = self.ctx.arena.get_extended(decl_idx) {
                                        let parent = ext.parent;
                                        if let Some(parent_node) = self.ctx.arena.get(parent)
                                            && parent_node.kind == syntax_kind_ext::NAMED_EXPORTS
                                            && let Some(grandparent_ext) =
                                                self.ctx.arena.get_extended(parent)
                                        {
                                            let gp = grandparent_ext.parent;
                                            if let Some(gp_node) = self.ctx.arena.get(gp)
                                                && gp_node.kind
                                                    == syntax_kind_ext::EXPORT_DECLARATION
                                                && let Some(export_decl) =
                                                    self.ctx.arena.get_export_decl(gp_node)
                                            {
                                                return export_decl.module_specifier.is_some();
                                            }
                                        }
                                    }
                                    false
                                } else {
                                    false
                                }
                            });
                            // `import X = require("m")` inside a different
                            // ambient module declaration (e.g.
                            //   declare module "m"      { export = T; }
                            //   declare module "node:m" { import m = require("m"); export = m; }
                            // ) names an external module — the alias resolves
                            // through `m`'s `export = ...`, not back to itself.
                            // Our binder can spuriously map the alias to itself
                            // because `m` is both a sibling declared-module
                            // specifier and an alias name in the same file.
                            // Suppress only for the cross-module-name case;
                            // genuine self-imports
                            //   declare module "moduleC" { import self = require("moduleC"); }
                            // remain TS2303.
                            let require_target_differs_from_enclosing_module =
                                sym.declarations.iter().any(|&decl_idx| {
                                    let Some(n) = self.ctx.arena.get(decl_idx) else {
                                        return false;
                                    };
                                    if n.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                                        return false;
                                    }
                                    let Some(imp) = self.ctx.arena.get_import_decl(n) else {
                                        return false;
                                    };
                                    let Some(target) =
                                        self.get_require_module_specifier(imp.module_specifier)
                                    else {
                                        return false;
                                    };
                                    let Some(enclosing) =
                                        self.enclosing_ambient_module_specifier(decl_idx)
                                    else {
                                        return false;
                                    };
                                    target != enclosing
                                });
                            if !has_reexport_from && !require_target_differs_from_enclosing_module {
                                cycle_detected = true;
                            }
                        } else {
                            cycle_detected = true;
                        }
                    }
                    break;
                }
                visited.push(key);
                visited_sym_ids.push(current_sym_id);

                let curr_sym = match current_binder.symbols.get(current_sym_id) {
                    Some(s) => s,
                    None => break,
                };

                if !curr_sym.has_any_flags(symbol_flags::ALIAS) {
                    break;
                }

                let mut found = false;

                // For import aliases with import_module, use cross-file resolution
                // to properly track which file we're resolving from.
                if let Some(ref module_name) = curr_sym.import_module {
                    let export_name = curr_sym
                        .import_name
                        .as_deref()
                        .unwrap_or(&curr_sym.escaped_name);

                    // Use checker's cross-file module resolution first.
                    // This correctly resolves relative specifiers from the
                    // current file's perspective and switches to the target
                    // file's binder for subsequent resolution.
                    if let Some(target_idx) = self
                        .ctx
                        .resolve_import_target_from_file(current_file_idx, module_name)
                        && let Some(target_binder) = self.ctx.get_binder_for_file(target_idx)
                    {
                        if let Some(target_sym_id) = target_binder
                            .resolve_import_with_reexports_type_only(module_name, export_name)
                            .map(|(sym_id, _)| sym_id)
                            .or_else(|| {
                                (curr_sym.import_name.is_none())
                                    .then(|| {
                                        target_binder
                                            .resolve_import_with_reexports_type_only(
                                                module_name,
                                                "export=",
                                            )
                                            .map(|(sym_id, _)| sym_id)
                                    })
                                    .flatten()
                            })
                        {
                            current_binder = target_binder;
                            current_file_idx = target_idx;
                            current_sym_id = target_sym_id;
                            found = true;
                        } else {
                            let target_arena = self.ctx.get_arena_for_file(target_idx as u32);
                            if let Some(sf) = target_arena.source_files.first()
                                && let Some(exports) = self
                                    .ctx
                                    .module_exports_for_module(target_binder, &sf.file_name)
                            {
                                if let Some(target_sym_id) = exports.get(export_name) {
                                    current_binder = target_binder;
                                    current_file_idx = target_idx;
                                    current_sym_id = target_sym_id;
                                    found = true;
                                } else if let Some(target_sym_id) = exports.get("export=") {
                                    current_binder = target_binder;
                                    current_file_idx = target_idx;
                                    current_sym_id = target_sym_id;
                                    found = true;
                                }
                            }
                        }
                    }

                    // Fall back to binder-level resolution (same-file or merged binder)
                    if !found
                        && let Some(resolved_id) =
                            current_binder.resolve_import_symbol(current_sym_id)
                    {
                        current_sym_id = resolved_id;
                        found = true;
                    }

                    // Try current binder's module_exports directly
                    if !found
                        && let Some(exports) = current_binder.module_exports.get(module_name)
                        && let Some(target_sym_id) = exports.get(export_name)
                    {
                        current_sym_id = target_sym_id;
                        found = true;
                    }
                    if !found
                        && let Some(exports) = current_binder.module_exports.get(module_name)
                        && let Some(target_sym_id) = exports.get("export=")
                    {
                        current_sym_id = target_sym_id;
                        found = true;
                    }

                    // Fall back to all_binders for cross-file resolution
                    if !found && let Some(binders) = &self.ctx.all_binders {
                        if let Some(file_indices) = self.ctx.files_for_module_specifier(module_name)
                        {
                            for &idx in file_indices {
                                if let Some(b) = binders.get(idx)
                                    && let Some(exports) = b.module_exports.get(module_name)
                                    && let Some(target_sym_id) = exports.get(export_name)
                                {
                                    current_binder = &**b;
                                    current_file_idx = idx;
                                    current_sym_id = target_sym_id;
                                    found = true;
                                    break;
                                }
                            }
                        } else {
                            for (idx, b) in binders.iter().enumerate() {
                                if let Some(exports) = b.module_exports.get(module_name)
                                    && let Some(target_sym_id) = exports.get(export_name)
                                {
                                    current_binder = &**b;
                                    current_file_idx = idx;
                                    current_sym_id = target_sym_id;
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                } else if let Some(resolved_id) =
                    current_binder.resolve_import_symbol(current_sym_id)
                {
                    // Non-import alias (e.g., import = require(...)) — use binder resolution
                    current_sym_id = resolved_id;
                    found = true;
                }

                if !found
                    && std::ptr::eq(current_binder as *const _, self.ctx.binder as *const _)
                    && curr_sym.value_declaration.is_some()
                {
                    let decl_idx = curr_sym.value_declaration;
                    if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                        && decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                        && let Some(import_decl) = self.ctx.arena.get_import_decl(decl_node)
                    {
                        let mut base_node = import_decl.module_specifier;
                        while let Some(node) = self.ctx.arena.get(base_node)
                            && let Some(qname) = self.ctx.arena.get_qualified_name(node)
                        {
                            base_node = qname.left;
                        }
                        if let Some(node) = self.ctx.arena.get(base_node)
                            && let Some(ident) = self.ctx.arena.get_identifier(node)
                            && let Some(target_sym_id) =
                                self.resolve_name_at_node(&ident.escaped_text, base_node)
                        {
                            current_sym_id = target_sym_id;
                            found = true;
                        }
                    }
                }

                if !found {
                    break;
                }
            }

            if cycle_detected {
                // For cross-file cycles, use max SymbolId heuristic to deduplicate:
                // only report the cycle from the file containing the highest SymbolId.
                // For same-file cycles, report on the first symbol encountered (no dedup needed).
                let this_file_idx = self.ctx.current_file_idx;
                let is_cross_file = visited.iter().any(|key| key.0 != this_file_idx);
                if is_cross_file {
                    let max_sym_in_cycle = visited_sym_ids
                        .iter()
                        .max_by_key(|s| s.0)
                        .copied()
                        .unwrap_or(sym_id);
                    if sym_id != max_sym_in_cycle {
                        continue;
                    }
                }

                for key in &visited {
                    if key.0 == this_file_idx {
                        reported_cycle_symbols.insert(tsz_binder::SymbolId(key.1 as u32));
                    }
                }

                let Some(decl_idx) = sym.primary_declaration() else {
                    continue;
                };
                let fallback_span = sym
                    .first_declaration_span
                    .or_else(|| {
                        sym.stable_value_declaration.is_known().then_some((
                            sym.stable_value_declaration.pos,
                            sym.stable_value_declaration.end,
                        ))
                    })
                    .or_else(|| {
                        sym.stable_declarations
                            .iter()
                            .find(|stable| stable.is_known())
                            .map(|stable| (stable.pos, stable.end))
                    });

                let mut error_node_idx = decl_idx;

                if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
                    if decl_node.kind == syntax_kind_ext::EXPORT_SPECIFIER
                        || decl_node.kind == syntax_kind_ext::IMPORT_SPECIFIER
                    {
                        if let Some(spec) = self.ctx.arena.get_specifier(decl_node) {
                            let name_idx = if spec.name.is_some() {
                                spec.name
                            } else {
                                spec.property_name
                            };
                            if name_idx.is_some() {
                                error_node_idx = name_idx;
                            }
                        }
                    } else if decl_node.kind == syntax_kind_ext::IMPORT_CLAUSE
                        && let Some(import_clause) = self.ctx.arena.get_import_clause(decl_node)
                        && import_clause.name.is_some()
                    {
                        error_node_idx = import_clause.name;
                    }
                }

                let message = format_message(
                    diagnostic_messages::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS,
                    &[&sym.escaped_name],
                );
                let code = diagnostic_codes::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS;
                if self.get_node_span(error_node_idx).is_some() {
                    self.error_at_node(error_node_idx, &message, code);
                } else if let Some((start, end)) = fallback_span {
                    self.error(start, end.saturating_sub(start), message, code);
                }
            }
        }
    }

    /// Detects circular aliases in CommonJS export property assignments.
    ///
    /// In JS files, `exports.X = exports.Y` creates an alias from X to Y on
    /// the same module. tsc emits TS2303 when:
    /// - The alias chain is explicitly circular (X -> Y -> X)
    /// - The alias target doesn't resolve to a concrete (non-alias) value
    ///   (e.g., `exports.blah = exports.someProp` where someProp is not defined)
    pub(crate) fn check_commonjs_circular_aliases(&mut self, statements: &[NodeIndex]) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        // alias_map: property_name -> (target_property_name, lhs_node_index)
        // for `exports.X = exports.Y` patterns
        let mut alias_map: FxHashMap<String, (String, NodeIndex)> = FxHashMap::default();
        // concrete_props: properties assigned a concrete (non-exports-ref) value
        // e.g., `exports.foo = 42` or `exports.bar = someFunction`
        let mut concrete_props: FxHashSet<String> = FxHashSet::default();

        for &stmt_idx in statements {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(expr_stmt) = self.ctx.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let Some(expr_node) = self.ctx.arena.get(expr_stmt.expression) else {
                continue;
            };
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(bin) = self.ctx.arena.get_binary_expr(expr_node) else {
                continue;
            };
            if bin.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }

            // Check LHS is `exports.X`
            let Some(lhs_prop) = self.get_exports_property_name(bin.left) else {
                continue;
            };

            // Check if RHS is `exports.Y` (alias) or a concrete value
            if let Some(rhs_prop) = self.get_exports_property_name(bin.right) {
                alias_map.insert(lhs_prop, (rhs_prop, bin.left));
            } else {
                concrete_props.insert(lhs_prop);
            }
        }

        // For each alias, follow the chain. If it resolves to a concrete
        // property, it's not circular. If it cycles or reaches a name that
        // has no definition (neither alias nor concrete), it's circular.
        let mut reported: FxHashSet<String> = FxHashSet::default();
        for start_name in alias_map.keys().cloned().collect::<Vec<_>>() {
            if reported.contains(&start_name) {
                continue;
            }

            let mut visited = FxHashSet::default();
            let mut current = start_name.clone();
            let mut is_circular = false;

            loop {
                // If we reach a concrete property, chain is resolved
                if concrete_props.contains(&current) {
                    break;
                }
                if !visited.insert(current.clone()) {
                    // Visited this name before → cycle detected
                    is_circular = true;
                    break;
                }
                match alias_map.get(&current) {
                    Some((next, _)) => current = next.clone(),
                    None => {
                        // Target doesn't exist as alias or concrete → unresolvable
                        is_circular = true;
                        break;
                    }
                }
            }

            if is_circular && let Some((_, error_node)) = alias_map.get(&start_name) {
                let message = format_message(
                    diagnostic_messages::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS,
                    &[&start_name],
                );
                self.error_at_node(
                    *error_node,
                    &message,
                    diagnostic_codes::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS,
                );
                for name in &visited {
                    reported.insert(name.clone());
                }
            }
        }
    }

    /// Helper: if `idx` points to `exports.X` (property access where the
    /// object is `exports`), return `Some("X")`. Otherwise `None`.
    fn get_exports_property_name(&self, idx: NodeIndex) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let node = self.ctx.arena.get(idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.ctx.arena.get_access_expr(node)?;

        // Check that the object is `exports`
        let obj_node = self.ctx.arena.get(access.expression)?;
        if obj_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let obj_ident = self.ctx.arena.get_identifier(obj_node)?;
        if obj_ident.escaped_text != "exports" {
            return None;
        }

        // Get the property name
        let name_node = self.ctx.arena.get(access.name_or_argument)?;
        let name_ident = self.ctx.arena.get_identifier(name_node)?;
        Some(name_ident.escaped_text.clone())
    }
}
