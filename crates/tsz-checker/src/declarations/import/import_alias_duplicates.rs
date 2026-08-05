//! Duplicate import-equals alias diagnostics.

use crate::state::CheckerState;
use std::collections::HashMap;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

impl<'a> CheckerState<'a> {
    /// Collect every `import X = ...` declaration reachable from `stmt_idx`
    /// without crossing a declaration-container boundary.
    ///
    /// `tsc`'s binder keeps two cursors while binding: `container` (source
    /// file, module body, class body, or any function-like node) and
    /// `blockScopeContainer` (additionally every plain `Block`/`if`/`for`/
    /// `try`/`switch`/... body). An alias is not block-scoped, so a
    /// position-invalid `import x = ...` nested inside `if`/`for`/`try`/
    /// `switch`/labeled/`with` bodies is recorded in the *container*, exactly
    /// like a bare `{ }` block (see #16428). This walk mirrors that
    /// transparency so two aliases nested in different blocks of the same
    /// container are grouped together for the TS2300 check below, while a
    /// function body, class body, or nested namespace — genuine containers —
    /// stop the walk, matching `nearest_declaration_container_scope`.
    fn collect_import_equals_transparently(&self, stmt_idx: NodeIndex, out: &mut Vec<NodeIndex>) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };
        match node.kind {
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => out.push(stmt_idx),
            syntax_kind_ext::BLOCK => {
                // `get_block` also covers CLASS_STATIC_BLOCK_DECLARATION and
                // CASE_BLOCK, both excluded above/below — this arm only ever
                // sees a plain `{ }` block here.
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &inner in &block.statements.nodes {
                        self.collect_import_equals_transparently(inner, out);
                    }
                }
            }
            syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
                    self.collect_import_equals_transparently(if_stmt.then_statement, out);
                    if if_stmt.else_statement.is_some() {
                        self.collect_import_equals_transparently(if_stmt.else_statement, out);
                    }
                }
            }
            syntax_kind_ext::WHILE_STATEMENT
            | syntax_kind_ext::DO_STATEMENT
            | syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.ctx.arena.get_loop(node) {
                    self.collect_import_equals_transparently(loop_data.statement, out);
                }
            }
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT => {
                if let Some(for_in_of) = self.ctx.arena.get_for_in_of(node) {
                    self.collect_import_equals_transparently(for_in_of.statement, out);
                }
            }
            syntax_kind_ext::WITH_STATEMENT => {
                if let Some(with_stmt) = self.ctx.arena.get_with_statement(node) {
                    self.collect_import_equals_transparently(with_stmt.then_statement, out);
                }
            }
            syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.ctx.arena.get_try(node) {
                    self.collect_import_equals_transparently(try_data.try_block, out);
                    if try_data.catch_clause.is_some()
                        && let Some(catch_node) = self.ctx.arena.get(try_data.catch_clause)
                        && let Some(catch_data) = self.ctx.arena.get_catch_clause(catch_node)
                    {
                        self.collect_import_equals_transparently(catch_data.block, out);
                    }
                    if try_data.finally_block.is_some() {
                        self.collect_import_equals_transparently(try_data.finally_block, out);
                    }
                }
            }
            syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch_data) = self.ctx.arena.get_switch(node)
                    && let Some(case_block_node) = self.ctx.arena.get(switch_data.case_block)
                    && let Some(case_block) = self.ctx.arena.get_block(case_block_node)
                {
                    for &clause_idx in &case_block.statements.nodes {
                        let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                            continue;
                        };
                        if let Some(clause) = self.ctx.arena.get_case_clause(clause_node) {
                            for &inner in &clause.statements.nodes {
                                self.collect_import_equals_transparently(inner, out);
                            }
                        }
                    }
                }
            }
            syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.ctx.arena.get_labeled_statement(node) {
                    self.collect_import_equals_transparently(labeled.statement, out);
                }
            }
            _ => {}
        }
    }

    /// Check for duplicate import alias declarations within a scope.
    ///
    /// TS2300: Emitted when multiple `import X = ...` declarations have the same name
    /// within the same scope (namespace, module, or file), including a
    /// position-invalid alias nested in a different block of that scope
    /// (`{ import x = ...; } { import x = ...; }`, #16429).
    pub(crate) fn check_import_alias_duplicates(&mut self, statements: &[NodeIndex]) {
        let mut alias_map: HashMap<String, Vec<NodeIndex>> = HashMap::new();

        let mut collected = Vec::new();
        for &stmt_idx in statements {
            self.collect_import_equals_transparently(stmt_idx, &mut collected);
        }

        for stmt_idx in collected {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            let Some(import_decl) = self.ctx.arena.get_import_decl(node) else {
                continue;
            };

            let Some(alias_node) = self.ctx.arena.get(import_decl.import_clause) else {
                continue;
            };
            let Some(alias_id) = self.ctx.arena.get_identifier(alias_node) else {
                continue;
            };

            alias_map
                .entry(alias_id.escaped_text.to_string())
                .or_default()
                .push(stmt_idx);
        }

        for (alias_name, indices) in alias_map {
            if indices.len() <= 1 {
                continue;
            }

            for &import_idx in &indices {
                let Some(import_node) = self.ctx.arena.get(import_idx) else {
                    continue;
                };
                let Some(import_decl) = self.ctx.arena.get_import_decl(import_node) else {
                    continue;
                };

                let alias_node = import_decl.import_clause;
                let Some(sym_id) = self.resolve_identifier_symbol(alias_node) else {
                    tracing::trace!("Could not resolve identifier symbol");
                    continue;
                };
                let symbol = self
                    .ctx
                    .binder
                    .symbols
                    .get(sym_id)
                    .expect("sym_id resolved from resolve_identifier_symbol");
                tracing::trace!("Symbol flags: {:?}", symbol.flags);
                if self.symbol_is_value_only(sym_id, Some(&alias_name)) {
                    self.report_wrong_meaning_diagnostic(
                        &alias_name,
                        import_decl.import_clause,
                        crate::query_boundaries::name_resolution::NameLookupKind::Value,
                    );
                } else {
                    self.error_at_node(
                        import_decl.import_clause,
                        &format!("Duplicate identifier '{alias_name}'."),
                        crate::diagnostics::diagnostic_codes::DUPLICATE_IDENTIFIER,
                    );
                }
            }
        }
    }

    /// TS2300 for duplicate ES import declaration local bindings.
    ///
    /// `import { x } ...; import { y as x } ...;` reports duplicate
    /// identifiers at both local binding names, independent of whether module
    /// resolution succeeds.
    pub(crate) fn check_import_declaration_duplicate_bindings(&mut self, statements: &[NodeIndex]) {
        let mut binding_map: HashMap<String, Vec<NodeIndex>> = HashMap::new();

        for &stmt_idx in statements {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }

            let Some(import_decl) = self.ctx.arena.get_import_decl(node) else {
                continue;
            };
            let Some(clause_node) = self.ctx.arena.get(import_decl.import_clause) else {
                continue;
            };
            let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
                continue;
            };

            if clause.name.is_some()
                && let Some(ident) = self.ctx.arena.get_identifier_at(clause.name)
            {
                binding_map
                    .entry(ident.escaped_text.to_string())
                    .or_default()
                    .push(clause.name);
            }

            let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings) else {
                continue;
            };
            if bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                if let Some(ns) = self.ctx.arena.get_named_imports(bindings_node)
                    && ns.name.is_some()
                    && let Some(ident) = self.ctx.arena.get_identifier_at(ns.name)
                {
                    binding_map
                        .entry(ident.escaped_text.to_string())
                        .or_default()
                        .push(ns.name);
                }
                continue;
            }

            if bindings_node.kind != syntax_kind_ext::NAMED_IMPORTS {
                continue;
            }

            let Some(named) = self.ctx.arena.get_named_imports(bindings_node) else {
                continue;
            };
            for &spec_idx in &named.elements.nodes {
                let Some(spec_node) = self.ctx.arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = self.ctx.arena.get_specifier(spec_node) else {
                    continue;
                };
                let local_name_idx = if spec.name.is_some() {
                    spec.name
                } else {
                    spec.property_name
                };
                if local_name_idx.is_some()
                    && let Some(ident) = self.ctx.arena.get_identifier_at(local_name_idx)
                {
                    binding_map
                        .entry(ident.escaped_text.to_string())
                        .or_default()
                        .push(local_name_idx);
                }
            }
        }

        for (name, binding_indices) in binding_map {
            if binding_indices.len() <= 1 {
                continue;
            }
            for binding_idx in binding_indices {
                self.error_at_node(
                    binding_idx,
                    &format!("Duplicate identifier '{name}'."),
                    crate::diagnostics::diagnostic_codes::DUPLICATE_IDENTIFIER,
                );
            }
        }
    }
}
