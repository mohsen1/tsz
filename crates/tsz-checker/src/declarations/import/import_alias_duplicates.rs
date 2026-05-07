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
        self.check_import_alias_duplicates_impl(statements, true);
    }

    /// TS2300 for top-level source-file import alias duplicates.
    ///
    /// tsc reports the follow-up alias declarations at source-file scope, while
    /// namespace/module bodies report each duplicate alias declaration.
    pub(crate) fn check_import_alias_duplicate_followups(&mut self, statements: &[NodeIndex]) {
        self.check_import_alias_duplicates_impl(statements, false);
    }

    fn check_import_alias_duplicates_impl(
        &mut self,
        statements: &[NodeIndex],
        report_all_duplicates: bool,
    ) {
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

            for (position, &import_idx) in indices.iter().enumerate() {
                if !report_all_duplicates && position == 0 {
                    continue;
                }
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
}
