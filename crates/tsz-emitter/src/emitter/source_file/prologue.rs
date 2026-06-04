use super::super::Printer;
use tsz_parser::parser::node::{Node, SourceFileData};
use tsz_parser::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn source_has_use_strict_prologue(
        &self,
        source: &SourceFileData,
    ) -> bool {
        for &idx in &source.statements.nodes {
            let Some(stmt_node) = self.arena.get(idx) else {
                break;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                break;
            }
            let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
                break;
            };
            let Some(expr_node) = self.arena.get(expr_stmt.expression) else {
                break;
            };
            if !expr_node.is_string_literal() {
                break;
            }
            if self.expression_is_use_strict_literal(expr_node) {
                return true;
            }
        }
        false
    }

    pub(in crate::emitter) fn source_prologue_directive_count(
        &self,
        source: &SourceFileData,
    ) -> usize {
        source
            .statements
            .nodes
            .iter()
            .take_while(|&&idx| {
                let Some(stmt_node) = self.arena.get(idx) else {
                    return false;
                };
                if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                    return false;
                }
                let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
                    return false;
                };
                self.arena
                    .get(expr_stmt.expression)
                    .is_some_and(|expr_node| expr_node.is_string_literal())
            })
            .count()
    }

    pub(in crate::emitter) fn source_has_module_wrapper_statement(
        &self,
        source: &SourceFileData,
    ) -> bool {
        source.statements.nodes.iter().any(|&idx| {
            let callee_idx = self
                .arena
                .get(idx)
                .and_then(|stmt| self.arena.get_expression_statement(stmt))
                .and_then(|expr_stmt| self.arena.get(expr_stmt.expression))
                .and_then(|expr| self.arena.get_call_expr(expr))
                .map(|call| call.expression);
            let Some(callee_idx) = callee_idx else {
                return false;
            };
            let Some(callee_node) = self.arena.get(callee_idx) else {
                return false;
            };
            if let Some(ident) = self.arena.get_identifier(callee_node) {
                return ident.escaped_text.as_str() == "define";
            }
            if let Some(access) = self.arena.get_access_expr(callee_node) {
                let obj_is_system = self
                    .arena
                    .get(access.expression)
                    .and_then(|obj| self.arena.get_identifier(obj))
                    .is_some_and(|ident| ident.escaped_text.as_str() == "System");
                let prop_is_register = self
                    .arena
                    .get(access.name_or_argument)
                    .and_then(|name| self.arena.get_identifier(name))
                    .is_some_and(|ident| ident.escaped_text.as_str() == "register");
                return obj_is_system && prop_is_register;
            }
            false
        })
    }

    fn expression_is_use_strict_literal(&self, expr_node: &Node) -> bool {
        if let Some(lit) = self.arena.get_literal(expr_node) {
            return lit.text == "use strict";
        }
        let Some(text) = self.source_text else {
            return false;
        };
        crate::safe_slice::slice(text, expr_node.pos as usize, expr_node.end as usize)
            .is_ok_and(|s| s == "\"use strict\"" || s == "'use strict'")
    }
}

pub(in crate::emitter) fn jsx_dev_file_name(file_name: &str) -> String {
    let normalized = file_name.replace('\\', "/");
    if let Some(src_start) = normalized.find("/.src/") {
        return normalized[src_start..].to_string();
    }
    if let Some(stripped) = normalized.strip_prefix(".src/") {
        return format!("/.src/{stripped}");
    }
    normalized
        .rsplit('/')
        .next()
        .unwrap_or(&normalized)
        .to_string()
}
