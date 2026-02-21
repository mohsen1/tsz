//! Class member emission: methods, properties, constructors, accessors.
//!
//! Handles class member modifiers, constructor prologue with parameter
//! properties and field initializers, and destructuring temp estimation.

use super::*;
use tsz_parser::parser::NodeList;

impl<'a> Printer<'a> {
    // =========================================================================
    // Class Members
    // =========================================================================

    /// Emit class member modifiers (static, public, private, etc.)
    pub(super) fn emit_class_member_modifiers(&mut self, modifiers: &Option<NodeList>) {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    // Emit the modifier keyword based on its kind
                    let keyword = match mod_node.kind as u32 {
                        k if k == SyntaxKind::StaticKeyword as u32 => "static",
                        k if k == SyntaxKind::PublicKeyword as u32 => "public",
                        k if k == SyntaxKind::PrivateKeyword as u32 => "private",
                        k if k == SyntaxKind::ProtectedKeyword as u32 => "protected",
                        k if k == SyntaxKind::ReadonlyKeyword as u32 => "readonly",
                        k if k == SyntaxKind::AbstractKeyword as u32 => "abstract",
                        k if k == SyntaxKind::OverrideKeyword as u32 => "override",
                        k if k == SyntaxKind::AsyncKeyword as u32 => "async",
                        k if k == SyntaxKind::DeclareKeyword as u32 => "declare",
                        _ => continue,
                    };
                    self.write(keyword);
                    self.write_space();
                }
            }
        }
    }

    pub(super) fn emit_method_declaration(&mut self, node: &Node) {
        let Some(method) = self.arena.get_method_decl(node) else {
            return;
        };

        // Parser recovery for `*() {}` can produce an identifier name token `"("`.
        // Treat that as an omitted name to match tsc emit.
        let has_recovery_missing_name = self.arena.get(method.name).is_some_and(|name_node| {
            self.arena
                .get_identifier(name_node)
                .is_some_and(|id| id.escaped_text == "(")
        });

        // Skip method declarations without bodies (TypeScript-only overloads)
        if method.body.is_none() {
            // Keep parse-recovery emit for invalid generator member `*() {}`.
            if method.asterisk_token && has_recovery_missing_name {
                self.write("*() { }");
            } else {
                self.skip_comments_for_erased_node(node);
            }
            return;
        }

        let is_async = self.has_modifier(&method.modifiers, SyntaxKind::AsyncKeyword as u16);
        let needs_async_lowering =
            is_async && self.ctx.needs_async_lowering && !method.asterisk_token;

        if needs_async_lowering {
            // Emit static modifier if present
            if self.has_modifier(&method.modifiers, SyntaxKind::StaticKeyword as u16) {
                self.write("static ");
            }
        } else {
            // Emit modifiers (static, async only for JavaScript)
            self.emit_method_modifiers_js(&method.modifiers);
        }

        // Emit generator asterisk
        if method.asterisk_token {
            self.write("*");
        }

        if method.name.is_some() && !has_recovery_missing_name {
            self.emit(method.name);
        }
        // Map opening `(` to its source position
        {
            let search_start = if method.name.is_some() {
                self.arena.get(method.name).map_or(node.pos, |n| n.end)
            } else {
                node.pos
            };
            self.map_token_after(search_start, node.end, b'(');
        }
        self.write("(");
        self.emit_function_parameters_js(&method.parameters.nodes);
        // Map closing `)` — scan backward from body start since parser may
        // include `)` in the last parameter node's range.
        {
            let search_start = method
                .parameters
                .nodes
                .first()
                .and_then(|&idx| self.arena.get(idx))
                .map_or(node.pos, |n| n.pos);
            let search_end = if method.body.is_some() {
                self.arena.get(method.body).map_or(node.end, |n| n.pos)
            } else {
                node.end
            };
            self.map_closing_paren_backward(search_start, search_end);
        }
        self.write(")");

        // Skip return type for JavaScript emit

        if needs_async_lowering {
            self.emit_method_async_lowered_body(method.body, &method.parameters.nodes);
        } else {
            self.write(" ");
            let prev_emitting_function_body_block = self.emitting_function_body_block;
            self.emitting_function_body_block = true;
            let prev_in_generator = self.ctx.flags.in_generator;
            self.ctx.block_scope_state.enter_scope();
            self.push_temp_scope();
            self.prepare_logical_assignment_value_temps(method.body);
            self.ctx.flags.in_generator = method.asterisk_token;
            self.emit(method.body);
            self.pop_temp_scope();
            self.ctx.block_scope_state.exit_scope();
            self.ctx.flags.in_generator = prev_in_generator;
            self.emitting_function_body_block = prev_emitting_function_body_block;
        }
    }

    /// Emit async method body lowered to __awaiter + function* for ES2015 target
    fn emit_method_async_lowered_body(&mut self, body: NodeIndex, params: &[NodeIndex]) {
        let params_have_top_level_await = params
            .iter()
            .copied()
            .any(|p| self.param_initializer_has_top_level_await(p));
        let move_params_to_generator = params_have_top_level_await;

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

        self.write(" {");
        self.write_line();
        self.increase_indent();

        self.write("return __awaiter(this");
        if move_params_to_generator {
            self.write(", arguments, void 0, function* (");
            let saved = self.ctx.emit_await_as_yield;
            self.ctx.emit_await_as_yield = true;
            self.emit_function_parameters_js(params);
            self.ctx.emit_await_as_yield = saved;
            if body_is_empty_single_line {
                self.write(") { });");
            } else {
                self.write(") {");
            }
        } else if body_is_empty_single_line {
            self.write(", void 0, void 0, function* () { });");
        } else {
            self.write(", void 0, void 0, function* () {");
        }

        if body_is_empty_single_line {
            self.write_line();
            self.decrease_indent();
            self.write("}");
            return;
        }

        self.write_line();
        self.increase_indent();

        // Emit function body with await→yield substitution
        self.ctx.emit_await_as_yield = true;
        if let Some(body_node) = self.arena.get(body)
            && let Some(block) = self.arena.get_block(body_node)
        {
            for &stmt in &block.statements.nodes {
                if let Some(stmt_node) = self.arena.get(stmt) {
                    let actual_start = self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
                    self.emit_comments_before_pos(actual_start);
                }
                self.emit(stmt);
                self.write_line();
            }
        }
        self.ctx.emit_await_as_yield = false;

        self.decrease_indent();
        self.write("});");
        self.write_line();
        self.decrease_indent();
        self.write("}");
    }

    /// Emit method modifiers for JavaScript (static, async only)
    pub(super) fn emit_method_modifiers_js(&mut self, modifiers: &Option<NodeList>) {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    match mod_node.kind {
                        k if k == SyntaxKind::StaticKeyword as u16 => self.write("static "),
                        k if k == SyntaxKind::AsyncKeyword as u16 => self.write("async "),
                        _ => {} // Skip private/protected/public/readonly/abstract
                    }
                }
            }
        }
    }

    pub(super) fn emit_property_declaration(&mut self, node: &Node) {
        let Some(prop) = self.arena.get_property_decl(node) else {
            return;
        };

        // Skip abstract property declarations (they don't exist at runtime)
        if self.has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword as u16) {
            self.skip_comments_for_erased_node(node);
            return;
        }

        // For JavaScript: Skip property declarations without initializers
        // (they are TypeScript-only declarations: typed props, bare props)
        // Exception: Private fields (#name) are always emitted — they are runtime declarations.
        // Exception: `accessor` fields are always emitted — they are ES2024 auto-accessors.
        // Exception: useDefineForClassFields (ES2022+) keeps uninitialised props as class fields.
        if prop.initializer.is_none() && !self.ctx.options.use_define_for_class_fields {
            let is_private = self
                .arena
                .get(prop.name)
                .is_some_and(|n| n.kind == SyntaxKind::PrivateIdentifier as u16);
            let has_accessor =
                self.has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword as u16);
            if !is_private && !has_accessor {
                self.skip_comments_for_erased_node(node);
                return;
            }
        }

        // Emit modifiers (static and accessor for JavaScript)
        self.emit_class_member_modifiers_js(&prop.modifiers);

        self.emit(prop.name);

        // Skip type annotations for JavaScript emit

        if prop.initializer.is_some() {
            self.write(" = ");
            self.emit(prop.initializer);
        }

        self.write_semicolon();
    }

    /// Emit class member modifiers for JavaScript (static and accessor are valid)
    pub(super) fn emit_class_member_modifiers_js(&mut self, modifiers: &Option<NodeList>) {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    if mod_node.kind == SyntaxKind::StaticKeyword as u16 {
                        self.write("static ");
                    } else if mod_node.kind == SyntaxKind::AccessorKeyword as u16 {
                        self.write("accessor ");
                    }
                }
            }
        }
    }

    pub(super) fn emit_constructor_declaration(&mut self, node: &Node) {
        let Some(ctor) = self.arena.get_constructor(node) else {
            return;
        };

        // Skip declaration-only constructors (no body).
        // These are overload signatures or ambient declarations, not emitted in JS.
        if ctor.body.is_none() {
            self.skip_comments_for_erased_node(node);
            return;
        }

        // Collect parameter property names (public/private/protected/readonly params)
        let param_props = self.collect_parameter_properties(&ctor.parameters.nodes);
        let field_inits = std::mem::take(&mut self.pending_class_field_inits);

        self.write("constructor");
        // Map opening `(` to its source position
        self.map_token_after(node.pos, node.end, b'(');
        self.write("(");
        self.emit_function_parameters_js(&ctor.parameters.nodes);
        // Map closing `)` to its source position
        {
            let search_start = ctor
                .parameters
                .nodes
                .last()
                .and_then(|&idx| self.arena.get(idx))
                .map_or(node.pos, |n| n.end);
            let search_end = if ctor.body.is_some() {
                self.arena.get(ctor.body).map_or(node.end, |n| n.pos)
            } else {
                node.end
            };
            self.map_token_after(search_start, search_end, b')');
        }
        self.write(")");
        self.write(" ");

        let prev_emitting_function_body_block = self.emitting_function_body_block;
        self.emitting_function_body_block = true;
        self.ctx.block_scope_state.enter_scope();
        self.push_temp_scope();
        if let Some(body_node) = self.arena.get(ctor.body) {
            let temp_count = self.estimate_assignment_destructuring_temps_in_constructor(body_node);
            if temp_count > 0 {
                self.preallocate_assignment_temps(temp_count);
            }
        }
        self.prepare_logical_assignment_value_temps(ctor.body);
        self.emit_constructor_body_with_prologue(ctor.body, &param_props, &field_inits);
        self.pop_temp_scope();
        self.ctx.block_scope_state.exit_scope();
        self.emitting_function_body_block = prev_emitting_function_body_block;
    }

    /// Collect parameter property names from constructor parameters.
    /// Returns names of parameters that have accessibility modifiers (public/private/protected/readonly).
    pub(super) fn collect_parameter_properties(&self, params: &[NodeIndex]) -> Vec<String> {
        let mut names = Vec::new();
        for &param_idx in params {
            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_parameter(param_node)
                && self.has_parameter_property_modifier(&param.modifiers)
            {
                let name = self.get_identifier_text_idx(param.name);
                if !name.is_empty() {
                    names.push(name);
                }
            }
        }
        names
    }

    /// Check if parameter modifiers include an accessibility or readonly modifier.
    fn has_parameter_property_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    let kind = mod_node.kind as u32;
                    if kind == SyntaxKind::PublicKeyword as u32
                        || kind == SyntaxKind::PrivateKeyword as u32
                        || kind == SyntaxKind::ProtectedKeyword as u32
                        || kind == SyntaxKind::ReadonlyKeyword as u32
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Emit constructor body block with parameter property and field initializer assignments.
    fn emit_constructor_body_with_prologue(
        &mut self,
        block_idx: NodeIndex,
        param_props: &[String],
        field_inits: &[(String, NodeIndex)],
    ) {
        let Some(block_node) = self.arena.get(block_idx) else {
            return;
        };
        let Some(block) = self.arena.get_block(block_node) else {
            return;
        };

        let has_function_temps = !self.hoisted_assignment_temps.is_empty()
            || !self.hoisted_assignment_value_temps.is_empty()
            || !self.hoisted_for_of_temps.is_empty();

        // Empty constructor with no prologue: check source format
        if block.statements.nodes.is_empty()
            && param_props.is_empty()
            && field_inits.is_empty()
            && !has_function_temps
        {
            // TypeScript preserves the source formatting: if the body was
            // on a single line in the source (e.g. `{ }`), keep it single-line.
            // If it was multi-line, emit multi-line with empty body.
            if self.is_single_line(block_node) {
                self.write("{ }");
            } else {
                self.write("{");
                self.write_line();
                self.write("}");
            }
            return;
        }

        self.write("{");
        self.write_line();
        self.increase_indent();

        if has_function_temps {
            self.emit_function_body_hoisted_temps();
        }

        let has_prologue = !param_props.is_empty() || !field_inits.is_empty();

        // Find the super() call index so we can emit prologue after it.
        // In derived class constructors, super() must be called before
        // accessing `this`, so param property and field init assignments
        // go after the super() call.
        let super_call_idx = if has_prologue {
            block.statements.nodes.iter().position(|&stmt_idx| {
                self.arena.get(stmt_idx).is_some_and(|stmt_node| {
                    stmt_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
                        && self
                            .arena
                            .get_expression_statement(stmt_node)
                            .is_some_and(|expr_stmt| {
                                self.arena
                                    .get(expr_stmt.expression)
                                    .is_some_and(|expr_node| {
                                        expr_node.kind == syntax_kind_ext::CALL_EXPRESSION
                                            && self.arena.get_call_expr(expr_node).is_some_and(
                                                |call| {
                                                    self.arena.get(call.expression).is_some_and(
                                                        |callee| {
                                                            callee.kind
                                                == tsz_scanner::SyntaxKind::SuperKeyword as u16
                                                        },
                                                    )
                                                },
                                            )
                                    })
                            })
                })
            })
        } else {
            None
        };

        // Emit original body statements, inserting prologue after super() if present
        let mut prologue_emitted = !has_prologue;
        for (stmt_i, &stmt_idx) in block.statements.nodes.iter().enumerate() {
            if let Some(stmt_node) = self.arena.get(stmt_idx) {
                let actual_start = self.skip_whitespace_forward(stmt_node.pos, stmt_node.end);
                self.emit_comments_before_pos(actual_start);
            }

            // If no super() call exists, emit prologue before first body statement
            if !prologue_emitted && super_call_idx.is_none() && stmt_i == 0 {
                self.emit_constructor_prologue(param_props, field_inits);
                prologue_emitted = true;
            }

            let before_len = self.writer.len();
            self.emit(stmt_idx);
            if self.writer.len() > before_len {
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    let token_end = self.find_token_end_before_trivia(stmt_node.pos, stmt_node.end);
                    self.emit_trailing_comments(token_end);
                }
                self.write_line();
            }

            // Emit prologue after super() call
            if !prologue_emitted && super_call_idx == Some(stmt_i) {
                self.emit_constructor_prologue(param_props, field_inits);
                prologue_emitted = true;
            }
        }

        // If we never emitted the prologue (empty body or no super), emit it now
        if !prologue_emitted {
            self.emit_constructor_prologue(param_props, field_inits);
        }

        self.decrease_indent();
        self.write("}");
    }

    /// Emit parameter property and field initializer assignments (constructor prologue).
    fn emit_constructor_prologue(
        &mut self,
        param_props: &[String],
        field_inits: &[(String, NodeIndex)],
    ) {
        for name in param_props {
            self.write("this.");
            self.write(name);
            self.write(" = ");
            self.write(name);
            self.write(";");
            self.write_line();
        }
        for (name, init_idx) in field_inits {
            if self.ctx.options.use_define_for_class_fields {
                self.write("Object.defineProperty(this, ");
                self.emit_string_literal_text(name);
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
                self.emit_expression(*init_idx);
                self.write_line();
                self.decrease_indent();
                self.write("});");
            } else {
                self.write("this.");
                self.write(name);
                self.write(" = ");
                self.emit_expression(*init_idx);
                self.write(";");
            }
            self.write_line();
        }
    }

    fn estimate_assignment_destructuring_temps_in_constructor(&self, node: &Node) -> usize {
        match node.kind {
            kind if kind == syntax_kind_ext::BLOCK => {
                let Some(block) = self.arena.get_block(node) else {
                    return 0;
                };
                let mut count = 0;
                for &stmt_idx in &block.statements.nodes {
                    count += self.estimate_constructor_assignment_temps_in_statement(stmt_idx);
                }
                count
            }
            _ => 0,
        }
    }

    fn estimate_constructor_assignment_temps_in_statement(&self, stmt_idx: NodeIndex) -> usize {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return 0;
        };

        match stmt_node.kind {
            kind if kind == syntax_kind_ext::EXPRESSION_STATEMENT => {
                let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
                    return 0;
                };
                self.estimate_destructuring_assignment_temps(expr_stmt.expression)
            }
            kind if kind == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.estimate_variable_decl_destructuring_temps(stmt_node)
            }
            kind if kind == syntax_kind_ext::BLOCK => {
                self.estimate_assignment_destructuring_temps_in_constructor(stmt_node)
            }
            _ => 0,
        }
    }

    fn estimate_variable_decl_destructuring_temps(&self, node: &Node) -> usize {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return 0;
        };
        let mut count = 0;
        for &decl_idx in &var_stmt.declarations.nodes {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if decl.initializer.is_none() {
                continue;
            }
            let Some(left_node) = self.arena.get(decl.name) else {
                continue;
            };
            if left_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
                && left_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
            {
                continue;
            }
            let is_simple = self
                .arena
                .get(decl.initializer)
                .is_some_and(|n| n.kind == SyntaxKind::Identifier as u16);
            if !is_simple {
                count += 1;
            }
        }
        count
    }

    fn estimate_destructuring_assignment_temps(&self, node_idx: NodeIndex) -> usize {
        let Some(node) = self.arena.get(node_idx) else {
            return 0;
        };
        match node.kind {
            kind if kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let Some(paren) = self.arena.get_parenthesized(node) else {
                    return 0;
                };
                self.estimate_destructuring_assignment_temps(paren.expression)
            }
            kind if kind == syntax_kind_ext::BINARY_EXPRESSION => {
                let Some(binary) = self.arena.get_binary_expr(node) else {
                    return 0;
                };
                let right_is_simple = self
                    .arena
                    .get(binary.right)
                    .is_some_and(|n| n.kind == SyntaxKind::Identifier as u16);
                let left = self.arena.get(binary.left);
                if binary.operator_token == SyntaxKind::CommaToken as u16 {
                    self.estimate_destructuring_assignment_temps(binary.left)
                        + self.estimate_destructuring_assignment_temps(binary.right)
                } else if binary.operator_token == SyntaxKind::EqualsToken as u16
                    && let Some(left_node) = left
                {
                    if matches!(
                        left_node.kind,
                        syntax_kind_ext::ARRAY_BINDING_PATTERN
                            | syntax_kind_ext::OBJECT_BINDING_PATTERN
                            | syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    ) {
                        self.estimate_destructuring_pattern_temps(left_node, right_is_simple)
                    } else {
                        0
                    }
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    fn estimate_destructuring_pattern_temps(
        &self,
        pattern_node: &Node,
        rhs_is_simple: bool,
    ) -> usize {
        match pattern_node.kind {
            kind if kind == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
                    return 0;
                };
                let needs_temp = !rhs_is_simple;
                let mut count = if needs_temp { 1 } else { 0 };
                for &elem_idx in &pattern.elements.nodes {
                    if elem_idx.is_none() {
                        continue;
                    }
                    let Some(elem_node) = self.arena.get(elem_idx) else {
                        continue;
                    };
                    if let Some(elem) = self.arena.get_binding_element(elem_node) {
                        let target = self.arena.get(elem.name);
                        if let Some(target_node) = target
                            && (target_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                || target_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN)
                        {
                            count += self.estimate_destructuring_pattern_temps(target_node, false);
                        }
                        if let Some(bin) = self.arena.get_binary_expr(elem_node)
                            && bin.operator_token == SyntaxKind::EqualsToken as u16
                        {
                            let rhs_node = self.arena.get(bin.right);
                            if rhs_node.is_some_and(|n| n.kind != SyntaxKind::Identifier as u16) {
                                count += 1;
                            }
                        }
                    }
                }
                count
            }
            kind if kind == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
                    return 0;
                };
                let needs_temp = !rhs_is_simple && !pattern.elements.nodes.is_empty();
                let mut count = if needs_temp { 1 } else { 0 };
                for &elem_idx in &pattern.elements.nodes {
                    if elem_idx.is_none() {
                        continue;
                    }
                    let Some(elem_node) = self.arena.get(elem_idx) else {
                        continue;
                    };
                    if let Some(prop) = self.arena.get_property_assignment(elem_node)
                        && let Some(value_node) = self.arena.get(prop.initializer)
                    {
                        if matches!(
                            value_node.kind,
                            syntax_kind_ext::ARRAY_BINDING_PATTERN
                                | syntax_kind_ext::OBJECT_BINDING_PATTERN
                        ) {
                            count += self.estimate_destructuring_pattern_temps(value_node, false);
                        } else if value_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                            && let Some(bin) = self.arena.get_binary_expr(value_node)
                            && bin.operator_token == SyntaxKind::EqualsToken as u16
                        {
                            let left = self.arena.get(bin.left);
                            if let Some(left_node) = left {
                                if matches!(
                                    left_node.kind,
                                    syntax_kind_ext::ARRAY_BINDING_PATTERN
                                        | syntax_kind_ext::OBJECT_BINDING_PATTERN
                                ) {
                                    count +=
                                        self.estimate_destructuring_pattern_temps(left_node, false);
                                } else {
                                    count += 1;
                                }
                            } else {
                                count += 1;
                            }
                        }
                    }
                    if let Some(bin) = self.arena.get_binary_expr(elem_node)
                        && bin.operator_token == SyntaxKind::EqualsToken as u16
                        && let Some(bin_right) = self.arena.get(bin.right)
                        && bin_right.kind != SyntaxKind::Identifier as u16
                    {
                        count += 1;
                    }
                }
                count
            }
            _ => 0,
        }
    }

    pub(super) fn emit_get_accessor(&mut self, node: &Node) {
        let Some(accessor) = self.arena.get_accessor(node) else {
            return;
        };

        // Emit modifiers (static only for JavaScript)
        self.emit_class_member_modifiers_js(&accessor.modifiers);

        self.write("get ");
        self.emit(accessor.name);
        self.write("()");

        // Skip type annotation for JS emit

        if accessor.body.is_some() {
            let prev_emitting_function_body_block = self.emitting_function_body_block;
            self.emitting_function_body_block = true;
            self.ctx.block_scope_state.enter_scope();
            self.push_temp_scope();
            self.prepare_logical_assignment_value_temps(accessor.body);
            self.write(" ");
            self.emit(accessor.body);
            self.pop_temp_scope();
            self.ctx.block_scope_state.exit_scope();
            self.emitting_function_body_block = prev_emitting_function_body_block;
        } else {
            // For JS emit, add empty body for accessors without body
            self.write(" { }");
        }
    }

    pub(super) fn emit_set_accessor(&mut self, node: &Node) {
        let Some(accessor) = self.arena.get_accessor(node) else {
            return;
        };

        // Emit modifiers (static only for JavaScript)
        self.emit_class_member_modifiers_js(&accessor.modifiers);

        self.write("set ");
        self.emit(accessor.name);
        self.write("(");
        self.emit_function_parameters_js(&accessor.parameters.nodes);
        // Map closing `)` — scan backward from body start
        if let Some(body_node) = self.arena.get(accessor.body) {
            let search_start = accessor
                .parameters
                .nodes
                .first()
                .and_then(|&idx| self.arena.get(idx))
                .map_or(node.pos, |n| n.pos);
            self.map_closing_paren_backward(search_start, body_node.pos);
        }
        self.write(")");

        if accessor.body.is_some() {
            let prev_emitting_function_body_block = self.emitting_function_body_block;
            self.emitting_function_body_block = true;
            self.ctx.block_scope_state.enter_scope();
            self.push_temp_scope();
            self.prepare_logical_assignment_value_temps(accessor.body);
            self.write(" ");
            self.emit(accessor.body);
            self.pop_temp_scope();
            self.ctx.block_scope_state.exit_scope();
            self.emitting_function_body_block = prev_emitting_function_body_block;
        } else {
            // For JS emit, add empty body for accessors without body
            self.write(" { }");
        }
    }
}
