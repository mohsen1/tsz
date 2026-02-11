use super::{ParamTransformPlan, Printer};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::syntax::transform_utils::{contains_arguments_reference, contains_this_reference};

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
    fn emit_arrow_function_native(&mut self, func: &tsz_parser::parser::node::FunctionData) {
        if func.is_async {
            self.write("async ");
        }

        // TypeScript preserves parentheses from source:
        // - If source had `(x) => x`, emit `(x) => x` even though x is simple
        // - If source had `x => x`, emit `x => x`
        // - If source had `(x: string) => x`, emit `(x) => x` (parens preserved)
        let source_had_parens = self.source_has_arrow_function_parens(&func.parameters.nodes);
        let is_simple = self.is_simple_single_parameter(&func.parameters.nodes);
        let needs_parens = source_had_parens || !is_simple;

        if needs_parens {
            self.write("(");
        }
        self.emit_function_parameters_js(&func.parameters.nodes);
        if needs_parens {
            self.write(")");
        }

        // Skip return type for JavaScript

        self.write(" => ");

        // Body
        self.emit(func.body);
    }

    /// Check if the source had parentheses around the parameters
    fn source_has_arrow_function_parens(&self, params: &[NodeIndex]) -> bool {
        if params.is_empty() {
            // Empty param list always has parens: () => x
            return true;
        }

        // FIRST: Check source text if available (most reliable)
        // Scan forward from the last parameter to find ')' before '=>'
        if let Some(source) = self.source_text {
            if let Some(last_param) = params.last() {
                if let Some(param_node) = self.arena.get(*last_param) {
                    let end_pos = param_node.end as usize;

                    // Ensure we don't go out of bounds
                    if end_pos < source.len() {
                        // Scan forward from the end of the last parameter
                        // Look for ')' (had parens) or '=' from '=>' (no parens)
                        let suffix = &source[end_pos..];
                        for ch in suffix.chars() {
                            match ch {
                                // Whitespace - skip
                                ' ' | '\t' | '\n' | '\r' => continue,
                                // Found closing paren - had parens
                                ')' => return true,
                                // Found '=' from '=>' - no parens
                                '=' => return false,
                                // Any other character (colon for type, etc.) - keep scanning
                                _ => continue,
                            }
                        }
                    }
                }
            }
        }

        // FALLBACK: If source text check failed or no source available,
        // check if parameter has modifiers or type annotations.
        // Parameters with these MUST have had parens in valid TS.
        if let Some(first_param) = params.first() {
            if let Some(param_node) = self.arena.get(*first_param) {
                if let Some(param) = self.arena.get_parameter(param_node) {
                    // Check for modifiers (public, private, protected, readonly, etc.)
                    if let Some(mods) = &param.modifiers {
                        if !mods.nodes.is_empty() {
                            return true;
                        }
                    }
                    // Check for type annotation
                    if !param.type_annotation.is_none() {
                        return true;
                    }
                }
            }
        }

        // Default to parens if we couldn't determine
        true
    }

    /// Check if parameters are a simple single parameter that doesn't need parens
    /// For JS emit, type annotations don't matter since they're always stripped.
    fn is_simple_single_parameter(&self, params: &[NodeIndex]) -> bool {
        // Must have exactly one parameter
        if params.len() != 1 {
            return false;
        }

        let param_idx = params[0];
        let Some(param_node) = self.arena.get(param_idx) else {
            return false;
        };
        let Some(param) = self.arena.get_parameter(param_node) else {
            return false;
        };

        // Must not be a rest parameter
        if param.dot_dot_dot_token {
            return false;
        }

        // Type annotations are irrelevant for JS emit - they're always stripped

        // Must have no initializer
        if !param.initializer.is_none() {
            return false;
        }

        // The name must be a simple identifier (not a destructuring pattern)
        if param.name.is_none() {
            return false;
        }

        let Some(name_node) = self.arena.get(param.name) else {
            return false;
        };

        // Check if it's an identifier (not ArrayBindingPattern or ObjectBindingPattern)
        name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
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
                    if name_node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16 {
                        continue;
                    }
                    if name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
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

        // Emit trailing comments after the last parameter (e.g., `...rest /* comment */`).
        // We use the name node's end position, not the parameter node's end, because
        // the parameter node's end may extend past the comment (including trivia).
        if let Some(&last_param) = params.last() {
            if let Some(last_node) = self.arena.get(last_param)
                && let Some(last_param) = self.arena.get_parameter(last_node)
            {
                // Use the end of the last thing we emitted (initializer or name)
                let scan_pos = if !last_param.initializer.is_none() {
                    if let Some(init_node) = self.arena.get(last_param.initializer) {
                        init_node.end
                    } else {
                        last_node.end
                    }
                } else if let Some(name_node) = self.arena.get(last_param.name) {
                    name_node.end
                } else {
                    last_node.end
                };
                self.emit_trailing_comments(scan_pos);
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
