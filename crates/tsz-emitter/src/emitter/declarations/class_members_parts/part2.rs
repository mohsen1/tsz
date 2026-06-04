impl<'a> Printer<'a> {
    // =========================================================================
    // Class Members
    // =========================================================================

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
