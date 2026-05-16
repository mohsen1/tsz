//! ES5 async function, class expression, template, and spread call emission.
//!
//! Contains async function body transformation (__awaiter/__generator),
//! ES5 function parameter lowering, class expression IIFE emission,
//! tagged template support, and spread call lowering.

use super::super::*;
use super::helpers::ArraySegment;
use crate::emitter::core::PropertyNameEmit;
use crate::emitter::declarations::class::replace_identifier;
use crate::transforms::emit_utils;
use std::sync::Arc;

#[derive(Clone)]
pub(in crate::emitter) struct Es5StaticClassExpressionField {
    pub name_emit: PropertyNameEmit,
    pub initializer: NodeIndex,
    pub member_pos: u32,
}

#[derive(Clone)]
pub(in crate::emitter) enum Es5StaticClassExpressionElement {
    Field(Es5StaticClassExpressionField),
    StaticBlock {
        block: NodeIndex,
        saved_comment_idx: usize,
        member_pos: u32,
    },
}

impl Es5StaticClassExpressionElement {
    const fn member_pos(&self) -> u32 {
        match self {
            Self::Field(field) => field.member_pos,
            Self::StaticBlock { member_pos, .. } => *member_pos,
        }
    }
}

fn avoid_generator_state_collision(segment: &str, class_temp: &str) -> String {
    if class_temp != "_a" || !segment.contains("function (_a)") {
        return segment.to_string();
    }

    segment
        .replace("function (_a)", "function (_b)")
        .replace("_a.label", "_b.label")
        .replace("_a.sent()", "_b.sent()")
}

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

    fn get_class_expression_name(&self, class_node: NodeIndex) -> Option<String> {
        let mut current = class_node;
        let mut hops = 0;

        while hops < 8 {
            let parent = self.arena.get_extended(current)?.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.arena.get(parent)?;

            match parent_node.kind {
                // Parenthesized class expressions can be unwrapped.
                syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    current = parent;
                    hops += 1;
                    continue;
                }
                // `const C = class {}`
                syntax_kind_ext::VARIABLE_DECLARATION => {
                    let decl = self.arena.get_variable_declaration(parent_node)?;
                    if decl.initializer != current {
                        return None;
                    }
                    let name = emit_utils::identifier_text_or_empty(self.arena, decl.name);
                    if name.is_empty() || !is_valid_identifier_name(&name) {
                        return None;
                    }
                    return Some(name);
                }
                // `C = class {}`
                syntax_kind_ext::BINARY_EXPRESSION => {
                    let binary = self.arena.get_binary_expr(parent_node)?;
                    if binary.right != current {
                        return None;
                    }
                    if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                        return None;
                    }
                    let name = emit_utils::identifier_text_or_empty(self.arena, binary.left);
                    if name.is_empty() || !is_valid_identifier_name(&name) {
                        return None;
                    }
                    return Some(name);
                }
                _ => return None,
            }
        }

        None
    }

    pub(in crate::emitter) fn es5_static_class_expression_elements(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> Vec<Es5StaticClassExpressionElement> {
        let mut inits = Vec::new();

        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                inits.push(Es5StaticClassExpressionElement::StaticBlock {
                    block: member_idx,
                    saved_comment_idx: self.static_block_inner_comment_index(member_node),
                    member_pos: member_node.pos,
                });
                continue;
            }
            if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                continue;
            }
            let Some(prop) = self.arena.get_property_decl(member_node) else {
                continue;
            };
            if prop.initializer.is_none()
                || !self.has_effective_static_modifier_js(&prop.modifiers)
                || self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                || self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                || self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
                || self
                    .arena
                    .get(prop.name)
                    .is_some_and(|n| n.kind == SyntaxKind::PrivateIdentifier as u16)
            {
                continue;
            }

            if let Some(name_emit) = self.get_property_name_emit(prop.name) {
                inits.push(Es5StaticClassExpressionElement::Field(
                    Es5StaticClassExpressionField {
                        name_emit,
                        initializer: prop.initializer,
                        member_pos: member_node.pos,
                    },
                ));
            }
        }

        inits.sort_by_key(Es5StaticClassExpressionElement::member_pos);
        inits
    }

    fn static_block_inner_comment_index(&self, member_node: &Node) -> usize {
        let brace_pos = if let Some(text) = self.source_text_for_map() {
            let bytes = text.as_bytes();
            let start = std::cmp::min(member_node.pos as usize, bytes.len());
            let end = std::cmp::min(member_node.end as usize, bytes.len());
            bytes[start..end]
                .iter()
                .position(|&byte| byte == b'{')
                .map(|offset| (start + offset + 1) as u32)
                .unwrap_or(member_node.end)
        } else {
            member_node.end
        };
        let mut idx = self.comment_emit_idx;
        while idx < self.all_comments.len() && self.all_comments[idx].end <= brace_pos {
            idx += 1;
        }
        idx
    }

    fn es5_class_iife_expression_from_var(output: &str, class_name: &str) -> Option<String> {
        let prefix = format!("var {class_name} = ");
        let output = output.trim_end();
        let output = output.strip_suffix(';').unwrap_or(output);
        output.strip_prefix(&prefix).map(str::to_string)
    }

    fn write_multiline_fragment(&mut self, text: &str) {
        let extra_indent = if self.writer.indent_level() > 0 {
            " ".repeat((self.writer.indent_width() * (self.writer.indent_level() + 2)) as usize)
        } else {
            String::new()
        };
        let mut lines = text.lines();
        if let Some(first) = lines.next() {
            self.write(first);
        }
        let lines: Vec<&str> = lines.collect();
        let strip_indent = lines
            .iter()
            .filter(|line| !line.is_empty())
            .map(|line| line.len() - line.trim_start_matches(' ').len())
            .min()
            .unwrap_or(0);
        for line in lines {
            self.write_line();
            if !line.is_empty() {
                self.write(&extra_indent);
                self.write(line.get(strip_indent..).unwrap_or(line));
            }
        }
    }

    fn class_expression_static_comma_needs_parens(&self, class_node: NodeIndex) -> bool {
        self.arena
            .get_extended(class_node)
            .and_then(|ext| self.arena.get(ext.parent))
            .is_none_or(|parent| {
                parent.kind != syntax_kind_ext::RETURN_STATEMENT
                    && parent.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION
            })
    }

    fn emit_es5_static_class_expression_comma(
        &mut self,
        class_node: NodeIndex,
        class_name: &str,
        class_iife_expr: &str,
        static_elements: &[Es5StaticClassExpressionElement],
        set_function_name: Option<&str>,
    ) {
        let needs_parens = self.class_expression_static_comma_needs_parens(class_node);
        let temp = if self.class_expression_is_in_loop_body(class_node) {
            let temp = self.make_unique_name();
            self.block_scoped_private_temps.push(temp.clone());
            temp
        } else {
            self.make_unique_name_hoisted()
        };

        if needs_parens {
            self.write("(");
        }
        self.write(&temp);
        self.write(" = ");
        self.write_multiline_fragment(class_iife_expr);

        if let Some(name) = set_function_name {
            self.emit_class_expr_set_function_name_comma_item(&temp, name);
        }

        for element in static_elements {
            match element {
                Es5StaticClassExpressionElement::Field(field) => {
                    self.write(",");
                    self.write_line();
                    self.increase_indent();
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
                        let replaced = avoid_generator_state_collision(&replaced, &temp);
                        if replaced != segment {
                            self.writer.truncate(before);
                            self.write(&replaced);
                        }
                    } else {
                        let full = self.writer.get_output().to_string();
                        let segment = &full[before..after];
                        let replaced = avoid_generator_state_collision(segment, &temp);
                        if replaced != segment {
                            self.writer.truncate(before);
                            self.write(&replaced);
                        }
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
        let any_param_needs_forwarding = use_native_generators
            && params.iter().copied().any(|p| {
                let Some(node) = self.arena.get(p) else {
                    return false;
                };
                let Some(param) = self.arena.get_parameter(node) else {
                    return false;
                };
                // Default initializer
                if param.initializer.is_some() {
                    return true;
                }
                // Destructuring pattern (name is not a simple identifier)
                if let Some(name_node) = self.arena.get(param.name)
                    && name_node.kind != tsz_scanner::SyntaxKind::Identifier as u16
                {
                    return true;
                }
                false
            });
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
            let blocked_disposable_names = self
                .file_identifiers
                .iter()
                .chain(self.generated_temp_names.iter())
                .cloned()
                .collect::<Vec<_>>();
            async_emitter
                .set_disposable_env_context(self.next_disposable_env_id, blocked_disposable_names);

            let body_has_await = async_emitter.body_contains_await(body);
            let body_is_single_line = self.arena.get(body).is_some_and(|n| self.is_single_line(n));
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

            let (generator_body, hoisted_var_groups, directive_prologue) = if body_has_await {
                async_emitter.emit_generator_body_with_await_and_hoisted_var_groups(body)
            } else {
                let (generator_body, hoisted_var_groups) =
                    async_emitter.emit_simple_generator_body_with_hoisted_var_groups(body);
                (generator_body, hoisted_var_groups, Vec::new())
            };
            let generator_mappings = async_emitter.take_mappings();
            self.next_disposable_env_id = async_emitter.disposable_env_counter();
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
        let generator_hoist_byte_offset = self.writer.len();
        let generator_hoist_line = self.writer.current_line();
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
        let mut ref_vars = Vec::new();
        ref_vars.extend(
            self.hoisted_assignment_temps
                .drain(hoisted_assignment_start..),
        );
        ref_vars.extend(self.hoisted_for_of_temps.drain(hoisted_for_of_start..));
        if !ref_vars.is_empty() {
            let indent = " ".repeat(self.writer.indent_width() as usize);
            let var_decl = format!("{}var {};", indent, ref_vars.join(", "));
            self.writer.insert_line_at(
                generator_hoist_byte_offset,
                generator_hoist_line,
                &var_decl,
            );
        }
        if !self.hoisted_assignment_value_temps[hoisted_value_start..].is_empty() {
            let value_vars = self
                .hoisted_assignment_value_temps
                .drain(hoisted_value_start..)
                .collect::<Vec<_>>();
            let indent = " ".repeat(self.writer.indent_width() as usize);
            let var_decl = format!("{}var {};", indent, value_vars.join(", "));
            self.writer.insert_line_at(
                generator_hoist_byte_offset,
                generator_hoist_line,
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
        let ir = transformer.transform_generator_function(function_node);
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
                && name != "Promise"
                && name != "PromiseLike"
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
                self.make_unique_name_from_base(&name)
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

    /// Emit an ES5-compatible class expression by wrapping the class IIFE in an expression.
    pub(in crate::emitter) fn emit_class_expression_es5(&mut self, class_node: NodeIndex) {
        let Some(node) = self.arena.get(class_node) else {
            return;
        };
        let Some(class_data) = self.arena.get_class(node) else {
            return;
        };

        let static_elements = self.es5_static_class_expression_elements(class_data);

        let mut es5_emitter = ClassES5Emitter::new(self.arena);
        es5_emitter.set_temp_var_counter(self.ctx.destructuring_state.temp_var_counter);
        es5_emitter.set_indent_level(0);
        // Pass transform directives to the ClassES5Emitter
        es5_emitter.set_transforms(self.transforms.clone());
        es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
        es5_emitter.set_printer_options(self.ctx.options.clone());
        es5_emitter.set_module_kind(
            self.ctx
                .original_module_kind
                .unwrap_or(self.ctx.options.module),
        );
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
        let class_expr_set_function_name = if class_data.name.is_none() {
            self.resolve_class_expr_binding_name(class_node)
        } else {
            None
        };
        let use_static_comma =
            !static_elements.is_empty() && !self.ctx.options.use_define_for_class_fields;
        if use_static_comma {
            es5_emitter.set_skip_static_members(true);
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
        } else if use_static_comma {
            let temp_name = self.make_unique_name_from_base("class");
            let output = es5_emitter.emit_class_with_name(class_node, &temp_name);
            (temp_name, output)
        } else {
            let temp_name = self
                .get_class_expression_name(class_node)
                .unwrap_or_else(|| self.get_temp_var_name());
            let output = es5_emitter.emit_class_with_name(class_node, &temp_name);
            (temp_name, output)
        };
        self.ctx.destructuring_state.temp_var_counter = es5_emitter.temp_var_counter();
        let es5_mappings = es5_emitter.take_mappings();

        if use_static_comma
            && let Some(class_iife_expr) =
                Self::es5_class_iife_expression_from_var(&es5_output, &class_name)
        {
            self.emit_es5_static_class_expression_comma(
                class_node,
                &class_name,
                &class_iife_expr,
                &static_elements,
                class_expr_set_function_name.as_deref(),
            );
            return;
        }

        if class_data.name.is_some()
            && let Some(class_iife_expr) =
                Self::es5_class_iife_expression_from_var(&es5_output, &class_name)
        {
            self.write_multiline_fragment(&class_iife_expr);
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

        // Emit: obj.method.apply(obj, args_array)
        self.emit(access.expression);
        self.write(".");
        self.emit(access.name_or_argument);
        self.write(".apply(");
        self.emit(access.expression);
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
