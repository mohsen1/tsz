impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_do_statement(&mut self, node: &Node) {
        let Some(loop_stmt) = self.arena.get_loop(node) else {
            return;
        };

        // ES5: Check if closures capture body-scoped let/const variables
        if self.ctx.target_es5 {
            let body_info = super::super::es5::loop_capture::collect_loop_body_vars(
                self.arena,
                loop_stmt.statement,
            );
            if !body_info.block_scoped_vars.is_empty()
                && super::super::es5::loop_capture::check_loop_needs_capture(
                    self.arena,
                    loop_stmt.statement,
                    &[],
                    &body_info.block_scoped_vars,
                )
                .is_some()
            {
                self.emit_condition_loop_with_capture(
                    loop_stmt,
                    &body_info,
                    super::super::es5::loop_capture::ConditionLoopKind::DoWhile,
                );
                return;
            }
        }

        self.write("do");
        let prev_lexical_block_missing_initializer_function_depth =
            self.lexical_block_missing_initializer_function_depth;
        let prev_lexical_block_missing_initializer_is_loop_body =
            self.lexical_block_missing_initializer_is_loop_body;
        if self.ctx.target_es5 {
            self.lexical_block_missing_initializer_function_depth = Some(self.function_scope_depth);
            self.lexical_block_missing_initializer_is_loop_body = true;
        }
        let body_is_block = self
            .arena
            .get(loop_stmt.statement)
            .is_some_and(|n| n.kind == syntax_kind_ext::BLOCK);
        if body_is_block {
            self.write(" ");
            self.emit(loop_stmt.statement);
            self.write(" ");
        } else {
            self.write_line();
            self.increase_indent();
            let before = self.writer.len();
            self.emit(loop_stmt.statement);
            // If the body was completely erased (e.g. const enum, interface),
            // emit `;` to produce a valid empty statement.
            if self.writer.len() == before {
                self.write(";");
            }
            self.decrease_indent();
            self.write_line();
        }
        self.lexical_block_missing_initializer_function_depth =
            prev_lexical_block_missing_initializer_function_depth;
        self.lexical_block_missing_initializer_is_loop_body =
            prev_lexical_block_missing_initializer_is_loop_body;
        self.write("while (");
        self.emit(loop_stmt.condition);
        // Map closing `)` — scan backward from node end (past `;`)
        self.map_closing_paren_backward(node.pos, node.end);
        self.write(")");
        self.map_trailing_semicolon(node);
        self.write_semicolon();
        self.emit_trailing_comment_after_semicolon(node);
    }

    pub(in crate::emitter) fn emit_debugger_statement(&mut self, node: &Node) {
        self.write("debugger");
        self.map_trailing_semicolon(node);
        self.write_semicolon();
        self.emit_trailing_comment_after_semicolon(node);
    }

    pub(in crate::emitter) fn emit_with_statement(&mut self, node: &Node) {
        let Some(with_stmt) = self.arena.get_with_statement(node) else {
            return;
        };

        self.write("with (");
        self.emit(with_stmt.expression);
        // Map closing `)` — scan backward from body start
        if let Some(body_node) = self.arena.get(with_stmt.then_statement) {
            self.map_closing_paren_backward(node.pos, body_node.pos);
        }
        self.write(")");
        let body_is_block = self
            .arena
            .get(with_stmt.then_statement)
            .is_some_and(|n| n.kind == syntax_kind_ext::BLOCK);
        if body_is_block {
            self.write(" ");
            self.emit(with_stmt.then_statement);
        } else {
            self.write_line();
            self.increase_indent();
            let before = self.writer.len();
            self.emit(with_stmt.then_statement);
            // If the body was completely erased (e.g. const enum, interface),
            // emit `;` to produce a valid empty statement.
            if self.writer.len() == before {
                self.write(";");
            }
            self.decrease_indent();
        }
    }

    /// Check if a for-statement initializer is a `using` declaration list.
    fn for_initializer_has_using(&self, initializer: NodeIndex) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return false;
        }
        (init_node.flags as u32 & node_flags::USING) != 0
    }

    /// Emit `for (using d of items) { body }` with dispose lowering.
    /// Transforms to:
    /// ```js
    /// for (const d_1 of items) {
    ///     const env_1 = { stack: [], error: void 0, hasError: false };
    ///     try {
    ///         const d = __addDisposableResource(env_1, d_1, false);
    ///         // ... body statements
    ///     }
    ///     catch (e_1) { env_1.error = e_1; env_1.hasError = true; }
    ///     finally { __disposeResources(env_1); }
    /// }
    /// ```
    fn emit_for_of_with_using_lowering(
        &mut self,
        node: &Node,
        for_in_of: &tsz_parser::parser::node::ForInOfData,
        using_info: crate::transforms::emit_utils::ForOfUsingInfo,
    ) {
        let var_name = using_info.binding_name;
        let using_async = using_info.using_async;
        let (env_name, error_name, result_name) = self.next_disposable_env_names();
        // Generate a temp name based on original: d1 -> d1_1 (uses the env counter)
        let temp_name = format!("{}_{}", var_name, self.next_disposable_env_id - 1);
        self.generated_temp_names.insert(temp_name.clone());

        self.write("for ");
        if for_in_of.await_modifier {
            self.write("await ");
        }
        self.write("(const ");
        self.write(&temp_name);
        self.write(" of ");
        self.emit(for_in_of.expression);
        // Map closing `)` — scan backward from body start
        if let Some(body_node) = self.arena.get(for_in_of.statement) {
            self.map_closing_paren_backward(node.pos, body_node.pos);
        }
        self.write(") {");
        self.write_line();
        self.increase_indent();

        // Emit: const env_1 = { stack: [], error: void 0, hasError: false };
        self.write("const ");
        self.write(&env_name);
        self.write(" = { stack: [], error: void 0, hasError: false };");
        self.write_line();

        // Emit: try {
        self.write("try {");
        self.write_line();
        self.increase_indent();

        // Emit: const d = __addDisposableResource(env_1, d_1, false);
        self.write("const ");
        self.write(&var_name);
        self.write(" = ");
        self.write_helper("__addDisposableResource");
        self.write("(");
        self.write(&env_name);
        self.write(", ");
        self.write(&temp_name);
        self.write(", ");
        self.write(if using_async { "true" } else { "false" });
        self.write(");");
        self.write_line();

        // Emit the original loop body statements (unwrap the block)
        if let Some(body_node) = self.arena.get(for_in_of.statement) {
            if body_node.kind == syntax_kind_ext::BLOCK {
                if let Some(block) = self.arena.get_block(body_node) {
                    for &stmt in &block.statements.nodes {
                        self.emit(stmt);
                        if !self.writer.is_at_line_start() {
                            self.write_line();
                        }
                    }
                }
            } else {
                self.emit(for_in_of.statement);
                if !self.writer.is_at_line_start() {
                    self.write_line();
                }
            }
        }

        // Close try
        self.decrease_indent();
        self.write("}");
        self.write_line();

        // Emit catch
        self.write("catch (");
        self.write(&error_name);
        self.write(") {");
        self.write_line();
        self.increase_indent();
        self.write(&env_name);
        self.write(".error = ");
        self.write(&error_name);
        self.write(";");
        self.write_line();
        self.write(&env_name);
        self.write(".hasError = true;");
        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.write_line();

        // Emit finally
        self.write("finally {");
        self.write_line();
        self.increase_indent();
        if using_async {
            let await_kw = if self.ctx.emit_await_as_yield {
                "yield"
            } else {
                "await"
            };
            self.write("const ");
            self.write(&result_name);
            self.write(" = ");
            self.write_helper("__disposeResources");
            self.write("(");
            self.write(&env_name);
            self.write(");");
            self.write_line();
            self.write("if (");
            self.write(&result_name);
            self.write(")");
            self.write_line();
            self.increase_indent();
            self.write(await_kw);
            self.write(" ");
            self.write(&result_name);
            self.write(";");
            self.write_line();
            self.decrease_indent();
        } else {
            self.write_helper("__disposeResources");
            self.write("(");
            self.write(&env_name);
            self.write(");");
            self.write_line();
        }
        self.decrease_indent();
        self.write("}");
        self.write_line();

        // Close outer for loop body
        self.decrease_indent();
        self.write("}");
    }

    /// Emit `for (using d1 = expr, d2 = expr2;;) { body }` with dispose lowering.
    /// Transforms to:
    /// ```js
    /// {
    ///     const env_1 = { stack: [], error: void 0, hasError: false };
    ///     try {
    ///         const d1 = __addDisposableResource(env_1, expr, false), d2 = ...;
    ///         for (;;) { body }
    ///     }
    ///     catch (e_1) { env_1.error = e_1; env_1.hasError = true; }
    ///     finally { __disposeResources(env_1); }
    /// }
    /// ```
    fn emit_for_with_using_lowering(
        &mut self,
        node: &Node,
        loop_stmt: &tsz_parser::parser::node::LoopData,
    ) {
        let init_node = self.arena.get(loop_stmt.initializer).unwrap();
        let flags = init_node.flags as u32;
        let using_async = node_flags::is_await_using(flags);
        let decl_list = self.arena.get_variable(init_node).unwrap();
        let (env_name, error_name, result_name) = self.next_disposable_env_names();
        let decl_keyword = if self.ctx.target_es5 {
            "var "
        } else {
            "const "
        };

        // Emit wrapping block: {
        self.write("{");
        self.write_line();
        self.increase_indent();

        // Emit: const/var env_1 = { stack: [], error: void 0, hasError: false };
        self.write(decl_keyword);
        self.write(&env_name);
        self.write(" = { stack: [], error: void 0, hasError: false };");
        self.write_line();

        // Emit: try {
        self.write("try {");
        self.write_line();
        self.increase_indent();

        // Emit: const/var d1 = __addDisposableResource(env_1, expr, false), d2 = ...;
        let initialized_decls: Vec<_> = decl_list
            .declarations
            .nodes
            .iter()
            .copied()
            .filter(|&decl_idx| {
                self.arena
                    .get(decl_idx)
                    .and_then(|n| self.arena.get_variable_declaration(n))
                    .is_some_and(|d| d.initializer.is_some())
            })
            .collect();

        if !initialized_decls.is_empty() {
            self.write(decl_keyword);
            for (i, &decl_idx) in initialized_decls.iter().enumerate() {
                if let Some(decl_node) = self.arena.get(decl_idx)
                    && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                {
                    self.emit(decl.name);
                    self.write(" = ");
                    self.write_helper("__addDisposableResource");
                    self.write("(");
                    self.write(&env_name);
                    self.write(", ");
                    if !self
                        .try_emit_object_literal_es5_inline_computed_expression(decl.initializer)
                    {
                        self.emit(decl.initializer);
                    }
                    self.write(", ");
                    self.write(if using_async { "true" } else { "false" });
                    self.write(")");
                    if i + 1 < initialized_decls.len() {
                        self.write(", ");
                    }
                }
            }
            self.write(";");
            self.write_line();
        }

        // Emit the for loop with no initializer: for (;;) { body }
        self.write("for (");
        // No initializer
        // Emit condition and incrementor (both should be None for `using` in for-init)
        self.write(";");
        if loop_stmt.condition.is_some() {
            self.write(" ");
            self.emit(loop_stmt.condition);
        }
        self.write(";");
        if loop_stmt.incrementor.is_some() {
            self.write(" ");
            self.emit(loop_stmt.incrementor);
        }
        // Map closing `)` — scan backward from body start
        if let Some(body_node) = self.arena.get(loop_stmt.statement) {
            self.map_closing_paren_backward(node.pos, body_node.pos);
        }
        self.write(")");
        self.emit_loop_body(loop_stmt.statement);
        self.write_line();

        // Close try
        self.decrease_indent();
        self.write("}");
        self.write_line();

        // Emit catch
        self.write("catch (");
        self.write(&error_name);
        self.write(") {");
        self.write_line();
        self.increase_indent();
        self.write(&env_name);
        self.write(".error = ");
        self.write(&error_name);
        self.write(";");
        self.write_line();
        self.write(&env_name);
        self.write(".hasError = true;");
        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.write_line();

        // Emit finally
        self.write("finally {");
        self.write_line();
        self.increase_indent();
        if using_async {
            let await_kw = if self.ctx.emit_await_as_yield {
                "yield"
            } else {
                "await"
            };
            self.write("const ");
            self.write(&result_name);
            self.write(" = ");
            self.write_helper("__disposeResources");
            self.write("(");
            self.write(&env_name);
            self.write(");");
            self.write_line();
            self.write("if (");
            self.write(&result_name);
            self.write(")");
            self.write_line();
            self.increase_indent();
            self.write(await_kw);
            self.write(" ");
            self.write(&result_name);
            self.write(";");
            self.write_line();
            self.decrease_indent();
        } else {
            self.write_helper("__disposeResources");
            self.write("(");
            self.write(&env_name);
            self.write(");");
            self.write_line();
        }
        self.decrease_indent();
        self.write("}");
        self.write_line();

        // Close wrapping block
        self.decrease_indent();
        self.write("}");
    }

    fn emit_static_block_await_labeled_jump_recovery(&mut self, stmt_idx: NodeIndex) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        let jump_keyword = if stmt_node.kind == syntax_kind_ext::BREAK_STATEMENT {
            "break"
        } else if stmt_node.kind == syntax_kind_ext::CONTINUE_STATEMENT {
            "continue"
        } else {
            return false;
        };
        if !self.static_block_jump_source_has_await_label(stmt_node, jump_keyword) {
            return false;
        }

        self.write(jump_keyword);
        self.write(" ;");
        true
    }

    fn static_block_jump_source_has_await_label(
        &self,
        stmt_node: &Node,
        jump_keyword: &str,
    ) -> bool {
        if !self.ctx.flags.in_class_static_block {
            return false;
        }
        if self
            .arena
            .get_jump_data(stmt_node)
            .is_some_and(|jump| jump.label.is_some())
        {
            return false;
        }
        let Some(text) = self.source_text else {
            return false;
        };
        let start = stmt_node.pos as usize;
        if start >= text.len() {
            return false;
        }
        let line_end = text[start..]
            .find('\n')
            .map_or(text.len(), |offset| start + offset);
        let Ok(line) = crate::safe_slice::slice(text, start, line_end) else {
            return false;
        };
        let Some(rest) = line.trim_start().strip_prefix(jump_keyword) else {
            return false;
        };
        let rest = rest.trim_start();
        rest.starts_with("await")
            && rest["await".len()..]
                .chars()
                .next()
                .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '$')
    }
}
