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

        // Parser recovery parity: malformed return type like `(a): => {}` should
        // preserve recovered shape instead of applying arrow lowering.
        if self.is_recovery_arrow_missing_return_type(node, func) {
            self.write("(");
            self.emit_function_parameters_js(&func.parameters.nodes);
            self.write(")");
            if let Some(body_node) = self.arena.get(func.body)
                && body_node.kind == syntax_kind_ext::BLOCK
            {
                self.write(";");
                self.write_line();
                let prev_emitting_function_body_block = self.emitting_function_body_block;
                self.emitting_function_body_block = true;
                self.emit(func.body);
                self.emitting_function_body_block = prev_emitting_function_body_block;
                self.write_line();
            }
            return;
        }

        if self.ctx.target_es5 {
            let captures_this = contains_this_reference(self.arena, _idx);
            let captures_arguments = contains_arguments_reference(self.arena, _idx);
            self.emit_arrow_function_es5(node, func, captures_this, captures_arguments, &None);
            return;
        }

        self.emit_arrow_function_native(func);
    }

    fn is_recovery_arrow_missing_return_type(
        &self,
        node: &Node,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> bool {
        if let Some(text) = self.source_text {
            let start = node.pos as usize;
            let end = node.end as usize;
            if start < end && end <= text.len() {
                let slice = &text[start..end];
                if slice.contains("): =>") || slice.contains("):=>") {
                    return true;
                }
            }
        }

        if func.type_annotation.is_none() {
            return false;
        }

        let Some(type_node) = self.arena.get(func.type_annotation) else {
            return false;
        };

        // Parser recovery can surface malformed return types as bare identifier
        // placeholders; treat them as invalid arrow return type annotations.
        type_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
    }

    /// Emit native ES6+ arrow function syntax
    #[tracing::instrument(level = "trace", skip(self, func), fields(param_count = func.parameters.nodes.len()))]
    fn emit_arrow_function_native(&mut self, func: &tsz_parser::parser::node::FunctionData) {
        // For ES2015/ES2016, lower async arrows: () => __awaiter(this, void 0, void 0, function* () { ... })
        if func.is_async && self.ctx.needs_async_lowering {
            self.emit_arrow_function_async_lowered(func);
            return;
        }

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

        tracing::trace!(
            source_had_parens,
            is_simple,
            needs_parens,
            "Arrow function parenthesis decision"
        );

        if needs_parens {
            self.write("(");
        }
        self.emit_function_parameters_js(&func.parameters.nodes);
        if needs_parens {
            self.write(")");
        }

        // Skip return type for JavaScript

        self.write(" => ");

        // Body - wrap in parens if it resolves to an object literal
        // (e.g., `a => <any>{}` → `a => ({})` to avoid block ambiguity)
        let body_is_block = self
            .arena
            .get(func.body)
            .is_some_and(|n| n.kind == syntax_kind_ext::BLOCK);
        if !body_is_block && self.concise_body_needs_parens(func.body) {
            self.write("(");
            self.emit(func.body);
            self.write(")");
        } else {
            let prev_emitting_function_body_block = self.emitting_function_body_block;
            self.emitting_function_body_block = true;
            self.emit(func.body);
            self.emitting_function_body_block = prev_emitting_function_body_block;
        }
    }

    /// Emit an async arrow function lowered for ES2015/ES2016 targets.
    /// Transforms: `async (p) => body` → `(p) => __awaiter(this, void 0, void 0, function* () { body })`
    fn emit_arrow_function_async_lowered(&mut self, func: &tsz_parser::parser::node::FunctionData) {
        // Don't emit `async` - it's lowered away

        // Parameters (same paren logic as native)
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

        // Arrow functions don't have their own `this`, so TSC passes `void 0`
        // as the this-arg to __awaiter. The arrow captures `this` lexically from
        // the enclosing scope, but __awaiter doesn't need it.

        let body_node = self.arena.get(func.body);
        let is_block = body_node.is_some_and(|n| n.kind == syntax_kind_ext::BLOCK);

        // Check if body is empty and single-line in source for compact formatting
        let body_is_empty_single_line = is_block
            && self
                .arena
                .get(func.body)
                .and_then(|n| {
                    let block = self.arena.get_block(n)?;
                    if block.statements.nodes.is_empty() {
                        Some(self.is_single_line(n))
                    } else {
                        None
                    }
                })
                .unwrap_or(false);

        if body_is_empty_single_line {
            self.write(" => __awaiter(void 0, void 0, void 0, function* () { })");
            return;
        }

        self.write(" => __awaiter(void 0, void 0, void 0, function* () {");
        self.write_line();
        self.increase_indent();

        // Emit body with await→yield substitution
        self.ctx.emit_await_as_yield = true;

        if is_block {
            // Block body: emit statements directly
            if let Some(body_node) = self.arena.get(func.body)
                && let Some(block) = self.arena.get_block(body_node)
            {
                for &stmt in &block.statements.nodes {
                    self.emit(stmt);
                    self.write_line();
                }
            }
        } else {
            // Concise expression body: wrap in return
            self.write("return ");
            self.emit_expression(func.body);
            self.write(";");
            self.write_line();
        }

        self.ctx.emit_await_as_yield = false;

        self.decrease_indent();
        self.write("})");
    }

    /// Check if the source had parentheses around the parameters
    #[tracing::instrument(level = "trace", skip(self, params), fields(param_count = params.len()))]
    fn source_has_arrow_function_parens(&self, params: &[NodeIndex]) -> bool {
        if params.is_empty() {
            // Empty param list always has parens: () => x
            tracing::trace!("Empty param list, returning true");
            return true;
        }

        // FIRST: Check source text if available (most reliable)
        // Scan forward from the last parameter NAME to find ')' before '=>'
        // Important: Use the parameter NAME's end, not the whole parameter's end
        // (which includes type annotations that we want to detect)
        if let Some(source) = self.source_text
            && let Some(last_param) = params.last()
            && let Some(param_node) = self.arena.get(*last_param)
            && let Some(param_data) = self.arena.get_parameter(param_node)
        {
            // Get the parameter NAME's end position, not the whole parameter
            if let Some(name_node) = self.arena.get(param_data.name) {
                let end_pos = name_node.end as usize;
                tracing::trace!(
                    end_pos,
                    source_len = source.len(),
                    "Scanning source from param NAME end"
                );

                // Ensure we don't go out of bounds
                if end_pos < source.len() {
                    // Scan forward from the end of the parameter NAME
                    // Look for ')' (had parens) or '=' from '=>' (no parens)
                    let suffix = &source[end_pos..];
                    let preview = &suffix[..std::cmp::min(30, suffix.len())];
                    tracing::trace!(preview, "Source suffix preview");
                    for ch in suffix.chars() {
                        match ch {
                            // Whitespace - skip
                            // Found closing paren - had parens
                            ')' => {
                                tracing::trace!("Found ')' in source, returning true");
                                return true;
                            }
                            // Found '=' from '=>' - no parens (simple param without parens)
                            '=' => {
                                tracing::trace!("Found '=' in source, returning false");
                                return false;
                            }
                            // Colon indicates type annotation, keep scanning
                            ':' => {
                                tracing::trace!("Found ':' (type annotation), continuing scan");
                                continue;
                            }
                            // Any other character - keep scanning
                            _ => continue,
                        }
                    }
                }
            }
        }

        // FALLBACK: If source text check failed or no source available,
        // check if parameter has modifiers or type annotations.
        // Parameters with these MUST have had parens in valid TS.
        tracing::trace!("Entering fallback check for modifiers/type annotations");
        if let Some(first_param) = params.first()
            && let Some(param_node) = self.arena.get(*first_param)
            && let Some(param) = self.arena.get_parameter(param_node)
        {
            // Check for modifiers (public, private, protected, readonly, etc.)
            if let Some(mods) = &param.modifiers {
                let mod_count = mods.nodes.len();
                tracing::trace!(mod_count, "Found modifiers");
                if !mods.nodes.is_empty() {
                    tracing::trace!("Has modifiers, returning true");
                    return true;
                }
            }
            // Check for type annotation
            let has_type = !param.type_annotation.is_none();
            tracing::trace!(has_type, "Type annotation check");
            if has_type {
                tracing::trace!("Has type annotation, returning true");
                return true;
            }
        }

        // Default to parens if we couldn't determine
        tracing::trace!("Fallback: returning true (conservative default)");
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

        if func.is_async && self.ctx.needs_async_lowering && !func.asterisk_token {
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

        // Push temp scope and block scope for function body - each function gets fresh variables.
        let prev_emitting_function_body_block = self.emitting_function_body_block;
        self.emitting_function_body_block = true;
        self.ctx.block_scope_state.enter_scope();
        self.push_temp_scope();
        self.prepare_logical_assignment_value_temps(func.body);
        let prev_in_generator = self.ctx.flags.in_generator;
        self.ctx.flags.in_generator = func.asterisk_token;
        self.emit(func.body);
        self.ctx.flags.in_generator = prev_in_generator;
        self.pop_temp_scope();
        self.ctx.block_scope_state.exit_scope();
        self.emitting_function_body_block = prev_emitting_function_body_block;
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
                if expr_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    && let Some(obj) = self.arena.get_literal_expr(expr_node)
                    && obj.elements.nodes.len() > 1
                    && !self.is_single_line(expr_node)
                {
                    return false;
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
                    if name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                        && let Some(text) = self.source_text
                    {
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

                if !first {
                    self.write(", ");
                }
                first = false;

                if param.dot_dot_dot_token {
                    self.write("...");
                }
                self.emit_parameter_name_js(param.name);
                // Skip type annotations and defaults for JS emit
                if !param.initializer.is_none() {
                    self.write(" = ");
                    self.emit(param.initializer);
                }
            }
        }

        // NOTE: Do NOT emit trailing comments here. Comments after the last
        // parameter (e.g., `p3:any // OK`) appear on the same source line but
        // logically follow the closing `)`. Since type annotations are erased,
        // scanning from name_node.end would place these comments INSIDE the
        // parameter list. The caller (statement-level comment emission) handles
        // trailing comments after the whole function declaration.
    }

    pub(super) fn emit_parameter(&mut self, node: &Node) {
        let Some(param) = self.arena.get_parameter(node) else {
            return;
        };

        if param.dot_dot_dot_token {
            self.write("...");
        }

        self.emit_parameter_name_js(param.name);

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

    fn emit_parameter_name_js(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };
        let kind = name_node.kind;
        let is_normal_binding_name = kind == tsz_scanner::SyntaxKind::Identifier as u16
            || kind == tsz_scanner::SyntaxKind::ThisKeyword as u16
            || kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            || kind == syntax_kind_ext::ARRAY_BINDING_PATTERN;

        if is_normal_binding_name {
            self.emit(name_idx);
            return;
        }

        // Recovery path: malformed parameter names like `yield`/`await`
        // can be parsed as expressions. Preserve original text for JS parity.
        if let Some(source) = self.source_text {
            let text = crate::printer::safe_slice::slice(
                source,
                name_node.pos as usize,
                name_node.end as usize,
            )
            .trim();
            if !text.is_empty() {
                self.write(text);
                return;
            }
        }

        self.emit(name_idx);
    }
}
