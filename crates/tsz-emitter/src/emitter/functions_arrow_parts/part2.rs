impl<'a> Printer<'a> {
    // =========================================================================
    // Functions
    // =========================================================================

    /// Emit an async arrow function lowered for ES2015/ES2016 targets.
    /// Transforms simple arrows as `async () => body` → `() => __awaiter(...)`.
    /// Arrows with binding-pattern parameters forward temp parameters into the
    /// generator, matching tsc's `(_a) => __awaiter(..., [_a], ..., function* ({ x }) {})`.
    fn emit_arrow_function_async_lowered(&mut self, func: &tsz_parser::parser::node::FunctionData) {
        // Don't emit `async` - it's lowered away

        // For arrow functions on ES2015+, TSC passes `this` to __awaiter when
        // the arrow's lexical `this` comes from a non-arrow function/method.
        // Arrow-only nesting at the top level still has no meaningful `this`.
        let this_arg = if self.function_scope_depth > self.arrow_function_scope_depth {
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

        let has_object_rest_param = self.ctx.needs_es2018_lowering
            && !self.ctx.target_es5
            && self.any_param_has_object_rest(&func.parameters.nodes);
        // Issue #3758: when any parameter has a default initializer, tsc
        // shifts the entire parameter list into the generator and forwards
        // arguments via `(...args_<n>) => __awaiter(..., [...args_<n>], ...,
        // function* (<orig params>) { ... })`. This makes default-initializer
        // expressions evaluate inside the generator, so synchronous throws
        // turn into rejected promises instead of escaping the call site.
        let needs_default_param_forwarding =
            !has_object_rest_param && self.async_arrow_has_default_param(&func.parameters.nodes);
        if needs_default_param_forwarding {
            self.emit_async_arrow_default_param_forwarding(func, this_arg);
            return;
        }
        let forward_parameter_names = (!has_object_rest_param
            && self.async_arrow_needs_parameter_forwarding(&func.parameters.nodes))
        .then(|| self.async_arrow_forwarded_parameter_names(&func.parameters.nodes));

        // TSC always wraps parameters in parens when lowering async arrows,
        // even if the original source had `async x => ...` without parens.
        self.write("(");
        if let Some(names) = forward_parameter_names.as_ref() {
            for (idx, name) in names.iter().enumerate() {
                if idx > 0 {
                    self.write(", ");
                }
                self.write(name);
            }
        } else {
            self.emit_function_parameters_js(&func.parameters.nodes);
        }
        self.write(")");
        let object_rest_param_prologue: Vec<(String, NodeIndex)> =
            std::mem::take(&mut self.pending_object_rest_params);
        let has_object_rest_param_prologue = !object_rest_param_prologue.is_empty();

        // Check if the body references `arguments`. If so, we must capture it
        // before entering the generator: `() => { var arguments_1 = arguments; return __awaiter(...); }`
        // However, if we're already inside a generator body that has captured arguments
        // (rewrite_arguments_to_arguments_1 is true), don't create another capture -
        // the references are already being rewritten to `arguments_1`.
        let body_uses_arguments = !self.ctx.rewrite_arguments_to_arguments_1
            && contains_arguments_reference(self.arena, func.body);
        let enclosing_arguments_capture_name = if body_uses_arguments {
            self.ctx.arguments_capture_name.clone()
        } else {
            None
        };
        let captures_arguments = body_uses_arguments && enclosing_arguments_capture_name.is_none();

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
        let body_has_for_await = is_block && self.body_contains_for_await(func.body);

        if body_is_empty_single_line
            && !has_object_rest_param_prologue
            && forward_parameter_names.is_none()
        {
            self.write(" => ");
            self.write_helper("__awaiter");
            self.write("(");
            self.write(this_arg);
            self.write(", void 0, void 0, function* () { })");
            return;
        }

        if let Some(names) = forward_parameter_names {
            self.write(" => ");
            self.write_helper("__awaiter");
            self.write("(");
            self.write(this_arg);
            self.write(", [");
            for (idx, name) in names.iter().enumerate() {
                if idx > 0 {
                    self.write(", ");
                }
                self.write(name);
            }
            self.write("], void 0, function* (");
            self.emit_function_parameters_js(&func.parameters.nodes);
            let forwarded_object_rest_param_prologue: Vec<(String, NodeIndex)> =
                std::mem::take(&mut self.pending_object_rest_params);
            let has_forwarded_object_rest_param_prologue =
                !forwarded_object_rest_param_prologue.is_empty();
            self.write(") {");

            let saved_yield = self.ctx.emit_await_as_yield;
            let saved_args = self.ctx.rewrite_arguments_to_arguments_1;
            let saved_arguments_capture_name = self.ctx.arguments_capture_name.clone();
            self.ctx.emit_await_as_yield = true;
            if let Some(capture_name) = enclosing_arguments_capture_name.clone() {
                self.ctx.rewrite_arguments_to_arguments_1 = true;
                self.ctx.arguments_capture_name = Some(capture_name);
            }
            if is_block {
                if body_is_single_line
                    && !has_forwarded_object_rest_param_prologue
                    && !body_has_for_await
                {
                    if let Some(body_node) = self.arena.get(func.body)
                        && let Some(block) = self.arena.get_block(body_node)
                    {
                        for &stmt in &block.statements.nodes {
                            self.write(" ");
                            self.emit(stmt);
                        }
                    }
                } else {
                    self.write_line();
                    self.increase_indent();
                    let hoist_start =
                        body_has_for_await.then(|| self.begin_async_arrow_generator_hoists());
                    self.emit_object_rest_param_prologue_entries(
                        &forwarded_object_rest_param_prologue,
                    );
                    if let Some(body_node) = self.arena.get(func.body)
                        && let Some(block) = self.arena.get_block(body_node)
                    {
                        for &stmt in &block.statements.nodes {
                            self.emit(stmt);
                            self.write_line();
                        }
                    }
                    if let Some(hoist_start) = hoist_start {
                        self.insert_async_arrow_generator_hoists(hoist_start);
                    }
                    self.decrease_indent();
                }
            } else {
                self.write(" return ");
                self.emit_expression(func.body);
                self.write(";");
            }
            self.ctx.emit_await_as_yield = saved_yield;
            self.ctx.rewrite_arguments_to_arguments_1 = saved_args;
            self.ctx.arguments_capture_name = saved_arguments_capture_name;
            self.write(" })");
            return;
        }

        // When capturing arguments, always use block form:
        // `() => { var arguments_1 = arguments; return __awaiter(..., function* () { ... arguments_1 ... }); }`
        if captures_arguments {
            let arguments_capture_name = loop {
                self.ctx.arguments_capture_counter += 1;
                let candidate = format!("arguments_{}", self.ctx.arguments_capture_counter);
                if !self.file_identifiers.contains(&candidate) {
                    break candidate;
                }
            };
            self.write(" => {");
            self.write_line();
            self.increase_indent();
            self.write("var ");
            self.write(&arguments_capture_name);
            self.write(" = arguments;");
            self.write_line();
            self.write("return ");
            self.write_helper("__awaiter");
            self.write("(");
            self.write(this_arg);
            self.write(", void 0, void 0, function* () {");

            let saved_yield = self.ctx.emit_await_as_yield;
            let saved_args = self.ctx.rewrite_arguments_to_arguments_1;
            let saved_arguments_capture_name = self.ctx.arguments_capture_name.clone();
            self.ctx.emit_await_as_yield = true;
            self.ctx.rewrite_arguments_to_arguments_1 = true;
            self.ctx.arguments_capture_name = Some(arguments_capture_name);

            if is_block {
                if body_is_single_line && !has_object_rest_param_prologue && !body_has_for_await {
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
                    let hoist_start =
                        body_has_for_await.then(|| self.begin_async_arrow_generator_hoists());
                    self.emit_object_rest_param_prologue_entries(&object_rest_param_prologue);
                    self.emit_async_body_block_statements(func.body);
                    if let Some(hoist_start) = hoist_start {
                        self.insert_async_arrow_generator_hoists(hoist_start);
                    }
                    self.decrease_indent();
                    self.write("})");
                }
            } else if has_object_rest_param_prologue {
                self.write_line();
                self.increase_indent();
                self.emit_object_rest_param_prologue_entries(&object_rest_param_prologue);
                self.write("return ");
                self.emit_expression(func.body);
                self.write(";");
                self.write_line();
                self.decrease_indent();
                self.write("})");
            } else {
                self.write(" return ");
                self.emit_expression(func.body);
                self.write("; })");
            }

            self.ctx.emit_await_as_yield = saved_yield;
            self.ctx.rewrite_arguments_to_arguments_1 = saved_args;
            self.ctx.arguments_capture_name = saved_arguments_capture_name;

            self.write(";");
            self.write_line();
            self.decrease_indent();
            self.write("}");
            return;
        }

        if body_is_single_line && !has_object_rest_param_prologue && !body_has_for_await {
            // Single-line body: emit inline like TSC
            // e.g., () => __awaiter(this, void 0, void 0, function* () { return yield this; })
            self.write(" => ");
            self.write_helper("__awaiter");
            self.write("(");
            self.write(this_arg);
            self.write(", void 0, void 0, function* () {");

            let saved_yield = self.ctx.emit_await_as_yield;
            let saved_args = self.ctx.rewrite_arguments_to_arguments_1;
            let saved_arguments_capture_name = self.ctx.arguments_capture_name.clone();
            self.ctx.emit_await_as_yield = true;
            if let Some(capture_name) = enclosing_arguments_capture_name.clone() {
                self.ctx.rewrite_arguments_to_arguments_1 = true;
                self.ctx.arguments_capture_name = Some(capture_name);
            }
            if let Some(body_node) = self.arena.get(func.body)
                && let Some(block) = self.arena.get_block(body_node)
            {
                for &stmt in &block.statements.nodes {
                    self.write(" ");
                    self.emit(stmt);
                }
            }
            self.ctx.emit_await_as_yield = saved_yield;
            self.ctx.rewrite_arguments_to_arguments_1 = saved_args;
            self.ctx.arguments_capture_name = saved_arguments_capture_name;
            self.write(" })");
            return;
        }

        if !is_block {
            // Concise expression body: emit single-line unless parameter
            // lowering needs a generator prologue.
            self.write(" => ");
            self.write_helper("__awaiter");
            self.write("(");
            self.write(this_arg);
            self.write(", void 0, void 0, function* () {");
            let saved_yield = self.ctx.emit_await_as_yield;
            let saved_args = self.ctx.rewrite_arguments_to_arguments_1;
            let saved_arguments_capture_name = self.ctx.arguments_capture_name.clone();
            self.ctx.emit_await_as_yield = true;
            if let Some(capture_name) = enclosing_arguments_capture_name.clone() {
                self.ctx.rewrite_arguments_to_arguments_1 = true;
                self.ctx.arguments_capture_name = Some(capture_name);
            }
            if has_object_rest_param_prologue {
                self.write_line();
                self.increase_indent();
                self.emit_object_rest_param_prologue_entries(&object_rest_param_prologue);
                self.write("return ");
                self.emit_expression(func.body);
                self.write(";");
                self.write_line();
                self.decrease_indent();
            } else {
                self.write(" return ");
                self.emit_expression(func.body);
                self.write(";");
            }
            self.ctx.emit_await_as_yield = saved_yield;
            self.ctx.rewrite_arguments_to_arguments_1 = saved_args;
            self.ctx.arguments_capture_name = saved_arguments_capture_name;
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
        let hoist_start = body_has_for_await.then(|| self.begin_async_arrow_generator_hoists());

        // Emit body with await→yield substitution
        let saved_yield = self.ctx.emit_await_as_yield;
        let saved_args = self.ctx.rewrite_arguments_to_arguments_1;
        let saved_arguments_capture_name = self.ctx.arguments_capture_name.clone();
        self.ctx.emit_await_as_yield = true;
        if let Some(capture_name) = enclosing_arguments_capture_name {
            self.ctx.rewrite_arguments_to_arguments_1 = true;
            self.ctx.arguments_capture_name = Some(capture_name);
        }
        self.emit_object_rest_param_prologue_entries(&object_rest_param_prologue);

        // Block body: emit statements directly
        if let Some(body_node) = self.arena.get(func.body)
            && let Some(block) = self.arena.get_block(body_node)
        {
            let statements = block.statements.clone();
            if !self.emit_statement_list_with_using_scope(&statements) {
                self.emit_async_body_block_statements(func.body);
            }
        }
        if let Some(hoist_start) = hoist_start {
            self.insert_async_arrow_generator_hoists(hoist_start);
        }

        self.ctx.emit_await_as_yield = saved_yield;
        self.ctx.rewrite_arguments_to_arguments_1 = saved_args;
        self.ctx.arguments_capture_name = saved_arguments_capture_name;

        self.decrease_indent();
        self.write("})");
    }

    const fn begin_async_arrow_generator_hoists(&self) -> AsyncArrowGeneratorHoistStart {
        AsyncArrowGeneratorHoistStart {
            anchor: self.capture_hoist_anchor(),
            assignment_start: self.hoisted_assignment_temps.len(),
            for_of_start: self.hoisted_for_of_temps.len(),
            value_start: self.hoisted_assignment_value_temps.len(),
        }
    }

    fn insert_async_arrow_generator_hoists(&mut self, start: AsyncArrowGeneratorHoistStart) {
        let indent = self.writer.indent_string_at(start.anchor.indent_level);

        let mut ref_vars = Vec::new();
        ref_vars.extend(
            self.hoisted_assignment_temps
                .drain(start.assignment_start..),
        );
        ref_vars.extend(self.hoisted_for_of_temps.drain(start.for_of_start..));
        if !ref_vars.is_empty() {
            let var_decl = format!("{}var {};", indent, ref_vars.join(", "));
            self.writer
                .insert_line_at(start.anchor.byte_offset, start.anchor.line_no, &var_decl);
        }

        if !self.hoisted_assignment_value_temps[start.value_start..].is_empty() {
            let value_vars = self
                .hoisted_assignment_value_temps
                .drain(start.value_start..)
                .collect::<Vec<_>>();
            let var_decl = format!("{}var {};", indent, value_vars.join(", "));
            self.writer
                .insert_line_at(start.anchor.byte_offset, start.anchor.line_no, &var_decl);
        }
    }

    fn body_contains_for_await(&self, body_idx: NodeIndex) -> bool {
        let mut stack = vec![body_idx];
        while let Some(idx) = stack.pop() {
            if idx.is_none() {
                continue;
            }
            let Some(node) = self.arena.get(idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::FOR_OF_STATEMENT
                && self
                    .arena
                    .get_for_in_of(node)
                    .is_some_and(|for_in_of| for_in_of.await_modifier)
            {
                return true;
            }

            if idx != body_idx && self.is_function_like_hoist_boundary(node.kind) {
                continue;
            }

            stack.extend(self.arena.get_children(idx));
        }
        false
    }

    pub(in crate::emitter) const fn is_function_like_hoist_boundary(&self, kind: u16) -> bool {
        kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || kind == syntax_kind_ext::ARROW_FUNCTION
            || kind == syntax_kind_ext::METHOD_DECLARATION
            || kind == syntax_kind_ext::CONSTRUCTOR
            || kind == syntax_kind_ext::GET_ACCESSOR
            || kind == syntax_kind_ext::SET_ACCESSOR
    }

    fn emit_object_rest_param_prologue_entries(&mut self, entries: &[(String, NodeIndex)]) {
        for (temp_name, _) in entries {
            self.generated_temp_names.insert(temp_name.clone());
        }
        for (temp_name, pattern_idx) in entries {
            self.write("var ");
            self.emit_object_rest_var_decl(*pattern_idx, NodeIndex::NONE, Some(temp_name));
            self.write(";");
            self.write_line();
        }
        self.emit_pending_object_rest_param_defaults(false);
    }

    /// Issue #3758: lower `async (x = init()) => body` so the default
    /// initializer evaluates inside the generator function rather than at
    /// outer call time. tsc emits:
    ///
    /// ```js
    /// (...args_1) => __awaiter(this, [...args_1], void 0, function* (x = init()) { return x; })
    /// ```
    ///
    /// — synchronous throws from `init()` reject the returned promise
    /// instead of escaping the call site.
    fn emit_async_arrow_default_param_forwarding(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
        this_arg: &str,
    ) {
        self.push_temp_scope();
        let first_default_param_idx = func
            .parameters
            .nodes
            .iter()
            .position(|&param_idx| {
                self.arena
                    .get(param_idx)
                    .and_then(|param_node| self.arena.get_parameter(param_node))
                    .is_some_and(|param| param.initializer.is_some())
            })
            .unwrap_or(0);
        let leading_names = self.async_arrow_forwarded_parameter_names(
            &func.parameters.nodes[..first_default_param_idx],
        );
        let args_name = self.make_unique_name_from_base_in_temp_scope("args");
        let captures_arguments = !self.ctx.rewrite_arguments_to_arguments_1
            && contains_arguments_reference(self.arena, func.body);
        let existing_arguments_capture_name = self.ctx.arguments_capture_name.clone();
        let mut emits_arguments_capture = false;
        let arguments_capture_name = if captures_arguments {
            if existing_arguments_capture_name.is_some() {
                existing_arguments_capture_name
            } else {
                emits_arguments_capture = true;
                Some(loop {
                    self.ctx.arguments_capture_counter += 1;
                    let candidate = format!("arguments_{}", self.ctx.arguments_capture_counter);
                    if !self.file_identifiers.contains(&candidate) {
                        break candidate;
                    }
                })
            }
        } else {
            None
        };

        if emits_arguments_capture && let Some(capture_name) = arguments_capture_name.clone() {
            self.ctx.arguments_capture_name = Some(capture_name);
        }

        self.write("(");
        for (idx, name) in leading_names.iter().enumerate() {
            if idx > 0 {
                self.write(", ");
            }
            self.write(name);
        }
        if !leading_names.is_empty() {
            self.write(", ");
        }
        self.write("...");
        self.write(&args_name);
        self.write(") => ");
        if emits_arguments_capture && let Some(capture_name) = arguments_capture_name.as_deref() {
            self.write("{");
            self.write_line();
            self.increase_indent();
            self.write("var ");
            self.write(capture_name);
            self.write(" = arguments;");
            self.write_line();
            self.write("return ");
        }
        self.write_helper("__awaiter");
        self.write("(");
        self.write(this_arg);
        self.write(", [");
        for (idx, name) in leading_names.iter().enumerate() {
            if idx > 0 {
                self.write(", ");
            }
            self.write(name);
        }
        if !leading_names.is_empty() {
            self.write(", ");
        }
        self.write("...");
        self.write(&args_name);
        self.write("], void 0, function* (");
        self.emit_function_parameters_js(&func.parameters.nodes);
        self.write(") {");

        let body_node = self.arena.get(func.body);
        let is_block = body_node.is_some_and(|n| n.kind == syntax_kind_ext::BLOCK);
        let body_is_single_line = is_block
            && self
                .arena
                .get(func.body)
                .map(|n| self.is_single_line(n))
                .unwrap_or(false);
        let body_has_for_await = is_block && self.body_contains_for_await(func.body);

        let saved_yield = self.ctx.emit_await_as_yield;
        let saved_args = self.ctx.rewrite_arguments_to_arguments_1;
        let saved_arguments_capture_name = self.ctx.arguments_capture_name.clone();
        self.ctx.emit_await_as_yield = true;
        if let Some(capture_name) = arguments_capture_name.clone() {
            self.ctx.rewrite_arguments_to_arguments_1 = true;
            self.ctx.arguments_capture_name = Some(capture_name);
        }
        if is_block {
            if body_is_single_line && !body_has_for_await {
                if let Some(body_node) = self.arena.get(func.body)
                    && let Some(block) = self.arena.get_block(body_node)
                {
                    for &stmt in &block.statements.nodes {
                        self.write(" ");
                        self.emit(stmt);
                    }
                }
            } else {
                self.write_line();
                self.increase_indent();
                let hoist_start =
                    body_has_for_await.then(|| self.begin_async_arrow_generator_hoists());
                self.emit_async_body_block_statements(func.body);
                if let Some(hoist_start) = hoist_start {
                    self.insert_async_arrow_generator_hoists(hoist_start);
                }
                self.decrease_indent();
            }
        } else {
            self.write(" return ");
            self.emit_expression(func.body);
            self.write(";");
        }
        self.ctx.emit_await_as_yield = saved_yield;
        self.ctx.rewrite_arguments_to_arguments_1 = saved_args;
        if emits_arguments_capture {
            self.ctx.arguments_capture_name = arguments_capture_name.clone();
        } else {
            self.ctx.arguments_capture_name = saved_arguments_capture_name;
        }
        self.write(" })");
        if emits_arguments_capture {
            self.write(";");
            self.write_line();
            self.decrease_indent();
            self.write("}");
        }
        self.pop_temp_scope();
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
        let args_name = self.make_unique_name_from_base_in_temp_scope("args");

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
}
