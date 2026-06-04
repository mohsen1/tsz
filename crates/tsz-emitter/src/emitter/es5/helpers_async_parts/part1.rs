impl<'a> Printer<'a> {
    fn next_arguments_capture_name(&mut self) -> String {
        loop {
            self.ctx.arguments_capture_counter += 1;
            let candidate = format!("arguments_{}", self.ctx.arguments_capture_counter);
            if !self.file_identifiers.contains(&candidate) {
                return candidate;
            }
        }
    }

    fn es5_class_iife_expression_from_var(output: &str, class_name: &str) -> Option<String> {
        let prefix = format!("var {class_name} = ");
        let output = output.trim_end();
        let output = output.strip_suffix(';').unwrap_or(output);
        output.strip_prefix(&prefix).map(str::to_string)
    }

    fn write_multiline_fragment_preserving_indent(&mut self, text: &str) {
        let mut lines = text.lines();
        if let Some(first) = lines.next() {
            self.write(first);
        }
        for line in lines {
            self.write_line();
            if !line.is_empty() {
                self.write(line);
            }
        }
    }

    fn write_multiline_fragment_with_continuation_indent(
        &mut self,
        text: &str,
        continuation_indent_level: u32,
    ) {
        let indent_unit = self.writer.indent_unit_width() as usize;
        let indent_unit = if indent_unit == 0 { 4 } else { indent_unit };

        let mut lines = text.lines();
        if let Some(first) = lines.next() {
            self.write(first);
        }
        for line in lines {
            self.write_line();
            if !line.is_empty() {
                let leading = line.len() - line.trim_start_matches(' ').len();
                let original_level = (leading / indent_unit) as u32;
                let trimmed = &line[leading..];
                // Formula: output_level = (continuation - 1) + original_level
                // Naturally handles the `}())` closing (original_level=0 → continuation-1)
                // and all deeper lines by adding their nesting relative to the IIFE root.
                let output_level = continuation_indent_level.saturating_sub(1) + original_level;
                self.write_line_with_absolute_indent(output_level, trimmed);
            }
        }
    }

    fn write_line_with_absolute_indent(&mut self, indent_level: u32, text: &str) {
        let original_indent_level = self.writer.indent_level();
        let indent = " ".repeat((self.writer.indent_unit_width() * indent_level) as usize);
        self.writer.set_indent_level(0);
        self.writer.write_raw_text(&indent);
        self.write(text);
        self.writer.set_indent_level(original_indent_level);
    }

    fn class_expression_static_comma_needs_parens(&self, class_node: NodeIndex) -> bool {
        let mut current = class_node;
        loop {
            let Some(ext) = self.arena.get_extended(current) else {
                return true;
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return true;
            }
            let Some(parent) = self.arena.get(parent_idx) else {
                return true;
            };

            match parent.kind {
                syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    current = parent_idx;
                }
                syntax_kind_ext::RETURN_STATEMENT => return false,
                _ => return true,
            }
        }
    }

    fn current_statement_continuation_indent_level(&self) -> u32 {
        self.writer
            .indent_level()
            .max(self.writer.current_line_visual_indent_level())
            + 2
    }

    fn emit_es5_static_class_expression_comma(
        &mut self,
        class_node: NodeIndex,
        class_name: &str,
        class_iife_expr: &str,
        class_value_temp: Option<&str>,
        computed_init_exprs: &[IRNode],
        static_elements: &[Es5StaticClassExpressionElement],
        set_function_name: Option<&str>,
    ) {
        let needs_parens = self.class_expression_static_comma_needs_parens(class_node);
        let temp = class_value_temp.map_or_else(
            || {
                if self.class_expression_is_in_loop_body(class_node) {
                    let temp = self.make_class_static_temp_name(class_node);
                    self.block_scoped_private_temps.push(temp.clone());
                    temp
                } else {
                    self.make_class_static_temp_name_hoisted(class_node)
                }
            },
            str::to_string,
        );
        let continuation_indent_level = self.current_statement_continuation_indent_level();

        if needs_parens {
            self.write("(");
        }
        self.write(&temp);
        self.write(" = ");
        self.write_multiline_fragment_with_continuation_indent(
            class_iife_expr,
            continuation_indent_level,
        );

        for init_expr in computed_init_exprs {
            self.write(",");
            self.write_line();
            self.increase_indent();
            self.write(&self.render_es5_class_ir_comma_expression(init_expr));
            self.decrease_indent();
        }

        if let Some(name) = set_function_name {
            self.emit_class_expr_set_function_name_comma_item(&temp, name);
        }

        for element in static_elements {
            match element {
                Es5StaticClassExpressionElement::Field(field) => {
                    self.write(",");
                    self.write_line();
                    self.increase_indent();
                    if self.ctx.options.use_define_for_class_fields {
                        self.write("Object.defineProperty(");
                        self.write(&temp);
                        self.write(", ");
                        match &field.name_emit {
                            PropertyNameEmit::Dot(name) => {
                                self.write("\"");
                                self.write(name);
                                self.write("\"");
                            }
                            PropertyNameEmit::Bracket(name)
                            | PropertyNameEmit::BracketNumeric(name) => {
                                self.write(name);
                            }
                        }
                        self.write(", {");
                        self.write_line();
                        self.increase_indent();
                        self.write("enumerable: true,");
                        self.write_line();
                        self.write("configurable: true,");
                        self.write_line();
                        self.write("writable: true,");
                        self.write_line();
                        self.write("value: ");
                    } else {
                        self.write(&temp);
                        match &field.name_emit {
                            PropertyNameEmit::Dot(name) => {
                                self.write(".");
                                self.write(name);
                            }
                            PropertyNameEmit::Bracket(name)
                            | PropertyNameEmit::BracketNumeric(name) => {
                                self.write("[");
                                self.write(name);
                                self.write("]");
                            }
                        }
                        self.write(" = ");
                    }

                    let prev_self_alias = self.scoped_class_expression_self_alias.clone();
                    if !class_name.is_empty() && class_name != temp {
                        self.scoped_class_expression_self_alias = Some((
                            Arc::<str>::from(class_name),
                            Arc::<str>::from(temp.as_str()),
                        ));
                    }
                    let before = self.writer.len();
                    self.with_scoped_static_initializer_context_cleared(|this| {
                        this.emit_expression(field.initializer);
                    });
                    let after = self.writer.len();
                    self.scoped_class_expression_self_alias = prev_self_alias;

                    if !class_name.is_empty() && class_name != temp {
                        let full = self.writer.get_output().to_string();
                        let segment = &full[before..after];
                        let replaced = replace_identifier(segment, class_name, &temp);
                        if replaced != segment {
                            self.writer.truncate(before);
                            self.write(&replaced);
                        }
                    }
                    if self.ctx.options.use_define_for_class_fields {
                        self.write_line();
                        self.decrease_indent();
                        self.write("})");
                    }
                    self.decrease_indent();
                }
                Es5StaticClassExpressionElement::StaticBlock {
                    block,
                    saved_comment_idx,
                    ..
                } => {
                    self.write(",");
                    self.write_line();
                    self.increase_indent();
                    self.emit_static_block_iife_expression(*block, *saved_comment_idx);
                    self.decrease_indent();
                }
            }
        }

        self.write(",");
        self.write_line();
        self.increase_indent();
        self.write(&temp);
        if needs_parens {
            self.write(")");
        }
        self.decrease_indent();
    }

    fn render_es5_class_ir_comma_expression(&self, node: &IRNode) -> String {
        let expr = match node {
            IRNode::ExpressionStatement(inner) => inner.as_ref(),
            other => other,
        };
        let mut printer = IRPrinter::with_arena(self.arena);
        printer.set_transforms(self.transforms.clone());
        printer.set_target_es5(true);
        printer.set_remove_comments(self.ctx.options.remove_comments);
        printer.set_indent_level(self.writer.indent_level());
        if let Some(text) = self.source_text {
            printer.set_source_text(text);
        }
        if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
            printer.set_tslib_prefix(true);
            printer.set_tslib_import_binding(self.commonjs_tslib_import_binding.clone());
        }
        printer.emit(expr);
        printer.take_output()
    }

    fn emit_es5_static_class_expression_statements(
        &mut self,
        class_name: &str,
        static_elements: &[Es5StaticClassExpressionElement],
    ) {
        for element in static_elements {
            match element {
                Es5StaticClassExpressionElement::Field(field) => {
                    self.write(class_name);
                    match &field.name_emit {
                        PropertyNameEmit::Dot(name) => {
                            self.write(".");
                            self.write(name);
                        }
                        PropertyNameEmit::Bracket(name)
                        | PropertyNameEmit::BracketNumeric(name) => {
                            self.write("[");
                            self.write(name);
                            self.write("]");
                        }
                    }
                    self.write(" = ");
                    self.with_scoped_static_initializer_context_cleared(|this| {
                        this.emit_expression(field.initializer);
                    });
                    self.write(";");
                    self.write_line();
                }
                Es5StaticClassExpressionElement::StaticBlock {
                    block,
                    saved_comment_idx,
                    ..
                } => {
                    self.emit_static_block_iife_expression(*block, *saved_comment_idx);
                    self.write(";");
                    self.write_line();
                }
            }
        }
    }

    fn async_body_function_declarations(&self, body: NodeIndex) -> Vec<NodeIndex> {
        let Some(body_node) = self.arena.get(body) else {
            return Vec::new();
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return Vec::new();
        };

        block
            .statements
            .nodes
            .iter()
            .copied()
            .filter(|&stmt_idx| {
                self.arena
                    .get(stmt_idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::FUNCTION_DECLARATION)
            })
            .collect()
    }

    fn emit_async_hoisted_function_declarations(&mut self, hoisted_function_decls: &[NodeIndex]) {
        for &stmt in hoisted_function_decls {
            if let Some(stmt_node) = self.arena.get(stmt) {
                let actual_start = self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
                self.emit_comments_before_pos(actual_start);
            }
            self.emit(stmt);
            self.write_line();
        }
    }

    /// Emit an async function transformed to ES5 __awaiter/__generator pattern
    pub(in crate::emitter) fn emit_async_function_es5(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
        func_name: &str,
        this_expr: &str,
    ) {
        self.emit_async_function_es5_body(
            func_name,
            &func.parameters.nodes,
            func.body,
            this_expr,
            func.type_annotation,
        );
    }

    pub(in crate::emitter) fn skip_comments_for_async_lowered_body(&mut self, body: NodeIndex) {
        if let Some(body_node) = self.arena.get(body) {
            self.skip_comments_for_erased_node(body_node);
        }
    }

    pub(in crate::emitter) fn emit_async_function_es5_body(
        &mut self,
        func_name: &str,
        params: &[NodeIndex],
        body: NodeIndex,
        this_expr: &str,
        type_annotation: NodeIndex,
    ) {
        // For ES2015/ES2016 targets, use function* + yield pattern
        // For ES5, use function + __generator state machine pattern
        let use_native_generators = !self.ctx.target_es5;

        // Extract qualified promise constructor from return type annotation.
        // Only used for ES5 target; ES2015+ targets always emit `void 0`.
        let promise_ctor = if !use_native_generators {
            self.extract_awaiter_promise_constructor(type_annotation)
        } else {
            None
        };
        let params_have_top_level_await = params
            .iter()
            .copied()
            .any(|p| self.param_initializer_has_top_level_await(p));
        // For ES2015+, tsc moves parameters into the generator function when
        // ANY parameter has a default initializer or destructuring pattern.
        // The outer function forwards `arguments` to __awaiter. This ensures
        // parameter evaluation happens inside the generator context.
        let any_param_needs_forwarding =
            use_native_generators && self.async_params_need_generator_forwarding(params);
        let move_params_to_generator =
            use_native_generators && (params_have_top_level_await || any_param_needs_forwarding);
        let es5_await_param_recovery = !use_native_generators
            && params_have_top_level_await
            && emit_utils::block_is_empty(self.arena, body)
            && self.first_await_default_param_name(params).is_some();

        // function name(params) { ... } or function (params) { ... }
        if func_name.is_empty() {
            self.write("function (");
        } else {
            self.write("function ");
            self.write(func_name);
            self.write("(");
        }
        if use_native_generators {
            self.push_temp_scope();
            // ES2015: when a parameter initializer starts with `await`, match tsc
            // by moving parameters to the inner generator and forwarding `arguments`.
            if !move_params_to_generator {
                self.emit_function_parameters_js(params);
            } else {
                self.emit_async_outer_parameter_placeholders(params);
            }
        } else {
            if es5_await_param_recovery {
                self.write(") {");
                self.write_line();
                self.increase_indent();

                self.write("return ");
                self.write_helper("__awaiter");
                self.write("(");
                self.write(this_expr);
                self.write(", arguments, ");
                self.write_awaiter_promise_arg(&promise_ctor);
                self.write(", function (");
                self.emit_function_parameter_names_only(params);
                self.emit_recovered_async_await_arrow_parameter(params);
                self.write(") {");
                self.write_line();
                self.increase_indent();

                if let Some(param_name) = self.first_await_default_param_name(params) {
                    self.write("if (");
                    self.write(&param_name);
                    self.write(" === void 0) { ");
                    self.write(&param_name);
                    self.write(" = _a.sent(); }");
                    self.write_line();
                }

                self.write("return ");
                self.write_helper("__generator");
                self.write("(this, function (_a) {");
                self.write_line();
                self.increase_indent();
                self.write("switch (_a.label) {");
                self.write_line();
                self.increase_indent();
                self.write("case 0: return [4 /*yield*/, ];");
                self.write_line();
                self.write("case 1: return [2 /*return*/];");
                self.write_line();
                self.decrease_indent();
                self.write("}");
                self.write_line();
                self.decrease_indent();
                self.write("});");
                self.write_line();
                self.decrease_indent();
                self.write("});");
                self.write_line();
                self.decrease_indent();
                self.write("}");
                self.skip_comments_for_async_lowered_body(body);
                return;
            }

            // ES5: apply destructuring/default transforms
            let param_transforms = self.emit_function_parameters_es5(params);
            self.write(") {");
            self.write_line();
            self.increase_indent();
            self.emit_param_prologue(&param_transforms);

            // Capture `arguments` if the body references it.
            // tsc emits: var arguments_1 = arguments;
            // placed before return __awaiter(...) so the generator closure
            // can access the original arguments object.
            let body_captures_arguments =
                tsz_parser::syntax::transform_utils::contains_arguments_reference(self.arena, body);
            if body_captures_arguments {
                self.write("var arguments_1 = arguments;");
                self.write_line();
            }

            // ES5 path: __awaiter + __generator state machine
            let mut async_emitter = crate::transforms::async_es5::AsyncES5Emitter::new(self.arena);
            async_emitter.set_system_import_meta(self.in_system_execute_body);
            async_emitter.set_module_kind(self.ctx.outer_module_kind());
            async_emitter.set_target_es5(self.ctx.target_es5);
            async_emitter.set_dynamic_import_promise_counter(self.next_dynamic_import_promise_id);
            async_emitter
                .set_catch_binding_ordinals(std::mem::take(&mut self.next_catch_binding_ordinals));
            async_emitter.set_downlevel_iteration(self.ctx.options.downlevel_iteration);
            // The generator body is nested inside `function () { ... }` in the __awaiter
            // callback, so render it at one extra indent level (matching tsc multi-line format).
            async_emitter.set_indent_level(self.writer.indent_level() + 1);
            if let Some(text) = self.source_text_for_map() {
                async_emitter.set_source_map_context(text, self.writer.current_source_index());
            }
            async_emitter.set_lexical_this(this_expr != "this");
            if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
                async_emitter.set_tslib_prefix(true);
                async_emitter.set_tslib_import_binding(self.commonjs_tslib_import_binding.clone());
            }
            let blocked_disposable_names = self.blocked_disposable_names_for_transform();
            async_emitter
                .set_disposable_env_context(self.next_disposable_env_id, blocked_disposable_names);
            if let Some((_, alias)) = &self.scoped_class_expression_self_alias {
                async_emitter
                    .set_outer_reserved_for_generator_state(vec![alias.as_ref().to_string()]);
            }

            let body_has_await = async_emitter.body_contains_await(body);
            let body_is_single_line = self.arena.get(body).is_some_and(|n| self.is_single_line(n));
            let hoisted_function_decls = self.async_body_function_declarations(body);
            let hoist_function_decls_only =
                !body_has_await && self.block_has_only_function_decls(body);
            if hoist_function_decls_only {
                self.write("return ");
                self.write_helper("__awaiter");
                self.write("(");
                self.write(this_expr);
                self.write(", void 0, ");
                self.write_awaiter_promise_arg(&promise_ctor);
                self.write(", function () {");
                self.write_line();
                self.increase_indent();

                if let Some(body_node) = self.arena.get(body)
                    && let Some(block) = self.arena.get_block(body_node)
                {
                    for &stmt in &block.statements.nodes {
                        if let Some(stmt_node) = self.arena.get(stmt) {
                            let actual_start =
                                self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
                            self.emit_comments_before_pos(actual_start);
                        }
                        self.emit(stmt);
                        self.write_line();
                    }
                }

                self.write("return ");
                self.write_helper("__generator");
                self.write("(this, function (_a) {");
                self.write_line();
                self.increase_indent();
                self.write("return [2 /*return*/];");
                self.write_line();
                self.decrease_indent();
                self.write("});");
                self.decrease_indent();
                self.write_line();
                self.write("});");
                self.write_line();
                self.decrease_indent();
                self.write("}");
                self.pop_temp_scope();
                self.skip_comments_for_async_lowered_body(body);
                return;
            }
            if !body_has_await
                && let Some(body_node) = self.arena.get(body)
                && let Some(block) = self.arena.get_block(body_node)
                && let Some(&first_stmt_idx) = block.statements.nodes.first()
                && let Some(first_stmt_node) = self.arena.get(first_stmt_idx)
                && first_stmt_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            {
                let actual_start =
                    self.skip_trivia_forward(first_stmt_node.pos, first_stmt_node.end);
                while self.comment_emit_idx < self.all_comments.len()
                    && self.all_comments[self.comment_emit_idx].end <= actual_start
                {
                    self.comment_emit_idx += 1;
                }
            }

            let (generator_body, hoisted_var_groups, directive_prologue, _) = async_emitter
                .emit_generator_body_and_hoisted_vars_skipping(
                    body,
                    body_has_await,
                    &hoisted_function_decls,
                );
            let generator_mappings = async_emitter.take_mappings();
            self.next_disposable_env_id = async_emitter.disposable_env_counter();
            self.next_dynamic_import_promise_id = async_emitter.dynamic_import_promise_counter();
            self.next_catch_binding_ordinals = async_emitter.take_catch_binding_ordinals();
            for generated_name in async_emitter.take_generated_disposable_env_names() {
                self.generated_temp_names.insert(generated_name);
            }

            // Write with surrounding __awaiter wrapper
            self.write("return ");
            self.write_helper("__awaiter");
            self.write("(");
            self.write(this_expr);
            if hoisted_var_groups.is_empty() {
                let can_inline_wrapper = body_is_single_line
                    && hoisted_function_decls.is_empty()
                    && directive_prologue.is_empty()
                    && !(this_expr != "this" && generator_body.contains("return _this"))
                    && generator_mappings.is_empty();
                if can_inline_wrapper {
                    self.write(", void 0, ");
                    self.write_awaiter_promise_arg(&promise_ctor);
                    self.write(", function () { ");
                    self.write(&Self::inline_async_generator_body(&generator_body));
                    self.write(" });");
                    self.write_line();
                    self.decrease_indent();
                    self.write("}");
                    self.skip_comments_for_async_lowered_body(body);
                    // emit_function_parameters_es5() pushed a temp scope; the
                    // other early-return paths in this function (and the
                    // multi-line/normal exit below) all call pop_temp_scope.
                    // Forgetting it here would leak temp-name state across
                    // functions and corrupt subsequent emissions.
                    self.pop_temp_scope();
                    return;
                }

                // Multi-line format (matches tsc):
                // return __awaiter(this, void 0, void 0, function () {
                //     return __generator(this, function (_a) {
                //         ...
                //     });
                // });
                self.write(", void 0, ");
                self.write_awaiter_promise_arg(&promise_ctor);
                self.write(", function () {");
                self.write_line();
                self.increase_indent();
                for directive in &directive_prologue {
                    self.write("\"");
                    self.write(directive);
                    self.write("\";");
                    self.write_line();
                }
                self.emit_async_hoisted_function_declarations(&hoisted_function_decls);
                if this_expr != "this" && generator_body.contains("return _this") {
                    self.write("var _this = this;");
                    self.write_line();
                }
                if !generator_mappings.is_empty() && self.writer.has_source_map() {
                    self.writer.write("");
                    let base_line = self.writer.current_line();
                    let base_column = self.writer.current_column();
                    self.writer
                        .add_offset_mappings(base_line, base_column, &generator_mappings);
                    self.writer.write(&generator_body);
                } else {
                    self.write(&generator_body);
                }
                self.decrease_indent();
                self.write_line();
                self.write("});");
            } else {
                // Multi-line format with hoisted vars
                self.write(", void 0, ");
                self.write_awaiter_promise_arg(&promise_ctor);
                self.write(", function () {");
                self.write_line();
                self.increase_indent();
                for directive in &directive_prologue {
                    self.write("\"");
                    self.write(directive);
                    self.write("\";");
                    self.write_line();
                }
                self.emit_async_hoisted_function_declarations(&hoisted_function_decls);
                for group in &hoisted_var_groups {
                    self.write("var ");
                    for (i, var_name) in group.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }
                        self.write(var_name);
                    }
                    self.write(";");
                    self.write_line();
                }
                if this_expr != "this" && generator_body.contains("return _this") {
                    self.write("var _this = this;");
                    self.write_line();
                }
                if !generator_mappings.is_empty() && self.writer.has_source_map() {
                    self.writer.write("");
                    let base_line = self.writer.current_line();
                    let base_column = self.writer.current_column();
                    self.writer
                        .add_offset_mappings(base_line, base_column, &generator_mappings);
                    self.writer.write(&generator_body);
                } else {
                    self.write(&generator_body);
                }
                self.decrease_indent();
                self.write_line();
                self.write("});");
            }
            self.write_line();
            self.decrease_indent();
            self.write("}");
            self.pop_temp_scope();
            self.skip_comments_for_async_lowered_body(body);
            return;
        }

        // ES2015 path: __awaiter + function* with yield

        // Check if the body is empty and was single-line in source for compact formatting
        let body_is_single_line = self.arena.get(body).is_some_and(|n| self.is_single_line(n));
        let body_is_empty_single_line = self
            .arena
            .get(body)
            .and_then(|n| {
                let block = self.arena.get_block(n)?;
                if block.statements.nodes.is_empty() {
                    Some(self.is_single_line(n))
                } else {
                    None
                }
            })
            .unwrap_or(false);

        // Check if the body references `arguments`. If so, capture it before
        // entering the generator: `var arguments_1 = arguments;`
        let body_captures_arguments =
            tsz_parser::syntax::transform_utils::contains_arguments_reference(self.arena, body);
        let async_parameter_names = self.async_generator_parameter_binding_names(params);
        let async_shadowed_var_names =
            self.async_generator_shadowed_var_names(body, &async_parameter_names);

        self.write(") {");
        self.write_line();
        self.increase_indent();

        let arguments_capture_name = if body_captures_arguments {
            Some(self.next_arguments_capture_name())
        } else {
            None
        };

        // Emit captured `arguments` before __awaiter for ES2015 path.
        if body_captures_arguments {
            self.write("var ");
            self.write(arguments_capture_name.as_deref().unwrap_or("arguments_1"));
            self.write(" = arguments;");
            self.write_line();
        }

        // return __awaiter(this, void 0, void 0, function* () {
        self.write("return ");
        self.write_helper("__awaiter");
        self.write("(");
        self.write(this_expr);
        if move_params_to_generator {
            self.write(", arguments, ");
            self.write_awaiter_promise_arg(&promise_ctor);
            self.write(", function* (");
            let saved = self.ctx.emit_await_as_yield;
            self.ctx.emit_await_as_yield = true;
            self.emit_function_parameters_js(params);
            self.emit_recovered_async_await_arrow_parameter(params);
            self.ctx.emit_await_as_yield = saved;
            if body_is_empty_single_line {
                self.write(") { });");
            } else {
                self.write(") {");
            }
        } else if body_is_empty_single_line {
            self.write(", void 0, ");
            self.write_awaiter_promise_arg(&promise_ctor);
            self.write(", function* () { });");
        } else {
            self.write(", void 0, ");
            self.write_awaiter_promise_arg(&promise_ctor);
            self.write(", function* () {");
        }

        if body_is_empty_single_line {
            self.write_line();
            self.decrease_indent();
            self.write("}");
            self.pop_temp_scope();
            return;
        }

        if body_is_single_line && async_shadowed_var_names.is_empty() {
            let saved_yield = self.ctx.emit_await_as_yield;
            let saved_args = self.ctx.rewrite_arguments_to_arguments_1;
            let saved_arguments_capture_name = self.ctx.arguments_capture_name.clone();
            self.ctx.emit_await_as_yield = true;
            if body_captures_arguments {
                self.ctx.rewrite_arguments_to_arguments_1 = true;
                self.ctx.arguments_capture_name = arguments_capture_name;
            }
            self.function_scope_depth += 1;
            if let Some(body_node) = self.arena.get(body)
                && let Some(block) = self.arena.get_block(body_node)
            {
                for &stmt in &block.statements.nodes {
                    self.write(" ");
                    self.emit(stmt);
                }
            }
            self.function_scope_depth -= 1;
            self.ctx.emit_await_as_yield = saved_yield;
            self.ctx.rewrite_arguments_to_arguments_1 = saved_args;
            self.ctx.arguments_capture_name = saved_arguments_capture_name;
            self.write(" });");
            self.write_line();
            self.decrease_indent();
            self.write("}");
            self.pop_temp_scope();
            return;
        }

        self.write_line();
        self.increase_indent();
        if !async_shadowed_var_names.is_empty() {
            self.write("var ");
            self.write(&async_shadowed_var_names.join(", "));
            self.write(";");
            self.write_line();
        }
        // Anchor the hoist insertion at the surrounding-scope indent - the
        // body emit below may open a `using` try wrapper that raises the
        // live `indent_level` before the hoist line is inserted.
        let generator_hoist_anchor = self.capture_hoist_anchor();
        let hoisted_assignment_start = self.hoisted_assignment_temps.len();
        let hoisted_for_of_start = self.hoisted_for_of_temps.len();
        let hoisted_value_start = self.hoisted_assignment_value_temps.len();

        // Emit function body with await→yield substitution
        let saved_yield = self.ctx.emit_await_as_yield;
        let saved_args = self.ctx.rewrite_arguments_to_arguments_1;
        let saved_arguments_capture_name = self.ctx.arguments_capture_name.clone();
        let saved_shadowed_parameter_names =
            std::mem::take(&mut self.ctx.async_generator_shadowed_parameter_names);
        self.ctx.emit_await_as_yield = true;
        self.ctx.async_generator_shadowed_parameter_names = if async_shadowed_var_names.is_empty() {
            Vec::new()
        } else {
            async_parameter_names
        };
        if body_captures_arguments {
            self.ctx.rewrite_arguments_to_arguments_1 = true;
            self.ctx.arguments_capture_name = arguments_capture_name;
        }
        self.function_scope_depth += 1;
        // Emit the block body's statements directly
        if let Some(body_node) = self.arena.get(body)
            && let Some(block) = self.arena.get_block(body_node)
        {
            let statements = block.statements.clone();
            if !self.emit_statement_list_with_using_scope(&statements) {
                for &stmt in &statements.nodes {
                    if let Some(stmt_node) = self.arena.get(stmt) {
                        let actual_start = self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
                        self.emit_comments_before_pos(actual_start);
                    }
                    let before_emit_len = self.writer.len();
                    self.emit(stmt);
                    if self.writer.len() > before_emit_len && !self.writer.is_at_line_start() {
                        self.write_line();
                    }
                }
            }
        }
        let indent = self
            .writer
            .indent_string_at(generator_hoist_anchor.indent_level);
        let mut ref_vars = Vec::new();
        ref_vars.extend(
            self.hoisted_assignment_temps
                .drain(hoisted_assignment_start..),
        );
        ref_vars.extend(self.hoisted_for_of_temps.drain(hoisted_for_of_start..));
        if !ref_vars.is_empty() {
            let var_decl = format!("{}var {};", indent, ref_vars.join(", "));
            self.writer.insert_line_at(
                generator_hoist_anchor.byte_offset,
                generator_hoist_anchor.line_no,
                &var_decl,
            );
        }
        if !self.hoisted_assignment_value_temps[hoisted_value_start..].is_empty() {
            let value_vars = self
                .hoisted_assignment_value_temps
                .drain(hoisted_value_start..)
                .collect::<Vec<_>>();
            let var_decl = format!("{}var {};", indent, value_vars.join(", "));
            self.writer.insert_line_at(
                generator_hoist_anchor.byte_offset,
                generator_hoist_anchor.line_no,
                &var_decl,
            );
        }
        self.function_scope_depth -= 1;
        self.ctx.emit_await_as_yield = saved_yield;
        self.ctx.rewrite_arguments_to_arguments_1 = saved_args;
        self.ctx.arguments_capture_name = saved_arguments_capture_name;
        self.ctx.async_generator_shadowed_parameter_names = saved_shadowed_parameter_names;

        self.decrease_indent();
        self.write("});");
        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.pop_temp_scope();
    }

    pub(in crate::emitter) fn emit_generator_function_es5(&mut self, function_node: NodeIndex) {
        use crate::transforms::async_es5_ir::AsyncES5Transformer;
        use crate::transforms::ir_printer::IRPrinter;
        let mut transformer = AsyncES5Transformer::new(self.arena);
        if let Some(text) = self.source_text {
            transformer.set_source_text(text);
        }
        transformer.set_module_kind(self.ctx.outer_module_kind());
        transformer.set_target_es5(self.ctx.target_es5);
        transformer.set_downlevel_iteration(self.ctx.options.downlevel_iteration);
        let blocked_disposable_names = self.blocked_disposable_names_for_transform();
        transformer
            .set_disposable_env_context(self.next_disposable_env_id, blocked_disposable_names);
        let ir = transformer.transform_generator_function(function_node);
        self.next_disposable_env_id = transformer.disposable_env_counter();
        for generated_name in transformer.take_generated_disposable_env_names() {
            self.generated_temp_names.insert(generated_name);
        }
        let mut printer = IRPrinter::with_arena(self.arena);
        printer.set_transforms(self.transforms.clone());
        if let Some(text) = self.source_text {
            printer.set_source_text(text);
        }
        printer.set_indent_level(self.writer.indent_level());
        if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
            printer.set_tslib_prefix(true);
            printer.set_tslib_import_binding(self.commonjs_tslib_import_binding.clone());
        }
        printer.emit(&ir);
        self.write(&printer.take_output());
        if let Some(node) = self.arena.get(function_node) {
            while self.comment_emit_idx < self.all_comments.len()
                && self.all_comments[self.comment_emit_idx].end <= node.end
            {
                self.comment_emit_idx += 1;
            }
        }
    }

    fn block_has_only_function_decls(&self, body: NodeIndex) -> bool {
        let Some(body_node) = self.arena.get(body) else {
            return false;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return false;
        };
        if block.statements.nodes.is_empty() {
            return false;
        }
        block.statements.nodes.iter().all(|&stmt_idx| {
            self.arena
                .get(stmt_idx)
                .is_some_and(|stmt_node| stmt_node.kind == syntax_kind_ext::FUNCTION_DECLARATION)
        })
    }

    pub(in crate::emitter) fn param_initializer_has_top_level_await(
        &self,
        param_idx: NodeIndex,
    ) -> bool {
        emit_utils::param_initializer_has_top_level_await(self.arena, param_idx)
    }

    fn first_await_default_param_name(&self, params: &[NodeIndex]) -> Option<String> {
        emit_utils::first_await_default_param_name(self.arena, params)
    }

    /// Extract a qualified promise constructor from a function's return type annotation.
    pub(in crate::emitter) fn extract_awaiter_promise_constructor(
        &self,
        type_annotation: NodeIndex,
    ) -> Option<String> {
        use tsz_parser::parser::syntax_kind_ext;
        let type_node = self.arena.get(type_annotation)?;
        if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = self.arena.get_type_ref(type_node)?;
        let type_name_node = self.arena.get(type_ref.type_name)?;
        if type_name_node.kind == syntax_kind_ext::QUALIFIED_NAME {
            Some(self.qualified_name_to_expr(type_ref.type_name))
        } else if type_name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            let name = emit_utils::identifier_text_or_empty(self.arena, type_ref.type_name);
            if name.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
                && !matches!(name.as_str(), "Promise" | "PromiseLike")
                && !self.is_namespace_import_binding_name(&name)
                && !self.is_type_only_declaration_name(&name)
            {
                self.commonjs_named_import_substitutions
                    .get(&name)
                    .cloned()
                    .or(Some(name))
            } else {
                None
            }
        } else {
            None
        }
    }

    pub(in crate::emitter) fn is_type_only_declaration_name(&self, name: &str) -> bool {
        if self.ctx.module_state.value_declaration_names.contains(name) {
            return false;
        }

        self.arena.nodes.iter().any(|node| {
            if node.kind == tsz_parser::parser::syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                self.arena.get_type_alias(node).is_some_and(|alias| {
                    emit_utils::identifier_text_or_empty(self.arena, alias.name) == name
                })
            } else if node.kind == tsz_parser::parser::syntax_kind_ext::INTERFACE_DECLARATION {
                self.arena.get_interface(node).is_some_and(|interface| {
                    emit_utils::identifier_text_or_empty(self.arena, interface.name) == name
                })
            } else {
                false
            }
        })
    }

    /// Convert a qualified name or identifier AST node to a dotted JS expression string.
    fn qualified_name_to_expr(&self, idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(idx) else {
            return String::new();
        };
        if node.kind == tsz_parser::parser::syntax_kind_ext::QUALIFIED_NAME
            && let Some(qn) = self.arena.get_qualified_name(node)
        {
            let left = self.qualified_name_to_expr(qn.left);
            let right = emit_utils::identifier_text_or_empty(self.arena, qn.right);
            return format!("{left}.{right}");
        }
        emit_utils::identifier_text_or_empty(self.arena, idx)
    }

    /// Write the third argument for `__awaiter`: either the qualified promise constructor
    /// or `void 0` (default).
    pub(in crate::emitter) fn write_awaiter_promise_arg(&mut self, promise_ctor: &Option<String>) {
        if let Some(ctor) = promise_ctor {
            self.write(ctor);
        } else {
            self.write("void 0");
        }
    }

    fn inline_async_generator_body(generator_body: &str) -> String {
        let mut lines = generator_body.lines();
        let Some(first_line) = lines.next() else {
            return String::new();
        };

        let following_strip = 4;
        let mut output = String::from(first_line.trim_start());
        for line in lines {
            output.push('\n');
            output.push_str(line.get(following_strip..).unwrap_or(line).trim_end());
        }
        output
    }

    pub(in crate::emitter) fn emit_function_parameter_names_only(&mut self, params: &[NodeIndex]) {
        let mut first = true;
        for &param_idx in params {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            if !first {
                self.write(", ");
            }
            first = false;
            if param.dot_dot_dot_token {
                self.write("...");
            }
            self.emit(param.name);
        }
    }

    pub(in crate::emitter) fn emit_async_outer_parameter_placeholders(
        &mut self,
        params: &[NodeIndex],
    ) {
        let mut first = true;
        for &param_idx in params {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            if param.dot_dot_dot_token || param.initializer.is_some() {
                break;
            }
            let Some(name_node) = self.arena.get(param.name) else {
                continue;
            };
            if name_node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16 {
                continue;
            }
            if name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                && let Some(text) = self.source_text
                && let Ok(name_text) =
                    crate::safe_slice::slice(text, name_node.pos as usize, name_node.end as usize)
                && name_text.trim() == "this"
            {
                continue;
            }

            let placeholder = if self.is_binding_pattern(param.name) {
                self.get_temp_var_name()
            } else {
                let name = emit_utils::identifier_text_or_empty(self.arena, param.name);
                if name.is_empty() {
                    continue;
                }
                self.make_unique_name_from_base_in_temp_scope(&name)
            };

            if !first {
                self.write(", ");
            }
            first = false;
            self.write(&placeholder);
        }
    }

    pub(in crate::emitter) fn emit_function_parameters_es5(
        &mut self,
        params: &[NodeIndex],
    ) -> ParamTransformPlan {
        // Push a fresh temp scope for this function.
        // Each function has its own temp naming starting from _a.
        // Caller MUST call pop_temp_scope() after emitting the function body.
        self.push_temp_scope();

        let mut plan = ParamTransformPlan::default();
        let mut first = true;

        for (index, &param_idx) in params.iter().enumerate() {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };

            if param.dot_dot_dot_token {
                let rest_target = param.name;
                let rest_is_pattern = self.is_binding_pattern(rest_target);
                let rest_name = if rest_is_pattern {
                    self.get_temp_var_name()
                } else {
                    emit_utils::identifier_text_or_empty(self.arena, rest_target)
                };

                if !rest_name.is_empty() {
                    plan.rest = Some(RestParamTransform {
                        name: rest_name,
                        pattern: rest_is_pattern.then_some(rest_target),
                        index,
                    });
                }
                break;
            }

            if !first {
                self.write(", ");
            }
            first = false;

            // Emit leading comments before the parameter.
            self.emit_comments_before_pos(param_node.pos);

            if self.is_binding_pattern(param.name) {
                let temp_name = self.get_temp_var_name();
                self.write(&temp_name);
                plan.params.push(ParamTransform {
                    name: temp_name,
                    pattern: Some(param.name),
                    initializer: if param.initializer.is_none() {
                        None
                    } else {
                        Some(param.initializer)
                    },
                });
            } else {
                self.emit(param.name);
                if param.initializer.is_some() {
                    let name = emit_utils::identifier_text_or_empty(self.arena, param.name);
                    if !name.is_empty() {
                        plan.params.push(ParamTransform {
                            name,
                            pattern: None,
                            initializer: Some(param.initializer),
                        });
                    }
                }
            }
        }

        plan
    }
}
