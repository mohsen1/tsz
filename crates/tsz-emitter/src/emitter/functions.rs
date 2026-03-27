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
                self.function_scope_depth += 1;
                self.emit(func.body);
                self.function_scope_depth -= 1;
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
        let needs_parens = source_had_parens || !is_simple || func.is_async;

        tracing::trace!(
            source_had_parens,
            is_simple,
            needs_parens,
            "Arrow function parenthesis decision"
        );

        if needs_parens {
            // Emit any comments that appear before the opening paren in the source.
            // e.g., `f: /**own f*/ (a) => 0` → comment should be before `(`.
            if let Some(&first_param_idx) = func.parameters.nodes.first()
                && let Some(first_param) = self.arena.get(first_param_idx)
                && let Some(source) = self.source_text
            {
                let bytes = source.as_bytes();
                let mut pos = first_param.pos as usize;
                // Scan backward from first parameter to find `(`
                while pos > 0 {
                    pos -= 1;
                    if bytes[pos] == b'(' {
                        break;
                    }
                }
                if bytes.get(pos) == Some(&b'(') {
                    // Emit comments that are before the `(` position
                    if self.has_pending_comment_before(pos as u32) {
                        self.emit_comments_before_pos(pos as u32);
                        self.pending_block_comment_space = false;
                        self.write(" ");
                    }
                }
            }
            self.write("(");
        }
        self.emit_function_parameters_js(&func.parameters.nodes);
        if needs_parens {
            // Map closing `)` — scan backward from body start since parser
            // may include `)` in the parameter node's range.
            if let Some(body_node) = self.arena.get(func.body) {
                let search_start = func
                    .parameters
                    .nodes
                    .first()
                    .and_then(|&idx| self.arena.get(idx))
                    .map_or(0, |n| n.pos);
                self.map_closing_paren_backward(search_start, body_node.pos);
            }
            self.write(")");
        }

        // Map `=>` arrow to source position (split space from token to get correct mapping column)
        self.write_space();
        {
            let search_start = func
                .parameters
                .nodes
                .last()
                .and_then(|&idx| self.arena.get(idx))
                .map_or(0, |n| n.end);
            let search_end = self.arena.get(func.body).map_or(u32::MAX, |n| n.pos);
            self.map_token_after(search_start, search_end, b'=');
        }
        self.write("=> ");

        // Body - wrap in parens if it resolves to an object literal
        // (e.g., `a => <any>{}` → `a => ({})` to avoid block ambiguity)
        let body_is_block = self
            .arena
            .get(func.body)
            .is_some_and(|n| n.kind == syntax_kind_ext::BLOCK);

        // Arrow functions introduce their own temp scope. Without this, hoisted temps
        // created by the enclosing scope can be spuriously injected into single-line
        // arrow bodies during block emission.
        self.push_temp_scope();

        // If we have pending object rest params and a concise body, convert to block body
        if !body_is_block && !self.pending_object_rest_params.is_empty() {
            let rest_params: Vec<(String, NodeIndex)> =
                std::mem::take(&mut self.pending_object_rest_params);
            self.write("{");
            self.write_line();
            self.increase_indent();
            // Emit the rest preamble
            for (temp_name, pattern_idx) in &rest_params {
                self.write("var ");
                self.emit_object_rest_var_decl(*pattern_idx, NodeIndex::NONE, Some(temp_name));
                self.write(";");
                self.write_line();
            }
            // Emit the concise body as a return statement
            self.write("return ");
            self.function_scope_depth += 1;
            self.emit(func.body);
            self.function_scope_depth -= 1;
            self.write(";");
            self.write_line();
            self.decrease_indent();
            self.write("}");
        } else if !body_is_block && self.concise_body_needs_parens(func.body) {
            // Emit comments between => and the body expression (e.g. triple-slash comments)
            if let Some(body_node) = self.arena.get(func.body) {
                self.emit_comments_before_pos(body_node.pos);
            }
            self.write("(");
            self.emit(func.body);
            self.write(")");
        } else {
            // Emit comments between => and the body expression (e.g. triple-slash comments)
            // tsc preserves these and places the body on a new line when comments exist.
            if !body_is_block && let Some(body_node) = self.arena.get(func.body) {
                self.emit_comments_before_pos(body_node.pos);
            }
            let prev_emitting_function_body_block = self.emitting_function_body_block;
            self.emitting_function_body_block = true;
            self.function_scope_depth += 1;
            let prev_declared = std::mem::take(&mut self.declared_namespace_names);
            self.emit(func.body);
            self.declared_namespace_names = prev_declared;
            self.function_scope_depth -= 1;
            self.emitting_function_body_block = prev_emitting_function_body_block;
        }

        self.pop_temp_scope();
    }

    /// Emit an async arrow function lowered for ES2015/ES2016 targets.
    /// Transforms: `async (p) => body` → `(p) => __awaiter(this, void 0, void 0, function* () { body })`
    fn emit_arrow_function_async_lowered(&mut self, func: &tsz_parser::parser::node::FunctionData) {
        // Don't emit `async` - it's lowered away

        // For arrow functions on ES2015+, TSC passes `this` to __awaiter when
        // the arrow is inside a function/method scope (where `this` is bound).
        // At top level (file scope), use `void 0` since there's no meaningful `this`.
        let this_arg = if self.function_scope_depth > 0 {
            "this"
        } else {
            "void 0"
        };

        let await_param_recovery = func
            .parameters
            .nodes
            .iter()
            .copied()
            .any(|param_idx| self.param_initializer_has_top_level_await(param_idx))
            && crate::transforms::emit_utils::block_is_empty(self.arena, func.body)
            && crate::transforms::emit_utils::first_await_default_param_name(
                self.arena,
                &func.parameters.nodes,
            )
            .is_some();

        if await_param_recovery {
            self.emit_async_arrow_await_param_recovery(func, this_arg);
            return;
        }

        // TSC always wraps parameters in parens when lowering async arrows,
        // even if the original source had `async x => ...` without parens.
        self.write("(");
        self.emit_function_parameters_js(&func.parameters.nodes);
        self.write(")");

        // Check if the body references `arguments`. If so, we must capture it
        // before entering the generator: `() => { var arguments_1 = arguments; return __awaiter(...); }`
        // However, if we're already inside a generator body that has captured arguments
        // (rewrite_arguments_to_arguments_1 is true), don't create another capture -
        // the references are already being rewritten to `arguments_1`.
        let captures_arguments = !self.ctx.rewrite_arguments_to_arguments_1
            && contains_arguments_reference(self.arena, func.body);

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

        // Check if the entire body is single-line in source
        let body_is_single_line = is_block
            && self
                .arena
                .get(func.body)
                .map(|n| self.is_single_line(n))
                .unwrap_or(false);

        if body_is_empty_single_line {
            self.write(" => ");
            self.write_helper("__awaiter");
            self.write("(");
            self.write(this_arg);
            self.write(", void 0, void 0, function* () { })");
            return;
        }

        // When capturing arguments, always use block form:
        // `() => { var arguments_1 = arguments; return __awaiter(..., function* () { ... arguments_1 ... }); }`
        if captures_arguments {
            self.write(" => {");
            self.write_line();
            self.increase_indent();
            self.write("var arguments_1 = arguments;");
            self.write_line();
            self.write("return ");
            self.write_helper("__awaiter");
            self.write("(");
            self.write(this_arg);
            self.write(", void 0, void 0, function* () {");

            let saved_yield = self.ctx.emit_await_as_yield;
            let saved_args = self.ctx.rewrite_arguments_to_arguments_1;
            self.ctx.emit_await_as_yield = true;
            self.ctx.rewrite_arguments_to_arguments_1 = true;

            if is_block {
                if body_is_single_line {
                    if let Some(body_node) = self.arena.get(func.body)
                        && let Some(block) = self.arena.get_block(body_node)
                    {
                        for &stmt in &block.statements.nodes {
                            self.write(" ");
                            self.emit(stmt);
                        }
                    }
                    self.write(" })");
                } else {
                    self.write_line();
                    self.increase_indent();
                    if let Some(body_node) = self.arena.get(func.body)
                        && let Some(block) = self.arena.get_block(body_node)
                    {
                        for &stmt in &block.statements.nodes {
                            self.emit(stmt);
                            self.write_line();
                        }
                    }
                    self.decrease_indent();
                    self.write("})");
                }
            } else {
                self.write(" return ");
                self.emit_expression(func.body);
                self.write("; })");
            }

            self.ctx.emit_await_as_yield = saved_yield;
            self.ctx.rewrite_arguments_to_arguments_1 = saved_args;

            self.write(";");
            self.write_line();
            self.decrease_indent();
            self.write("}");
            return;
        }

        if body_is_single_line {
            // Single-line body: emit inline like TSC
            // e.g., () => __awaiter(this, void 0, void 0, function* () { return yield this; })
            self.write(" => ");
            self.write_helper("__awaiter");
            self.write("(");
            self.write(this_arg);
            self.write(", void 0, void 0, function* () {");

            self.ctx.emit_await_as_yield = true;
            if let Some(body_node) = self.arena.get(func.body)
                && let Some(block) = self.arena.get_block(body_node)
            {
                for &stmt in &block.statements.nodes {
                    self.write(" ");
                    self.emit(stmt);
                }
            }
            self.ctx.emit_await_as_yield = false;
            self.write(" })");
            return;
        }

        if !is_block {
            // Concise expression body: emit single-line
            // e.g., () => __awaiter(this, void 0, void 0, function* () { return yield expr; })
            self.write(" => ");
            self.write_helper("__awaiter");
            self.write("(");
            self.write(this_arg);
            self.write(", void 0, void 0, function* () { ");
            self.ctx.emit_await_as_yield = true;
            self.write("return ");
            self.emit_expression(func.body);
            self.write(";");
            self.ctx.emit_await_as_yield = false;
            self.write(" })");
            return;
        }

        self.write(" => ");
        self.write_helper("__awaiter");
        self.write("(");
        self.write(this_arg);
        self.write(", void 0, void 0, function* () {");
        self.write_line();
        self.increase_indent();

        // Emit body with await→yield substitution
        self.ctx.emit_await_as_yield = true;

        // Block body: emit statements directly
        if let Some(body_node) = self.arena.get(func.body)
            && let Some(block) = self.arena.get_block(body_node)
        {
            for &stmt in &block.statements.nodes {
                self.emit(stmt);
                self.write_line();
            }
        }

        self.ctx.emit_await_as_yield = false;

        self.decrease_indent();
        self.write("})");
    }

    fn emit_async_arrow_await_param_recovery(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
        this_arg: &str,
    ) {
        let Some(param_name) = crate::transforms::emit_utils::first_await_default_param_name(
            self.arena,
            &func.parameters.nodes,
        ) else {
            return;
        };
        let args_name = self.make_unique_name_from_base("args");

        self.write("(...");
        self.write(&args_name);
        self.write(") => ");
        self.write_helper("__awaiter");
        self.write("(");
        self.write(this_arg);
        self.write(", [...");
        self.write(&args_name);
        self.write("], void 0, function* (");
        self.write(&param_name);
        self.write(" = yield ");
        self.write(") {");
        self.write_line();
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
            let has_type = param.type_annotation.is_some();
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
        if param.initializer.is_some() {
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

        // Consume the paren flag: when set, this function expression is the direct
        // callee of an expression-statement call and should self-parenthesize to
        // produce TSC-style `(function(){})()` instead of `(function(){}())`.
        let self_paren = self.ctx.flags.paren_leftmost_function_or_object;
        if self_paren {
            self.ctx.flags.paren_leftmost_function_or_object = false;
            self.write("(");
        }

        if func.is_async && self.ctx.needs_async_lowering && !func.asterisk_token {
            let func_name = if func.name.is_some() {
                self.get_identifier_text_idx(func.name)
            } else {
                String::new()
            };
            self.emit_async_function_es5(func, &func_name, "this");
            if self_paren {
                self.write(")");
            }
            return;
        }

        // Async generator: async function* f() → function f() { return __asyncGenerator(...) }
        if func.is_async && self.ctx.needs_async_lowering && func.asterisk_token {
            let func_name = if func.name.is_some() {
                self.get_identifier_text_idx(func.name)
            } else {
                String::new()
            };
            self.emit_async_generator_lowered(func, &func_name);
            if self_paren {
                self.write(")");
            }
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
        if func.name.is_some() {
            self.write_space();
            self.emit_decl_name(func.name);
        } else {
            // Space before ( only for anonymous functions: function (x) vs function name(x)
            self.write(" ");
        }

        // Parameters (without types for JavaScript)
        // Map opening `(` to its source position
        let open_paren_pos = {
            let search_start = if func.name.is_some() {
                self.arena.get(func.name).map_or(node.pos, |n| n.end)
            } else {
                node.pos
            };
            self.map_token_after(search_start, node.end, b'(');
            self.pending_source_pos
                .map(|source_pos| source_pos.pos)
                .unwrap_or(search_start)
        };
        self.write("(");
        let search_start = func
            .parameters
            .nodes
            .first()
            .and_then(|&idx| self.arena.get(idx))
            .map_or(node.pos, |n| n.pos);
        let search_end = if func.body.is_some() {
            self.arena.get(func.body).map_or(node.end, |n| n.pos)
        } else {
            node.end
        };
        // Increment function_scope_depth BEFORE parameters so that async arrow
        // functions in parameter defaults see the correct scope depth (enables
        // `__awaiter(this, ...)` instead of `__awaiter(void 0, ...)`)
        self.function_scope_depth += 1;
        self.emit_function_parameters_with_trailing_comments(
            &func.parameters.nodes,
            open_paren_pos,
            search_start,
            search_end,
        );
        self.write(") ");

        // Emit body - tsc never collapses multi-line function expression bodies
        // to single lines. Single-line formatting is preserved via emit_block
        // when the source was originally single-line.

        // Push temp scope and block scope for function body - each function gets fresh variables.
        let prev_emitting_function_body_block = self.emitting_function_body_block;
        self.emitting_function_body_block = true;
        self.ctx.block_scope_state.enter_scope();
        self.push_temp_scope();
        // Save/restore declared_namespace_names so enum/namespace names from the
        // outer scope don't suppress declarations inside this function, and names
        // declared inside don't leak to sibling functions at the outer scope.
        let prev_declared = std::mem::take(&mut self.declared_namespace_names);
        self.prepare_logical_assignment_value_temps(func.body);
        let prev_in_generator = self.ctx.flags.in_generator;
        self.ctx.flags.in_generator = func.asterisk_token;
        // Regular functions have their own `arguments`, so turn off the rewrite flag
        let prev_rewrite_args = self.ctx.rewrite_arguments_to_arguments_1;
        self.ctx.rewrite_arguments_to_arguments_1 = false;
        self.emit(func.body);
        self.ctx.rewrite_arguments_to_arguments_1 = prev_rewrite_args;
        self.ctx.flags.in_generator = prev_in_generator;
        self.declared_namespace_names = prev_declared;
        self.pop_temp_scope();
        self.ctx.block_scope_state.exit_scope();
        self.function_scope_depth -= 1;
        self.emitting_function_body_block = prev_emitting_function_body_block;
        if self_paren {
            self.write(")");
        }
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

        if block.statements.nodes.is_empty() {
            self.write("{ }");
            return;
        }

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
        // Check if any parameter needs ES2018 object rest lowering
        let needs_rest_lowering = self.ctx.needs_es2018_lowering
            && !self.ctx.target_es5
            && self.any_param_has_object_rest(params);

        // Clear any previous pending rest params
        self.pending_object_rest_params.clear();

        let mut first = true;
        for &param_idx in params {
            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_parameter(param_node)
            {
                // Skip parameters with no name (parser error recovery artifacts).
                // e.g., `function f(a,¬)` should emit `function f(a)`.
                // But preserve rest parameters (`...`) even with missing names,
                // matching tsc behavior: `function sum(...) { }`.
                if param.name.is_none() && !param.dot_dot_dot_token {
                    continue;
                }
                // Skip parameters where the name is an empty/missing identifier
                // (parser error recovery for invalid characters like ¬).
                // But preserve rest parameters with empty names - tsc emits
                // the `...` even when the parameter name is missing.
                if !param.dot_dot_dot_token
                    && let Some(name_node) = self.arena.get(param.name)
                    && name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                    && let Some(ident) = self.arena.get_identifier(name_node)
                    && ident.escaped_text.is_empty()
                {
                    continue;
                }

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
                        let name_text = crate::safe_slice::slice(
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

                // Emit leading comments before the parameter (e.g., inline JSDoc
                // comments like `/** comment */ a`). tsc preserves these in JS output.
                self.emit_comments_before_pos(param_node.pos);

                // ES2018 object rest lowering: replace destructuring param with a temp
                if needs_rest_lowering && self.param_has_object_rest(param_idx) {
                    let temp = self.get_temp_var_name();
                    self.write(&temp);
                    // Skip type annotation comments
                    if param.type_annotation.is_some()
                        && let Some(type_node) = self.arena.get(param.type_annotation)
                    {
                        self.skip_comments_in_range(type_node.pos, type_node.end);
                    }
                    // Don't emit default value here — it'll be in the body as `if (_a === void 0)`
                    // Skip initializer comments too
                    self.pending_object_rest_params.push((temp, param.name));
                    continue;
                }

                if param.dot_dot_dot_token {
                    self.write("...");
                    // Emit inline comments between `...` and parameter name
                    // (e.g., `.../*3*/y` → `... /*3*/y`)
                    if let Some(name_node) = self.arena.get(param.name)
                        && self.has_pending_comment_before(name_node.pos)
                    {
                        self.write(" ");
                        self.emit_comments_before_pos(name_node.pos);
                        self.pending_block_comment_space = false;
                    }
                }
                self.emit_parameter_name_js(param.name);
                // Skip type annotations — consume comments inside the erased range,
                // but preserve trailing comments between the type and delimiter.
                // e.g., `a: any/*2*/,` → `a /*2*/,`
                //
                // Note: in tsz's parser, type_node.end extends past trailing
                // trivia into the delimiter (`,` or `)`), so we scan from
                // type_node.pos to find the actual delimiter position.
                if param.type_annotation.is_some()
                    && let Some(type_node) = self.arena.get(param.type_annotation)
                {
                    // Find the delimiter (`,` or `)`) within the type annotation range.
                    // The type node's range includes the delimiter, so scan from the start
                    // to find it, properly handling nesting and string/comment literals.
                    let delimiter_pos = if let Some(text) = self.source_text {
                        let bytes = text.as_bytes();
                        let mut scan = type_node.pos as usize;
                        let limit = type_node.end as usize;
                        let mut depth = 0i32;
                        let mut found = limit;
                        while scan < limit {
                            match bytes[scan] {
                                b',' | b')' if depth == 0 => {
                                    found = scan;
                                    break;
                                }
                                b'(' | b'[' | b'{' | b'<' => {
                                    depth += 1;
                                    scan += 1;
                                }
                                b')' | b']' | b'}' | b'>' => {
                                    depth -= 1;
                                    scan += 1;
                                }
                                b'/' if scan + 1 < limit && bytes[scan + 1] == b'*' => {
                                    scan += 2;
                                    while scan + 1 < limit
                                        && !(bytes[scan] == b'*' && bytes[scan + 1] == b'/')
                                    {
                                        scan += 1;
                                    }
                                    if scan + 1 < limit {
                                        scan += 2;
                                    }
                                }
                                b'/' if scan + 1 < limit && bytes[scan + 1] == b'/' => {
                                    while scan < limit
                                        && bytes[scan] != b'\n'
                                        && bytes[scan] != b'\r'
                                    {
                                        scan += 1;
                                    }
                                }
                                b'\'' | b'"' | b'`' => {
                                    let q = bytes[scan];
                                    scan += 1;
                                    while scan < limit && bytes[scan] != q {
                                        if bytes[scan] == b'\\' {
                                            scan += 1;
                                        }
                                        scan += 1;
                                    }
                                    if scan < limit {
                                        scan += 1;
                                    }
                                }
                                _ => scan += 1,
                            }
                        }
                        found as u32
                    } else {
                        type_node.end
                    };
                    // Skip comments inside the type annotation (before delimiter).
                    // Using delimiter_pos ensures we don't consume trailing comments
                    // like /*2*/ that should be preserved in the output.
                    self.skip_comments_in_range(type_node.pos, delimiter_pos);
                    // Emit trailing comments between erased type and delimiter
                    if self.has_pending_comment_before(delimiter_pos) {
                        self.write(" ");
                        self.emit_comments_before_pos(delimiter_pos);
                        self.pending_block_comment_space = false;
                    }
                }
                if param.initializer.is_some() {
                    self.write(" = ");
                    self.emit(param.initializer);
                }

                // Emit trailing comments between the parameter and its delimiter
                // (`,` or `)`). For typed parameters, this is handled above via
                // delimiter_pos. For untyped parameters, scan for the delimiter
                // within the parameter node's range.
                // e.g., `a /* comment */, b` → preserve comment before the comma.
                if (param.type_annotation.is_none() || param.initializer.is_some())
                    && let Some(text) = self.source_text
                {
                    // Scan from the end of the parameter name/initializer to
                    // find the next `,` or `)`, bounded by param_node.end
                    // to avoid scanning into arrow function bodies or other
                    // unrelated syntax.
                    let scan_start = if param.initializer.is_some()
                        && let Some(init_node) = self.arena.get(param.initializer)
                    {
                        init_node.end as usize
                    } else if let Some(name_node) = self.arena.get(param.name) {
                        name_node.end as usize
                    } else {
                        param_node.end as usize
                    };
                    let bytes = text.as_bytes();
                    let limit = std::cmp::min(param_node.end as usize, text.len());
                    let mut scan = std::cmp::min(scan_start, limit);
                    let mut found_delimiter = false;
                    while scan < limit {
                        match bytes[scan] {
                            b',' | b')' => {
                                found_delimiter = true;
                                break;
                            }
                            b'/' if scan + 1 < limit && bytes[scan + 1] == b'*' => {
                                scan += 2;
                                while scan + 1 < limit
                                    && !(bytes[scan] == b'*' && bytes[scan + 1] == b'/')
                                {
                                    scan += 1;
                                }
                                if scan + 1 < limit {
                                    scan += 2;
                                }
                            }
                            b'/' if scan + 1 < limit && bytes[scan + 1] == b'/' => {
                                while scan < limit && bytes[scan] != b'\n' {
                                    scan += 1;
                                }
                            }
                            _ => scan += 1,
                        }
                    }
                    if found_delimiter {
                        let delimiter_pos = scan as u32;
                        if self.has_pending_comment_before(delimiter_pos) {
                            self.write(" ");
                            self.emit_comments_before_pos(delimiter_pos);
                            self.pending_block_comment_space = false;
                        }
                    }
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

        if param.type_annotation.is_some() {
            self.write(": ");
            self.emit(param.type_annotation);
        }

        if param.initializer.is_some() {
            self.write(" = ");
            self.emit_expression(param.initializer);
        }
    }

    pub(super) fn emit_function_parameters_with_trailing_comments(
        &mut self,
        params: &[NodeIndex],
        open_paren_pos: u32,
        search_start: u32,
        search_end: u32,
    ) {
        self.emit_function_parameters_js(params);
        self.map_closing_paren_backward(search_start, search_end);
        if !params.is_empty() {
            return;
        }

        let close_paren_pos = self
            .pending_source_pos
            .map(|source_pos| source_pos.pos)
            .unwrap_or(search_end);

        let mut comment_start = open_paren_pos.saturating_add(1);
        if let Some(source_text) = self.source_text {
            let bytes = source_text.as_bytes();
            let mut open_paren_pos_usize = open_paren_pos as usize;
            while open_paren_pos_usize > 0 && bytes.get(open_paren_pos_usize) != Some(&b'(') {
                open_paren_pos_usize = open_paren_pos_usize.saturating_sub(1);
            }
            if bytes.get(open_paren_pos_usize) == Some(&b'(') {
                comment_start = open_paren_pos_usize
                    .checked_add(1)
                    .map_or(comment_start, |start| start as u32);
            }
        }
        if comment_start < close_paren_pos {
            self.emit_comments_in_range(comment_start, close_paren_pos, true, false);
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
            self.emit_decl_name(name_idx);
            return;
        }

        // Recovery path: malformed parameter names like `yield`/`await`
        // can be parsed as expressions. Preserve original text for JS parity.
        if let Some(source) = self.source_text {
            let text =
                crate::safe_slice::slice(source, name_node.pos as usize, name_node.end as usize)
                    .trim();
            if !text.is_empty() {
                self.write(text);
                return;
            }
        }

        self.emit(name_idx);
    }
}

#[cfg(test)]
mod tests {
    use crate::output::printer::{PrintOptions, Printer};
    use tsz_parser::ParserState;

    /// Async arrow functions must always have parenthesized parameters,
    /// matching tsc behavior. `async x => x` becomes `async (x) => x`.
    #[test]
    fn async_arrow_always_parenthesizes_params() {
        let source = "const f = async i => i;";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("async (i) =>"),
            "Async arrow with single param should always have parens.\nOutput:\n{output}"
        );
    }

    /// Non-async arrow functions with a single simple param preserve source parens.
    /// `x => x` stays as `x => x` (no forced parens).
    #[test]
    fn non_async_arrow_preserves_no_parens() {
        let source = "const f = x => x;";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        // Should NOT add parens for non-async single-param arrow
        assert!(
            !output.contains("(x) =>"),
            "Non-async arrow without source parens should not add parens.\nOutput:\n{output}"
        );
        assert!(
            output.contains("x =>"),
            "Non-async arrow should preserve no-paren form.\nOutput:\n{output}"
        );
    }

    /// Async arrow with parens in source should keep them.
    #[test]
    fn async_arrow_with_source_parens_keeps_them() {
        let source = "const f = async (x) => x;";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("async (x) =>"),
            "Async arrow with source parens should keep them.\nOutput:\n{output}"
        );
    }

    /// Async arrow inside a function body should pass `this` to __awaiter.
    /// Arrow functions lexically capture `this` from the enclosing scope.
    #[test]
    fn async_arrow_in_function_passes_this_to_awaiter() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "function f() { (async () => { return 10; })(); }";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains("__awaiter(this,"),
            "Async arrow inside function should pass `this` to __awaiter.\nOutput:\n{}",
            result.code
        );
    }

    /// Async arrow at top level should pass `void 0` to __awaiter.
    #[test]
    fn async_arrow_at_top_level_passes_void_0_to_awaiter() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "const g = async () => { return 10; };";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains("__awaiter(void 0,"),
            "Async arrow at top level should pass `void 0` to __awaiter.\nOutput:\n{}",
            result.code
        );
    }

    /// Async arrow inside a class method should pass `this` to __awaiter.
    #[test]
    fn async_arrow_in_class_method_passes_this_to_awaiter() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "class C { method() { return (async () => 42)(); } }";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains("__awaiter(this,"),
            "Async arrow inside class method should pass `this` to __awaiter.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn function_with_empty_parameter_comment_preserves_comment() {
        let source = "function foo(/** nothing */) { return 1; }";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("function foo( /** nothing */) {"),
            "Comment inside empty parameter list should be emitted inside the parens for JS parity.\nOutput: {output}"
        );
        assert!(
            !output.contains("function foo() /** nothing */"),
            "Comment should not drift after closing paren.\nOutput: {output}"
        );
    }

    #[test]
    fn async_arrow_await_default_param_es2015_forwards_args() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "var foo = async (a = await): Promise<void> => {}";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains("var foo = (...args_1) => __awaiter(void 0, [...args_1], void 0, function* (a = yield ) {"),
            "Async arrow await-default recovery should forward args in ES2015 emit.\nOutput:\n{}",
            result.code
        );
    }

    /// Parameters with empty/missing identifier names (from parser error recovery)
    /// should be dropped, matching tsc behavior.
    #[test]
    fn empty_param_name_dropped() {
        let source = "function f(a,\u{00AC}) {}";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("function f(a)"),
            "Invalid character parameter should be dropped.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("(a, )"),
            "Should not have trailing comma for dropped param.\nOutput:\n{output}"
        );
    }

    /// Omitted arguments in call expressions should be dropped.
    #[test]
    fn omitted_call_args_dropped() {
        let source = "foo(a,,b);";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("foo(a, b)"),
            "Omitted argument should be dropped.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("foo(a, , b)"),
            "Should not have extra comma for omitted arg.\nOutput:\n{output}"
        );
    }
}
