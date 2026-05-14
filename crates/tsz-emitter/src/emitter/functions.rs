use super::{ParamTransformPlan, Printer};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
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

        let has_generator_asterisk = func.asterisk_token
            || crate::transforms::emit_utils::source_header_has_async_generator_asterisk(
                self.source_text,
                node.pos,
                self.arena.get(func.body).map_or(node.end, |body| body.pos),
            );

        if func.is_async && self.ctx.needs_async_lowering && !has_generator_asterisk {
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
        if func.is_async && self.ctx.needs_es2018_lowering && has_generator_asterisk {
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
        let prev_pending_function_body_parameters = std::mem::replace(
            &mut self.pending_function_body_parameters,
            func.parameters.nodes.clone(),
        );
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
        let prev_arguments_capture_name = self.ctx.arguments_capture_name.take();
        let prev_namespace_exported_names = self.namespace_exported_names.clone();
        self.push_commonjs_exported_var_parameter_shadow_names(&func.parameters.nodes);
        for &param_idx in &func.parameters.nodes {
            if let Some(param) = self.arena.get_parameter_at(param_idx) {
                let name = self.get_identifier_text_idx(param.name);
                if !name.is_empty() {
                    self.namespace_exported_names.remove(name.as_str());
                }
            }
        }
        self.emit(func.body);
        self.pop_commonjs_exported_var_parameter_shadow_names();
        self.namespace_exported_names = prev_namespace_exported_names;
        self.ctx.rewrite_arguments_to_arguments_1 = prev_rewrite_args;
        self.ctx.arguments_capture_name = prev_arguments_capture_name;
        self.ctx.flags.in_generator = prev_in_generator;
        self.declared_namespace_names = prev_declared;
        self.pop_temp_scope();
        self.ctx.block_scope_state.exit_scope();
        self.pending_function_body_parameters = prev_pending_function_body_parameters;
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
        self.ctx.block_scope_state.enter_function_scope();
        self.skip_block_opening_line_comments(block_node, block);
        self.emit_param_prologue(transforms);

        for &stmt_idx in &block.statements.nodes {
            let before_len = self.writer.len();
            self.emit(stmt_idx);
            if self.writer.len() > before_len {
                self.write_line();
            }
        }

        self.ctx.block_scope_state.exit_scope();
        self.decrease_indent();
        self.write("}");
    }

    fn skip_block_opening_line_comments(
        &mut self,
        block_node: &Node,
        block: &tsz_parser::parser::node::BlockData,
    ) {
        let Some(text) = self.source_text else {
            return;
        };
        let bytes = text.as_bytes();
        // Search FORWARD from `block_node.pos` for the opening `{`. In the
        // TypeScript AST, `node.pos` includes leading trivia, so the brace
        // is at or after `node.pos`, never before it. The previous backward
        // search either bailed out (block at offset 0) or found a `{` from
        // an earlier construct in the file, causing comments belonging to
        // that earlier brace to be incorrectly skipped. Mirrors the forward
        // scan used in `emitter/statements/core.rs::emit_block`.
        let start = block_node.pos as usize;
        let end = std::cmp::min(block_node.end as usize, bytes.len());
        let Some(offset) = bytes
            .get(start..end)
            .and_then(|slice| slice.iter().position(|&b| b == b'{'))
        else {
            return;
        };
        let open_brace = start + offset;

        let mut line_end = open_brace;
        while line_end < bytes.len() && bytes[line_end] != b'\n' && bytes[line_end] != b'\r' {
            line_end += 1;
        }
        let first_stmt_pos = block
            .statements
            .nodes
            .first()
            .and_then(|&idx| self.arena.get(idx))
            .map_or(block_node.end, |node| node.pos);
        let skip_end = std::cmp::min(line_end as u32, first_stmt_pos);
        while self.comment_emit_idx < self.all_comments.len() {
            let comment = &self.all_comments[self.comment_emit_idx];
            if comment.pos >= open_brace as u32 && comment.end <= skip_end {
                self.comment_emit_idx += 1;
            } else {
                break;
            }
        }
    }

    /// Emit function parameters for JavaScript (no types)
    pub(super) fn emit_function_parameters_js(&mut self, params: &[NodeIndex]) {
        // Check if any parameter needs ES2018 object rest lowering
        let needs_rest_lowering = self.ctx.needs_es2018_lowering
            && !self.ctx.target_es5
            && self.any_param_has_object_rest(params);

        // Clear any previous pending rest params
        self.pending_object_rest_params.clear();

        let prev_namespace_exported_names = self.namespace_exported_names.clone();
        let mut first = true;
        let mut object_rest_temp_counter = 0u32;
        let mut object_rest_temp_names = Vec::<String>::new();
        for &param_idx in params {
            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_parameter(param_node)
            {
                // Skip parameters with no name (parser error recovery artifacts).
                // e.g., `function f(a,¬)` should emit `function f(a)`.
                // But preserve rest parameters (`...`) even with missing names,
                // matching tsc behavior: `function sum(...) { }`.
                if param.name.is_none() && !param.dot_dot_dot_token {
                    if let Some(recovered_name) =
                        self.recovered_parameter_name_from_type_or_range(param_idx, param)
                    {
                        if !first {
                            self.write(", ");
                        }
                        first = false;
                        self.write(&recovered_name);
                        self.remove_namespace_exported_parameter_name(param_idx);
                    }
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
                    let Some(recovered_name) =
                        self.recovered_parameter_name_from_type_or_range(param_idx, param)
                    else {
                        continue;
                    };
                    if !first {
                        self.write(", ");
                    }
                    first = false;
                    self.write(&recovered_name);
                    self.remove_namespace_exported_parameter_name(param_idx);
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
                        // safe_slice: C → migrated. The "this" parameter must
                        // be skipped; a silent empty fallback would let it
                        // through and corrupt the parameter list. On a bad
                        // span we conservatively keep the parameter (don't
                        // continue) but log via tracing::debug! so the bug
                        // surfaces in dev rather than malformed output.
                        if let Ok(name_text) = crate::safe_slice::slice(
                            text,
                            name_node.pos as usize,
                            name_node.end as usize,
                        ) {
                            if name_text.trim() == "this" {
                                continue;
                            }
                        }
                    }
                }

                if !first {
                    self.write(", ");
                }
                first = false;

                // Emit leading comments before the parameter (e.g., inline JSDoc
                // comments like `/** comment */ a`). tsc preserves these in JS output.
                if self.pending_parameter_leading_comment_starts_line(param_node.pos)
                    && !self.writer.is_at_line_start()
                {
                    self.write_line();
                }
                self.emit_comments_before_pos(param_node.pos);

                // ES2018 object rest lowering: replace destructuring param with a temp
                if needs_rest_lowering && self.param_has_object_rest(param_idx) {
                    let temp = self.next_object_rest_param_temp_name(
                        &mut object_rest_temp_counter,
                        &mut object_rest_temp_names,
                    );
                    if param.dot_dot_dot_token {
                        self.emit_rest_parameter_spread_prefix(param_node.pos, param.name);
                    }
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
                    self.emit_rest_parameter_spread_prefix(param_node.pos, param.name);
                }
                self.emit_parameter_name_js(param.name);
                self.remove_namespace_exported_parameter_name(param_idx);
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
                } else if self.parameter_has_missing_initializer(param_node, param) {
                    self.write(" = ");
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
        self.namespace_exported_names = prev_namespace_exported_names;

        // NOTE: Do NOT emit trailing comments here. Comments after the last
        // parameter (e.g., `p3:any // OK`) appear on the same source line but
        // logically follow the closing `)`. Since type annotations are erased,
        // scanning from name_node.end would place these comments INSIDE the
        // parameter list. The caller (statement-level comment emission) handles
        // trailing comments after the whole function declaration.
    }

    fn pending_parameter_leading_comment_starts_line(&self, pos: u32) -> bool {
        if self.ctx.options.remove_comments || self.comment_emit_idx >= self.all_comments.len() {
            return false;
        }

        let actual_start = self.skip_trivia_forward(pos, pos + 1024);
        let comment = &self.all_comments[self.comment_emit_idx];
        comment.end <= actual_start && comment.has_trailing_new_line
    }

    pub(in crate::emitter) fn register_pending_function_body_parameters(&mut self) {
        let params = std::mem::take(&mut self.pending_function_body_parameters);
        for param_idx in params {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            self.register_function_parameter_binding_name(param.name);
        }
    }

    fn register_function_parameter_binding_name(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        if name_node.is_identifier() {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                let name = self.arena.resolve_identifier_text(ident);
                if !name.is_empty() && name != "this" {
                    self.ctx.block_scope_state.register_function_parameter(name);
                }
            }
        } else if matches!(
            name_node.kind,
            syntax_kind_ext::ARRAY_BINDING_PATTERN | syntax_kind_ext::OBJECT_BINDING_PATTERN
        ) && let Some(pattern) = self.arena.get_binding_pattern(name_node)
        {
            for &elem_idx in &pattern.elements.nodes {
                if let Some(elem_node) = self.arena.get(elem_idx)
                    && let Some(elem) = self.arena.get_binding_element(elem_node)
                {
                    self.register_function_parameter_binding_name(elem.name);
                }
            }
        }
    }

    fn next_object_rest_param_temp_name(
        &self,
        counter: &mut u32,
        used: &mut Vec<String>,
    ) -> String {
        loop {
            let current = *counter;
            *counter += 1;

            if current < 26 && (current == 8 || current == 13) {
                continue;
            }

            let name = if current < 26 {
                format!("_{}", (b'a' + current as u8) as char)
            } else {
                format!("_{}", current - 26)
            };

            if !self.file_identifiers.contains(&name) && !used.contains(&name) {
                used.push(name.clone());
                return name;
            }
        }
    }

    pub(super) fn emit_parameter(&mut self, node: &Node) {
        let Some(param) = self.arena.get_parameter(node) else {
            return;
        };

        if self.ctx.options.legacy_decorators
            && let Some(modifiers) = param.modifiers.as_ref()
        {
            for &mod_idx in &modifiers.nodes {
                let Some(mod_node) = self.arena.get(mod_idx) else {
                    continue;
                };
                if mod_node.kind == syntax_kind_ext::DECORATOR {
                    self.skip_comments_for_erased_node(mod_node);
                }
            }
        }

        if param.dot_dot_dot_token {
            self.write("...");
            if let Some(name_node) = self.arena.get(param.name) {
                self.emit_comments_after_dot_dot_dot(node.pos, name_node.pos, false);
            }
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
        } else if self.parameter_has_missing_initializer(node, param) {
            self.write(" = ");
        }
    }

    fn parameter_has_missing_initializer(
        &self,
        node: &Node,
        param: &tsz_parser::parser::node::ParameterData,
    ) -> bool {
        let Some(source_text) = self.source_text else {
            return false;
        };

        fn skip_trivia(bytes: &[u8], mut index: usize, scan_end: usize) -> usize {
            loop {
                while index < scan_end && matches!(bytes[index], b' ' | b'\t' | b'\r' | b'\n') {
                    index += 1;
                }

                if index + 1 < scan_end && bytes[index] == b'/' && bytes[index + 1] == b'*' {
                    index += 2;
                    while index + 1 < scan_end
                        && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                    {
                        index += 1;
                    }
                    if index + 1 < scan_end {
                        index += 2;
                    }
                    continue;
                }

                if index + 1 < scan_end && bytes[index] == b'/' && bytes[index + 1] == b'/' {
                    while index < scan_end && bytes[index] != b'\n' {
                        index += 1;
                    }
                    continue;
                }

                return index;
            }
        }

        let scan_end = node.end as usize;
        let bytes = source_text.as_bytes();
        if scan_end > bytes.len() {
            return false;
        }

        let scan_start = param
            .type_annotation
            .into_option()
            .and_then(|idx| self.arena.get(idx))
            .map_or_else(
                || {
                    let name_end = self
                        .arena
                        .get(param.name)
                        .map_or(node.pos, |name_node| name_node.end)
                        as usize;
                    if !param.question_token {
                        return name_end;
                    }

                    let optional_token_start = skip_trivia(bytes, name_end, scan_end);
                    if bytes.get(optional_token_start) == Some(&b'?') {
                        optional_token_start + 1
                    } else {
                        name_end
                    }
                },
                |type_node| type_node.end as usize,
            );
        if scan_start >= scan_end {
            return false;
        }

        let index = skip_trivia(bytes, scan_start, scan_end);
        if index >= scan_end {
            return false;
        }
        match bytes.get(index) {
            Some(b'=') if bytes.get(index + 1) == Some(&b'>') => false,
            Some(b'=') => true,
            _ => false,
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

        if let Some(recovered_name) =
            self.recovered_empty_parameter_name_from_header(open_paren_pos, close_paren_pos)
        {
            self.write(&recovered_name);
        }

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

    fn recovered_parameter_name_from_type_or_range(
        &self,
        param_idx: NodeIndex,
        param: &tsz_parser::parser::node::ParameterData,
    ) -> Option<String> {
        let source = self.source_text?;

        let raw = self
            .arena
            .get(param.type_annotation)
            .and_then(|type_node| {
                crate::safe_slice::slice(source, type_node.pos as usize, type_node.end as usize)
                    .ok()
            })
            .or_else(|| {
                self.arena.get(param_idx).and_then(|param_node| {
                    crate::safe_slice::slice(
                        source,
                        param_node.pos as usize,
                        param_node.end as usize,
                    )
                    .ok()
                })
            })?;

        raw.trim_matches(|ch: char| ch == ':' || ch.is_whitespace())
            .split(|ch: char| !matches!(ch, '_' | '$') && !ch.is_ascii_alphanumeric())
            .find(|part| !part.is_empty())
            .map(str::to_string)
    }

    pub(in crate::emitter) fn recovered_empty_parameter_name_from_header(
        &self,
        open_paren_pos: u32,
        close_paren_pos: u32,
    ) -> Option<String> {
        let source = self.source_text?;
        let start = open_paren_pos.checked_add(1)? as usize;
        let end = close_paren_pos as usize;
        let raw = crate::safe_slice::slice(source, start, end).ok()?;
        recovered_parameter_name_from_colon_header(raw)
    }

    pub(in crate::emitter) fn emit_parameter_name_js(&mut self, name_idx: NodeIndex) {
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
        if let Some(source) = self.source_text
            && let Ok(raw) =
                crate::safe_slice::slice(source, name_node.pos as usize, name_node.end as usize)
        {
            let text = raw.trim();
            if !text.is_empty() {
                self.write(text);
                return;
            }
        }

        self.emit(name_idx);
    }

    pub(in crate::emitter) fn emit_recovered_async_await_arrow_parameter(
        &mut self,
        params: &[NodeIndex],
    ) {
        if self
            .recovered_async_await_arrow_parameter_name(params)
            .is_some()
        {
            self.write(", await");
        }
    }

    fn recovered_async_await_arrow_parameter_name(&self, params: &[NodeIndex]) -> Option<&'a str> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let source_end = text.len().min(u32::MAX as usize) as u32;

        for &param_idx in params {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            let Some(init_node) = self.arena.get(param.initializer) else {
                continue;
            };
            if init_node.kind != syntax_kind_ext::AWAIT_EXPRESSION {
                continue;
            }

            let mut pos = self.skip_trivia_forward(init_node.pos, source_end) as usize;
            if bytes.get(pos..pos + "await".len()) != Some(b"await") {
                continue;
            }
            pos += "await".len();
            pos = self.skip_trivia_forward(pos as u32, source_end) as usize;
            if bytes.get(pos) != Some(&b'=') || bytes.get(pos + 1) != Some(&b'>') {
                continue;
            }
            pos = self.skip_trivia_forward((pos + 2) as u32, source_end) as usize;
            let end = pos.checked_add("await".len())?;
            if bytes.get(pos..end) != Some(b"await") {
                continue;
            }
            let next = bytes.get(end).copied();
            if next.is_some_and(|b| b == b'_' || b == b'$' || b.is_ascii_alphanumeric()) {
                continue;
            }
            return crate::safe_slice::slice(text, pos, end).ok();
        }

        None
    }
}

fn recovered_parameter_name_from_colon_header(raw: &str) -> Option<String> {
    let after_colon = raw.trim().strip_prefix(':')?;
    after_colon
        .split(|ch: char| !matches!(ch, '_' | '$') && !ch.is_ascii_alphanumeric())
        .find(|part| !part.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use crate::output::printer::{PrintOptions, Printer};
    use tsz_common::ScriptTarget;
    fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
        let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        (parser, root)
    }

    /// Async arrow functions must always have parenthesized parameters,
    /// matching tsc behavior. `async x => x` becomes `async (x) => x`.
    #[test]
    fn async_arrow_always_parenthesizes_params() {
        let source = "const f = async i => i;";

        let (parser, root) = parse_test_source(source);

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

        let (parser, root) = parse_test_source(source);

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

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("async (x) =>"),
            "Async arrow with source parens should keep them.\nOutput:\n{output}"
        );
    }

    #[test]
    fn parenthesized_arrow_body_type_erasure_strips_parens_across_comment() {
        let source = "const x = (a: any[]) => (\n    // comment\n    undefined as number\n);";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(
            &parser.arena,
            PrintOptions {
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("const x = (a) => \n// comment\nundefined;"),
            "Arrow body type erasure should strip recovery parens and hoist the comment.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("=> ("),
            "Arrow body type erasure should not preserve the opening paren.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("undefined);"),
            "Arrow body type erasure should not preserve the closing paren.\nOutput:\n{output}"
        );
    }

    /// Async arrow inside a function body should pass `this` to __awaiter.
    /// Arrow functions lexically capture `this` from the enclosing scope.
    #[test]
    fn async_arrow_in_function_passes_this_to_awaiter() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "function f() { (async () => { return 10; })(); }";
        let (parser, root) = parse_test_source(source);
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
        let (parser, root) = parse_test_source(source);
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
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains("__awaiter(this,"),
            "Async arrow inside class method should pass `this` to __awaiter.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn async_arrow_inside_top_level_arrow_passes_void_0_to_awaiter() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "const outer = () => async () => 1;";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains("() => __awaiter(void 0,"),
            "Async arrow nested only in top-level arrows should pass void 0.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn async_arrow_with_binding_pattern_params_forwards_arguments_to_generator() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "const f = async (dispatch: Dispatch, { foo }: OwnProps) => { return foo; };";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains(
                "(dispatch_1, _a) => __awaiter(void 0, [dispatch_1, _a], void 0, function* (dispatch, { foo }) { return foo; })"
            ),
            "Async arrow with a binding pattern should forward temp parameters into the generator.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn async_arrow_object_rest_param_uses_generator_prologue() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "async ({ foo, bar, ...rest }) => bar(await foo);";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result
                .code
                .contains("__awaiter(void 0, void 0, void 0, function* () {"),
            "Async arrow with object rest should not forward the rest temp as generator args.\nOutput:\n{}",
            result.code
        );
        assert!(
            result
                .code
                .contains("var { foo, bar } = _a, rest = __rest(_a, [\"foo\", \"bar\"]);"),
            "Async arrow with object rest should emit a generator prologue.\nOutput:\n{}",
            result.code
        );
        assert!(
            result.code.contains("return bar(yield foo);"),
            "Async arrow body should still lower await to yield.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn function_with_empty_parameter_comment_preserves_comment() {
        let source = "function foo(/** nothing */) { return 1; }";

        let (parser, root) = parse_test_source(source);
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
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains("var foo = (...args_1) => __awaiter(void 0, [...args_1], void 0, function* (a = yield ) {"),
            "Async arrow await-default recovery should forward args in ES2015 emit.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn async_arrow_default_param_preserves_leading_args_and_reuses_arguments_capture() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "function f() { const a1 = async (x, y = z) => {}; const a2 = async (x = z) => { return async () => arguments; }; const a3 = async () => { return async (x = z) => arguments; }; }";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains(
                "const a1 = (x_1, ...args_1) => __awaiter(this, [x_1, ...args_1], void 0, function* (x, y = z) { });"
            ),
            "Leading parameters before the first default should stay explicit and the default tail should be forwarded.\nOutput:\n{}",
            result.code
        );
        assert!(
            result.code.contains("var arguments_1 = arguments;"),
            "The first async arrow that needs lexical arguments should create a function-scoped capture.\nOutput:\n{}",
            result.code
        );
        assert!(
            result.code.contains(
                "const a3 = () => __awaiter(this, void 0, void 0, function* () { return (...args_1) => __awaiter(this, [...args_1], void 0, function* (x = z) { return arguments_1; }); });"
            ),
            "Sibling async arrows should reuse the existing function-scoped arguments capture.\nOutput:\n{}",
            result.code
        );
        assert!(
            !result.code.contains("var arguments_2 = arguments;"),
            "Sibling async arrows should not create redundant lexical arguments captures.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn async_function_es2015_single_line_moved_params_stays_inline() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "async function f(x = z) { return async () => arguments; }";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains(
                "return __awaiter(this, arguments, void 0, function* (x = z) { return () => __awaiter(this, void 0, void 0, function* () { return arguments_1; }); });"
            ),
            "Single-line async function bodies with moved parameters should stay inline in the generator wrapper.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn async_function_await_arrow_param_recovery_native_keeps_await_param() {
        use crate::output::printer::lower_and_print;

        let source = "async function foo(a = await => await): Promise<void> {}";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(
            &parser.arena,
            root,
            PrintOptions {
                target: ScriptTarget::ES2017,
                ..Default::default()
            },
        );

        assert!(
            result
                .code
                .contains("async function foo(a = await , await) {"),
            "Native async function recovery should preserve the trailing `await` parameter.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn async_function_await_arrow_param_recovery_es2015_keeps_await_param() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "async function foo(a = await => await): Promise<void> {}";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains("function* (a = yield , await) {"),
            "Lowered async function recovery should preserve the trailing `await` parameter.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn async_function_es2015_destructured_param_preserves_outer_arity() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "async function foo({ foo = await bar }) {}";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains("function foo(_a) {"),
            "Outer async function should keep a placeholder parameter.\nOutput:\n{}",
            result.code
        );
        assert!(
            result.code.contains("function* ({ foo = yield bar })"),
            "Moved generator parameter should preserve the destructuring pattern.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn async_function_es2015_moved_params_avoid_inner_name_collisions() {
        use crate::output::printer::{PrintOptions, lower_and_print};

        let source = "async function h(a, { x }) {}";
        let (parser, root) = parse_test_source(source);
        let result = lower_and_print(&parser.arena, root, PrintOptions::es6());

        assert!(
            result.code.contains("function h(a_1, _a) {"),
            "Outer async function placeholders should avoid colliding with inner generator parameters.\nOutput:\n{}",
            result.code
        );
        assert!(
            result.code.contains("function* (a, { x })"),
            "Moved generator parameters should preserve original names and patterns.\nOutput:\n{}",
            result.code
        );
    }

    #[test]
    fn malformed_rest_parameter_modifier_recovers_following_parameter() {
        let source = "class C { constructor(...public rest: string[]) {} }";

        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("constructor(...public, rest)"),
            "Malformed rest parameter should preserve the recovered parameter.\nOutput:\n{output}"
        );
    }

    #[test]
    fn parameter_leading_jsdoc_preserves_multiline_parameter_list_shape() {
        let source = r"class Type {
  constructor(
    /** a unique name for this codec */
    readonly name: string,
    /** a custom type guard */
    readonly is: boolean
  ) {}
}";

        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains(
                "constructor(\n    /** a unique name for this codec */\n    name, \n    /** a custom type guard */\n    is)"
            ),
            "Parameter JSDoc should keep the multiline parameter-list shape.\nOutput:\n{output}"
        );
    }

    /// Parameters with empty/missing identifier names (from parser error recovery)
    /// should be dropped, matching tsc behavior.
    #[test]
    fn empty_param_name_dropped() {
        let source = "function f(a,\u{00AC}) {}";

        let (parser, root) = parse_test_source(source);

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

        let (parser, root) = parse_test_source(source);

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
