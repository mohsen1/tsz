impl<'a> Printer<'a> {
    // =========================================================================
    // Transform Application (Phase 2 Architecture)
    // =========================================================================

    pub(in crate::emitter) fn seed_tc39_decorator_static_member(
        &mut self,
        emitter: &mut crate::transforms::es_decorators::TC39DecoratorEmitter<'a>,
        member_idx: NodeIndex,
        prop: &tsz_parser::parser::node::PropertyDeclData,
        this_alias: Option<&str>,
        super_alias: Option<&str>,
    ) {
        let start = self.writer.len();
        self.emit_class_member_modifiers_js(&prop.modifiers);
        self.emit(prop.name);
        self.write(" = ");
        let prev_statement_expression = self.ctx.flags.in_statement_expression;
        self.ctx.flags.in_statement_expression = false;
        self.emit_expression_with_scoped_static_initializer_mode(
            prop.initializer,
            this_alias,
            super_alias,
            false,
        );
        self.ctx.flags.in_statement_expression = prev_statement_expression;
        self.write_semicolon();
        let output = self.writer.get_output()[start..].trim_start().to_string();
        self.writer.truncate(start);
        emitter.set_static_member_text(member_idx, output);
    }

    fn node_contains_super_keyword(&self, idx: NodeIndex) -> bool {
        let mut stack = vec![idx];
        while let Some(current) = stack.pop() {
            let Some(node) = self.arena.get(current) else {
                continue;
            };
            if node.kind == tsz_scanner::SyntaxKind::SuperKeyword as u16 {
                return true;
            }
            stack.extend(self.arena.get_children(current));
        }
        false
    }

    fn node_contains_this_keyword(&self, idx: NodeIndex) -> bool {
        let mut stack = vec![idx];
        while let Some(current) = stack.pop() {
            let Some(node) = self.arena.get(current) else {
                continue;
            };
            if node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16 {
                return true;
            }
            stack.extend(self.arena.get_children(current));
        }
        false
    }

    fn node_contains_private_identifier(&self, idx: NodeIndex) -> bool {
        let mut stack = vec![idx];
        while let Some(current) = stack.pop() {
            let Some(node) = self.arena.get(current) else {
                continue;
            };
            if node.kind == tsz_scanner::SyntaxKind::PrivateIdentifier as u16 {
                return true;
            }
            stack.extend(self.arena.get_children(current));
        }
        false
    }

    fn node_is_this_keyword(&self, idx: NodeIndex) -> bool {
        self.arena
            .get(idx)
            .is_some_and(|node| node.kind == tsz_scanner::SyntaxKind::ThisKeyword as u16)
    }

    fn seed_tc39_decorator_extends_text(
        &mut self,
        emitter: &mut crate::transforms::es_decorators::TC39DecoratorEmitter<'a>,
        expr_idx: NodeIndex,
    ) {
        let runtime_expr = self.tc39_decorator_extends_runtime_expression(expr_idx);
        let needs_named_eval_suppression =
            self.tc39_decorator_extends_needs_named_eval_suppression(runtime_expr);
        let is_arrow = self
            .arena
            .get(runtime_expr)
            .is_some_and(|node| node.kind == syntax_kind_ext::ARROW_FUNCTION);

        let previous_name = self.pending_tc39_class_expression_name.take();
        let mut output = self.capture_emit(runtime_expr);
        self.pending_tc39_class_expression_name = previous_name;

        // `capture_emit` rendered the base expression at the writer's current
        // indent and trimmed only its first line. The captured text is spliced
        // after `let _classSuper = `, which sits one level deeper than the class
        // being lowered. Re-base any continuation lines to that insertion indent
        // so multi-line bases (e.g. a class expression with members, or an empty
        // class body whose `}` lands on its own line) are not left flush-left.
        let base_level = self.writer.indent_level();
        output = self.reindent_captured_block(&output, base_level, base_level + 1);

        if is_arrow {
            output = format!("({output})");
        }
        if needs_named_eval_suppression {
            output = format!("(0, {output})");
        }
        emitter.set_extends_text(output);
    }

    fn tc39_decorator_extends_runtime_expression(&self, mut idx: NodeIndex) -> NodeIndex {
        loop {
            let Some(node) = self.arena.get(idx) else {
                return idx;
            };
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.arena.get_parenthesized(node)
                && !paren.expression.is_none()
            {
                idx = paren.expression;
                continue;
            }
            if (node.kind == syntax_kind_ext::TYPE_ASSERTION
                || node.kind == syntax_kind_ext::AS_EXPRESSION
                || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
                && let Some(assertion) = self.arena.get_type_assertion(node)
            {
                idx = assertion.expression;
                continue;
            }
            return idx;
        }
    }

    fn tc39_decorator_extends_needs_named_eval_suppression(&self, idx: NodeIndex) -> bool {
        self.arena.get(idx).is_some_and(|node| {
            node.kind == syntax_kind_ext::CLASS_EXPRESSION
                || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || node.kind == syntax_kind_ext::ARROW_FUNCTION
        })
    }

    fn capture_tc39_class_expression_with_name(
        &mut self,
        class_expr: NodeIndex,
        name: String,
    ) -> String {
        let previous = self
            .pending_tc39_class_expression_name
            .replace((name, false));
        let output = self.capture_emit(class_expr);
        self.pending_tc39_class_expression_name = previous;
        output
    }

    fn tc39_class_expression_name_from_class_field_name(&self, name: NodeIndex) -> Option<String> {
        let name_node = self.arena.get(name)?;
        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.arena.get_computed_property(name_node)?;
            self.tc39_class_expression_name_from_computed_property_expr(computed.expression)
        } else {
            self.tc39_class_expression_name_from_property_name(name)
        }
    }

    fn render_tc39_decorator_function_body(&self, body_idx: NodeIndex) -> String {
        let options = self.ctx.options.clone();
        let ctx = crate::context::emit::EmitContext::with_options(options.clone());
        let transforms = crate::lowering::LoweringPass::new(self.arena, &ctx).run(body_idx);
        let mut printer = Self::with_transforms_and_options(self.arena, transforms, options);
        if let Some(text) = self.source_text_for_map() {
            printer.set_source_text(text);
        }
        printer.emitting_function_body_block = true;
        printer.emit(body_idx);
        let output = printer.get_output().to_string();
        if output.trim().is_empty() {
            "{ }".to_string()
        } else {
            output
        }
    }

    fn emit_commonjs_inner(
        &mut self,
        node: &Node,
        idx: NodeIndex,
        inner: &EmitDirective,
        export_name: Option<IdentifierId>,
    ) {
        match inner {
            EmitDirective::ES5Class { class_node } => {
                let class_binding_name = self.register_es5_class_binding_name(*class_node);
                let mut es5_emitter = self.create_es5_class_emitter_with_decorators(*class_node);
                let es5_output = self.emit_es5_class_output(
                    &mut es5_emitter,
                    *class_node,
                    class_binding_name.as_deref(),
                );
                self.sync_es5_class_emitter_state(&mut es5_emitter);
                let es5_mappings = es5_emitter.take_mappings();
                if !es5_mappings.is_empty() && self.writer.has_source_map() {
                    self.writer.write("");
                    let base_line = self.writer.current_line();
                    let base_column = self.writer.current_column();
                    self.writer
                        .add_offset_mappings(base_line, base_column, &es5_mappings);
                    self.writer.write(&es5_output);
                } else {
                    self.write(&es5_output);
                }
                let class_close_pos = self.find_token_end_before_trivia(node.pos, node.end);
                while self.comment_emit_idx < self.all_comments.len()
                    && self.all_comments[self.comment_emit_idx].pos < class_close_pos
                {
                    self.comment_emit_idx += 1;
                }
                self.emit_trailing_comments(class_close_pos);
                self.skip_comments_for_erased_node(node);
            }
            EmitDirective::ES5ClassExpression { class_node } => {
                self.emit_class_expression_es5(*class_node);
            }
            EmitDirective::ES5Namespace {
                namespace_node,
                should_declare_var,
            } => {
                let mut ns_name_for_exports = String::new();
                if let Some(ns_node) = self.arena.get(*namespace_node)
                    && let Some(ns_data) = self.arena.get_module(ns_node)
                {
                    let ns_name = self.get_identifier_text_idx(ns_data.name);
                    if !ns_name.is_empty() {
                        ns_name_for_exports = ns_name.clone();
                        self.declared_namespace_names.insert(ns_name);
                    }
                    if self.in_top_level_using_scope && self.ctx.target_es5 {
                        self.emit_namespace_iife(ns_data, None, None);
                        while self.comment_emit_idx < self.all_comments.len()
                            && self.all_comments[self.comment_emit_idx].end <= node.end
                        {
                            self.comment_emit_idx += 1;
                        }
                        return;
                    }
                }
                let mut ns_emitter =
                    NamespaceES5Emitter::with_commonjs(self.arena, self.ctx.is_commonjs());
                ns_emitter.set_module_kind(self.ctx.outer_module_kind());
                ns_emitter.set_const_enum_facts(
                    self.const_enum_values.clone(),
                    self.const_enum_import_aliases.clone(),
                );
                // Collect this block's exported vars and accumulate for cross-block sharing
                if !ns_name_for_exports.is_empty() {
                    let block_exports = ns_emitter.collect_exported_var_names(*namespace_node);
                    let entry = self
                        .namespace_prior_exports
                        .entry(ns_name_for_exports)
                        .or_default();
                    entry.extend(block_exports);
                    ns_emitter.set_prior_exported_vars(entry.clone());
                }
                ns_emitter.set_indent_level(self.writer.indent_level());
                ns_emitter.set_target_es5(self.ctx.target_es5);
                ns_emitter.set_remove_comments(self.ctx.options.remove_comments);
                ns_emitter.set_legacy_decorators(self.ctx.options.legacy_decorators);
                ns_emitter.set_emit_decorator_metadata(self.ctx.options.emit_decorator_metadata);
                ns_emitter.set_transforms(self.transforms.clone());
                self.configure_es5_namespace_emitter_block_scope(&mut ns_emitter);
                if let Some(text) = self.source_text_for_map() {
                    ns_emitter.set_source_text(text);
                }
                ns_emitter
                    .set_should_declare_var(*should_declare_var && !self.in_top_level_using_scope);
                ns_emitter.set_iife_param_rename_counter(self.namespace_iife_param_counter.clone());
                let output = ns_emitter.emit_exported_namespace(*namespace_node);
                self.namespace_iife_param_counter = ns_emitter.take_iife_param_rename_counter();
                self.sync_es5_namespace_emitter_block_scope(&ns_emitter);
                self.write(output.trim_end_matches('\n'));
                // Advance comment cursor past comments inside the namespace body,
                // since the sub-emitter already handled them.
                while self.comment_emit_idx < self.all_comments.len()
                    && self.all_comments[self.comment_emit_idx].end <= node.end
                {
                    self.comment_emit_idx += 1;
                }
            }
            EmitDirective::ES5Enum { enum_node } => {
                self.emit_es5_enum_directive(node, *enum_node);
            }
            EmitDirective::ES5AsyncFunction { function_node } => {
                if let Some(func_node) = self.arena.get(*function_node)
                    && let Some(func) = self.arena.get_function(func_node)
                {
                    if func.asterisk_token {
                        let func_name = if func.name.is_some() {
                            self.get_identifier_text_idx(func.name)
                        } else if let Some(export_name) = export_name {
                            self.arena
                                .identifiers
                                .get(export_name as usize)
                                .map(|ident| ident.escaped_text.clone())
                                .unwrap_or_default()
                        } else {
                            String::new()
                        };
                        if self.should_emit_invalid_namespace_static_modifier(
                            func_node,
                            &func.modifiers,
                        ) {
                            self.write("static ");
                        }
                        self.emit_async_generator_lowered(func, &func_name);
                    } else if func.name.is_some() {
                        let func_name = self.get_identifier_text_idx(func.name);
                        if self.should_emit_invalid_namespace_static_modifier(
                            func_node,
                            &func.modifiers,
                        ) {
                            self.write("static ");
                        }
                        self.emit_async_function_es5(func, &func_name, "this");
                    } else if let Some(export_name) = export_name {
                        if self.should_emit_invalid_namespace_static_modifier(
                            func_node,
                            &func.modifiers,
                        ) {
                            self.write("static ");
                        }
                        if let Some(ident) = self.arena.identifiers.get(export_name as usize) {
                            self.emit_async_function_es5(func, &ident.escaped_text, "this");
                        } else {
                            self.emit_async_function_es5(func, "", "this");
                        }
                    } else {
                        if self.should_emit_invalid_namespace_static_modifier(
                            func_node,
                            &func.modifiers,
                        ) {
                            self.write("static ");
                        }
                        self.emit_async_function_es5(func, "", "this");
                    }
                }
            }
            EmitDirective::ES5ArrowFunction {
                arrow_node,
                captures_this,
                captures_arguments,
                class_alias,
            } => {
                if let Some(arrow_node) = self.arena.get(*arrow_node)
                    && let Some(func) = self.arena.get_function(arrow_node)
                {
                    self.emit_arrow_function_es5(
                        arrow_node,
                        func,
                        *captures_this,
                        *captures_arguments,
                        class_alias,
                    );
                }
            }
            EmitDirective::ES5FunctionParameters { function_node } => {
                if let Some(func_node) = self.arena.get(*function_node) {
                    match func_node.kind {
                        k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                            self.emit_function_declaration_es5_params(func_node);
                        }
                        k if k == syntax_kind_ext::FUNCTION_EXPRESSION => {
                            self.emit_function_expression_es5_params(func_node, *function_node);
                        }
                        k if k == syntax_kind_ext::ARROW_FUNCTION && !self.ctx.target_es5 => {
                            if let Some(func) = self.arena.get_function(func_node) {
                                self.emit_arrow_function_native_with_parameter_prologue(func);
                            }
                        }
                        _ => {}
                    }
                }
            }
            EmitDirective::TC39Decorators {
                class_node,
                function_name,
            } => {
                self.emit_tc39_decorators(node, idx, *class_node, function_name.as_deref());
            }
            EmitDirective::Chain(directives) => {
                self.emit_chained_directives(node, idx, directives.as_slice());
            }
            _ => {
                self.emit_node_default(node, idx);
            }
        }
    }
}
