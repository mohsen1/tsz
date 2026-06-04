impl<'a> Printer<'a> {
    /// Emit an ES5-compatible class expression by wrapping the class IIFE in an expression.
    pub(in crate::emitter) fn emit_class_expression_es5_inner(&mut self, class_node: NodeIndex) {
        let Some(node) = self.arena.get(class_node) else {
            return;
        };
        let Some(class_data) = self.arena.get_class(node) else {
            return;
        };

        let static_elements = self.es5_static_class_expression_elements(class_data);

        let mut es5_emitter = ClassES5Emitter::new(self.arena);
        es5_emitter.set_temp_var_counter(self.ctx.destructuring_state.temp_var_counter);
        es5_emitter
            .set_async_generator_inner_name_counts(self.async_generator_inner_name_counts.clone());
        self.configure_es5_class_emitter_disposable_context(&mut es5_emitter);
        es5_emitter.set_indent_level(0);
        // Pass transform directives to the ClassES5Emitter
        es5_emitter.set_transforms(self.transforms.clone());
        es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
        es5_emitter.set_printer_options(self.ctx.options.clone());
        es5_emitter.set_module_kind(self.ctx.outer_module_kind());
        if let Some(text) = self.source_text_for_map() {
            if self.writer.has_source_map() {
                es5_emitter.set_source_map_context(text, self.writer.current_source_index());
            } else {
                es5_emitter.set_source_text(text);
            }
        }
        if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
            es5_emitter.set_tslib_prefix(true);
            es5_emitter.set_tslib_import_binding(self.commonjs_tslib_import_binding.clone());
        }
        es5_emitter.set_use_define_for_class_fields(self.ctx.options.use_define_for_class_fields);
        self.configure_nested_es5_class_aliases(&mut es5_emitter);
        if self.es5_class_expression_extends_this_captured {
            es5_emitter.set_extends_this_captured(true);
        }
        if self.ctx.target_es5
            && !self.ctx.options.legacy_decorators
            && self.can_render_simple_tc39_decorated_class_es5(node)
        {
            let decorator_exprs = self
                .collect_class_decorators(&class_data.modifiers)
                .into_iter()
                .filter_map(|decorator_idx| {
                    let decorator_node = self.arena.get(decorator_idx)?;
                    let decorator = self.arena.get_decorator(decorator_node)?;
                    let before_len = self.writer.len();
                    self.emit_expression(decorator.expression);
                    let after_len = self.writer.len();
                    let full_output = self.writer.get_output().to_string();
                    let emitted = full_output[before_len..after_len].trim().to_string();
                    self.writer.truncate(before_len);
                    Some(emitted)
                })
                .collect::<Vec<_>>();
            let binding_name = self
                .get_identifier_text_opt(class_data.name)
                .or_else(|| self.resolve_class_expr_binding_name(class_node))
                .unwrap_or_else(|| self.next_tc39_anonymous_class_name());
            let inner_name = self.next_tc39_anonymous_class_name();

            es5_emitter.set_indent_level(self.writer.indent_level() + 1);
            es5_emitter.set_skip_static_members(true);
            es5_emitter.set_tc39_decorators(true);
            es5_emitter.set_tc39_wrap_output(false);
            let inner_output = es5_emitter
                .emit_class_with_name(class_node, &inner_name)
                .trim_end_matches('\n')
                .to_string();
            self.sync_es5_class_emitter_state(&mut es5_emitter);
            if let Some(output) = es5_emitter.wrap_tc39_es5_class_decorated_expression_output(
                class_node,
                &inner_name,
                &binding_name,
                &binding_name,
                &inner_output,
                &decorator_exprs,
            ) {
                self.write_multiline_fragment_preserving_indent(&output);
                return;
            }
        }
        let class_expr_set_function_name = if class_data.name.is_none() {
            self.resolve_class_expr_binding_name(class_node)
        } else {
            None
        };
        let defer_static_block_only_tail = self.defer_class_static_blocks
            && !static_elements.is_empty()
            && static_elements.iter().all(|element| {
                matches!(element, Es5StaticClassExpressionElement::StaticBlock { .. })
            });
        let use_static_comma = !static_elements.is_empty()
            && !self.ctx.options.use_define_for_class_fields
            && !defer_static_block_only_tail;
        let computed_instance_static_comma = self
            .es5_class_expression_has_computed_instance_fields(class_data)
            && self.es5_class_expression_has_static_runtime_elements(class_data);
        if use_static_comma || defer_static_block_only_tail || computed_instance_static_comma {
            es5_emitter.set_skip_static_members(true);
        }

        if self.es5_class_expression_has_computed_instance_fields(class_data) {
            let class_emit_name = if class_data.name.is_some() {
                let candidate = emit_utils::identifier_text_or_empty(self.arena, class_data.name);
                if candidate.is_empty() || !is_valid_identifier_name(&candidate) {
                    self.get_class_expression_name(class_node)
                        .unwrap_or_else(|| self.get_temp_var_name())
                } else {
                    candidate
                }
            } else {
                self.make_unique_name_from_base("class")
            };
            let (iife_expr, computed_decls, computed_init_exprs) =
                es5_emitter.emit_class_as_iife_expr(class_node, &class_emit_name);
            self.sync_es5_class_emitter_state(&mut es5_emitter);
            let _ = es5_emitter.take_mappings();

            let in_loop = self.class_expression_is_in_loop_body(class_node);
            for decl in &computed_decls {
                if in_loop {
                    self.block_scoped_private_temps.push(decl.clone());
                } else {
                    self.hoisted_assignment_temps.push(decl.clone());
                }
            }
            let class_temp = if in_loop {
                let t = self.make_class_static_temp_name(class_node);
                self.block_scoped_private_temps.push(t.clone());
                t
            } else {
                self.make_class_static_temp_name_hoisted(class_node)
            };

            let computed_static_elements = self
                .es5_static_class_expression_elements_with_computed_temps(
                    class_data,
                    &computed_decls,
                );
            let comma_static_elements = if computed_instance_static_comma {
                computed_static_elements.as_slice()
            } else if use_static_comma {
                static_elements.as_slice()
            } else {
                &[]
            };
            let comma_set_function_name = if use_static_comma || computed_instance_static_comma {
                class_expr_set_function_name.as_deref()
            } else {
                None
            };
            self.emit_es5_static_class_expression_comma(
                class_node,
                &class_emit_name,
                &iife_expr,
                Some(&class_temp),
                &computed_init_exprs,
                comma_static_elements,
                comma_set_function_name,
            );
            if defer_static_block_only_tail {
                self.deferred_class_static_blocks
                    .extend(static_elements.iter().filter_map(|element| match element {
                        Es5StaticClassExpressionElement::StaticBlock {
                            block,
                            saved_comment_idx,
                            ..
                        } => Some((*block, *saved_comment_idx)),
                        Es5StaticClassExpressionElement::Field(_) => None,
                    }));
            }
            return;
        }

        let (class_name, es5_output) = if class_data.name.is_some() {
            let candidate = emit_utils::identifier_text_or_empty(self.arena, class_data.name);
            if candidate.is_empty() || !is_valid_identifier_name(&candidate) {
                let temp_name = self
                    .get_class_expression_name(class_node)
                    .unwrap_or_else(|| self.get_temp_var_name());
                let output = es5_emitter.emit_class_with_name(class_node, &temp_name);
                (temp_name, output)
            } else {
                let output = es5_emitter.emit_class(class_node);
                (candidate, output)
            }
        } else if use_static_comma || self.es5_class_expression_has_instance_fields(class_data) {
            let temp_name = self.make_unique_name_from_base("class");
            let output = es5_emitter.emit_class_with_name(class_node, &temp_name);
            (temp_name, output)
        } else {
            let temp_name = self
                .get_class_expression_name(class_node)
                .unwrap_or_else(|| self.make_unique_name_from_base("class"));
            let output = es5_emitter.emit_class_with_name(class_node, &temp_name);
            (temp_name, output)
        };
        self.sync_es5_class_emitter_state(&mut es5_emitter);
        let es5_mappings = es5_emitter.take_mappings();

        if use_static_comma
            && let Some(class_iife_expr) =
                Self::es5_class_iife_expression_from_var(&es5_output, &class_name)
        {
            self.emit_es5_static_class_expression_comma(
                class_node,
                &class_name,
                &class_iife_expr,
                None,
                &[],
                &static_elements,
                class_expr_set_function_name.as_deref(),
            );
            return;
        }

        if (class_data.name.is_some() || class_expr_set_function_name.is_some())
            && let Some(class_iife_expr) =
                Self::es5_class_iife_expression_from_var(&es5_output, &class_name)
        {
            self.write_multiline_fragment_preserving_indent(&class_iife_expr);
            if defer_static_block_only_tail {
                self.deferred_class_static_blocks
                    .extend(static_elements.iter().filter_map(|element| match element {
                        Es5StaticClassExpressionElement::StaticBlock {
                            block,
                            saved_comment_idx,
                            ..
                        } => Some((*block, *saved_comment_idx)),
                        Es5StaticClassExpressionElement::Field(_) => None,
                    }));
            }
            return;
        }
        if !use_static_comma
            && !defer_static_block_only_tail
            && let Some(class_iife_expr) =
                Self::es5_class_iife_expression_from_var(&es5_output, &class_name)
        {
            self.write_multiline_fragment_preserving_indent(&class_iife_expr);
            return;
        }

        self.write("(function () {");
        self.write_line();
        self.increase_indent();

        if !es5_mappings.is_empty() && self.writer.has_source_map() {
            let base_line = self.writer.current_line();
            let column_offset = self.writer.indent_width();
            self.writer.add_mappings_with_line_column_offset(
                base_line,
                column_offset,
                &es5_mappings,
            );
        }

        for line in es5_output.lines() {
            if !line.is_empty() {
                self.write(line);
            }
            self.write_line();
        }
        if use_static_comma {
            self.emit_es5_static_class_expression_statements(&class_name, &static_elements);
        }

        self.write("return ");
        self.write(&class_name);
        self.write(";");
        self.write_line();

        self.decrease_indent();
        self.write("})()");
        if defer_static_block_only_tail {
            self.deferred_class_static_blocks
                .extend(static_elements.iter().filter_map(|element| match element {
                    Es5StaticClassExpressionElement::StaticBlock {
                        block,
                        saved_comment_idx,
                        ..
                    } => Some((*block, *saved_comment_idx)),
                    Es5StaticClassExpressionElement::Field(_) => None,
                }));
        }
    }

    pub(in crate::emitter) fn has_es5_transforms(&self) -> bool {
        self.transforms
            .iter()
            .any(|(_, directive)| Self::directive_has_es5(directive))
    }

    pub(in crate::emitter) fn directive_has_es5(directive: &TransformDirective) -> bool {
        match directive {
            TransformDirective::ES5Class { .. }
            | TransformDirective::ES5ClassExpression { .. }
            | TransformDirective::ES5Namespace { .. }
            | TransformDirective::ES5Enum { .. }
            | TransformDirective::ES5ArrowFunction { .. }
            | TransformDirective::ES5AsyncFunction { .. }
            | TransformDirective::ES5GeneratorFunction { .. }
            | TransformDirective::ES5ForOf { .. }
            | TransformDirective::ES5ObjectLiteral { .. }
            | TransformDirective::ES5VariableDeclarationList { .. }
            | TransformDirective::ES5FunctionParameters { .. }
            | TransformDirective::ES5TemplateLiteral { .. }
            | TransformDirective::CommonJSExportDefaultClassES5 { .. } => true,
            TransformDirective::CommonJSExport { inner, .. } => Self::directive_has_es5(inner),
            TransformDirective::Chain(directives) => directives.iter().any(Self::directive_has_es5),
            _ => false,
        }
    }

    pub(in crate::emitter) fn tagged_template_var_name(&self, idx: NodeIndex) -> String {
        if let Some(name) = self.tagged_template_var_map.get(&idx) {
            name.clone()
        } else {
            format!("templateObject_{}", idx.0)
        }
    }

    /// Build the sequential mapping from tagged template node indices to variable names.
    pub(in crate::emitter) fn build_tagged_template_var_map(&mut self) {
        let mut indices: Vec<NodeIndex> = if self.transforms.helpers_populated() {
            self.transforms
                .iter()
                .filter_map(|(&idx, directive)| {
                    if !matches!(directive, TransformDirective::ES5TemplateLiteral { .. }) {
                        return None;
                    }
                    let node = self.arena.get(idx)?;
                    if node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION {
                        Some(idx)
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            self.arena
                .nodes
                .iter()
                .enumerate()
                .filter_map(|(i, node)| {
                    if node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION {
                        Some(NodeIndex(i as u32))
                    } else {
                        None
                    }
                })
                .collect()
        };
        indices.sort_by_key(|idx| idx.0);
        for (seq, idx) in indices.iter().enumerate() {
            self.tagged_template_var_map
                .insert(*idx, format!("templateObject_{}", seq + 1));
        }
    }

    pub(in crate::emitter) fn collect_tagged_template_vars(&self) -> Vec<String> {
        let mut entries: Vec<(&NodeIndex, &String)> = self.tagged_template_var_map.iter().collect();
        entries.sort_by_key(|(idx, _)| idx.0);
        entries.into_iter().map(|(_, name)| name.clone()).collect()
    }

    /// Emit a call expression with spread arguments transformed for ES5
    ///
    /// Examples:
    /// - `foo(...arr)` -> `foo.apply(void 0, arr)`
    /// - `foo(...iterable)` with downlevelIteration -> `foo.apply(void 0, __spreadArray([], __read(iterable), false))`
    /// - `foo(...arr, 1, 2)` -> `foo.apply(void 0, __spreadArray(__spreadArray([], arr, false), [1, 2], false))`
    /// - `obj.method(...arr)` -> `obj.method.apply(obj, arr)`
    pub(in crate::emitter) fn emit_call_expression_es5_spread(&mut self, node: &Node) {
        let Some(call) = self.arena.get_call_expr(node) else {
            return;
        };

        let optional_call_token =
            self.has_optional_call_token_in_spread(node, call.expression, call.arguments.as_ref());

        let Some(ref args) = call.arguments else {
            // No arguments - shouldn't happen if we detected spread
            self.emit(call.expression);
            self.write("()");
            return;
        };

        // Check if this is a method call (property access)
        let callee_node = self.arena.get(call.expression);
        let is_method_call =
            callee_node.is_some_and(|n| n.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION);

        if is_method_call {
            if optional_call_token {
                self.emit_optional_method_call_with_spread(call.expression, args, true);
            } else {
                self.emit_method_call_with_spread(call.expression, args);
            }
        } else if optional_call_token {
            self.emit_optional_function_call_with_spread(call.expression, args);
        } else {
            self.emit_function_call_with_spread(call.expression, args);
        }
    }

    fn has_optional_call_token_in_spread(
        &self,
        node: &Node,
        callee: NodeIndex,
        args: Option<&tsz_parser::parser::NodeList>,
    ) -> bool {
        let Some(source) = self.source_text_for_map() else {
            let Some(callee_node) = self.arena.get(callee) else {
                return false;
            };
            return self.arena.get_access_expr(callee_node).is_none();
        };

        let Some(callee_node) = self.arena.get(callee) else {
            return false;
        };
        let Some(open_paren) = self.find_open_paren_position_optional_call(node, args) else {
            return false;
        };
        let bytes = source.as_bytes();
        let mut i = std::cmp::min(open_paren as usize, source.len());
        let start = std::cmp::min(callee_node.pos as usize, source.len());

        while i > start {
            if i == 0 {
                break;
            }
            match bytes[i - 1] {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    i -= 1;
                }
                b'/' if i >= 2 && bytes[i - 2] == b'/' => {
                    while i > start && bytes[i - 1] != b'\n' {
                        i -= 1;
                    }
                    if i > start {
                        i -= 1;
                    }
                }
                b'/' if i >= 2 && bytes[i - 2] == b'*' => {
                    if i >= 2 {
                        i -= 2;
                    }
                    while i >= 2 && !(bytes[i - 2] == b'*' && bytes[i - 1] == b'/') {
                        i -= 1;
                    }
                    if i >= 2 {
                        i -= 2;
                    }
                }
                b'?' if i >= 2 && bytes[i - 2] == b'.' => {
                    return true;
                }
                b'.' if i >= 2 && bytes[i - 2] == b'?' && bytes[i - 1] == b'.' => {
                    return true;
                }
                _ => return false,
            }
        }

        false
    }

    fn find_open_paren_position_optional_call(
        &self,
        node: &Node,
        args: Option<&tsz_parser::parser::NodeList>,
    ) -> Option<u32> {
        let text = self.source_text_for_map()?;
        let bytes = text.as_bytes();
        let start = std::cmp::min(node.pos as usize, bytes.len());
        let mut end = std::cmp::min(node.end as usize, bytes.len());
        if let Some(args) = args
            && let Some(first_arg) = args.nodes.first()
            && let Some(first_node) = self.arena.get(*first_arg)
        {
            end = std::cmp::min(first_node.pos as usize, end);
        }
        (start..end)
            .position(|i| bytes[i] == b'(')
            .map(|offset| (start + offset) as u32)
    }

    fn emit_optional_function_call_with_spread(
        &mut self,
        callee_idx: NodeIndex,
        args: &tsz_parser::parser::NodeList,
    ) {
        let temp = self.get_temp_var_name();
        self.write("(");
        self.write(&temp);
        self.write(" = ");
        self.emit(callee_idx);
        self.write(")");
        self.write(" === null || ");
        self.write(&temp);
        self.write(" === void 0 ? void 0 : ");
        self.write(&temp);
        self.write(".apply(void 0, ");
        self.emit_spread_args_array(&args.nodes);
        self.write(")");
    }

    fn emit_optional_method_call_with_spread(
        &mut self,
        access_idx: NodeIndex,
        args: &tsz_parser::parser::NodeList,
        has_optional_call_token: bool,
    ) {
        // obj.method?.(...args) -> obj.method.call.apply(obj, [args]) with optional checks
        let Some(access_node) = self.arena.get(access_idx) else {
            return;
        };
        let Some(access) = self.arena.get_access_expr(access_node) else {
            return;
        };

        if !has_optional_call_token {
            let this_temp = self.get_temp_var_name();
            self.write("(");
            self.write(&this_temp);
            self.write(" = ");
            self.emit(access.expression);
            self.write(")");
            if access.question_dot_token {
                self.write(" === null || ");
                self.write(&this_temp);
                self.write(" === void 0 ? void 0 : ");
            }

            if access_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                self.write(".");
                self.emit(access.name_or_argument);
            } else {
                self.write("[");
                self.emit(access.name_or_argument);
                self.write("]");
            }
            self.write(".apply(");
            self.write(&this_temp);
            self.write(", ");
            self.emit_spread_args_array(&args.nodes);
            self.write(")");
            return;
        }

        let this_temp = self.get_temp_var_name();
        let method_temp = self.get_temp_var_name();

        self.write("(");
        self.write(&method_temp);
        self.write(" = ");
        self.write("(");
        self.write(&this_temp);
        self.write(" = ");
        self.emit(access.expression);
        self.write(")");
        if access.question_dot_token {
            self.write(" === null || ");
            self.write(&this_temp);
            self.write(" === void 0 ? void 0 : ");
        }
        if access_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if access.question_dot_token {
                self.write(&this_temp);
            }
            self.write(".");
            self.emit(access.name_or_argument);
        } else {
            if access.question_dot_token {
                self.write(&this_temp);
            }
            self.write("[");
            self.emit(access.name_or_argument);
            self.write("]");
        }
        self.write(") === null || ");
        self.write(&method_temp);
        self.write(" === void 0 ? void 0 : ");
        self.write(&method_temp);
        self.write(".call.apply(");
        self.write(&method_temp);
        self.write(", ");
        self.write_helper("__spreadArray");
        self.write("([");
        self.write(&this_temp);
        self.write("], ");
        self.emit_spread_args_array(&args.nodes);
        self.write(", false)");
        self.write(")");
    }

    fn emit_function_call_with_spread(
        &mut self,
        callee_idx: NodeIndex,
        args: &tsz_parser::parser::NodeList,
    ) {
        // foo(...args) -> foo.apply(void 0, args_array)
        self.emit(callee_idx);
        self.write(".apply(void 0, ");
        self.emit_spread_args_array(&args.nodes);
        self.write(")");
    }

    fn emit_method_call_with_spread(
        &mut self,
        access_idx: NodeIndex,
        args: &tsz_parser::parser::NodeList,
    ) {
        // obj.method(...args) -> obj.method.apply(obj, args_array)
        let Some(access_node) = self.arena.get(access_idx) else {
            return;
        };
        let Some(access) = self.arena.get_access_expr(access_node) else {
            return;
        };

        // The receiver is emitted twice (once as the property base, once as the
        // `apply` `thisArg`). When it is a non-simple expression (anything other
        // than an identifier, `this`, or a literal), evaluating it twice would be
        // observable, so tsc captures it once into a hoisted temp and reuses it:
        // `(_a = obj.prop).method.apply(_a, args)`.
        if crate::transforms::emit_utils::is_simple_copiable_expression(
            self.arena,
            access.expression,
        ) {
            self.emit(access.expression);
            if access_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                self.write(".");
                self.emit(access.name_or_argument);
            } else {
                self.write("[");
                self.emit(access.name_or_argument);
                self.write("]");
            }
            self.write(".apply(");
            self.emit(access.expression);
            self.write(", ");
            self.emit_spread_args_array(&args.nodes);
            self.write(")");
            return;
        }

        let receiver_temp = self.make_unique_name_hoisted_assignment_fresh();
        self.write("(");
        self.write(&receiver_temp);
        self.write(" = ");
        self.emit(access.expression);
        self.write(")");
        if access_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            self.write(".");
            self.emit(access.name_or_argument);
        } else {
            self.write("[");
            self.emit(access.name_or_argument);
            self.write("]");
        }
        self.write(".apply(");
        self.write(&receiver_temp);
        self.write(", ");
        self.emit_spread_args_array(&args.nodes);
        self.write(")");
    }

    fn emit_spread_args_array(&mut self, args: &[NodeIndex]) {
        // Build arguments array using __spreadArray for spread elements
        if args.is_empty() {
            self.write("[]");
            return;
        }

        // Check if there are any spread elements
        let has_spread = args
            .iter()
            .any(|&arg_idx| emit_utils::is_spread_element(self.arena, arg_idx));

        if !has_spread {
            // No spreads, just emit an array literal
            self.write("[");
            self.emit_comma_separated(args);
            self.write("]");
            return;
        }

        // Build segments by grouping consecutive non-spread and spread elements
        let mut segments: Vec<ArraySegment> = Vec::new();
        let mut current_start = 0;

        for (i, &arg_idx) in args.iter().enumerate() {
            if emit_utils::is_spread_element(self.arena, arg_idx) {
                // Add non-spread segment before this spread
                if current_start < i {
                    segments.push(ArraySegment::Elements(&args[current_start..i]));
                }
                // Add the spread element
                segments.push(ArraySegment::Spread(arg_idx));
                current_start = i + 1;
            }
        }

        // Add remaining elements after last spread
        if current_start < args.len() {
            segments.push(ArraySegment::Elements(&args[current_start..]));
        }

        // Emit using nested __spreadArray calls
        self.emit_spread_segments(&segments);
    }

    fn emit_spread_segments(&mut self, segments: &[ArraySegment]) {
        if segments.is_empty() {
            self.write("[]");
            return;
        }

        let wrap_spread_with_read = self.ctx.target_es5 && self.ctx.options.downlevel_iteration;

        if segments.len() == 1 {
            match &segments[0] {
                ArraySegment::Spread(spread_idx) => {
                    // Just a single spread with no other arguments:
                    // TypeScript optimization - pass arrays directly unless
                    // downlevelIteration requires __read for iterable inputs.
                    if let Some(spread_node) = self.arena.get(*spread_idx) {
                        if wrap_spread_with_read {
                            self.write_helper("__spreadArray");
                            self.write("([], ");
                            self.emit_spread_expression_with_read(spread_node, true);
                            self.write(", false)");
                        } else {
                            self.emit_spread_expression(spread_node);
                        }
                    }
                }
                ArraySegment::Elements(elems) => {
                    // Just elements: [1, 2, 3]
                    self.write("[");
                    self.emit_comma_separated(elems);
                    self.write("]");
                }
            }
            return;
        }

        // Multiple segments: build nested __spreadArray calls
        // Pattern: __spreadArray(__spreadArray(base, segment1, false), segment2, false)

        // Open __spreadArray calls for all but the last segment
        for _ in 0..segments.len() - 1 {
            self.write_helper("__spreadArray");
            self.write("(");
        }

        // Emit the first segment as a complete unit
        match &segments[0] {
            ArraySegment::Elements(elems) => {
                self.write("[");
                self.emit_comma_separated(elems);
                self.write("]");
            }
            ArraySegment::Spread(spread_idx) => {
                // First segment is spread: emit as __spreadArray([], spread, false)
                self.write_helper("__spreadArray");
                self.write("([], ");
                if let Some(spread_node) = self.arena.get(*spread_idx) {
                    self.emit_spread_expression_with_read(spread_node, wrap_spread_with_read);
                }
                self.write(", false)");
            }
        }

        // Emit remaining segments - each closes one __spreadArray call
        for segment in &segments[1..] {
            match segment {
                ArraySegment::Elements(elems) => {
                    self.write(", [");
                    self.emit_comma_separated(elems);
                    self.write("], false)");
                }
                ArraySegment::Spread(spread_idx) => {
                    self.write(", ");
                    if let Some(spread_node) = self.arena.get(*spread_idx) {
                        self.emit_spread_expression_with_read(spread_node, wrap_spread_with_read);
                    }
                    self.write(", false)");
                }
            }
        }
    }

    /// Emit a new expression with spread arguments, lowered for ES5.
    pub(in crate::emitter) fn emit_new_expression_es5_spread(&mut self, node: &Node) {
        let Some(new_expr) = self.arena.get_call_expr(node) else {
            return;
        };

        let Some(ref args) = new_expr.arguments else {
            self.write("new ");
            self.emit(new_expr.expression);
            self.write("()");
            return;
        };

        // Determine if the constructor expression needs a temp variable.
        // Simple identifiers can be emitted twice; anything else (property access,
        // element access, call expressions, parenthesized expressions) needs a temp
        // to avoid double evaluation.
        let callee_node = self.arena.get(new_expr.expression);
        let needs_temp = callee_node.is_some_and(|n| n.kind != SyntaxKind::Identifier as u16);

        self.write("new (");

        if needs_temp {
            let temp = self.make_unique_name_hoisted();
            self.write("(");
            self.write(&temp);
            self.write(" = ");
            self.emit(new_expr.expression);
            self.write(").bind.apply(");
            self.write(&temp);
        } else {
            self.emit(new_expr.expression);
            self.write(".bind.apply(");
            self.emit(new_expr.expression);
        }

        self.write(", ");
        self.emit_new_spread_args_array(&args.nodes);
        self.write("))()");
    }

    fn emit_new_spread_args_array(&mut self, args: &[NodeIndex]) {
        let mut segments: Vec<ArraySegment> = Vec::new();
        let mut current_start = 0;

        for (i, &arg_idx) in args.iter().enumerate() {
            if emit_utils::is_spread_element(self.arena, arg_idx) {
                if current_start < i {
                    segments.push(ArraySegment::Elements(&args[current_start..i]));
                }
                segments.push(ArraySegment::Spread(arg_idx));
                current_start = i + 1;
            }
        }

        if current_start < args.len() {
            segments.push(ArraySegment::Elements(&args[current_start..]));
        }

        if segments.is_empty() {
            self.write("[void 0]");
            return;
        }

        if segments.len() == 1
            && let ArraySegment::Spread(spread_idx) = &segments[0]
        {
            self.write_helper("__spreadArray");
            self.write("([void 0], ");
            if let Some(spread_node) = self.arena.get(*spread_idx) {
                self.emit_spread_expression(spread_node);
            }
            self.write(", false)");
            return;
        }

        for _ in 0..segments.len() - 1 {
            self.write_helper("__spreadArray");
            self.write("(");
        }

        match &segments[0] {
            ArraySegment::Elements(elems) => {
                self.write("[void 0, ");
                self.emit_comma_separated(elems);
                self.write("]");
            }
            ArraySegment::Spread(spread_idx) => {
                self.write_helper("__spreadArray");
                self.write("([void 0], ");
                if let Some(spread_node) = self.arena.get(*spread_idx) {
                    self.emit_spread_expression(spread_node);
                }
                self.write(", false)");
            }
        }

        for segment in &segments[1..] {
            match segment {
                ArraySegment::Elements(elems) => {
                    self.write(", [");
                    self.emit_comma_separated(elems);
                    self.write("], false)");
                }
                ArraySegment::Spread(spread_idx) => {
                    self.write(", ");
                    if let Some(spread_node) = self.arena.get(*spread_idx) {
                        self.emit_spread_expression(spread_node);
                    }
                    self.write(", false)");
                }
            }
        }
    }
}
