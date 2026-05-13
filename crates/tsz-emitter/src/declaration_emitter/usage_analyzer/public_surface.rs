use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::UsageAnalyzer;

impl UsageAnalyzer<'_> {
    pub(super) fn analyze_commonjs_assignment_public_surface(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
            return;
        };
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_stmt.expression);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return;
        };
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return;
        }
        let Some(binary) = self.arena.get_binary_expr(expr_node) else {
            return;
        };
        if binary.operator_token != SyntaxKind::EqualsToken as u16 {
            return;
        }

        let lhs = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(binary.left);
        if !self.expression_is_module_exports_reference(lhs)
            && !self.expression_is_module_exports_property_reference(lhs)
        {
            return;
        }

        self.analyze_expression_public_surface(binary.right);
    }

    pub(super) fn analyze_export_default_initializer_reference(&mut self, initializer: NodeIndex) {
        let referenced = self.unwrap_export_default_expression(initializer);
        if referenced != initializer {
            self.analyze_reference_as_value_and_type(referenced);
        }
    }

    pub(super) fn analyze_constructor_public_surface_assignments(&mut self, node_idx: NodeIndex) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.arena.get_block(node) {
                    for &stmt_idx in &block.statements.nodes {
                        self.analyze_constructor_public_surface_assignments(stmt_idx);
                    }
                }
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.arena.get_expression_statement(node) {
                    self.analyze_constructor_public_surface_expression(expr_stmt.expression);
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.arena.get_if_statement(node) {
                    self.analyze_constructor_public_surface_assignments(if_data.then_statement);
                    if if_data.else_statement.is_some() {
                        self.analyze_constructor_public_surface_assignments(if_data.else_statement);
                    }
                }
            }
            _ => {}
        }
    }

    pub(super) fn analyze_expression_public_surface(&mut self, expr_idx: NodeIndex) {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return;
        };
        if expr_node.kind == syntax_kind_ext::CLASS_EXPRESSION {
            self.analyze_class_declaration(expr_idx);
        }
    }

    fn analyze_constructor_public_surface_expression(&mut self, expr_idx: NodeIndex) {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return;
        };
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return;
        }
        let Some(binary) = self.arena.get_binary_expr(expr_node) else {
            return;
        };
        if binary.operator_token != SyntaxKind::EqualsToken as u16
            || !self.expression_is_this_property_reference(binary.left)
        {
            return;
        }

        let referenced = self.unwrap_export_default_expression(binary.right);
        if referenced != binary.right {
            self.analyze_reference_as_value_and_type(referenced);
        } else {
            self.analyze_expression_public_surface(binary.right);
        }
    }

    fn expression_is_module_exports_reference(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.arena.get_access_expr(expr_node) else {
            return false;
        };
        self.identifier_text(access.expression) == Some("module")
            && self.identifier_text(access.name_or_argument) == Some("exports")
    }

    fn expression_is_module_exports_property_reference(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.arena.get_access_expr(expr_node) else {
            return false;
        };
        self.expression_is_module_exports_reference(access.expression)
    }

    fn expression_is_this_property_reference(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.arena.get_access_expr(expr_node) else {
            return false;
        };
        self.arena
            .get(access.expression)
            .is_some_and(|node| node.kind == SyntaxKind::ThisKeyword as u16)
    }

    fn identifier_text(&self, idx: NodeIndex) -> Option<&str> {
        self.arena
            .get(idx)
            .and_then(|node| self.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())
    }
}
