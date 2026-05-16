use super::Printer;
use tsz_common::ScriptTarget;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::syntax::transform_utils::{
    contains_arguments_reference, contains_this_reference, is_private_identifier,
};
use tsz_scanner::SyntaxKind;

enum NativeArrowParamPrologueEntry {
    Default {
        name: String,
        initializer: NodeIndex,
    },
    Binding {
        pattern: NodeIndex,
        temp_name: String,
        initializer: NodeIndex,
    },
}

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

        self.emit_arrow_function_native(func);
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
    fn emit_arrow_function_native(&mut self, func: &tsz_parser::parser::node::FunctionData) {
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
            self.write("(");
            self.emit(func.body);
            self.write(")");
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
                    return Some(self.make_unique_name_from_base(&ident.escaped_text));
                }
                Some(self.make_unique_name())
            })
            .collect()
    }

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
                if body_is_single_line && !has_forwarded_object_rest_param_prologue {
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
                if body_is_single_line && !has_object_rest_param_prologue {
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
                    self.emit_object_rest_param_prologue_entries(&object_rest_param_prologue);
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

        if body_is_single_line && !has_object_rest_param_prologue {
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
                for &stmt in &statements.nodes {
                    self.emit(stmt);
                    self.write_line();
                }
            }
        }

        self.ctx.emit_await_as_yield = saved_yield;
        self.ctx.rewrite_arguments_to_arguments_1 = saved_args;
        self.ctx.arguments_capture_name = saved_arguments_capture_name;

        self.decrease_indent();
        self.write("})");
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
        let args_name = self.make_unique_name_from_base("args");
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

        let saved_yield = self.ctx.emit_await_as_yield;
        let saved_args = self.ctx.rewrite_arguments_to_arguments_1;
        let saved_arguments_capture_name = self.ctx.arguments_capture_name.clone();
        self.ctx.emit_await_as_yield = true;
        if let Some(capture_name) = arguments_capture_name.clone() {
            self.ctx.rewrite_arguments_to_arguments_1 = true;
            self.ctx.arguments_capture_name = Some(capture_name);
        }
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
}
