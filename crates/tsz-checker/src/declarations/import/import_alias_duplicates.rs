//! Duplicate import-equals alias diagnostics.

use crate::state::CheckerState;
use std::collections::HashMap;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

impl<'a> CheckerState<'a> {
    /// Check for duplicate import alias declarations within a scope.
    ///
    /// TS2300: Emitted when multiple `import X = ...` declarations have the same name
    /// within the same scope (namespace, module, or file).
    pub(crate) fn check_import_alias_duplicates(&mut self, statements: &[NodeIndex]) {
        let mut alias_map: HashMap<String, Vec<NodeIndex>> = HashMap::new();

        for &stmt_idx in statements {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };

            if node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                continue;
            }

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
