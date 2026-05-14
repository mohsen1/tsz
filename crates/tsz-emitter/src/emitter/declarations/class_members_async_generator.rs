use super::super::Printer;
use tsz_parser::parser::NodeIndex;

impl<'a> Printer<'a> {
    /// Emit async generator method body lowered to __asyncGenerator.
    /// `async *f() { ... }` becomes `f() { return __asyncGenerator(this, arguments, function* f_1() { ... }); }`
    pub(in crate::emitter) fn emit_method_async_generator_lowered_body(
        &mut self,
        body: NodeIndex,
        name_idx: NodeIndex,
        params: &[NodeIndex],
    ) {
        let method_name = if name_idx.is_some() {
            crate::transforms::emit_utils::identifier_text_or_empty(self.arena, name_idx)
        } else {
            String::new()
        };
        let inner_name =
            (!method_name.is_empty()).then(|| self.next_async_generator_inner_name(&method_name));
        let move_params_to_generator = self.async_generator_params_need_forwarding(params);
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
        let body_is_single_line = self.arena.get(body).is_some_and(|n| self.is_single_line(n));
        let body_has_source_comment = self.arena.get(body).is_some_and(|body_node| {
            self.source_comment_ranges
                .iter()
                .any(|comment| comment.pos >= body_node.pos && comment.end <= body_node.end)
        });
        let body_text_has_comment = self
            .source_text
            .and_then(|text| {
                let body_node = self.arena.get(body)?;
                let start = (body_node.pos as usize).min(text.len());
                let end = (body_node.end as usize).min(text.len());
                (start < end).then(|| &text[start..end])
            })
            .is_some_and(|body_text| body_text.contains("//") || body_text.contains("/*"));
        let body_has_comment = body_has_source_comment || body_text_has_comment;
        let super_capture = if body_is_empty_single_line {
            crate::transforms::emit_utils::AsyncMethodSuperCapture::default()
        } else {
            crate::transforms::emit_utils::collect_async_method_super_capture(self.arena, body)
        };
        let source_text = self.source_text.unwrap_or_default();
        let super_alias_text =
            if super_capture.property_names.is_empty() && !super_capture.needs_element_index {
                None
            } else {
                Some(crate::transforms::emit_utils::hygienic_temp_name(
                    "_super",
                    source_text,
                ))
            };
        let super_index_alias_text = if super_capture.needs_element_index {
            Some(crate::transforms::emit_utils::hygienic_temp_name(
                "_superIndex",
                source_text,
            ))
        } else {
            None
        };
        let super_alias = super_alias_text.as_deref().map(std::sync::Arc::<str>::from);
        let super_index_alias = super_index_alias_text
            .as_deref()
            .map(std::sync::Arc::<str>::from);

        let has_super_capture =
            !super_capture.property_names.is_empty() || super_capture.needs_element_index;
        let emit_single_line_body = (body_is_empty_single_line || body_is_single_line)
            && !body_has_comment
            && !has_super_capture;

        self.write(" {");
        if self.ctx.target_es5 {
            self.write_line();
            self.increase_indent();
            self.write("return ");
            self.write_helper("__asyncGenerator");
            self.write("(this, arguments, ");
            self.emit_async_generator_es5_inner_function(
                inner_name.clone(),
                params,
                body,
                move_params_to_generator,
            );
            self.write(");");
            self.write_line();
            self.decrease_indent();
            self.write("}");
            return;
        }

        if emit_single_line_body {
            if let Some(index_alias) = super_index_alias_text.as_deref() {
                self.write(" const ");
                self.write(index_alias);
                if super_capture.needs_writable_element_index {
                    self.write(" = (function (geti, seti) {");
                    self.write_line();
                    self.increase_indent();
                    self.write("const cache = Object.create(null);");
                    self.write_line();
                    self.write("return name => cache[name] || (cache[name] = { get value() { return geti(name); }, set value(v) { seti(name, v); } });");
                    self.write_line();
                    self.decrease_indent();
                    self.write("})(name => super[name], (name, value) => super[name] = value);");
                } else {
                    self.write(" = name => super[name];");
                }
            }
            if let Some(super_alias_name) = super_alias_text.as_deref() {
                self.write(" const ");
                self.write(super_alias_name);
                self.write(" = Object.create(null, ");
                if super_capture.property_names.is_empty() {
                    self.write("{});");
                } else {
                    self.write("{");
                    self.write_line();
                    self.increase_indent();
                    for (i, name) in super_capture.property_names.iter().enumerate() {
                        self.write(name);
                        self.write(": { get: () => super.");
                        self.write(name);
                        if super_capture.writable_property_names.contains(name) {
                            self.write(", set: v => super.");
                            self.write(name);
                            self.write(" = v");
                        }
                        self.write(" }");
                        if i + 1 < super_capture.property_names.len() {
                            self.write(",");
                        }
                        self.write_line();
                    }
                    self.decrease_indent();
                    self.write("});");
                }
            }
            self.write(" return ");
            self.write_helper("__asyncGenerator");
            self.write("(this, arguments, function* ");
            if let Some(inner_name) = inner_name.as_deref() {
                self.write(inner_name);
            }
            self.write("(");
            let saved_await = self.ctx.emit_await_as_yield_await;
            self.ctx.emit_await_as_yield_await = true;
            if move_params_to_generator {
                self.emit_function_parameters_js(params);
            }
            self.ctx.emit_await_as_yield_await = saved_await;
            self.write(") {");
            if !body_is_empty_single_line {
                let saved_await = self.ctx.emit_await_as_yield_await;
                let prev_super_alias = self.scoped_static_super_base_alias.take();
                let prev_super_direct = self.scoped_static_super_direct_access;
                let prev_super_index_alias = self.scoped_static_super_index_alias.take();
                let prev_super_index_value = self.scoped_static_super_index_value_access;
                self.ctx.emit_await_as_yield_await = true;
                if let Some(alias) = super_alias.clone() {
                    self.scoped_static_super_base_alias = Some(alias);
                    self.scoped_static_super_direct_access = true;
                }
                if let Some(alias) = super_index_alias.clone() {
                    self.scoped_static_super_index_alias = Some(alias);
                    self.scoped_static_super_index_value_access =
                        super_capture.needs_writable_element_index;
                }
                self.function_scope_depth += 1;
                if !self.pending_object_rest_params.is_empty() {
                    self.write(" ");
                    self.emit_pending_object_rest_param_preamble(true);
                }
                if let Some(body_node) = self.arena.get(body)
                    && let Some(block) = self.arena.get_block(body_node)
                {
                    let statements = block.statements.clone();
                    if !self.emit_statement_list_with_using_scope(&statements) {
                        for &stmt in &statements.nodes {
                            self.write(" ");
                            self.emit(stmt);
                        }
                    }
                }
                self.function_scope_depth -= 1;
                self.scoped_static_super_base_alias = prev_super_alias;
                self.scoped_static_super_direct_access = prev_super_direct;
                self.scoped_static_super_index_alias = prev_super_index_alias;
                self.scoped_static_super_index_value_access = prev_super_index_value;
                self.ctx.emit_await_as_yield_await = saved_await;
            }
            self.write(" }); }");
            return;
        }

        self.write_line();
        self.increase_indent();

        if let Some(index_alias) = super_index_alias_text.as_deref() {
            self.write("const ");
            self.write(index_alias);
            if super_capture.needs_writable_element_index {
                self.write(" = (function (geti, seti) {");
                self.write_line();
                self.increase_indent();
                self.write("const cache = Object.create(null);");
                self.write_line();
                self.write("return name => cache[name] || (cache[name] = { get value() { return geti(name); }, set value(v) { seti(name, v); } });");
                self.write_line();
                self.decrease_indent();
                self.write("})(name => super[name], (name, value) => super[name] = value);");
            } else {
                self.write(" = name => super[name];");
            }
            self.write_line();
        }

        if let Some(super_alias_name) = super_alias_text.as_deref() {
            self.write("const ");
            self.write(super_alias_name);
            self.write(" = Object.create(null, ");
            if super_capture.property_names.is_empty() {
                self.write("{});");
                self.write_line();
            } else {
                self.write("{");
                self.write_line();
                self.increase_indent();
                for (i, name) in super_capture.property_names.iter().enumerate() {
                    self.write(name);
                    self.write(": { get: () => super.");
                    self.write(name);
                    if super_capture.writable_property_names.contains(name) {
                        self.write(", set: v => super.");
                        self.write(name);
                        self.write(" = v");
                    }
                    self.write(" }");
                    if i + 1 < super_capture.property_names.len() {
                        self.write(",");
                    }
                    self.write_line();
                }
                self.decrease_indent();
                self.write("});");
                self.write_line();
            }
        }

        self.write("return ");
        self.write_helper("__asyncGenerator");
        self.write("(this, arguments, function* ");
        if let Some(inner_name) = inner_name.as_deref() {
            self.write(inner_name);
        }
        self.write("(");
        let saved_await = self.ctx.emit_await_as_yield_await;
        self.ctx.emit_await_as_yield_await = true;
        if move_params_to_generator {
            self.emit_function_parameters_js(params);
        }
        self.ctx.emit_await_as_yield_await = saved_await;
        self.write(") {");
        self.write_line();
        self.increase_indent();

        let saved = self.ctx.emit_await_as_yield_await;
        self.ctx.emit_await_as_yield_await = true;
        let prev_super_alias = self.scoped_static_super_base_alias.take();
        let prev_super_direct = self.scoped_static_super_direct_access;
        let prev_super_index_alias = self.scoped_static_super_index_alias.take();
        let prev_super_index_value = self.scoped_static_super_index_value_access;
        if let Some(alias) = super_alias {
            self.scoped_static_super_base_alias = Some(alias);
            self.scoped_static_super_direct_access = true;
        }
        if let Some(alias) = super_index_alias {
            self.scoped_static_super_index_alias = Some(alias);
            self.scoped_static_super_index_value_access =
                super_capture.needs_writable_element_index;
        }
        self.function_scope_depth += 1;

        if !self.pending_object_rest_params.is_empty() {
            self.emit_pending_object_rest_param_preamble(false);
        }

        if let Some(body_node) = self.arena.get(body)
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

        self.function_scope_depth -= 1;
        self.scoped_static_super_base_alias = prev_super_alias;
        self.scoped_static_super_direct_access = prev_super_direct;
        self.scoped_static_super_index_alias = prev_super_index_alias;
        self.scoped_static_super_index_value_access = prev_super_index_value;
        self.ctx.emit_await_as_yield_await = saved;

        self.decrease_indent();
        self.write("});");
        self.write_line();
        self.decrease_indent();
        self.write("}");
    }
}
