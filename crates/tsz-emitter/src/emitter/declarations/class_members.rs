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

    pub(in crate::emitter) fn has_effective_static_modifier_js(
        &self,
        modifiers: &Option<NodeList>,
    ) -> bool {
        modifiers.as_ref().is_some_and(|mods| {
            mods.nodes
                .iter()
                .filter(|&&idx| {
                    self.arena
                        .get(idx)
                        .is_some_and(|n| n.kind == SyntaxKind::StaticKeyword as u16)
                })
                .count()
                == 1
        })
    }

    fn emit_class_member_name_preserving_class_expression_name(&mut self, name: NodeIndex) {
        if self
            .arena
            .get(name)
            .is_some_and(|n| n.kind == SyntaxKind::PrivateIdentifier as u16)
            && let Some(ident) = self.arena.get_identifier_at(name)
        {
            let private_name = ident.escaped_text.as_str();
            if self.class_member_emit_depth > 0 {
                self.write(private_name);
                return;
            }
            if private_name.trim_start_matches('#') == "constructor" {
                if private_name.starts_with('#') {
                    self.write(private_name);
                } else {
                    self.write("#");
                    self.write(private_name);
                }
                return;
            }
        }

        if self.is_emitting_object_literal_accessor()
            && let Some(name_node) = self.arena.get(name)
            && name_node.kind == SyntaxKind::NumericLiteral as u16
            && let Some(literal) = self.arena.get_literal(name_node)
        {
            self.write(&literal.text);
            return;
        }

        let prev_alias = self.scoped_class_expression_self_alias.take();
        if let Some(name_node) = self.arena.get(name)
            && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && !self.pending_weakmap_inits.is_empty()
            && let Some(computed) = self.arena.get_computed_property(name_node)
        {
            let weakmap_inits = std::mem::take(&mut self.pending_weakmap_inits);
            self.write("[(");
            for (i, init) in weakmap_inits.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.write(init);
            }
            self.write(", ");
            let expression = self
                .arena
                .get(computed.expression)
                .filter(|node| node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION)
                .and_then(|node| self.arena.get_parenthesized(node))
                .map_or(computed.expression, |paren| paren.expression);
            if let Some(temp_name) = self.computed_prop_temp_map.get(&expression) {
                self.write(&temp_name.clone());
            } else {
                self.emit(expression);
            }
            self.write(")]");
        } else {
            let is_computed = self
                .arena
                .get(name)
                .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME);
            if is_computed
                && let Some(name_node) = self.arena.get(name)
                && let Some(computed) = self.arena.get_computed_property(name_node)
            {
                let expression = self
                    .arena
                    .get(computed.expression)
                    .filter(|node| node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION)
                    .and_then(|node| self.arena.get_parenthesized(node))
                    .map_or(computed.expression, |paren| paren.expression);
                if let Some(temp_name) = self.computed_prop_temp_map.get(&expression).cloned() {
                    self.write("[");
                    self.write(&temp_name);
                    self.write("]");
                    self.scoped_class_expression_self_alias = prev_alias;
                    return;
                }
            }
            // Suppress namespace/CJS-export qualification only when emitting a
            // non-computed class member name. Object-literal methods and
            // computed class names are runtime expressions and must still pick
            // up namespace/export rewrites.
            let in_class_member = self.class_member_emit_depth > 0 && !is_computed;
            let prev_ns = self.suppress_ns_qualification;
            if in_class_member {
                self.suppress_ns_qualification = true;
            }
            self.emit(name);
            self.suppress_ns_qualification = prev_ns;
        }
        self.scoped_class_expression_self_alias = prev_alias;
    }

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

    pub(in crate::emitter) fn is_recovered_optional_bodyless_class_method(
        &self,
        node: &Node,
    ) -> bool {
        let Some(method) = self.arena.get_method_decl(node) else {
            return false;
        };
        if method.body.is_some() || method.question_token {
            return false;
        }
        let Some(text) = self.source_text else {
            return false;
        };

        let search_start = if let Some(ref tp) = method.type_parameters {
            tp.nodes
                .last()
                .and_then(|&idx| self.arena.get(idx))
                .map_or(node.pos, |n| n.end)
        } else if method.name.is_some() {
            self.arena.get(method.name).map_or(node.pos, |n| n.end)
        } else {
            node.pos
        } as usize;
        let search_end = if method.type_annotation.is_some() {
            self.arena
                .get(method.type_annotation)
                .map_or(node.end, |n| n.pos)
        } else {
            node.end
        } as usize;
        let bytes = text.as_bytes();
        let search_start = search_start.min(bytes.len());
        let search_end = search_end.min(bytes.len());
        if search_start >= search_end {
            return false;
        }

        let Some(close_paren_offset) = bytes[search_start..search_end]
            .iter()
            .rposition(|&byte| byte == b')')
        else {
            return false;
        };
        // When no return type parsed into the method node (`x()?: number` with
        // the `?` left unconsumed) the illegal `?` sits at the node's exclusive
        // `end`, so scan the raw source just past the `)` rather than stopping
        // at `search_end`. Only inline space is skipped, so a later member's `?`
        // on another line is never mistaken for this one.
        let mut cursor = search_start + close_paren_offset + 1;
        while cursor < bytes.len() && (bytes[cursor] == b' ' || bytes[cursor] == b'\t') {
            cursor += 1;
        }
        cursor < bytes.len() && bytes[cursor] == b'?'
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
        let is_quoted_constructor_name =
            self.class_member_emit_depth > 0 && self.is_quoted_constructor_method_name(method.name);

        // Skip method declarations without bodies (TypeScript-only overloads).
        // Recovered optional class methods like `x()?: number;` are syntax
        // errors, but tsc keeps a runtime empty method body for them.
        if method.body.is_none() {
            // Keep parse-recovery emit for invalid generator member `*() {}`.
            if method.asterisk_token && has_recovery_missing_name {
                self.write("*() { }");
            } else if (self.class_member_emit_depth > 0
                && self.is_recovered_optional_bodyless_class_method(node))
                || self.has_recovered_declaration_trailing_comma(node)
            {
                self.emit_recovered_object_method_without_body(node);
            } else {
                self.skip_comments_for_erased_node(node);
            }
            return;
        }

        let is_async = self
            .arena
            .has_modifier(&method.modifiers, SyntaxKind::AsyncKeyword);
        let has_generator_asterisk = method.asterisk_token
            || crate::transforms::emit_utils::source_header_has_async_generator_asterisk(
                self.source_text,
                node.pos,
                self.arena
                    .get(method.body)
                    .map_or(node.end, |body| body.pos),
            );
        let needs_async_lowering =
            is_async && self.ctx.needs_async_lowering && !has_generator_asterisk;
        let needs_async_generator_lowering =
            is_async && self.ctx.needs_es2018_lowering && has_generator_asterisk;

        if needs_async_lowering || needs_async_generator_lowering {
            // Emit static modifier if present
            if self.arena.is_static(&method.modifiers) {
                self.write("static ");
            }
        } else {
            if self.recovered_object_method_header_has_static(node, method.name) {
                self.write("static ");
            }
            // Emit modifiers (static, async only for JavaScript)
            self.emit_method_modifiers_js(&method.modifiers);
        }
        if self.should_preserve_native_decorator_comments(&method.modifiers)
            && let Some(name_node) = self.arena.get(method.name)
        {
            self.emit_comments_before_pos(name_node.pos);
        }

        // Emit generator asterisk (skip for async generators being lowered)
        if has_generator_asterisk && !needs_async_generator_lowering {
            self.write("*");
        }

        if is_quoted_constructor_name {
            self.write("constructor");
        } else if method.name.is_some() && !has_recovery_missing_name {
            self.emit_class_member_name_preserving_class_expression_name(method.name);
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
        // A comment in the source seam between the method name and `(`
        // (e.g. `duplicate /* <V> */(x, y)`) belongs right after the name,
        // before `(`. Emit it here so it does not leak into the parameter list.
        if !self.ctx.flags.in_declaration_emit && method.name.is_some() {
            self.emit_name_to_paren_seam_comments(method.name, node.end);
        }
        // Skip comments inside the type parameter list
        if !self.ctx.flags.in_declaration_emit && method.type_parameters.is_some() {
            let tp_skip_start = if method.name.is_some() {
                self.arena.get(method.name).map_or(node.pos, |n| n.end)
            } else {
                node.pos
            };
            self.skip_comments_in_range(tp_skip_start, open_paren_pos);
        }
        let async_method_params_need_forwarding = needs_async_lowering
            && !self.ctx.target_es5
            && self.async_params_need_generator_forwarding(&method.parameters.nodes);
        if needs_async_generator_lowering || async_method_params_need_forwarding {
            self.push_temp_scope();
        }
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
        if (needs_async_generator_lowering || async_method_params_need_forwarding)
            && self.async_params_need_generator_forwarding(&method.parameters.nodes)
        {
            self.emit_async_outer_parameter_placeholders(&method.parameters.nodes);
        } else {
            self.emit_function_parameters_with_trailing_comments(
                &method.parameters.nodes,
                open_paren_pos,
                search_start,
                search_end,
            );
        }
        self.write(")");

        if self.recovered_if_class_member_has_invalid_header(node, method.body) {
            self.write(" { }");
            if let Some(body_node) = self.arena.get(method.body) {
                self.skip_comments_for_erased_node(body_node);
            }
            return;
        }

        // Skip return type for JavaScript emit — skip comments inside erased return type
        if !self.ctx.flags.in_declaration_emit
            && method.type_annotation.is_some()
            && let Some(type_node) = self.arena.get(method.type_annotation)
        {
            self.skip_comments_in_range(type_node.pos, type_node.end);
        }

        if needs_async_generator_lowering {
            self.emit_method_async_generator_lowered_body(
                method.body,
                method.name,
                &method.parameters.nodes,
            );
            self.pop_temp_scope();
        } else if needs_async_lowering {
            self.emit_method_async_lowered_body(method.body, &method.parameters.nodes);
            if async_method_params_need_forwarding {
                self.pop_temp_scope();
            }
        } else {
            self.write(" ");
            let lowered_async_arrow_super_capture = if self.ctx.needs_async_lowering {
                crate::transforms::emit_utils::collect_lowered_async_arrow_super_capture(
                    self.arena,
                    method.body,
                )
            } else {
                crate::transforms::emit_utils::AsyncMethodSuperCapture::default()
            };
            let has_lowered_async_arrow_super_capture =
                !lowered_async_arrow_super_capture.property_names.is_empty()
                    || lowered_async_arrow_super_capture.needs_element_index;
            let prev_pending_lowered_async_arrow_super_capture =
                self.pending_lowered_async_arrow_super_capture.take();
            if has_lowered_async_arrow_super_capture {
                let source_text = self.source_text.unwrap_or_default();
                let super_alias_text = (!lowered_async_arrow_super_capture
                    .property_names
                    .is_empty())
                .then(|| crate::transforms::emit_utils::hygienic_temp_name("_super", source_text));
                let super_index_alias_text = lowered_async_arrow_super_capture
                    .needs_element_index
                    .then(|| {
                        crate::transforms::emit_utils::hygienic_temp_name(
                            "_superIndex",
                            source_text,
                        )
                    });
                self.pending_lowered_async_arrow_super_capture = Some((
                    lowered_async_arrow_super_capture,
                    super_alias_text,
                    super_index_alias_text,
                ));
            }
            let prev_emitting_function_body_block = self.emitting_function_body_block;
            self.emitting_function_body_block = true;
            self.function_scope_depth += 1;
            let prev_es5_super_home_depth = self.es5_super_home_function_depth;
            let prev_es5_super_home_static = self.es5_super_home_is_static;
            if self.ctx.target_es5 {
                self.es5_super_home_function_depth = Some(self.function_scope_depth);
                self.es5_super_home_is_static =
                    self.has_effective_static_modifier_js(&method.modifiers);
            }
            let prev_in_generator = self.ctx.flags.in_generator;
            self.ctx.block_scope_state.enter_scope();
            self.push_temp_scope();
            let prev_declared = std::mem::take(&mut self.declared_namespace_names);
            if let Some(body_node) = self.arena.get(method.body) {
                let temp_count =
                    self.estimate_private_destructuring_receiver_temps_in_function_body(body_node);
                if temp_count > 0 {
                    self.preallocate_assignment_temps(temp_count);
                }
            }
            self.prepare_logical_assignment_value_temps(method.body);
            self.ctx.flags.in_generator = has_generator_asterisk;
            self.emit(method.body);
            self.declared_namespace_names = prev_declared;
            self.pop_temp_scope();
            self.ctx.block_scope_state.exit_scope();
            self.ctx.flags.in_generator = prev_in_generator;
            self.es5_super_home_function_depth = prev_es5_super_home_depth;
            self.es5_super_home_is_static = prev_es5_super_home_static;
            self.function_scope_depth -= 1;
            self.emitting_function_body_block = prev_emitting_function_body_block;
            self.pending_lowered_async_arrow_super_capture =
                prev_pending_lowered_async_arrow_super_capture;
        }
    }

    fn recovered_if_class_member_has_invalid_header(&self, node: &Node, body: NodeIndex) -> bool {
        if self.class_member_emit_depth == 0 {
            return false;
        }
        let Some(text) = self.source_text else {
            return false;
        };
        let Some(method) = self.arena.get_method_decl(node) else {
            return false;
        };
        let Some(name_node) = self.arena.get(method.name) else {
            return false;
        };
        let is_if_keyword_name = name_node.kind == SyntaxKind::IfKeyword as u16
            || self
                .arena
                .get_identifier(name_node)
                .is_some_and(|ident| ident.escaped_text == "if");
        if !is_if_keyword_name {
            return false;
        }
        let Some(body_node) = self.arena.get(body) else {
            return false;
        };
        let start = (name_node.end as usize).min(text.len());
        let end = (body_node.pos as usize).min(text.len());
        if start >= end {
            return false;
        }
        let header = &text[start..end];
        header.contains("!=") || header.contains("==")
    }

    pub(in crate::emitter) fn emit_recovered_object_method_without_body(&mut self, node: &Node) {
        let Some(method) = self.arena.get_method_decl(node) else {
            return;
        };

        self.emit_method_modifiers_js(&method.modifiers);
        if self.should_preserve_native_decorator_comments(&method.modifiers)
            && let Some(name_node) = self.arena.get(method.name)
        {
            self.emit_comments_before_pos(name_node.pos);
        }

        if method.asterisk_token {
            self.write("*");
        }

        if method.name.is_some() {
            self.emit(method.name);
        }

        let open_paren_pos = {
            let search_start = if let Some(ref tp) = method.type_parameters {
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

        if !self.ctx.flags.in_declaration_emit && method.type_parameters.is_some() {
            let tp_skip_start = if method.name.is_some() {
                self.arena.get(method.name).map_or(node.pos, |n| n.end)
            } else {
                node.pos
            };
            self.skip_comments_in_range(tp_skip_start, open_paren_pos);
        }

        self.write("(");
        self.emit_comma_separated(&method.parameters.nodes);
        self.write(") { }");

        if !self.ctx.flags.in_declaration_emit
            && method.type_annotation.is_some()
            && let Some(type_node) = self.arena.get(method.type_annotation)
        {
            self.skip_comments_in_range(type_node.pos, type_node.end);
        }
    }

    fn is_quoted_constructor_method_name(&self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(name_idx) else {
            return false;
        };
        name_node.kind == SyntaxKind::StringLiteral as u16
            && self
                .arena
                .get_literal(name_node)
                .is_some_and(|lit| lit.text == "constructor")
    }

    fn recovered_object_method_header_has_static(&self, node: &Node, name: NodeIndex) -> bool {
        if self.class_member_emit_depth > 0
            || !self.should_emit_recovered_root_js_declaration_modifiers()
        {
            return false;
        }
        let Some(text) = self.source_text else {
            return false;
        };
        let Some(name_node) = self.arena.get(name) else {
            return false;
        };
        if name_node.pos <= node.pos {
            return false;
        }
        let Ok(header) = crate::safe_slice::slice(text, node.pos as usize, name_node.pos as usize)
        else {
            return false;
        };
        header.trim() == "static"
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
                    if self.should_preserve_native_decorator_comments(modifiers) {
                        self.emit_comments_before_pos(mod_node.pos);
                    }
                    if mod_node.kind == syntax_kind_ext::DECORATOR {
                        // ES decorators are emitted verbatim when not using legacy
                        // (experimental) decorator lowering via __decorate.
                        if !self.ctx.options.legacy_decorators {
                            self.emit(mod_idx);
                            self.write_line();
                        } else {
                            self.skip_comments_for_erased_node(mod_node);
                        }
                    } else if mod_node.kind == SyntaxKind::StaticKeyword as u16 {
                        if !suppress_static {
                            self.write("static ");
                        }
                    } else if mod_node.kind == SyntaxKind::AsyncKeyword as u16 {
                        self.write("async ");
                    } else if mod_node.kind == SyntaxKind::AccessorKeyword as u16
                        && self.ctx.options.target == ScriptTarget::ESNext
                    {
                        self.write("accessor ");
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

        // For JavaScript: skip recovered TypeScript-only property declarations
        // without initializers unless they still have a runtime class-field form.
        // In TypeScript files, native class-field targets preserve typed fields
        // such as `x: T;` as runtime `x;` declarations.
        let target_supports_native_fields =
            (self.ctx.options.target as u32) >= (ScriptTarget::ES2022 as u32);
        let preserves_uninitialized_fields =
            self.ctx.options.use_define_for_class_fields && target_supports_native_fields;
        if prop.initializer.is_none() {
            let is_private = self
                .arena
                .get(prop.name)
                .is_some_and(|n| n.kind == SyntaxKind::PrivateIdentifier as u16);
            let has_accessor = self
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword);
            let has_decorator = !self.collect_class_decorators(&prop.modifiers).is_empty();
            let type_only_native_field = self.source_is_js_file
                && preserves_uninitialized_fields
                && prop.type_annotation.is_some();
            if (!preserves_uninitialized_fields || type_only_native_field)
                && !is_private
                && !has_accessor
                && !has_decorator
            {
                self.skip_comments_for_erased_node(node);
                return;
            }
        }

        // For ES2022+ targets, static fields with initializers are emitted as
        // `static { this.fieldName = value; }` blocks (class static initialization blocks).
        // This preserves the correct `this` and `super` binding inside the class body.
        let is_static = self.has_effective_static_modifier_js(&prop.modifiers);
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
                        let expression = self
                            .arena
                            .get(computed.expression)
                            .filter(|node| node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION)
                            .and_then(|node| self.arena.get_parenthesized(node))
                            .map_or(computed.expression, |paren| paren.expression);
                        if let Some(temp_name) =
                            self.computed_prop_temp_map.get(&expression).cloned()
                        {
                            self.write(&temp_name);
                        } else {
                            self.emit_class_member_name_preserving_class_expression_name(
                                computed.expression,
                            );
                        }
                    }
                } else {
                    self.emit_class_member_name_preserving_class_expression_name(prop.name);
                }
                self.write("] = ");
            } else {
                // `static { this.fieldName = value; }`
                self.write("static { this.");
                self.emit_class_member_name_preserving_class_expression_name(prop.name);
                self.write(" = ");
            }
            self.emit_expression_with_scoped_static_initializer_mode(
                prop.initializer,
                Some("this"),
                None,
                true,
            );
            self.write("; }");
            return;
        }

        // Emit modifiers (static and accessor for JavaScript)
        self.emit_class_member_modifiers_js(&prop.modifiers);
        if self.should_preserve_native_decorator_comments(&prop.modifiers)
            && let Some(name_node) = self.arena.get(prop.name)
        {
            self.emit_comments_before_pos(name_node.pos);
        }

        if prop.initializer.is_some()
            && self.is_tc39_decorated_anonymous_class_expression(prop.initializer)
        {
            let name_node = self.arena.get(prop.name);
            if name_node.is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME) {
                if let Some(computed) = name_node.and_then(|n| self.arena.get_computed_property(n))
                {
                    if let Some(name) = self
                        .tc39_class_expression_name_from_computed_property_expr(computed.expression)
                    {
                        self.emit_class_member_name_preserving_class_expression_name(prop.name);
                        self.write(" = ");
                        self.with_scoped_static_initializer_context_cleared(|this| {
                            this.emit_with_tc39_class_expression_name(
                                prop.initializer,
                                name,
                                false,
                            );
                        });
                        self.write_semicolon();
                        return;
                    }
                    self.write("[");
                    let name_expr =
                        self.emit_tc39_named_class_computed_property_name(computed.expression);
                    self.write("]");
                    self.write(" = ");
                    self.with_scoped_static_initializer_context_cleared(|this| {
                        this.emit_with_tc39_class_expression_name(
                            prop.initializer,
                            name_expr,
                            true,
                        );
                    });
                    self.write_semicolon();
                    return;
                }
            } else if let Some(name) = self.tc39_class_expression_name_from_property_name(prop.name)
            {
                self.emit_class_member_name_preserving_class_expression_name(prop.name);
                self.write(" = ");
                self.with_scoped_static_initializer_context_cleared(|this| {
                    this.emit_with_tc39_class_expression_name(prop.initializer, name, false);
                });
                self.write_semicolon();
                return;
            }
        }

        self.emit_class_member_name_preserving_class_expression_name(prop.name);

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
        self.emit_class_member_modifiers_js_impl(modifiers, true);
    }

    fn emit_accessor_member_modifiers_js(&mut self, modifiers: &Option<NodeList>) {
        self.emit_class_member_modifiers_js_impl(
            modifiers,
            self.ctx.options.target == ScriptTarget::ESNext,
        );
    }

    fn emit_class_member_modifiers_js_impl(
        &mut self,
        modifiers: &Option<NodeList>,
        emit_accessor_keyword: bool,
    ) {
        if let Some(mods) = modifiers {
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
                    if self.should_preserve_native_decorator_comments(modifiers) {
                        self.emit_comments_before_pos(mod_node.pos);
                    }
                    if mod_node.kind == syntax_kind_ext::DECORATOR {
                        if !self.ctx.options.legacy_decorators {
                            self.emit(mod_idx);
                            self.write_line();
                        } else {
                            self.skip_comments_for_erased_node(mod_node);
                        }
                    } else if mod_node.kind == SyntaxKind::StaticKeyword as u16 {
                        if !suppress_static {
                            self.write("static ");
                        }
                    } else if mod_node.kind == SyntaxKind::AccessorKeyword as u16
                        && emit_accessor_keyword
                    {
                        self.write("accessor ");
                    } else if mod_node.kind == SyntaxKind::AsyncKeyword as u16
                        && self.should_emit_recovered_root_js_declaration_modifiers()
                    {
                        self.write("async ");
                    } else if mod_node.kind == SyntaxKind::ExportKeyword as u16 {
                        // `export` on a class member is a parse error, but tsc
                        // preserves it in emit for error-recovery fidelity.
                        self.write("export ");
                    }
                }
            }
        }
    }

    pub(in crate::emitter) fn should_preserve_native_decorator_comments(
        &self,
        modifiers: &Option<NodeList>,
    ) -> bool {
        self.ctx.options.target == ScriptTarget::ESNext
            && !self.ctx.options.legacy_decorators
            && modifiers.as_ref().is_some_and(|mods| {
                mods.nodes.iter().any(|&mod_idx| {
                    self.arena
                        .get(mod_idx)
                        .is_some_and(|node| node.kind == syntax_kind_ext::DECORATOR)
                })
            })
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

        // Emit invalid modifiers on constructors for error-recovery fidelity
        // (e.g., `static constructor()`, `export constructor()`).
        // `accessor` is never valid on a constructor and tsc suppresses it.
        // JS source files (allowJs) strip invalid constructor modifiers instead.
        if !self.source_is_js_file {
            self.emit_class_member_modifiers_js_impl(&ctor.modifiers, false);
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
        if let Some(return_type) =
            self.recovered_constructor_return_type(open_paren_pos, search_end)
        {
            self.write(return_type);
        }
        self.write(" ");

        let prev_emitting_function_body_block = self.emitting_function_body_block;
        self.emitting_function_body_block = true;
        self.function_scope_depth += 1;
        let prev_es5_super_home_depth = self.es5_super_home_function_depth;
        let prev_es5_super_home_static = self.es5_super_home_is_static;
        if self.ctx.target_es5 {
            self.es5_super_home_function_depth = Some(self.function_scope_depth);
            self.es5_super_home_is_static = false;
        }
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
        self.es5_super_home_function_depth = prev_es5_super_home_depth;
        self.es5_super_home_is_static = prev_es5_super_home_static;
        self.function_scope_depth -= 1;
        self.emitting_function_body_block = prev_emitting_function_body_block;
    }

    fn recovered_constructor_return_type(
        &self,
        open_paren_pos: u32,
        search_end: u32,
    ) -> Option<&'a str> {
        let source = self.source_text?;
        let bytes = source.as_bytes();
        let mut scan = open_paren_pos as usize;
        let limit = std::cmp::min(search_end as usize, bytes.len());
        if scan >= limit || bytes.get(scan) != Some(&b'(') {
            return None;
        }

        let mut depth = 0i32;
        while scan < limit {
            match bytes[scan] {
                b'(' | b'[' | b'{' => {
                    depth += 1;
                    scan += 1;
                }
                b')' => {
                    depth -= 1;
                    scan += 1;
                    if depth == 0 {
                        break;
                    }
                }
                b']' | b'}' => {
                    depth -= 1;
                    scan += 1;
                }
                b'/' if scan + 1 < limit && bytes[scan + 1] == b'*' => {
                    scan += 2;
                    while scan + 1 < limit && !(bytes[scan] == b'*' && bytes[scan + 1] == b'/') {
                        scan += 1;
                    }
                    if scan + 1 < limit {
                        scan += 2;
                    }
                }
                b'/' if scan + 1 < limit && bytes[scan + 1] == b'/' => {
                    while scan < limit && bytes[scan] != b'\n' && bytes[scan] != b'\r' {
                        scan += 1;
                    }
                }
                b'\'' | b'"' | b'`' => {
                    let quote = bytes[scan];
                    scan += 1;
                    while scan < limit && bytes[scan] != quote {
                        if bytes[scan] == b'\\' {
                            scan += 1;
                        }
                        scan += 1;
                    }
                    if scan < limit {
                        scan += 1;
                    }
                }
                _ => scan += 1,
            }
        }

        while scan < limit && matches!(bytes[scan], b' ' | b'\t' | b'\r' | b'\n') {
            scan += 1;
        }
        if bytes.get(scan) != Some(&b':') {
            return None;
        }

        let mut end = limit;
        while end > scan && matches!(bytes[end - 1], b' ' | b'\t' | b'\r' | b'\n') {
            end -= 1;
        }
        source.get(scan..end)
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
        auto_accessor_inits: &[(String, Option<NodeIndex>, u32)],
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
        let has_using_region = !self.ctx.options.target.supports_es2025()
            && self.block_has_using_declarations(&block.statements);

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
        // when the source was on one line, there's no prologue to inject, and no
        // hoisted temps. Both `constructor(x) { this.a = x; }` and the
        // multi-statement `constructor(a, b) { this.x = a; this.y = b; }` stay on
        // one line, mirroring tsc's `shouldEmitBlockFunctionBodyOnSingleLine`,
        // which preserves the source layout regardless of statement count (it only
        // forces multi-line when the body is synthesized — i.e. when a prologue or
        // hoisted-temp transform rewrites it, the cases excluded below).
        if !block.statements.nodes.is_empty()
            && !has_prologue
            && !has_function_temps
            && !has_using_region
            && self.is_single_line(block_node)
        {
            self.map_opening_brace(block_node);
            self.write("{ ");
            let var_insert_pos = self.writer.len();
            // An ES2018 object-rest parameter preamble (`var { a } = _a, rest =
            // __rest(_a, ["a"]);`) is written inline so a single-line source body
            // stays single-line, matching tsc and the shared `emit_block`
            // single-line path. Hoisted `??=`/optional-chaining temps still anchor
            // at `var_insert_pos` (captured above, ahead of this preamble).
            if self.emit_object_rest_param_prologue(true, true) {
                self.write(" ");
            }
            let block_close_pos = self
                .find_block_closing_brace_end(block_node)
                .saturating_sub(1);
            let previous_trailing_comment_scan_max = self.trailing_comment_scan_max_pos;
            self.trailing_comment_scan_max_pos = Some(block_close_pos);
            for (i, &stmt_idx) in block.statements.nodes.iter().enumerate() {
                if i > 0 {
                    self.write(" ");
                }
                self.emit(stmt_idx);
            }
            self.trailing_comment_scan_max_pos = previous_trailing_comment_scan_max;
            // A `??=` read-cache temp (or optional-chaining temp) can be created
            // while emitting a statement; declare it inline so the body stays
            // runnable instead of referencing an undeclared `_a`.
            let prologue = self.take_single_line_hoisted_temp_prologue();
            if !prologue.is_empty() {
                self.writer.insert_at(var_insert_pos, &prologue);
            }
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

        let block_close_pos = self
            .find_token_end_before_trivia(block_node.pos, block_node.end)
            .saturating_sub(1);
        let directive_prologue_count = self
            .emit_leading_directive_prologue_statements(&block.statements.nodes, block_close_pos);

        if has_function_temps {
            self.emit_function_body_hoisted_temps();
        }

        if !self.pending_param_prologue.is_empty() {
            self.emit_pending_object_rest_param_preamble(false);
        }

        // Capture anchor for inserting hoisted temps created during statement
        // emit (e.g., `_a` from `??` lowering inside the constructor body).
        // The constructor body emit below may open a `using` try wrapper
        // which raises the live `indent_level` - pinning the anchor's level
        // here keeps the inserted `var _a;` at the body's own indent.
        let hoisted_var_anchor = self.capture_hoist_anchor();
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
                                self.is_constructor_root_super_call(expr_stmt.expression)
                            })
                })
            })
        } else {
            None
        };
        let mut using_region = if has_using_region {
            Some(self.reserve_constructor_using_region(block_idx, &block.statements))
        } else {
            None
        };
        if has_using_region && has_prologue && super_call_idx.is_none() {
            self.emit_constructor_prologue(param_props, field_inits, auto_accessor_inits);
        }
        if let Some(region) = using_region.as_mut() {
            self.begin_constructor_using_region(region);
        }

        // Emit original body statements, inserting prologue after super() if present
        let mut prologue_emitted = !has_prologue || (has_using_region && super_call_idx.is_none());
        for (stmt_i, &stmt_idx) in block
            .statements
            .nodes
            .iter()
            .enumerate()
            .skip(directive_prologue_count)
        {
            if let Some(stmt_node) = self.arena.get(stmt_idx) {
                let actual_start = self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
                self.emit_comments_before_pos(actual_start);
            }

            // If no super() call exists, emit prologue before first body statement
            if !prologue_emitted && super_call_idx.is_none() && stmt_i == directive_prologue_count {
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
        if let Some(region) = using_region {
            self.end_constructor_using_region(region);
        }

        // Insert any hoisted temps created during statement emit (e.g., `_a` from
        // `??`/`??=` lowering). Assignment-target temps and logical-assignment
        // value temps are inserted at the same anchor (value temps end up on the
        // first line, matching `insert_function_body_hoisted_temps_at`).
        let hoisted_indent = self
            .writer
            .indent_string_at(hoisted_var_anchor.indent_level);
        let assignment_temps = std::mem::take(&mut self.hoisted_assignment_temps);
        self.insert_hoisted_var_line(&assignment_temps, &hoisted_var_anchor, &hoisted_indent);
        let value_temps = std::mem::take(&mut self.hoisted_assignment_value_temps);
        self.insert_hoisted_var_line(&value_temps, &hoisted_var_anchor, &hoisted_indent);

        self.decrease_indent();
        self.write("}");
    }

    fn is_constructor_root_super_call(&self, expression: NodeIndex) -> bool {
        let mut current = expression;
        while let Some(node) = self.arena.get(current) {
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                let Some(paren) = self.arena.get_parenthesized(node) else {
                    return false;
                };
                current = paren.expression;
                continue;
            }

            return node.kind == syntax_kind_ext::CALL_EXPRESSION
                && self.arena.get_call_expr(node).is_some_and(|call| {
                    self.arena
                        .get(call.expression)
                        .is_some_and(|callee| callee.kind == SyntaxKind::SuperKeyword as u16)
                });
        }
        false
    }

    /// Emit parameter property and field initializer assignments (constructor prologue).
    pub(in crate::emitter) fn emit_constructor_prologue(
        &mut self,
        param_props: &[String],
        field_inits: &[crate::emitter::core::FieldInit],
        auto_accessor_inits: &[(String, Option<NodeIndex>, u32)],
    ) {
        // Emit `_X_instances.add(this)` for private methods/accessors
        if let Some(ref ws_name) = self.pending_instances_weakset_add.clone() {
            self.write(ws_name);
            self.write(".add(this);");
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
        // Emit `#private`, public, and auto-accessor backing-storage instance field
        // initializers in source order after parameter properties. All three are
        // field initializers for ordering purposes even though `#private` fields and
        // auto-accessor storage lower to `WeakMap.set` — `tsc` evaluates their
        // initializers (which may have observable side effects) strictly in source
        // declaration order, so they must be interleaved by source position rather
        // than emitting the auto-accessor inits last.
        let private_inits = self.pending_private_field_constructor_inits.clone();
        let mut private_idx = 0;
        let mut public_idx = 0;
        let mut accessor_idx = 0;
        while private_idx < private_inits.len()
            || public_idx < field_inits.len()
            || accessor_idx < auto_accessor_inits.len()
        {
            let next_private_order = private_inits
                .get(private_idx)
                .map_or(u32::MAX, |(_, _, _, _, _, source_order, _)| *source_order);
            let next_public_order = field_inits
                .get(public_idx)
                .map_or(u32::MAX, |(_, _, _, _, _, source_order)| *source_order);
            let next_accessor_order = auto_accessor_inits
                .get(accessor_idx)
                .map_or(u32::MAX, |(_, _, source_order)| *source_order);
            // Pick the next initializer by source position. On ties (which should
            // not occur for distinct members) `#private` precedes public, which
            // precedes auto-accessor storage, for deterministic output.
            if next_private_order <= next_public_order && next_private_order <= next_accessor_order
            {
                let (
                    weakmap_name,
                    has_initializer,
                    initializer,
                    leading_comments,
                    trailing_comments,
                    _,
                    storage_kind,
                ) = &private_inits[private_idx];
                self.emit_private_field_constructor_init(
                    weakmap_name,
                    *has_initializer,
                    *initializer,
                    leading_comments,
                    trailing_comments,
                    *storage_kind,
                );
                private_idx += 1;
            } else if next_public_order <= next_accessor_order {
                self.emit_public_field_constructor_init(&field_inits[public_idx]);
                public_idx += 1;
            } else {
                let (storage_name, initializer, _) = &auto_accessor_inits[accessor_idx];
                self.emit_auto_accessor_constructor_init(storage_name, *initializer);
                accessor_idx += 1;
            }
        }
    }

    /// Emit a single auto-accessor backing-storage initializer
    /// (`storage.set(this, <init>)`) in the constructor prologue. Mirrors
    /// `emit_private_field_constructor_init` / `emit_public_field_constructor_init`
    /// so the three-way source-order merge dispatches uniformly.
    fn emit_auto_accessor_constructor_init(
        &mut self,
        storage_name: &str,
        initializer: Option<NodeIndex>,
    ) {
        self.write(storage_name);
        self.write(".set(this, ");
        match initializer {
            Some(init) => self.emit_expression(init),
            None => self.write("void 0"),
        }
        self.write(");");
        self.write_line();
    }

    fn emit_private_field_constructor_init(
        &mut self,
        weakmap_name: &str,
        has_initializer: bool,
        initializer: NodeIndex,
        leading_comments: &[String],
        trailing_comments: &[String],
        storage_kind: crate::emitter::core::PrivateFieldStorageKind,
    ) {
        for comment in leading_comments {
            self.write_comment(comment);
            self.write_line();
        }
        self.write(weakmap_name);
        match storage_kind {
            crate::emitter::core::PrivateFieldStorageKind::WeakMap => {
                self.write(".set(this, ");
            }
            crate::emitter::core::PrivateFieldStorageKind::Value => {
                self.write(" = { value: ");
            }
        }
        if has_initializer {
            self.emit_expression(initializer);
        } else {
            self.write("void 0");
        }
        match storage_kind {
            crate::emitter::core::PrivateFieldStorageKind::WeakMap => self.write(");"),
            crate::emitter::core::PrivateFieldStorageKind::Value => self.write(" };"),
        }
        for comment in trailing_comments {
            self.write_space();
            self.write_comment(comment);
        }
        self.write_line();
    }

    fn emit_public_field_constructor_init(&mut self, field_init: &crate::emitter::core::FieldInit) {
        let (name, init_idx, init_end, leading_comments, trailing_comments, _) = field_init;
        // Emit leading comments from the original property declaration.
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
            if init_idx.is_none() {
                self.write("void 0");
            } else {
                if let Some(init_node) = self.arena.get(*init_idx) {
                    while self.comment_emit_idx < self.all_comments.len()
                        && self.all_comments[self.comment_emit_idx].end <= init_node.pos
                    {
                        self.comment_emit_idx += 1;
                    }
                }
                self.with_scoped_static_initializer_context_cleared(|this| {
                    this.emit_expression(*init_idx);
                });
            }
            self.write_line();
            self.decrease_indent();
            self.write("});");
        } else {
            // Bracket names (e.g., `["constructor"]`) are encoded with `[` prefix.
            if name.starts_with('[') {
                self.write("this");
                self.write(name);
            } else {
                self.write("this.");
                self.write(name);
            }
            self.write(" = ");
            if init_idx.is_none() {
                self.write("void 0");
            } else {
                if let Some(init_node) = self.arena.get(*init_idx) {
                    while self.comment_emit_idx < self.all_comments.len()
                        && self.all_comments[self.comment_emit_idx].end <= init_node.pos
                    {
                        self.comment_emit_idx += 1;
                    }
                }
                let arrow_comment_scan_end =
                    self.source_text.map_or(*init_end, |text| text.len() as u32);
                let arrow_comment_range = self.rightmost_concise_arrow_deferred_comment_range(
                    *init_idx,
                    arrow_comment_scan_end,
                );
                self.with_scoped_static_initializer_context_cleared(|this| {
                    if let Some((comment_start, comment_end)) = arrow_comment_range {
                        this.with_arrow_concise_body_trailing_comments_deferred(
                            comment_start,
                            comment_end,
                            |this| {
                                this.emit_expression(*init_idx);
                            },
                        );
                    } else {
                        this.emit_expression(*init_idx);
                    }
                });
            }
            self.write(";");
            // Emit trailing comments from the original class field. If
            // pre-collected (field appeared before constructor in source), use
            // them. Otherwise fall back to position-based lookup.
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

    pub(in crate::emitter) fn emit_get_accessor(&mut self, node: &Node, accessor_node: NodeIndex) {
        let Some(accessor) = self.arena.get_accessor(node) else {
            return;
        };

        self.emit_accessor_member_modifiers_js(&accessor.modifiers);
        if self.should_preserve_native_decorator_comments(&accessor.modifiers)
            && let Some(name_node) = self.arena.get(accessor.name)
        {
            self.emit_comments_before_pos(name_node.pos);
        }

        self.write("get ");
        self.emit_class_member_name_preserving_class_expression_name(accessor.name);

        // A comment in the source seam between the accessor name and `(`
        // (e.g. `get val /* g */()`) belongs right after the name, before `(`.
        if !self.ctx.flags.in_declaration_emit && accessor.name.is_some() {
            self.emit_name_to_paren_seam_comments(accessor.name, node.end);
        }

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
        let is_static = self.has_effective_static_modifier_js(&accessor.modifiers);
        self.emit_accessor_body(accessor.body, compact_body, is_static);
    }

    pub(in crate::emitter) fn emit_set_accessor(&mut self, node: &Node, accessor_node: NodeIndex) {
        let Some(accessor) = self.arena.get_accessor(node) else {
            return;
        };

        self.emit_accessor_member_modifiers_js(&accessor.modifiers);
        if self.should_preserve_native_decorator_comments(&accessor.modifiers)
            && let Some(name_node) = self.arena.get(accessor.name)
        {
            self.emit_comments_before_pos(name_node.pos);
        }

        self.write("set ");
        self.emit_class_member_name_preserving_class_expression_name(accessor.name);

        // A comment in the source seam between the accessor name and `(`
        // (e.g. `set val /* s */(v)`) belongs right after the name, before `(`.
        if !self.ctx.flags.in_declaration_emit && accessor.name.is_some() {
            self.emit_name_to_paren_seam_comments(accessor.name, node.end);
        }

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
        let needs_es5_param_transform = self.ctx.target_es5
            && accessor.parameters.nodes.iter().any(|&param_idx| {
                self.arena
                    .get(param_idx)
                    .and_then(|param_node| self.arena.get_parameter(param_node))
                    .is_some_and(|param| {
                        param.dot_dot_dot_token
                            || param.initializer.is_some()
                            || self.is_binding_pattern(param.name)
                    })
            });
        let es5_param_transforms = if needs_es5_param_transform {
            Some(self.emit_function_parameters_es5(&accessor.parameters.nodes))
        } else {
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
            None
        };
        self.write(")");

        // Emit return type annotation for error recovery (e.g., `set foo(v): number {}`)
        // Setters cannot legally have return type annotations, but tsc preserves them in JS output.
        if accessor.type_annotation.is_some() {
            self.write(": ");
            self.emit(accessor.type_annotation);
        }

        if let Some(transforms) = es5_param_transforms {
            if transforms.has_transforms() {
                self.write(" ");
                let is_static = self.has_effective_static_modifier_js(&accessor.modifiers);
                let prev_es5_super_home_depth = self.es5_super_home_function_depth;
                let prev_es5_super_home_static = self.es5_super_home_is_static;
                let prev_es5_super_home_object_literal = self.es5_super_home_is_object_literal;
                self.function_scope_depth += 1;
                if self.ctx.target_es5 {
                    self.es5_super_home_function_depth = Some(self.function_scope_depth);
                    self.es5_super_home_is_static = is_static;
                    // See `emit_accessor_body`: ES5 class accessors are lowered
                    // via the class IR pipeline, so an accessor reaching this
                    // direct-printer path is an object-literal accessor whose
                    // `super` home is not prototype-qualified.
                    self.es5_super_home_is_object_literal = self.class_member_emit_depth == 0;
                }
                self.emit_block_with_param_prologue(accessor.body, &transforms);
                self.es5_super_home_function_depth = prev_es5_super_home_depth;
                self.es5_super_home_is_static = prev_es5_super_home_static;
                self.es5_super_home_is_object_literal = prev_es5_super_home_object_literal;
                self.function_scope_depth -= 1;
            } else {
                let compact_body = self.should_emit_compact_empty_accessor_body(accessor_node);
                let is_static = self.has_effective_static_modifier_js(&accessor.modifiers);
                self.emit_accessor_body(accessor.body, compact_body, is_static);
            }
            self.pop_temp_scope();
        } else {
            let compact_body = self.should_emit_compact_empty_accessor_body(accessor_node);
            let is_static = self.has_effective_static_modifier_js(&accessor.modifiers);
            self.emit_accessor_body(accessor.body, compact_body, is_static);
        }
    }

    /// Emit the body of a get/set accessor, handling scope management and fallback to empty body.
    fn emit_accessor_body(&mut self, body: NodeIndex, compact_empty_body: bool, is_static: bool) {
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
            let prev_es5_super_home_depth = self.es5_super_home_function_depth;
            let prev_es5_super_home_static = self.es5_super_home_is_static;
            let prev_es5_super_home_object_literal = self.es5_super_home_is_object_literal;
            if self.ctx.target_es5 {
                self.es5_super_home_function_depth = Some(self.function_scope_depth);
                self.es5_super_home_is_static = is_static;
                // At ES5, class accessor bodies are lowered through the class IR
                // pipeline and never reach this direct-printer path, so any
                // accessor emitted here is an object-literal accessor. Its
                // `super` home binds to the literal's `__proto__` and must emit
                // `_super.X`, not `_super.prototype.X`.
                self.es5_super_home_is_object_literal = self.class_member_emit_depth == 0;
            }
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
            self.es5_super_home_function_depth = prev_es5_super_home_depth;
            self.es5_super_home_is_static = prev_es5_super_home_static;
            self.es5_super_home_is_object_literal = prev_es5_super_home_object_literal;
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
#[path = "class_members_tests.rs"]
mod class_members_tests;
