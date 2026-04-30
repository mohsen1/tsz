//! Recursive helper for `check_ambient_default_namespace_export_duplicates`.
//!
//! Top-level value declarations (function, class, var) inside an ambient
//! module body are wrapped in `EXPORT_DECLARATION` when prefixed with
//! `export`, so we need to drill into the export clause to find the inner
//! declaration name. Extracted from `equals.rs` to keep that file under the
//! 2000-LOC checker boundary.

use crate::state::CheckerState;
use std::collections::{HashMap, HashSet};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(super) fn collect_ambient_default_export_dup_targets(
        &self,
        stmt_idx: NodeIndex,
        namespaces: &mut HashMap<String, NodeIndex>,
        default_export_names: &mut Vec<(String, NodeIndex)>,
        sibling_value_names: &mut HashSet<String>,
    ) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::MODULE_DECLARATION
            && let Some(module_decl) = self.ctx.arena.get_module(node)
            && let Some(name_node) = self.ctx.arena.get(module_decl.name)
            && name_node.kind == SyntaxKind::Identifier as u16
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            namespaces.insert(ident.escaped_text.clone(), module_decl.name);
            return;
        }

        if node.kind == syntax_kind_ext::EXPORT_DECLARATION
            && let Some(export_decl) = self.ctx.arena.get_export_decl(node)
        {
            if export_decl.is_default_export
                && let Some(exported_node) = self.ctx.arena.get(export_decl.export_clause)
                && exported_node.kind == SyntaxKind::Identifier as u16
                && let Some(ident) = self.ctx.arena.get_identifier(exported_node)
            {
                default_export_names.push((ident.escaped_text.clone(), export_decl.export_clause));
                return;
            }
            // `export function foo(){}` / `export class C {}` / `export const x = ...`
            // are EXPORT_DECLARATIONs whose export_clause is the inner declaration.
            // Recurse so the inner name participates in the value-sibling check.
            if export_decl.export_clause.is_some() {
                self.collect_ambient_default_export_dup_targets(
                    export_decl.export_clause,
                    namespaces,
                    default_export_names,
                    sibling_value_names,
                );
            }
            return;
        }

        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            && let Some(func) = self.ctx.arena.get_function(node)
            && let Some(ident) = self.ctx.arena.get_identifier_at(func.name)
        {
            sibling_value_names.insert(ident.escaped_text.clone());
            return;
        }

        if node.kind == syntax_kind_ext::CLASS_DECLARATION
            && let Some(class) = self.ctx.arena.get_class(node)
            && let Some(ident) = self.ctx.arena.get_identifier_at(class.name)
        {
            sibling_value_names.insert(ident.escaped_text.clone());
            return;
        }

        if node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            && let Some(var_stmt) = self.ctx.arena.get_variable(node)
        {
            for &list_idx in &var_stmt.declarations.nodes {
                let Some(list_node) = self.ctx.arena.get(list_idx) else {
                    continue;
                };
                let Some(list) = self.ctx.arena.get_variable(list_node) else {
                    continue;
                };
                for &decl_idx in &list.declarations.nodes {
                    let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                        continue;
                    };
                    if let Some(ident) = self.ctx.arena.get_identifier_at(var_decl.name) {
                        sibling_value_names.insert(ident.escaped_text.clone());
                    }
                }
            }
        }
    }
}
