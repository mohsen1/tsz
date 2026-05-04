//! Ambient external-module helpers for duplicate identifier checking.
//!
//! Handles the `declare module "x" { const value; export default value; }`
//! shape separately from the generic duplicate helper so synthetic default
//! import alias conflicts can reuse the same predicate and diagnostic anchor.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(super) fn ambient_external_module_default_export_has_value_sibling_named(
        &self,
        name: &str,
    ) -> bool {
        let Some(source_file) = self.ctx.arena.source_files.first() else {
            return false;
        };
        source_file.statements.nodes.iter().any(|&stmt_idx| {
            self.ambient_external_module_statement_default_export_has_value_sibling_named(
                stmt_idx, name,
            )
        })
    }

    fn ambient_external_module_statement_default_export_has_value_sibling_named(
        &self,
        stmt_idx: NodeIndex,
        name: &str,
    ) -> bool {
        let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };
        if stmt_node.kind != syntax_kind_ext::MODULE_DECLARATION {
            return false;
        }
        let Some(module_decl) = self.ctx.arena.get_module(stmt_node) else {
            return false;
        };
        let Some(name_node) = self.ctx.arena.get(module_decl.name) else {
            return false;
        };
        if self.ctx.arena.get_literal(name_node).is_none() {
            return false;
        }
        let Some(body_node) = self.ctx.arena.get(module_decl.body) else {
            return false;
        };
        if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
            return false;
        }
        let Some(block) = self.ctx.arena.get_module_block(body_node) else {
            return false;
        };
        let Some(statements) = block.statements.as_ref() else {
            return false;
        };

        let has_default_export = statements
            .nodes
            .iter()
            .any(|&inner_idx| self.statement_is_default_export_identifier_named(inner_idx, name));
        if !has_default_export {
            return false;
        }

        statements
            .nodes
            .iter()
            .any(|&inner_idx| self.statement_has_value_declaration_named(inner_idx, name, 0))
    }

    fn statement_is_default_export_identifier_named(
        &self,
        stmt_idx: NodeIndex,
        name: &str,
    ) -> bool {
        let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };
        if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
            return false;
        }
        let Some(export_decl) = self.ctx.arena.get_export_decl(stmt_node) else {
            return false;
        };
        export_decl.is_default_export
            && self
                .ctx
                .arena
                .get_identifier_at(export_decl.export_clause)
                .is_some_and(|ident| ident.escaped_text == name)
    }

    fn statement_has_value_declaration_named(
        &self,
        stmt_idx: NodeIndex,
        name: &str,
        depth: u8,
    ) -> bool {
        if depth > 4 {
            return false;
        }
        let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            let Some(export_decl) = self.ctx.arena.get_export_decl(stmt_node) else {
                return false;
            };
            return !export_decl.is_default_export
                && self.statement_has_value_declaration_named(
                    export_decl.export_clause,
                    name,
                    depth + 1,
                );
        }

        if stmt_node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
            return self
                .ctx
                .arena
                .get_function(stmt_node)
                .and_then(|func| self.ctx.arena.get_identifier_at(func.name))
                .is_some_and(|ident| ident.escaped_text == name);
        }

        if stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION {
            return self
                .ctx
                .arena
                .get_class(stmt_node)
                .and_then(|class| self.ctx.arena.get_identifier_at(class.name))
                .is_some_and(|ident| ident.escaped_text == name);
        }

        if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return false;
        }
        let Some(var_stmt) = self.ctx.arena.get_variable(stmt_node) else {
            return false;
        };
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
                if self
                    .ctx
                    .arena
                    .get_identifier_at(var_decl.name)
                    .is_some_and(|ident| ident.escaped_text == name)
                {
                    return true;
                }
            }
        }
        false
    }
}
