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
            self.open_paren();
            self.emit_function_parameters_js(&func.parameters.nodes);
            self.close_paren();
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

        if self.is_static_block_await_arrow_recovery(func) {
            self.emit_static_block_await_arrow_recovery(func);
            return;
        }

        if self.ctx.target_es5 {
            let captures_this = contains_this_reference(self.arena, _idx);
            let captures_arguments = contains_arguments_reference(self.arena, _idx);
            self.emit_arrow_function_es5(node, func, captures_this, captures_arguments, &None);
            return;
        }

        self.emit_arrow_function_native(node, func);
    }

    fn emit_static_block_await_arrow_recovery(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
    ) {
        let source_had_parens = self.source_has_arrow_function_parens(&func.parameters.nodes);
        let Some(&param_idx) = func.parameters.nodes.first() else {
            return;
        };
        let Some(param_node) = self.arena.get(param_idx) else {
            return;
        };
        let Some(param) = self.arena.get_parameter(param_node) else {
            return;
        };

        if source_had_parens {
            self.write("(");
        }
        self.emit(param.name);
        self.write(" ");
        if source_had_parens {
            self.write(")");
        }
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
    fn emit_arrow_function_native(
        &mut self,
        node: &Node,
        func: &tsz_parser::parser::node::FunctionData,
    ) {
        // For ES2015/ES2016, lower async arrows: () => __awaiter(this, void 0, void 0, function* () { ... })
        if func.is_async && self.ctx.needs_async_lowering {
            self.push_temp_scope();
            self.emit_arrow_function_async_lowered(func);
            self.pop_temp_scope();
            return;
        }

        if self.native_arrow_default_params_need_temp_prologue(func) {
            self.emit_arrow_function_native_with_default_prologue(func);
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
            self.open_paren();
        }
        let prev_namespace_exported_names = self.namespace_exported_names.clone();
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
            self.close_paren();
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
            if let Some(arrow_equals_pos) = self.find_char_after(search_start, search_end, b'=') {
                self.skip_arrow_pre_token_comments(search_start, arrow_equals_pos);
            }
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
        self.remove_namespace_exported_parameter_names(&func.parameters.nodes);
        self.push_commonjs_exported_var_parameter_shadow_names(&func.parameters.nodes);

        // If we have pending object rest params and a concise body, convert to block body
        if !body_is_block && !self.pending_object_rest_params.is_empty() {
            let rest_params: Vec<(String, NodeIndex)> =
                std::mem::take(&mut self.pending_object_rest_params);
            self.write("{");
            self.write_line();
            self.increase_indent();
            self.emit_object_rest_param_prologue_entries(&rest_params);
            // Emit the concise body as a return statement
            self.write("return ");
            self.function_scope_depth += 1;
            self.arrow_function_scope_depth += 1;
            self.emit(func.body);
            self.arrow_function_scope_depth -= 1;
            self.function_scope_depth -= 1;
            self.write(";");
            self.write_line();
            self.decrease_indent();
            self.write("}");
        } else if !body_is_block && self.arrow_concise_body_needs_temp_prologue(func.body) {
            self.emit_arrow_concise_body_with_temp_prologue(func.body);
        } else if !body_is_block && self.concise_body_needs_parens(func.body) {
            // Emit comments between => and the body expression (e.g. triple-slash comments)
            if let Some(body_node) = self.arena.get(func.body) {
                self.emit_arrow_concise_body_leading_comments(body_node.pos);
            }
            self.parenthesized(|emitter| emitter.emit(func.body));
            self.emit_arrow_concise_body_trailing_comments(func.body, node.end);
        } else {
            // Emit comments between => and the body expression (e.g. triple-slash comments)
            // tsc preserves these and places the body on a new line when comments exist.
            if !body_is_block && let Some(body_node) = self.arena.get(func.body) {
                self.emit_arrow_concise_body_leading_comments(body_node.pos);
            }
            let prev_emitting_function_body_block = self.emitting_function_body_block;
            self.emitting_function_body_block = true;
            let prev_pending_function_body_parameters = std::mem::replace(
                &mut self.pending_function_body_parameters,
                func.parameters.nodes.clone(),
            );
            self.function_scope_depth += 1;
            self.arrow_function_scope_depth += 1;
            let prev_declared = std::mem::take(&mut self.declared_namespace_names);
            if body_is_block
                || !self.emit_arrow_concise_body_with_stripped_type_erasure_parens(func.body)
            {
                self.emit(func.body);
            }
            if !body_is_block {
                self.emit_arrow_concise_body_trailing_comments(func.body, node.end);
            }
            self.declared_namespace_names = prev_declared;
            self.arrow_function_scope_depth -= 1;
            self.function_scope_depth -= 1;
            self.pending_function_body_parameters = prev_pending_function_body_parameters;
            self.emitting_function_body_block = prev_emitting_function_body_block;
        }

        self.pop_commonjs_exported_var_parameter_shadow_names();
        self.namespace_exported_names = prev_namespace_exported_names;
        self.pop_temp_scope();
    }

    fn skip_arrow_pre_token_comments(&mut self, search_start: u32, arrow_equals_pos: u32) {
        while self.comment_emit_idx < self.all_comments.len() {
            let comment = &self.all_comments[self.comment_emit_idx];
            if comment.pos >= search_start && comment.end <= arrow_equals_pos {
                self.comment_emit_idx += 1;
            } else {
                break;
            }
        }
    }

    pub(in crate::emitter) fn emit_arrow_concise_body_leading_comments(&mut self, body_pos: u32) {
        if self.pending_comment_before_pos_starts_after_newline(body_pos) {
            self.write_line();
        }
        self.emit_comments_before_pos(body_pos);
    }

    fn emit_arrow_concise_body_trailing_comments(
        &mut self,
        body_idx: tsz_parser::parser::NodeIndex,
        arrow_end: u32,
    ) {
        if self.ctx.options.remove_comments {
            return;
        }
        let Some(body_node) = self.arena.get(body_idx) else {
            return;
        };
        let body_token_end = self.find_last_expr_end_before_trivia(body_node.pos, body_node.end);
        let comment_end = std::cmp::max(body_node.end, arrow_end);
        if let Some((defer_start, defer_end)) = self.arrow_concise_body_trailing_comment_defer_range
            && body_token_end == defer_start
            && self
                .arrow_concise_body_deferred_comment_end(body_token_end, defer_end)
                .is_some()
        {
            return;
        }
        self.emit_comments_in_range(body_token_end, comment_end, true, false);
    }

    pub(crate) fn with_arrow_concise_body_trailing_comments_deferred<R>(
        &mut self,
        defer_start: u32,
        defer_end: u32,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let prev = self.arrow_concise_body_trailing_comment_defer_range;
        self.arrow_concise_body_trailing_comment_defer_range = Some((defer_start, defer_end));
        let result = f(self);
        self.arrow_concise_body_trailing_comment_defer_range = prev;
        result
    }

    pub(crate) fn rightmost_concise_arrow_body_comment_start(&self, idx: NodeIndex) -> Option<u32> {
        let node = self.arena.get(idx)?;
        if node.kind == syntax_kind_ext::ARROW_FUNCTION {
            let func = self.arena.get_function(node)?;
            let body = self.arena.get(func.body)?;
            if body.kind == syntax_kind_ext::BLOCK {
                return None;
            }
            return Some(self.find_last_expr_end_before_trivia(body.pos, body.end));
        }

        if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            let paren = self.arena.get_parenthesized(node)?;
            return self.rightmost_concise_arrow_body_comment_start(paren.expression);
        }

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            let binary = self.arena.get_binary_expr(node)?;
            return self.rightmost_concise_arrow_body_comment_start(binary.right);
        }

        None
    }

    pub(crate) fn rightmost_concise_arrow_deferred_comment_range(
        &self,
        idx: NodeIndex,
        scan_end: u32,
    ) -> Option<(u32, u32)> {
        if let Some(comment_start) = self.rightmost_concise_arrow_body_comment_start(idx) {
            let comment_end =
                self.arrow_concise_body_deferred_comment_end(comment_start, scan_end)?;
            return Some((comment_start, comment_end));
        }

        self.direct_transformed_jsx_trailing_comment_range(idx)
    }

    fn arrow_concise_body_deferred_comment_end(
        &self,
        body_token_end: u32,
        scan_end: u32,
    ) -> Option<u32> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let mut i = std::cmp::min(body_token_end as usize, bytes.len());
        let limit = std::cmp::min(scan_end as usize, bytes.len());
        let mut saw_line_comment = false;
        let mut last_comment_end = None;

        while i < limit {
            match bytes[i] {
                b';' => return last_comment_end,
                b' ' | b'\t' | b'\n' | b'\r' => i += 1,
                b'/' if i + 1 < limit && bytes[i + 1] == b'/' => {
                    saw_line_comment = true;
                    i += 2;
                    while i < limit && bytes[i] != b'\n' && bytes[i] != b'\r' {
                        i += 1;
                    }
                    last_comment_end = Some(i as u32);
                }
                b'/' if i + 1 < limit && bytes[i + 1] == b'*' => {
                    i += 2;
                    while i + 1 < limit && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                        i += 1;
                    }
                    i = std::cmp::min(i + 2, limit);
                    last_comment_end = Some(i as u32);
                }
                _ if saw_line_comment => return last_comment_end,
                _ => return None,
            }
        }
        if saw_line_comment {
            last_comment_end
        } else {
            None
        }
    }

    pub(in crate::emitter) fn pending_comment_before_pos_starts_after_newline(
        &self,
        pos: u32,
    ) -> bool {
        if self.ctx.options.remove_comments || self.comment_emit_idx >= self.all_comments.len() {
            return false;
        }
        let actual_start = self.skip_trivia_forward(pos, pos + 1024);
        let comment = &self.all_comments[self.comment_emit_idx];
        if comment.end > actual_start {
            return false;
        }
        let Some(source) = self.source_text else {
            return false;
        };
        let bytes = source.as_bytes();
        let mut idx = comment.pos as usize;
        while idx > 0 {
            idx -= 1;
            match bytes[idx] {
                b' ' | b'\t' => {}
                b'\n' | b'\r' => return true,
                _ => return false,
            }
        }
        false
    }

    fn emit_arrow_concise_body_with_stripped_type_erasure_parens(
        &mut self,
        body: NodeIndex,
    ) -> bool {
        let Some(body_node) = self.arena.get(body) else {
            return false;
        };
        if body_node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return false;
        };
        let Some(paren) = self.arena.get_parenthesized(body_node) else {
            return false;
        };
        let Some(inner) = self.arena.get(paren.expression) else {
            return false;
        };
        if inner.kind != syntax_kind_ext::TYPE_ASSERTION
            && inner.kind != syntax_kind_ext::AS_EXPRESSION
            && inner.kind != syntax_kind_ext::SATISFIES_EXPRESSION
        {
            return false;
        }

        let unwrapped_kind = self.unwrap_type_assertion_kind(paren.expression);
        let can_strip = matches!(
            unwrapped_kind,
            Some(k) if k == SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || k == SyntaxKind::ThisKeyword as u16
                || k == SyntaxKind::SuperKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::RegularExpressionLiteral as u16
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::NON_NULL_EXPRESSION
                || k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                || (k == syntax_kind_ext::CALL_EXPRESSION && !self.paren_in_new_callee)
                || (k == syntax_kind_ext::NEW_EXPRESSION && !self.paren_in_access_position)
                || ((k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::CLASS_EXPRESSION)
                    && !self.ctx.flags.paren_leftmost_function_or_object
                    && (!self.paren_in_access_position || self.paren_is_direct_call_callee))
        );
        if !can_strip {
            return false;
        }

        let Some(actual_inner_start) = self
            .source_text
            .map(|_| self.skip_trivia_forward(inner.pos, inner.pos.saturating_add(2048)))
        else {
            return false;
        };

        let has_newline_comment = self.all_comments.iter().any(|comment| {
            comment.pos >= body_node.pos
                && comment.end <= actual_inner_start
                && comment.has_trailing_new_line
        });
        if !has_newline_comment {
            return false;
        }

        self.write_line();
        self.emit(paren.expression);
        true
    }

    fn emit_arrow_function_native_with_default_prologue(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
    ) {
        let source_had_parens = self.source_has_arrow_function_parens(&func.parameters.nodes);
        let is_simple = self.is_simple_single_parameter(&func.parameters.nodes);
        let needs_parens = source_had_parens || !is_simple;

        if needs_parens {
            self.write("(");
        }
        self.emit_function_parameter_names_js(&func.parameters.nodes);
        if needs_parens {
            self.write(")");
        }

        self.write_space();
        self.write("=> ");

        self.push_temp_scope();
        let prev_namespace_exported_names = self.namespace_exported_names.clone();
        self.remove_namespace_exported_parameter_names(&func.parameters.nodes);
        self.push_commonjs_exported_var_parameter_shadow_names(&func.parameters.nodes);
        self.write("{");
        self.write_line();
        self.increase_indent();
        self.emit_native_default_param_prologue(&func.parameters.nodes);

        let body_node = self.arena.get(func.body);
        let is_block = body_node.is_some_and(|n| n.kind == syntax_kind_ext::BLOCK);
        self.function_scope_depth += 1;
        self.arrow_function_scope_depth += 1;
        if is_block {
            if let Some(block_node) = body_node
                && let Some(block) = self.arena.get_block(block_node)
            {
                for &stmt_idx in &block.statements.nodes {
                    let before_len = self.writer.len();
                    self.emit(stmt_idx);
                    if self.writer.len() > before_len {
                        self.write_line();
                    }
                }
            }
        } else {
            self.write("return ");
            self.emit(func.body);
            self.write(";");
            self.write_line();
        }
        self.arrow_function_scope_depth -= 1;
        self.function_scope_depth -= 1;

        self.decrease_indent();
        self.write("}");
        self.pop_commonjs_exported_var_parameter_shadow_names();
        self.namespace_exported_names = prev_namespace_exported_names;
        self.pop_temp_scope();
    }

    pub(in crate::emitter) fn emit_arrow_function_native_with_parameter_prologue(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
    ) {
        let source_had_parens = self.source_has_arrow_function_parens(&func.parameters.nodes);
        let is_simple = self.is_simple_single_parameter(&func.parameters.nodes);
        let needs_parens = source_had_parens || !is_simple;

        self.push_temp_scope();
        let prev_namespace_exported_names = self.namespace_exported_names.clone();

        if needs_parens {
            self.write("(");
        }
        let prologue_entries =
            self.emit_native_arrow_parameter_names_with_prologue(&func.parameters.nodes);
        if needs_parens {
            self.write(")");
        }

        self.write_space();
        self.write("=> ");

        self.remove_namespace_exported_parameter_names(&func.parameters.nodes);
        self.push_commonjs_exported_var_parameter_shadow_names(&func.parameters.nodes);

        let body_node = self.arena.get(func.body);
        let body_is_block = body_node.is_some_and(|n| n.kind == syntax_kind_ext::BLOCK);
        let can_emit_inline = body_is_block
            && body_node.is_some_and(|n| self.is_single_line(n))
            && prologue_entries
                .iter()
                .all(|entry| matches!(entry, NativeArrowParamPrologueEntry::Binding { .. }));

        self.function_scope_depth += 1;
        self.arrow_function_scope_depth += 1;
        if can_emit_inline {
            self.write("{ ");
            self.emit_native_arrow_parameter_prologue_entries(&prologue_entries, true);
            self.emit_native_arrow_inline_block_body(func.body);
            self.write("}");
        } else {
            self.write("{");
            self.write_line();
            self.increase_indent();
            self.emit_native_arrow_parameter_prologue_entries(&prologue_entries, false);
            if body_is_block {
                self.emit_native_arrow_block_body_statements(func.body);
            } else {
                self.write("return ");
                self.emit(func.body);
                self.write(";");
                self.write_line();
            }
            self.decrease_indent();
            self.write("}");
        }
        self.arrow_function_scope_depth -= 1;
        self.function_scope_depth -= 1;

        self.pop_commonjs_exported_var_parameter_shadow_names();
        self.namespace_exported_names = prev_namespace_exported_names;
        self.pop_temp_scope();
    }

    fn emit_native_arrow_parameter_names_with_prologue(
        &mut self,
        params: &[NodeIndex],
    ) -> Vec<NativeArrowParamPrologueEntry> {
        let mut entries = Vec::new();
        let mut first = true;
        for &param_idx in params {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            if param.name.is_none() || param.dot_dot_dot_token {
                continue;
            }
            if !first {
                self.write(", ");
            }
            first = false;
            self.emit_comments_before_pos(param_node.pos);

            if self.is_binding_pattern(param.name) {
                let temp_name = self.get_temp_var_name();
                self.write(&temp_name);
                entries.push(NativeArrowParamPrologueEntry::Binding {
                    pattern: param.name,
                    temp_name,
                    initializer: param.initializer,
                });
            } else {
                self.emit_parameter_name_js(param.name);
                if param.initializer.is_some() {
                    let name = crate::transforms::emit_utils::identifier_text_or_empty(
                        self.arena, param.name,
                    );
                    if !name.is_empty() {
                        entries.push(NativeArrowParamPrologueEntry::Default {
                            name,
                            initializer: param.initializer,
                        });
                    }
                }
            }
        }
        entries
    }

    fn emit_native_arrow_parameter_prologue_entries(
        &mut self,
        entries: &[NativeArrowParamPrologueEntry],
        inline: bool,
    ) {
        for entry in entries {
            match entry {
                NativeArrowParamPrologueEntry::Default { name, initializer } => {
                    self.emit_param_default_assignment(name, *initializer);
                }
                NativeArrowParamPrologueEntry::Binding {
                    pattern,
                    temp_name,
                    initializer,
                } => {
                    self.emit_native_arrow_binding_param_prologue(
                        *pattern,
                        temp_name,
                        *initializer,
                        inline,
                    );
                }
            }
        }
    }

    fn emit_native_arrow_binding_param_prologue(
        &mut self,
        pattern: NodeIndex,
        temp_name: &str,
        initializer: NodeIndex,
        inline: bool,
    ) {
        let hoisted_start = self.hoisted_assignment_temps.len();
        let value_start = self.hoisted_assignment_value_temps.len();
        let pattern_text = self.capture_emit(pattern);
        let initializer_text = initializer
            .is_some()
            .then(|| self.capture_emit(initializer));
        self.emit_native_arrow_param_temp_declarations(hoisted_start, value_start, inline);

        self.write("var ");
        self.write(&pattern_text);
        self.write(" = ");
        if let Some(initializer_text) = initializer_text {
            self.write(temp_name);
            self.write(" === void 0 ? ");
            self.write(&initializer_text);
            self.write(" : ");
            self.write(temp_name);
        } else {
            self.write(temp_name);
        }
        self.write(";");
        if inline {
            self.write_space();
        } else {
            self.write_line();
        }
    }

    fn emit_native_arrow_param_temp_declarations(
        &mut self,
        hoisted_start: usize,
        value_start: usize,
        inline: bool,
    ) {
        let value_temps: Vec<_> = self
            .hoisted_assignment_value_temps
            .drain(value_start..)
            .collect();
        if !value_temps.is_empty() {
            self.write("var ");
            self.write(&value_temps.join(", "));
            self.write(";");
            if inline {
                self.write_space();
            } else {
                self.write_line();
            }
        }

        let hoisted_temps: Vec<_> = self
            .hoisted_assignment_temps
            .drain(hoisted_start..)
            .collect();
        if !hoisted_temps.is_empty() {
            self.write("var ");
            self.write(&hoisted_temps.join(", "));
            self.write(";");
            if inline {
                self.write_space();
            } else {
                self.write_line();
            }
        }
    }

    fn emit_native_arrow_inline_block_body(&mut self, body: NodeIndex) {
        let Some(body_node) = self.arena.get(body) else {
            return;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return;
        };
        for &stmt_idx in &block.statements.nodes {
            let before_len = self.writer.len();
            self.emit(stmt_idx);
            if self.writer.len() > before_len {
                self.write_space();
            }
        }
    }

    fn emit_native_arrow_block_body_statements(&mut self, body: NodeIndex) {
        let Some(body_node) = self.arena.get(body) else {
            return;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return;
        };
        for &stmt_idx in &block.statements.nodes {
            let before_len = self.writer.len();
            self.emit(stmt_idx);
            if self.writer.len() > before_len {
                self.write_line();
            }
        }
    }

    fn emit_function_parameter_names_js(&mut self, params: &[NodeIndex]) {
        let mut first = true;
        for &param_idx in params {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            if param.name.is_none() || param.dot_dot_dot_token {
                continue;
            }
            if !first {
                self.write(", ");
            }
            first = false;
            self.emit_parameter_name_js(param.name);
        }
    }

    fn emit_native_default_param_prologue(&mut self, params: &[NodeIndex]) {
        for &param_idx in params {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                continue;
            };
            if param.initializer.is_none() {
                continue;
            }
            let name =
                crate::transforms::emit_utils::identifier_text_or_empty(self.arena, param.name);
            self.emit_param_default_assignment(&name, param.initializer);
        }
    }

    pub(in crate::emitter) fn remove_namespace_exported_parameter_name(
        &mut self,
        param_idx: NodeIndex,
    ) {
        if let Some(param) = self.arena.get_parameter_at(param_idx) {
            let name = self.get_identifier_text_idx(param.name);
            if !name.is_empty() {
                self.namespace_exported_names.remove(name.as_str());
            }
        }
    }

    pub(in crate::emitter) fn remove_namespace_exported_parameter_names(
        &mut self,
        params: &[NodeIndex],
    ) {
        for &param_idx in params {
            self.remove_namespace_exported_parameter_name(param_idx);
        }
    }

    fn native_arrow_default_params_need_temp_prologue(
        &self,
        func: &tsz_parser::parser::node::FunctionData,
    ) -> bool {
        if func.is_async || self.ctx.options.target.supports_es2020() {
            return false;
        }
        if !self.arrow_params_are_simple_identifiers(&func.parameters.nodes) {
            return false;
        }
        func.parameters.nodes.iter().copied().any(|param_idx| {
            let Some(param_node) = self.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                return false;
            };
            param.initializer.is_some()
                && self.param_initializer_generates_hoisted_temp(param.initializer)
        })
    }

    fn arrow_params_are_simple_identifiers(&self, params: &[NodeIndex]) -> bool {
        params.iter().copied().all(|param_idx| {
            let Some(param_node) = self.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                return false;
            };
            if param.dot_dot_dot_token || param.name.is_none() {
                return false;
            }
            self.arena
                .get(param.name)
                .is_some_and(|name| name.kind == tsz_scanner::SyntaxKind::Identifier as u16)
        })
    }

    pub(super) fn param_initializer_generates_hoisted_temp(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::CLASS_EXPRESSION
            && let Some(class) = self.arena.get_class(node)
            && self.class_expression_initializer_needs_temp_prologue(class)
        {
            return true;
        }

        if self.ctx.target_es5
            && node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            && let Some(literal) = self.arena.get_literal_expr(node)
            && literal.elements.nodes.iter().copied().any(|element| {
                crate::transforms::emit_utils::is_computed_property_member(self.arena, element)
            })
        {
            return true;
        }

        if let Some(binary) = self.arena.get_binary_expr(node) {
            if binary.operator_token == tsz_scanner::SyntaxKind::QuestionQuestionToken as u16
                && !self.is_simple_nullish_expression(binary.left)
            {
                return true;
            }
            return self.param_initializer_generates_hoisted_temp(binary.left)
                || self.param_initializer_generates_hoisted_temp(binary.right);
        }

        if let Some(access) = self.arena.get_access_expr(node) {
            if access.question_dot_token && !self.is_simple_nullish_expression(access.expression) {
                return true;
            }
            return self.param_initializer_generates_hoisted_temp(access.expression)
                || self.param_initializer_generates_hoisted_temp(access.name_or_argument);
        }

        if let Some(call) = self.arena.get_call_expr(node) {
            if node.is_optional_chain()
                && !self.optional_chain_call_uses_simple_receiver(call.expression)
                && !self.is_simple_nullish_expression(call.expression)
            {
                return true;
            }
            if self.param_initializer_generates_hoisted_temp(call.expression) {
                return true;
            }
            if let Some(args) = &call.arguments {
                return args
                    .nodes
                    .iter()
                    .copied()
                    .any(|arg| self.param_initializer_generates_hoisted_temp(arg));
            }
        }

        if let Some(paren) = self.arena.get_parenthesized(node) {
            return self.param_initializer_generates_hoisted_temp(paren.expression);
        }

        if let Some(assertion) = self.arena.get_type_assertion(node) {
            return self.param_initializer_generates_hoisted_temp(assertion.expression);
        }

        if let Some(cond) = self.arena.get_conditional_expr(node) {
            return self.param_initializer_generates_hoisted_temp(cond.condition)
                || self.param_initializer_generates_hoisted_temp(cond.when_true)
                || self.param_initializer_generates_hoisted_temp(cond.when_false);
        }

        if let Some(unary) = self.arena.get_unary_expr(node) {
            return self.param_initializer_generates_hoisted_temp(unary.operand);
        }

        if let Some(unary) = self.arena.get_unary_expr_ex(node) {
            return self.param_initializer_generates_hoisted_temp(unary.expression);
        }

        if let Some(literal) = self.arena.get_literal_expr(node) {
            return literal
                .elements
                .nodes
                .iter()
                .copied()
                .any(|element| self.param_initializer_generates_hoisted_temp(element));
        }

        false
    }

    fn optional_chain_call_uses_simple_receiver(&self, callee: NodeIndex) -> bool {
        let Some(callee_node) = self.arena.get(callee) else {
            return false;
        };
        let Some(access) = self.arena.get_access_expr(callee_node) else {
            return false;
        };
        access.question_dot_token && self.is_simple_nullish_expression(access.expression)
    }

    fn class_expression_initializer_needs_temp_prologue(
        &self,
        class: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        let target = self.ctx.options.target;
        let target_needs_field_lowering = (target as u32) < (ScriptTarget::ES2022 as u32)
            || !self.ctx.options.use_define_for_class_fields;
        let target_needs_static_block_lowering = (target as u32) < (ScriptTarget::ES2022 as u32);
        let needs_private_field_lowering =
            !target.supports_es2022() && target != ScriptTarget::ESNext;

        class.members.nodes.iter().copied().any(|member_idx| {
            let Some(member_node) = self.arena.get(member_idx) else {
                return false;
            };

            if target_needs_static_block_lowering
                && member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
            {
                return true;
            }

            if target_needs_field_lowering
                && member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                && let Some(prop) = self.arena.get_property_decl(member_node)
            {
                if self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                    || self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
                {
                    return false;
                }

                if self.arena.is_static(&prop.modifiers)
                    && (!is_private_identifier(self.arena, prop.name)
                        || needs_private_field_lowering)
                {
                    return true;
                }
            }

            needs_private_field_lowering && self.class_member_has_private_name(member_node)
        })
    }

    fn class_member_has_private_name(&self, member_node: &Node) -> bool {
        if let Some(prop) = self.arena.get_property_decl(member_node) {
            return is_private_identifier(self.arena, prop.name);
        }
        if let Some(method) = self.arena.get_method_decl(member_node) {
            return is_private_identifier(self.arena, method.name);
        }
        if let Some(accessor) = self.arena.get_accessor(member_node) {
            return is_private_identifier(self.arena, accessor.name);
        }
        false
    }

    fn async_arrow_needs_parameter_forwarding(&self, params: &[NodeIndex]) -> bool {
        params.iter().copied().any(|param_idx| {
            let Some(param_node) = self.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                return false;
            };
            self.arena.get(param.name).is_some_and(|name_node| {
                name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
            })
        })
    }

    /// Issue #3758: any parameter with a default initializer means tsc
    /// moves the entire parameter list into the generator function and
    /// forwards via `(...args_<n>) => __awaiter(..., [...args_<n>], ..., function* (<orig>) {})`
    /// so the default-initializer expression is evaluated lazily inside
    /// the generator (synchronous throws turn into rejected promises).
    fn async_arrow_has_default_param(&self, params: &[NodeIndex]) -> bool {
        params.iter().copied().any(|param_idx| {
            let Some(param_node) = self.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                return false;
            };
            param.initializer.is_some()
        })
    }

    fn async_arrow_forwarded_parameter_names(&mut self, params: &[NodeIndex]) -> Vec<String> {
        params
            .iter()
            .copied()
            .filter_map(|param_idx| {
                let param_node = self.arena.get(param_idx)?;
                let param = self.arena.get_parameter(param_node)?;
                let name_node = self.arena.get(param.name)?;
                if name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                    && let Some(ident) = self.arena.get_identifier(name_node)
                    && !ident.escaped_text.is_empty()
                {
                    return Some(
                        self.make_unique_name_from_base_in_temp_scope(&ident.escaped_text),
                    );
                }
                Some(self.make_unique_name())
            })
            .collect()
    }
}
