//! Class member emission: methods, properties, constructors, accessors.
//!
//! Handles class member modifiers, constructor prologue with parameter
//! properties and field initializers, and destructuring temp estimation.

use super::super::*;
use tsz_parser::parser::NodeList;

impl<'a> Printer<'a> {
    // =========================================================================
    // Class Members
    // =========================================================================

    /// Emit class member modifiers (static, public, private, etc.)
    pub(in crate::emitter) fn emit_class_member_modifiers(&mut self, modifiers: &Option<NodeList>) {
        if let Some(mods) = modifiers {
            // When there are duplicate static modifiers (parse error recovery),
            // suppress all static output to match tsc behavior.
            let static_count = mods
                .nodes
                .iter()
                .filter(|&&idx| {
                    self.arena
                        .get(idx)
                        .is_some_and(|n| n.kind == SyntaxKind::StaticKeyword as u16)
                })
                .count();
            let suppress_static = static_count > 1;

            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    // Emit the modifier keyword based on its kind
                    let keyword = match mod_node.kind as u32 {
                        k if k == SyntaxKind::StaticKeyword as u32 => {
                            if suppress_static {
                                continue;
                            }
                            "static"
                        }
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

    pub(in crate::emitter) fn emit_method_declaration(&mut self, node: &Node) {
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

        let is_async = self
            .arena
            .has_modifier(&method.modifiers, SyntaxKind::AsyncKeyword);
        let needs_async_lowering =
            is_async && self.ctx.needs_async_lowering && !method.asterisk_token;
        let needs_async_generator_lowering =
            is_async && self.ctx.needs_async_lowering && method.asterisk_token;

        if needs_async_lowering || needs_async_generator_lowering {
            // Emit static modifier if present
            if self
                .arena
                .has_modifier(&method.modifiers, SyntaxKind::StaticKeyword)
            {
                self.write("static ");
            }
        } else {
            // Emit modifiers (static, async only for JavaScript)
            self.emit_method_modifiers_js(&method.modifiers);
        }

        // Emit generator asterisk (skip for async generators being lowered)
        if method.asterisk_token && !needs_async_generator_lowering {
            self.write("*");
        }

        if method.name.is_some() && !has_recovery_missing_name {
            self.emit(method.name);
        }

        // Skip comments inside type parameter list (e.g., `<T, U /*extends T*/>`)
        // since type parameters are stripped in JS output — mirrors emit_function_declaration.
        if !self.ctx.flags.in_declaration_emit
            && let Some(ref type_params) = method.type_parameters
        {
            for &tp_idx in &type_params.nodes {
                if let Some(tp_node) = self.arena.get(tp_idx) {
                    self.skip_comments_in_range(tp_node.pos, tp_node.end);
                }
            }
        }

        // Map opening `(` to its source position
        let open_paren_pos = {
            let search_start = if let Some(ref tp) = method.type_parameters {
                // After type parameters, search for `(` past the closing `>`
                tp.nodes
                    .last()
                    .and_then(|&idx| self.arena.get(idx))
                    .map_or(node.pos, |n| n.end)
            } else if method.name.is_some() {
                self.arena.get(method.name).map_or(node.pos, |n| n.end)
            } else {
                node.pos
            };
            self.map_token_after(search_start, node.end, b'(');
            self.pending_source_pos
                .map(|source_pos| source_pos.pos)
                .unwrap_or(search_start)
        };
        self.write("(");
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
        self.emit_function_parameters_with_trailing_comments(
            &method.parameters.nodes,
            open_paren_pos,
            search_start,
            search_end,
        );
        self.write(")");

        // Skip return type for JavaScript emit — skip comments inside erased return type
        if !self.ctx.flags.in_declaration_emit
            && method.type_annotation.is_some()
            && let Some(type_node) = self.arena.get(method.type_annotation)
        {
            self.skip_comments_in_range(type_node.pos, type_node.end);
        }

        if needs_async_generator_lowering {
            self.emit_method_async_generator_lowered_body(method.body, method.name);
        } else if needs_async_lowering {
            self.emit_method_async_lowered_body(method.body, &method.parameters.nodes);
        } else {
            self.write(" ");
            let prev_emitting_function_body_block = self.emitting_function_body_block;
            self.emitting_function_body_block = true;
            self.function_scope_depth += 1;
            let prev_in_generator = self.ctx.flags.in_generator;
            self.ctx.block_scope_state.enter_scope();
            self.push_temp_scope();
            let prev_declared = std::mem::take(&mut self.declared_namespace_names);
            self.prepare_logical_assignment_value_temps(method.body);
            self.ctx.flags.in_generator = method.asterisk_token;
            self.emit(method.body);
            self.declared_namespace_names = prev_declared;
            self.pop_temp_scope();
            self.ctx.block_scope_state.exit_scope();
            self.ctx.flags.in_generator = prev_in_generator;
            self.function_scope_depth -= 1;
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

        self.write("return ");
        self.write_helper("__awaiter");
        self.write("(this");
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

    /// Emit async generator method body lowered to __asyncGenerator for ES2015 target.
    /// `async *f() { ... }` becomes `f() { return __asyncGenerator(this, arguments, function* f_1() { ... }); }`
    fn emit_method_async_generator_lowered_body(&mut self, body: NodeIndex, name_idx: NodeIndex) {
        let method_name = if name_idx.is_some() {
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, name_idx)
        } else {
            String::new()
        };

        self.write(" {");
        self.write_line();
        self.increase_indent();

        // return __asyncGenerator(this, arguments, function* name_1() {
        self.write("return ");
        self.write_helper("__asyncGenerator");
        self.write("(this, arguments, function* ");
        if !method_name.is_empty() {
            self.write(&method_name);
            self.write("_1");
        }
        self.write("() {");
        self.write_line();
        self.increase_indent();

        // Set flag so `await expr` emits as `yield __await(expr)`
        let saved = self.ctx.emit_await_as_yield_await;
        self.ctx.emit_await_as_yield_await = true;

        if let Some(body_node) = self.arena.get(body)
            && let Some(block) = self.arena.get_block(body_node)
        {
            for &stmt in &block.statements.nodes {
                self.emit(stmt);
                self.write_line();
            }
        }

        self.ctx.emit_await_as_yield_await = saved;

        self.decrease_indent();
        self.write("});");
        self.write_line();
        self.decrease_indent();
        self.write("}");
    }

    /// Emit method modifiers for JavaScript (static, async, and ES decorators)
    pub(in crate::emitter) fn emit_method_modifiers_js(&mut self, modifiers: &Option<NodeList>) {
        if let Some(mods) = modifiers {
            // Count static modifiers - when there are duplicates (parse error
            // recovery), tsc drops all static modifiers since the second
            // `static` is treated as the property name, not a modifier.
            let static_count = mods
                .nodes
                .iter()
                .filter(|&&idx| {
                    self.arena
                        .get(idx)
                        .is_some_and(|n| n.kind == SyntaxKind::StaticKeyword as u16)
                })
                .count();
            let suppress_static = static_count > 1;

            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    if mod_node.kind == syntax_kind_ext::DECORATOR {
                        // ES decorators are emitted verbatim when not using legacy
                        // (experimental) decorator lowering via __decorate.
                        if !self.ctx.options.legacy_decorators {
                            self.emit(mod_idx);
                            self.write_line();
                        }
                    } else if mod_node.kind == SyntaxKind::StaticKeyword as u16 {
                        if !suppress_static {
                            self.write("static ");
                        }
                    } else if mod_node.kind == SyntaxKind::AsyncKeyword as u16 {
                        self.write("async ");
                    } else if mod_node.kind == SyntaxKind::ExportKeyword as u16 {
                        // `export` on a class member is a parse error, but tsc
                        // preserves it in emit for error-recovery fidelity.
                        self.write("export ");
                    }
                    // Skip private/protected/public/readonly/abstract
                }
            }
        }
    }

    pub(in crate::emitter) fn emit_property_declaration(&mut self, node: &Node) {
        let Some(prop) = self.arena.get_property_decl(node) else {
            return;
        };

        // Skip abstract property declarations (they don't exist at runtime)
        if self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
        {
            self.skip_comments_for_erased_node(node);
            return;
        }

        // Skip `declare` property declarations — they are ambient/type-only declarations
        // that have no runtime representation regardless of target or useDefineForClassFields.
        // e.g. `declare x: number;` or `@dec declare field: T;` must never emit `x;` in JS.
        if self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
        {
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
            let has_accessor = self
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword);
            if !is_private && !has_accessor {
                self.skip_comments_for_erased_node(node);
                return;
            }
        }

        // For ES2022+ targets, static fields with initializers are emitted as
        // `static { this.fieldName = value; }` blocks (class static initialization blocks).
        // This preserves the correct `this` and `super` binding inside the class body.
        let is_static = self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::StaticKeyword);
        let target_es2022_plus = (self.ctx.options.target as u32) >= (ScriptTarget::ES2022 as u32);

        let is_private_field = self
            .arena
            .get(prop.name)
            .is_some_and(|n| n.kind == SyntaxKind::PrivateIdentifier as u16);
        let has_accessor = self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword);
        if is_static
            && target_es2022_plus
            && prop.initializer.is_some()
            && !is_private_field
            && !has_accessor
            && !self.ctx.options.use_define_for_class_fields
        {
            // Determine if the property name needs bracket notation
            let name_node = self.arena.get(prop.name);
            let is_computed = name_node
                .is_some_and(|n| n.kind == super::super::syntax_kind_ext::COMPUTED_PROPERTY_NAME);
            let is_string_or_numeric = name_node.is_some_and(|n| {
                n.kind == SyntaxKind::StringLiteral as u16
                    || n.kind == SyntaxKind::NumericLiteral as u16
            });
            if is_computed || is_string_or_numeric {
                // `static { this[expr] = value; }`
                self.write("static { this[");
                if is_computed {
                    if let Some(computed) =
                        name_node.and_then(|n| self.arena.get_computed_property(n))
                    {
                        self.emit(computed.expression);
                    }
                } else {
                    self.emit(prop.name);
                }
                self.write("] = ");
            } else {
                // `static { this.fieldName = value; }`
                self.write("static { this.");
                self.emit(prop.name);
                self.write(" = ");
            }
            self.with_scoped_static_initializer_context_cleared(|this| {
                this.emit(prop.initializer);
            });
            self.write("; }");
            return;
        }

        // Emit modifiers (static and accessor for JavaScript)
        self.emit_class_member_modifiers_js(&prop.modifiers);

        self.emit(prop.name);

        // Skip type annotations for JavaScript emit

        if prop.initializer.is_some() {
            self.write(" = ");
            self.with_scoped_static_initializer_context_cleared(|this| {
                this.emit(prop.initializer);
            });
        }

        self.write_semicolon();
    }

    /// Emit class member modifiers for JavaScript (static, accessor, and ES decorators)
    pub(in crate::emitter) fn emit_class_member_modifiers_js(
        &mut self,
        modifiers: &Option<NodeList>,
    ) {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    if mod_node.kind == syntax_kind_ext::DECORATOR {
                        if !self.ctx.options.legacy_decorators {
                            self.emit(mod_idx);
                            self.write_line();
                        }
                    } else if mod_node.kind == SyntaxKind::StaticKeyword as u16 {
                        self.write("static ");
                    } else if mod_node.kind == SyntaxKind::AccessorKeyword as u16 {
                        self.write("accessor ");
                    } else if mod_node.kind == SyntaxKind::ExportKeyword as u16 {
                        // `export` on a class member is a parse error, but tsc
                        // preserves it in emit for error-recovery fidelity.
                        self.write("export ");
                    }
                }
            }
        }
    }

    pub(in crate::emitter) fn emit_constructor_declaration(&mut self, node: &Node) {
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

        // Preserve invalid modifiers on constructors for error recovery (tsc behavior).
        // e.g., `static constructor() {}` or `export constructor() {}` are errors
        // but tsc preserves the keywords in the JS output.
        if let Some(ref mods) = ctor.modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    match mod_node.kind {
                        k if k == SyntaxKind::StaticKeyword as u16 => self.write("static "),
                        k if k == SyntaxKind::ExportKeyword as u16 => self.write("export "),
                        _ => {}
                    }
                }
            }
        }
        self.write("constructor");
        // Emit type parameters for error recovery (e.g., `constructor<T>() {}`)
        if let Some(ref type_params) = ctor.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.write("<");
            self.emit_comma_separated(&type_params.nodes);
            self.write(">");
        }
        // Map opening `(` to its source position
        let open_paren_pos = {
            self.map_token_after(node.pos, node.end, b'(');
            self.pending_source_pos
                .map(|source_pos| source_pos.pos)
                .unwrap_or(node.pos)
        };
        self.write("(");
        let search_start = ctor
            .parameters
            .nodes
            .last()
            .and_then(|&idx| self.arena.get(idx))
            .map_or(node.pos, |n| n.pos);
        let search_end = if ctor.body.is_some() {
            self.arena.get(ctor.body).map_or(node.end, |n| n.pos)
        } else {
            node.end
        };
        self.emit_function_parameters_with_trailing_comments(
            &ctor.parameters.nodes,
            open_paren_pos,
            search_start,
            search_end,
        );
        // Map closing `)` to its source position
        self.write(")");
        self.write(" ");

        let prev_emitting_function_body_block = self.emitting_function_body_block;
        self.emitting_function_body_block = true;
        self.function_scope_depth += 1;
        self.ctx.block_scope_state.enter_scope();
        self.push_temp_scope();
        // Save/restore declared_namespace_names so enum/namespace names from
        // outer scope don't leak into the constructor body.
        let prev_declared = std::mem::take(&mut self.declared_namespace_names);
        if let Some(body_node) = self.arena.get(ctor.body) {
            let temp_count = self.estimate_assignment_destructuring_temps_in_constructor(body_node);
            if temp_count > 0 {
                self.preallocate_assignment_temps(temp_count);
            }
        }
        self.prepare_logical_assignment_value_temps(ctor.body);
        let auto_accessor_inits = std::mem::take(&mut self.pending_auto_accessor_inits);
        self.emit_constructor_body_with_prologue(
            ctor.body,
            &param_props,
            &field_inits,
            &auto_accessor_inits,
        );
        self.declared_namespace_names = prev_declared;
        self.pop_temp_scope();
        self.ctx.block_scope_state.exit_scope();
        self.function_scope_depth -= 1;
        self.emitting_function_body_block = prev_emitting_function_body_block;
    }

    /// Collect parameter property names from constructor parameters.
    /// Returns names of parameters that have accessibility modifiers (public/private/protected/readonly).
    /// Uses emit text (preserving unicode escapes) to match tsc output.
    pub(in crate::emitter) fn collect_parameter_properties(
        &self,
        params: &[NodeIndex],
    ) -> Vec<String> {
        let mut names = Vec::new();
        for &param_idx in params {
            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_parameter(param_node)
                && self.has_parameter_property_modifier(&param.modifiers)
            {
                let name = crate::transforms::emit_utils::identifier_emit_text_or_empty(
                    self.arena, param.name,
                );
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
                        || kind == SyntaxKind::OverrideKeyword as u32
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
        field_inits: &[crate::emitter::core::FieldInit],
        auto_accessor_inits: &[(String, Option<NodeIndex>)],
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

        let has_prologue = !param_props.is_empty()
            || !field_inits.is_empty()
            || !auto_accessor_inits.is_empty()
            || !self.pending_private_field_constructor_inits.is_empty()
            || self.pending_instances_weakset_add.is_some();

        // Empty constructor with no prologue: check source format
        if block.statements.nodes.is_empty() && !has_prologue && !has_function_temps {
            // Check for inner comments (e.g., constructor body with only comments).
            // tsc preserves these comments even in otherwise-empty constructor bodies.
            let closing_brace_pos =
                self.find_token_end_before_trivia(block_node.pos, block_node.end);
            let has_inner_comments = !self.ctx.options.remove_comments
                && self
                    .all_comments
                    .get(self.comment_emit_idx)
                    .is_some_and(|c| c.end <= closing_brace_pos);
            if has_inner_comments {
                // Skip same-line-as-brace comments (suppressed for function bodies),
                // then emit any remaining inner comments on subsequent lines.
                self.skip_trailing_same_line_comments(block_node.pos, closing_brace_pos);
                let has_remaining = self
                    .all_comments
                    .get(self.comment_emit_idx)
                    .is_some_and(|c| c.end <= closing_brace_pos);
                if has_remaining {
                    self.write("{");
                    self.write_line();
                    self.increase_indent();
                    self.emit_comments_before_pos(closing_brace_pos);
                    self.decrease_indent();
                    self.write("}");
                } else if self.is_single_line(block_node) {
                    self.write("{ }");
                } else {
                    self.write("{");
                    self.write_line();
                    self.write("}");
                }
            } else if self.is_single_line(block_node) {
                self.write("{ }");
            } else {
                self.write("{");
                self.write_line();
                self.write("}");
            }
            return;
        }

        // Single-line non-empty constructor body: preserve single-line formatting
        // when source was on one line, there's no prologue to inject, and no
        // hoisted temps. e.g. `constructor(x) { this.a = x; }` stays on one line.
        if block.statements.nodes.len() == 1
            && !has_prologue
            && !has_function_temps
            && self.is_single_line(block_node)
        {
            self.map_opening_brace(block_node);
            self.write("{ ");
            self.emit(block.statements.nodes[0]);
            self.map_closing_brace(block_node);
            self.write(" }");
            return;
        }

        self.write("{");
        // Skip same-line comments on constructor body opening `{`.
        // tsc suppresses these for function/method/constructor bodies.
        // Use the first statement's position (or closing `}` position) as max_pos
        // to avoid consuming trailing comments that belong on the closing `}`.
        if !self.ctx.options.remove_comments
            && let Some(text) = self.source_text
        {
            let bytes = text.as_bytes();
            let start = block_node.pos as usize;
            let end = (block_node.end as usize).min(bytes.len());
            if let Some(offset) = bytes[start..end].iter().position(|&b| b == b'{') {
                let brace_end = (start + offset + 1) as u32;
                // Find first content position after opening brace (first statement
                // or closing `}` brace) to bound the skip range tightly.
                // Using the closing `}` position (not block_node.end which includes
                // trailing trivia) prevents consuming comments after `}`.
                let closing_brace_pos =
                    self.find_token_end_before_trivia(block_node.pos, block_node.end);
                let first_content_pos = block
                    .statements
                    .nodes
                    .first()
                    .and_then(|&s| self.arena.get(s))
                    .map_or(closing_brace_pos, |s| s.pos);
                self.skip_trailing_same_line_comments(brace_end, first_content_pos);
            }
        }
        self.write_line();
        self.increase_indent();

        if has_function_temps {
            self.emit_function_body_hoisted_temps();
        }

        // Capture position for inserting hoisted temps created during statement emit
        // (e.g., `_a` from `??` lowering inside the constructor body).
        let hoisted_var_insert_pos = (self.writer.len(), self.writer.current_line());

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
                let actual_start = self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
                self.emit_comments_before_pos(actual_start);
            }

            // If no super() call exists, emit prologue before first body statement
            if !prologue_emitted && super_call_idx.is_none() && stmt_i == 0 {
                self.emit_constructor_prologue(param_props, field_inits, auto_accessor_inits);
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
                self.emit_constructor_prologue(param_props, field_inits, auto_accessor_inits);
                prologue_emitted = true;
            }
        }

        // If we never emitted the prologue (empty body or no super), emit it now
        if !prologue_emitted {
            self.emit_constructor_prologue(param_props, field_inits, auto_accessor_inits);
        }

        // Insert any hoisted temps created during statement emit (e.g., `_a` from `??` lowering).
        if !self.hoisted_assignment_temps.is_empty() {
            let indent = " ".repeat(self.writer.indent_width() as usize);
            let var_decl = format!(
                "{}var {};",
                indent,
                self.hoisted_assignment_temps.join(", ")
            );
            self.writer.insert_line_at(
                hoisted_var_insert_pos.0,
                hoisted_var_insert_pos.1,
                &var_decl,
            );
            self.hoisted_assignment_temps.clear();
        }

        self.decrease_indent();
        self.write("}");
    }

    /// Emit parameter property and field initializer assignments (constructor prologue).
    fn emit_constructor_prologue(
        &mut self,
        param_props: &[String],
        field_inits: &[crate::emitter::core::FieldInit],
        auto_accessor_inits: &[(String, Option<NodeIndex>)],
    ) {
        // Emit `_X_instances.add(this)` for private methods/accessors
        if let Some(ref ws_name) = self.pending_instances_weakset_add.clone() {
            self.write(ws_name);
            self.write(".add(this);");
            self.write_line();
        }
        // Emit private field WeakMap.set inits
        let private_inits = self.pending_private_field_constructor_inits.clone();
        for (weakmap_name, has_initializer, initializer) in &private_inits {
            self.write(weakmap_name);
            self.write(".set(this, ");
            if *has_initializer {
                self.emit_expression(*initializer);
            } else {
                self.write("void 0");
            }
            self.write(");");
            self.write_line();
        }
        // When useDefineForClassFields is true and fields are being lowered to
        // the constructor (target < ES2022), parameter properties should use
        // Object.defineProperty to match tsc's emit semantics.
        // When target >= ES2022 (native class fields), use simple assignment.
        let use_define_for_param_props = self.ctx.options.use_define_for_class_fields
            && (self.ctx.options.target as u32) < (ScriptTarget::ES2022 as u32);
        for name in param_props {
            if use_define_for_param_props {
                self.write("Object.defineProperty(this, \"");
                self.write(name);
                self.write("\", {");
                self.write_line();
                self.increase_indent();
                self.write("enumerable: true,");
                self.write_line();
                self.write("configurable: true,");
                self.write_line();
                self.write("writable: true,");
                self.write_line();
                self.write("value: ");
                self.write(name);
                self.write_line();
                self.decrease_indent();
                self.write("});");
            } else {
                self.write("this.");
                self.write(name);
                self.write(" = ");
                self.write(name);
                self.write(";");
            }
            self.write_line();
        }
        for (name, init_idx, init_end, leading_comments, trailing_comments) in field_inits {
            // Emit leading comments from the original property declaration
            for comment in leading_comments {
                self.write_comment(comment);
                self.write_line();
            }
            if self.ctx.options.use_define_for_class_fields {
                self.write("Object.defineProperty(this, ");
                if name.starts_with('[') && name.ends_with(']') {
                    self.write(&name[1..name.len() - 1]);
                } else {
                    self.emit_string_literal_text(name);
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
                self.with_scoped_static_initializer_context_cleared(|this| {
                    this.emit_expression(*init_idx);
                });
                self.write_line();
                self.decrease_indent();
                self.write("});");
            } else {
                // Bracket names (e.g., `["constructor"]`) are encoded with `[` prefix
                if name.starts_with('[') {
                    self.write("this");
                    self.write(name);
                } else {
                    self.write("this.");
                    self.write(name);
                }
                self.write(" = ");
                self.with_scoped_static_initializer_context_cleared(|this| {
                    this.emit_expression(*init_idx);
                });
                self.write(";");
                // Emit trailing comments from the original class field.
                // If pre-collected (field appeared before constructor in source), use them.
                // Otherwise fall back to position-based lookup (field after constructor).
                if !trailing_comments.is_empty() {
                    for comment in trailing_comments {
                        self.write_space();
                        self.write_comment(comment);
                    }
                } else {
                    self.emit_trailing_comments(*init_end);
                }
            }
            self.write_line();
        }
        for (name, init_idx) in auto_accessor_inits {
            self.write(name);
            self.write(".set(this, ");
            match init_idx {
                Some(init) => self.emit_expression(*init),
                None => self.write("void 0"),
            }
            self.write(");");
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

    pub(in crate::emitter) fn emit_get_accessor(&mut self, node: &Node, accessor_node: NodeIndex) {
        let Some(accessor) = self.arena.get_accessor(node) else {
            return;
        };

        // Emit modifiers (static only for JavaScript)
        self.emit_class_member_modifiers_js(&accessor.modifiers);

        self.write("get ");
        self.emit(accessor.name);

        // Emit type parameters for error recovery (e.g., `get foo<T>() {}`)
        // Getters cannot legally have type parameters, but tsc preserves them in JS output.
        if let Some(ref type_params) = accessor.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.write("<");
            self.emit_comma_separated(&type_params.nodes);
            self.write(">");
        }

        self.write("(");
        self.emit_function_parameters_js(&accessor.parameters.nodes);
        self.write(")");

        // Skip type annotation for JS emit

        let compact_body = self.should_emit_compact_empty_accessor_body(accessor_node);
        self.emit_accessor_body(accessor.body, compact_body);
    }

    pub(in crate::emitter) fn emit_set_accessor(&mut self, node: &Node, accessor_node: NodeIndex) {
        let Some(accessor) = self.arena.get_accessor(node) else {
            return;
        };

        // Emit modifiers (static only for JavaScript)
        self.emit_class_member_modifiers_js(&accessor.modifiers);

        self.write("set ");
        self.emit(accessor.name);

        // Emit type parameters for error recovery (e.g., `set foo<T>(v) {}`)
        // Setters cannot legally have type parameters, but tsc preserves them in JS output.
        if let Some(ref type_params) = accessor.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.write("<");
            self.emit_comma_separated(&type_params.nodes);
            self.write(">");
        }

        self.write("(");
        let open_paren_pos = {
            self.map_token_after(
                self.arena
                    .get(accessor.name)
                    .map_or(node.pos, |name| name.end),
                node.end,
                b'(',
            );
            self.pending_source_pos
                .map(|source_pos| source_pos.pos)
                .unwrap_or(node.pos)
        };
        let search_start = accessor
            .parameters
            .nodes
            .first()
            .and_then(|&idx| self.arena.get(idx))
            .map_or(node.pos, |n| n.pos);
        if let Some(body_node) = self.arena.get(accessor.body) {
            let search_end = body_node.pos;
            self.emit_function_parameters_with_trailing_comments(
                &accessor.parameters.nodes,
                open_paren_pos,
                search_start,
                search_end,
            );
        } else {
            self.emit_function_parameters_js(&accessor.parameters.nodes);
        }
        self.write(")");

        // Emit return type annotation for error recovery (e.g., `set foo(v): number {}`)
        // Setters cannot legally have return type annotations, but tsc preserves them in JS output.
        if accessor.type_annotation.is_some() {
            self.write(": ");
            self.emit(accessor.type_annotation);
        }

        let compact_body = self.should_emit_compact_empty_accessor_body(accessor_node);
        self.emit_accessor_body(accessor.body, compact_body);
    }

    /// Emit the body of a get/set accessor, handling scope management and fallback to empty body.
    fn emit_accessor_body(&mut self, body: NodeIndex, compact_empty_body: bool) {
        if body.is_some() {
            let can_emit_compact_empty_body =
                compact_empty_body && self.should_emit_compact_empty_accessor_body_impl(body);
            if can_emit_compact_empty_body {
                self.write(" {}");
                return;
            }

            let prev_emitting_function_body_block = self.emitting_function_body_block;
            self.emitting_function_body_block = true;
            self.function_scope_depth += 1;
            self.ctx.block_scope_state.enter_scope();
            self.push_temp_scope();
            // Save/restore declared_namespace_names for accessor body isolation.
            let prev_declared = std::mem::take(&mut self.declared_namespace_names);
            self.prepare_logical_assignment_value_temps(body);
            self.write(" ");
            self.emit(body);
            self.declared_namespace_names = prev_declared;
            self.pop_temp_scope();
            self.ctx.block_scope_state.exit_scope();
            self.function_scope_depth -= 1;
            self.emitting_function_body_block = prev_emitting_function_body_block;
        } else {
            // For JS-pass-through object-literal accessors, keep compact braces.
            if compact_empty_body {
                self.write(" {}");
            } else {
                // For TS emit, preserve spaced empty-body formatting.
                self.write(" { }");
            }
        }
    }

    const fn should_emit_compact_empty_accessor_body(&self, _accessor_node: NodeIndex) -> bool {
        self.is_current_root_js_source && self.is_emitting_object_literal_accessor()
    }

    /// Emit `{}` for object-literal accessors when the block is syntactically empty.
    fn should_emit_compact_empty_accessor_body_impl(&mut self, body: NodeIndex) -> bool {
        let Some(block_node) = self
            .arena
            .get(body)
            .and_then(|body_node| self.arena.get_block(body_node))
        else {
            return false;
        };

        if !block_node.statements.nodes.is_empty() {
            return false;
        }

        if self.ctx.options.remove_comments {
            return true;
        }

        let Some(body_node) = self.arena.get(body) else {
            return false;
        };

        let closing_brace_pos = self.find_token_end_before_trivia(body_node.pos, body_node.end);
        let has_inner_comments = self
            .all_comments
            .get(self.comment_emit_idx)
            .is_some_and(|c| c.end <= closing_brace_pos);
        !has_inner_comments
    }
}

#[cfg(test)]
mod tests {
    use crate::output::printer::{PrintOptions, Printer};
    use tsz_parser::ParserState;

    fn emit_ts(source: &str) -> String {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        printer.finish().code
    }

    #[test]
    fn es_decorator_on_method_emitted_at_esnext() {
        let source = "class C {\n    @dec\n    method() {}\n}";
        let output = emit_ts(source);
        assert!(
            output.contains("@dec"),
            "ES decorator on method should be emitted at ESNext target.\nOutput: {output}"
        );
        assert!(
            output.contains("method()"),
            "Decorated method should be emitted.\nOutput: {output}"
        );
    }

    #[test]
    fn es_decorator_on_static_method_emitted() {
        let source = "class C {\n    @dec\n    static foo() {}\n}";
        let output = emit_ts(source);
        assert!(
            output.contains("@dec"),
            "ES decorator on static method should be emitted.\nOutput: {output}"
        );
        assert!(
            output.contains("static foo()"),
            "Static modifier and method name should be emitted.\nOutput: {output}"
        );
    }

    #[test]
    fn es_decorator_on_getter_emitted() {
        let source = "class C {\n    @dec\n    get value() { return 1; }\n}";
        let output = emit_ts(source);
        assert!(
            output.contains("@dec"),
            "ES decorator on getter should be emitted.\nOutput: {output}"
        );
        assert!(
            output.contains("get value()"),
            "Getter should be emitted.\nOutput: {output}"
        );
    }

    #[test]
    fn multiple_es_decorators_on_method() {
        let source = "class C {\n    @first\n    @second\n    method() {}\n}";
        let output = emit_ts(source);
        assert!(
            output.contains("@first"),
            "First decorator should be emitted.\nOutput: {output}"
        );
        assert!(
            output.contains("@second"),
            "Second decorator should be emitted.\nOutput: {output}"
        );
    }

    #[test]
    fn es_decorator_with_arguments_on_method() {
        let source = "class C {\n    @dec(1, 2)\n    method() {}\n}";
        let output = emit_ts(source);
        assert!(
            output.contains("@dec(1, 2)"),
            "Decorator with arguments should be emitted verbatim.\nOutput: {output}"
        );
    }

    #[test]
    fn single_line_constructor_body_preserved() {
        let source = "class B {\n    constructor(x: number) { this.x = x; }\n}";
        let output = emit_ts(source);
        assert!(
            output.contains("constructor(x) { this.x = x; }"),
            "Single-line constructor body should stay on one line.\nOutput: {output}"
        );
    }

    #[test]
    fn multiline_constructor_body_stays_multiline() {
        let source = "class B {\n    constructor(x: number) {\n        this.x = x;\n    }\n}";
        let output = emit_ts(source);
        assert!(
            output.contains("constructor(x) {\n"),
            "Multi-line constructor body should stay multiline.\nOutput: {output}"
        );
        assert!(
            !output.contains("constructor(x) { this.x = x; }"),
            "Multi-line constructor body should not be collapsed to one line.\nOutput: {output}"
        );
    }

    #[test]
    fn single_line_constructor_body_with_return() {
        let source = "class C {\n    constructor(x: number) { return null; }\n}";
        let output = emit_ts(source);
        assert!(
            output.contains("constructor(x) { return null; }"),
            "Single-line constructor body with return should stay on one line.\nOutput: {output}"
        );
    }

    #[test]
    fn object_literal_accessor_empty_body_has_space_braces() {
        let source = "export const t = {\n    set setter(v) {},\n};";
        let output = emit_ts(source);

        assert!(
            !output.contains("set setter(v) {},"),
            "Object-literal setter should not use compact empty-body formatting.\nOutput: {output}"
        );
        assert!(
            output.contains("set setter(v) { },"),
            "Object-literal setter should preserve trailing comma when present.\nOutput: {output}"
        );
    }

    #[test]
    fn object_literal_accessor_empty_body_compact_in_js_file() {
        let source = "export const t = {\n    set setter(v) {},\n};";
        let mut parser = ParserState::new("test.js".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("set setter(v) {}"),
            "JS input object-literal accessor should use compact empty-body formatting.\nOutput: {output}"
        );
        assert!(
            !output.contains("set setter(v) { },"),
            "JS input object-literal accessor should prefer compact braces.\nOutput: {output}"
        );
    }

    #[test]
    fn generator_method_overloads_preserve_asterisk() {
        // When overloaded generator methods are emitted, the implementation
        // method should retain the * (generator asterisk).
        let source = "class C {\n    *f(s: string): Iterable<any>;\n    *f(s: number): Iterable<any>;\n    *f(s: any): Iterable<any> { }\n}";
        let output = emit_ts(source);
        assert!(
            output.contains("*f(s)"),
            "Generator method implementation should retain * after overload erasure.\nOutput: {output}"
        );
    }
}
