use super::{Printer, get_operator_text};
use crate::parser::{node::Node, syntax_kind_ext};
use crate::scanner::SyntaxKind;

impl<'a> Printer<'a> {
    // =========================================================================
    // Expressions
    // =========================================================================

    pub(super) fn emit_binary_expression(&mut self, node: &Node) {
        let Some(binary) = self.arena.get_binary_expr(node) else {
            return;
        };

        self.emit(binary.left);
        // Comma operator: no space before, space after (e.g., `(1, 2, 3)`)
        if binary.operator_token == SyntaxKind::CommaToken as u16 {
            self.write(", ");
        } else {
            self.write_space();
            self.write(get_operator_text(binary.operator_token));
            self.write_space();
        }
        self.emit(binary.right);
    }

    pub(super) fn emit_prefix_unary(&mut self, node: &Node) {
        let Some(unary) = self.arena.get_unary_expr(node) else {
            return;
        };

        self.write(get_operator_text(unary.operator));
        self.emit(unary.operand);
    }

    pub(super) fn emit_postfix_unary(&mut self, node: &Node) {
        let Some(unary) = self.arena.get_unary_expr(node) else {
            return;
        };

        self.emit(unary.operand);
        self.write(get_operator_text(unary.operator));
    }

    pub(super) fn emit_call_expression(&mut self, node: &Node) {
        let Some(call) = self.arena.get_call_expr(node) else {
            return;
        };

        if self.ctx.target_es5
            && let Some(expr_node) = self.arena.get(call.expression)
        {
            if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.arena.get_access_expr(expr_node)
                && let Some(base) = self.arena.get(access.expression)
                && base.kind == SyntaxKind::SuperKeyword as u16
            {
                self.write("_super.prototype.");
                self.emit(access.name_or_argument);
                self.write(".call(");
                if self.ctx.arrow_state.this_capture_depth > 0 {
                    self.write("_this");
                } else {
                    self.write("this");
                }
                if let Some(ref args) = call.arguments {
                    for &arg_idx in &args.nodes {
                        self.write(", ");
                        self.emit(arg_idx);
                    }
                }
                self.write(")");
                return;
            }
            if expr_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                && let Some(access) = self.arena.get_access_expr(expr_node)
                && let Some(base) = self.arena.get(access.expression)
                && base.kind == SyntaxKind::SuperKeyword as u16
            {
                self.write("_super.prototype[");
                self.emit(access.name_or_argument);
                self.write("].call(");
                if self.ctx.arrow_state.this_capture_depth > 0 {
                    self.write("_this");
                } else {
                    self.write("this");
                }
                if let Some(ref args) = call.arguments {
                    for &arg_idx in &args.nodes {
                        self.write(", ");
                        self.emit(arg_idx);
                    }
                }
                self.write(")");
                return;
            }
        }

        self.emit(call.expression);
        self.write("(");
        if let Some(ref args) = call.arguments {
            self.emit_comma_separated(&args.nodes);
        }
        self.write(")");
    }

    pub(super) fn emit_new_expression(&mut self, node: &Node) {
        let Some(call) = self.arena.get_call_expr(node) else {
            return;
        };

        self.write("new ");
        self.emit(call.expression);
        // Only emit parentheses if they were present in source (arguments is Some)
        if let Some(ref args) = call.arguments {
            self.write("(");
            self.emit_comma_separated(&args.nodes);
            self.write(")");
        }
    }

    pub(super) fn emit_property_access(&mut self, node: &Node) {
        let Some(access) = self.arena.get_access_expr(node) else {
            return;
        };

        self.emit(access.expression);

        // Preserve multi-line property access chains from the original source.
        // TypeScript preserves the original line break pattern. If there's a
        // newline between expression end and the property name, we need to
        // reproduce the original layout:
        // - If dot is before newline: `expr.\n    name` -> emit ".\n    name"
        // - If dot is after newline: `expr\n    .name` -> emit "\n    .name"
        if let Some(text) = self.source_text {
            if let Some(expr_node) = self.arena.get(access.expression) {
                if let Some(name_node) = self.arena.get(access.name_or_argument) {
                    let expr_end = expr_node.end as usize;
                    let name_start = name_node.pos as usize;
                    let between_end = std::cmp::min(name_start, text.len());
                    let between_start = std::cmp::min(expr_end, between_end);
                    let between = &text[between_start..between_end];
                    if between.contains('\n') {
                        // Find where the dot is relative to the newline
                        if let Some(dot_pos) = between.find('.') {
                            let after_dot = &between[dot_pos + 1..];
                            if after_dot.contains('\n') {
                                // Dot before newline: `expr.\n    name`
                                self.write(".");
                                self.write_line();
                                self.increase_indent();
                                self.emit(access.name_or_argument);
                                self.decrease_indent();
                            } else {
                                // Newline before dot: `expr\n    .name`
                                self.write_line();
                                self.increase_indent();
                                self.write(".");
                                self.emit(access.name_or_argument);
                                self.decrease_indent();
                            }
                        } else {
                            self.write(".");
                            self.emit(access.name_or_argument);
                        }
                        return;
                    }
                }
            }
        }

        self.write(".");
        self.emit(access.name_or_argument);
    }

    pub(super) fn emit_element_access(&mut self, node: &Node) {
        let Some(access) = self.arena.get_access_expr(node) else {
            return;
        };

        self.emit(access.expression);
        self.write("[");
        self.emit(access.name_or_argument);
        self.write("]");
    }

    pub(super) fn emit_parenthesized(&mut self, node: &Node) {
        let Some(paren) = self.arena.get_parenthesized(node) else {
            return;
        };

        // If the inner expression is a type assertion/as/satisfies expression,
        // the parens were only needed for the TS syntax (e.g., `(<Type>x).foo`).
        // In JS emit, the type assertion is stripped, making the parens unnecessary.
        if let Some(inner) = self.arena.get(paren.expression) {
            if inner.kind == syntax_kind_ext::TYPE_ASSERTION
                || inner.kind == syntax_kind_ext::AS_EXPRESSION
                || inner.kind == syntax_kind_ext::SATISFIES_EXPRESSION
            {
                // Emit the inner expression directly, without parens
                self.emit(paren.expression);
                return;
            }
        }

        self.write("(");
        self.emit(paren.expression);
        self.write(")");
    }

    pub(super) fn emit_type_assertion_expression(&mut self, node: &Node) {
        let Some(assertion) = self.arena.get_type_assertion(node) else {
            self.write("void 0");
            return;
        };

        self.emit_expression(assertion.expression);
    }

    pub(super) fn emit_non_null_expression(&mut self, node: &Node) {
        let Some(unary) = self.arena.get_unary_expr_ex(node) else {
            self.write("void 0");
            return;
        };

        self.emit_expression(unary.expression);
    }

    pub(super) fn emit_conditional(&mut self, node: &Node) {
        let Some(cond) = self.arena.get_conditional_expr(node) else {
            return;
        };

        self.emit(cond.condition);
        self.write(" ? ");
        self.emit(cond.when_true);
        self.write(" : ");
        self.emit(cond.when_false);
    }

    pub(super) fn emit_array_literal(&mut self, node: &Node) {
        let Some(array) = self.arena.get_literal_expr(node) else {
            return;
        };

        self.write("[");
        self.emit_comma_separated(&array.elements.nodes);
        self.write("]");
    }

    pub(super) fn emit_object_literal(&mut self, node: &Node) {
        let Some(obj) = self.arena.get_literal_expr(node) else {
            return;
        };

        if obj.elements.nodes.is_empty() {
            self.write("{}");
            return;
        }

        // ES5 computed/spread lowering is handled via TransformDirective::ES5ObjectLiteral.

        // Check if source had a trailing comma after the last element
        let has_trailing_comma = self.has_trailing_comma_in_source(node, &obj.elements.nodes);

        // Preserve single-line formatting from source
        if self.is_single_line(node) || obj.elements.nodes.len() == 1 {
            self.write("{ ");
            for (i, &prop) in obj.elements.nodes.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.emit(prop);
            }
            self.write(" }");
        } else {
            // Multi-line format for object literals
            self.write("{");
            self.write_line();
            self.increase_indent();
            for (i, &prop) in obj.elements.nodes.iter().enumerate() {
                self.emit(prop);
                if i < obj.elements.nodes.len() - 1 || has_trailing_comma {
                    self.write(",");
                }
                self.write_line();
            }
            self.decrease_indent();
            self.write("}");
        }
    }

    pub(super) fn emit_property_assignment(&mut self, node: &Node) {
        let Some(prop) = self.arena.get_property_assignment(node) else {
            return;
        };

        self.emit(prop.name);
        self.write(": ");
        self.emit_expression(prop.initializer);
    }

    pub(super) fn emit_shorthand_property(&mut self, node: &Node) {
        let Some(shorthand) = self.arena.get_shorthand_property(node) else {
            // Fallback: try to get identifier data directly
            if let Some(ident) = self.arena.get_identifier(node) {
                self.write(&ident.escaped_text);
            }
            return;
        };

        self.emit(shorthand.name);
        if shorthand.equals_token {
            self.write(" = ");
            // Object assignment pattern default value would go here
        }
    }
}
