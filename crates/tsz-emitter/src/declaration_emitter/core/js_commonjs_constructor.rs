use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn emit_js_commonjs_constructor_prototype_class(
        &mut self,
        name_idx: NodeIndex,
    ) -> bool {
        let Some(export_name) = self.js_commonjs_export_name_text(name_idx) else {
            return false;
        };
        let prototype_members = self.js_prototype_object_members_for_export_name(&export_name);
        if prototype_members.is_empty() {
            return false;
        }

        self.write_indent();
        self.write("export class ");
        self.write(&export_name);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        for member_idx in prototype_members {
            self.emit_js_commonjs_constructor_prototype_member(member_idx);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        self.emitted_module_indicator = true;
        true
    }

    fn emit_js_commonjs_constructor_prototype_member(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };
        let before_jsdoc_len = self.writer.len();
        let saved_comment_idx = self.comment_emit_idx;
        self.emit_leading_jsdoc_comments(member_node.pos);
        let before_member_len = self.writer.len();

        if let Some(prop) = self.arena.get_property_assignment(member_node) {
            if let Some(type_text) = self
                .resolve_declaration_type_text(&[prop.initializer], Some(prop.initializer))
                .map(|resolved| resolved.emitted_type_text)
                .or_else(|| self.allowlisted_initializer_type_text(prop.initializer))
            {
                self.write_indent();
                self.emit_node(prop.name);
                self.write(": ");
                self.write(&type_text);
                self.write(";");
                self.write_line();
            }
        } else {
            self.emit_class_member(member_idx);
        }

        if self.writer.len() == before_member_len {
            self.writer.truncate(before_jsdoc_len);
            self.comment_emit_idx = saved_comment_idx;
            self.skip_comments_in_node(member_node.pos, member_node.end);
        }
    }

    pub(super) fn js_prototype_object_members_for_export_name(&self, name: &str) -> Vec<NodeIndex> {
        let Some(source_file_idx) = self.current_source_file_idx else {
            return Vec::new();
        };
        let Some(source_file_node) = self.arena.get(source_file_idx) else {
            return Vec::new();
        };
        let Some(source_file) = self.arena.get_source_file(source_file_node) else {
            return Vec::new();
        };
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let expr_idx = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(expr_stmt.expression);
            let Some(expr_node) = self.arena.get(expr_idx) else {
                continue;
            };
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(binary) = self.arena.get_binary_expr(expr_node) else {
                continue;
            };
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }
            let lhs = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(binary.left);
            let Some(lhs_node) = self.arena.get(lhs) else {
                continue;
            };
            if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                continue;
            }
            let Some(lhs_access) = self.arena.get_access_expr(lhs_node) else {
                continue;
            };
            if self
                .get_identifier_text(lhs_access.name_or_argument)
                .as_deref()
                != Some("prototype")
            {
                continue;
            }
            let receiver_name = self
                .get_identifier_text(lhs_access.expression)
                .or_else(|| self.module_exports_property_reference_name(lhs_access.expression));
            if receiver_name.as_deref() != Some(name) {
                continue;
            }
            let rhs = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(binary.right);
            let Some(rhs_node) = self.arena.get(rhs) else {
                continue;
            };
            if rhs_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                continue;
            }
            let Some(object) = self.arena.get_literal_expr(rhs_node) else {
                continue;
            };
            return object.elements.nodes.clone();
        }
        Vec::new()
    }
}
