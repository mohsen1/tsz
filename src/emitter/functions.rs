use super::{ParamTransformPlan, Printer};
use crate::parser::NodeIndex;
use crate::parser::node::Node;
use crate::parser::syntax_kind_ext;
use crate::syntax::transform_utils::{contains_arguments_reference, contains_this_reference};

impl<'a> Printer<'a> {
    // =========================================================================
    // Functions
    // =========================================================================

    pub(super) fn emit_arrow_function(&mut self, node: &Node, _idx: NodeIndex) {
        let Some(func) = self.arena.get_function(node) else {
            return;
        };

        if self.ctx.target_es5 {
            let captures_this = contains_this_reference(self.arena, _idx);
            let captures_arguments = contains_arguments_reference(self.arena, _idx);
            self.emit_arrow_function_es5(node, func, captures_this, captures_arguments, &None);
            return;
        }

        self.emit_arrow_function_native(func);
    }

    /// Emit native ES6+ arrow function syntax
    fn emit_arrow_function_native(&mut self, func: &crate::parser::node::FunctionData) {
        if func.is_async {
            self.write("async ");
        }

        // Parameters (without types for JavaScript)
        self.write("(");
        self.emit_function_parameters_js(&func.parameters.nodes);
        self.write(")");

        // Skip return type for JavaScript

        self.write(" => ");

        // Body
        self.emit(func.body);
    }

    pub(super) fn emit_function_expression(&mut self, node: &Node, _idx: NodeIndex) {
        let Some(func) = self.arena.get_function(node) else {
            return;
        };

        if func.is_async && self.ctx.target_es5 && !func.asterisk_token {
            let func_name = if !func.name.is_none() {
                self.get_identifier_text_idx(func.name)
            } else {
                String::new()
            };
            self.emit_async_function_es5(func, &func_name, "this");
            return;
        }

        if func.is_async {
            self.write("async ");
        }

        self.write("function");

        if func.asterisk_token {
            self.write("*");
        }

        // Name (if any)
        if !func.name.is_none() {
            self.write_space();
            self.emit(func.name);
        } else {
            // Space before ( only for anonymous functions: function (x) vs function name(x)
            self.write(" ");
        }

        // Parameters (without types for JavaScript)
        self.write("(");
        self.emit_function_parameters_js(&func.parameters.nodes);
        self.write(") ");

        // Emit body - tsc never collapses multi-line function expression bodies
        // to single lines. Single-line formatting is preserved via emit_block
        // when the source was originally single-line.
        self.emit(func.body);
    }

    /// Check if a statement is a simple return statement (for single-line emission).
    /// A return is "simple" if it has an expression AND the expression doesn't
    /// contain multi-line constructs (like object literals with multiple properties).
    pub(super) fn is_simple_return_statement(&self, stmt_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(stmt_idx) else {
            return false;
        };

        if node.kind != syntax_kind_ext::RETURN_STATEMENT {
            return false;
        }
        if let Some(ret) = self.arena.get_return_statement(node) {
            if ret.expression.is_none() {
                return false;
            }
            // Check if the return expression is multi-line in the source
            if let Some(expr_node) = self.arena.get(ret.expression) {
                // Object literals with multiple properties are multi-line
                if expr_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                    if let Some(obj) = self.arena.get_literal_expr(expr_node) {
                        if obj.elements.nodes.len() > 1 && !self.is_single_line(expr_node) {
                            return false;
                        }
                    }
                }
                // Also check source text - if the expression spans multiple lines, not simple
                if !self.is_single_line(expr_node) {
                    // For non-object expressions that span multiple lines
                    if expr_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                        return false;
                    }
                }
            }
            return true;
        }
        false
    }

    /// Emit a block on a single line: { return expr; }
    pub(super) fn emit_single_line_block(&mut self, block_idx: NodeIndex) {
        let Some(block_node) = self.arena.get(block_idx) else {
            return;
        };
        let Some(block) = self.arena.get_block(block_node) else {
            return;
        };

        self.write("{ ");
        for (i, &stmt_idx) in block.statements.nodes.iter().enumerate() {
            if i > 0 {
                self.write(" ");
            }
            self.emit(stmt_idx);
        }
        self.write(" }");
    }

    pub(super) fn emit_block_with_param_prologue(
        &mut self,
        block_idx: NodeIndex,
        transforms: &ParamTransformPlan,
    ) {
        let Some(block_node) = self.arena.get(block_idx) else {
            return;
        };
        let Some(block) = self.arena.get_block(block_node) else {
            return;
        };

        self.write("{");
        self.write_line();
        self.increase_indent();
        self.emit_param_prologue(transforms);

        for &stmt_idx in &block.statements.nodes {
            let before_len = self.writer.len();
            self.emit(stmt_idx);
            if self.writer.len() > before_len {
                self.write_line();
            }
        }

        self.decrease_indent();
        self.write("}");
        self.emit_trailing_comments(block_node.end);
    }

    /// Emit function parameters for JavaScript (no types)
    pub(super) fn emit_function_parameters_js(&mut self, params: &[NodeIndex]) {
        let mut first = true;
        for &param_idx in params {
            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_parameter(param_node)
            {
                // Skip `this` parameter - it's TypeScript-only and erased in JS emit.
                // The parser may represent `this` as either a ThisKeyword token
                // or as an Identifier with text "this".
                if let Some(name_node) = self.arena.get(param.name) {
                    if name_node.kind == crate::scanner::SyntaxKind::ThisKeyword as u16 {
                        continue;
                    }
                    if name_node.kind == crate::scanner::SyntaxKind::Identifier as u16 {
                        if let Some(text) = self.source_text {
                            let name_text = crate::printer::safe_slice::slice(
                                text,
                                name_node.pos as usize,
                                name_node.end as usize,
                            )
                            .trim();
                            if name_text == "this" {
                                continue;
                            }
                        }
                    }
                }

                if !first {
                    self.write(", ");
                }
                first = false;

                if param.dot_dot_dot_token {
                    self.write("...");
                }
                self.emit(param.name);
                // Skip type annotations and defaults for JS emit
                if !param.initializer.is_none() {
                    self.write(" = ");
                    self.emit(param.initializer);
                }
            }
        }
    }

    pub(super) fn emit_parameter(&mut self, node: &Node) {
        let Some(param) = self.arena.get_parameter(node) else {
            return;
        };

        if param.dot_dot_dot_token {
            self.write("...");
        }

        self.emit(param.name);

        if param.question_token {
            self.write("?");
        }

        if !param.type_annotation.is_none() {
            self.write(": ");
            self.emit(param.type_annotation);
        }

        if !param.initializer.is_none() {
            self.write(" = ");
            self.emit_expression(param.initializer);
        }
    }
}
