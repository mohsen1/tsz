use super::super::*;

impl<'a> Printer<'a> {
    /// Emit async method body lowered to `__awaiter` + `function*` for ES2015 target.
    pub(in crate::emitter) fn emit_method_async_lowered_body(
        &mut self,
        body: NodeIndex,
        params: &[NodeIndex],
    ) {
        let move_params_to_generator =
            !self.ctx.target_es5 && self.async_params_need_generator_forwarding(params);

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

        // Issue #3759: Emit `super` capture before entering the generator. tsc
        // pre-binds each referenced `super.<name>` via an `Object.create` block so
        // the generator body can reach captured aliases; `super` is not
        // lexically valid inside a nested generator function.
        let super_capture = if body_is_empty_single_line {
            crate::transforms::emit_utils::AsyncMethodSuperCapture::default()
        } else {
            crate::transforms::emit_utils::collect_async_method_super_capture(self.arena, body)
        };
        let source_text = self.source_text.unwrap_or_default();
        let super_alias_text = if super_capture.property_names.is_empty() {
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

        self.write(" {");
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
            self.write(" = Object.create(null, {");
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
        let generator_hoist_anchor = self.capture_hoist_anchor();
        let outer_hoisted_assignment_temps = std::mem::take(&mut self.hoisted_assignment_temps);
        let outer_hoisted_assignment_value_temps =
            std::mem::take(&mut self.hoisted_assignment_value_temps);
        let outer_hoisted_for_of_temps = std::mem::take(&mut self.hoisted_for_of_temps);

        // Emit function body with await-to-yield substitution and an active
        // `_super` capture alias when the body references super.
        let saved_yield = self.ctx.emit_await_as_yield;
        self.ctx.emit_await_as_yield = true;
        let prev_super_alias = self.scoped_static_super_base_alias.take();
        let prev_super_direct = self.scoped_static_super_direct_access;
        let prev_super_index_alias = self.scoped_static_super_index_alias.take();
        let prev_super_index_value = self.scoped_static_super_index_value_access;
        let prev_function_scope_depth = self.function_scope_depth;
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
                    self.emit(stmt);
                    self.write_line();
                }
            }
        }
        self.function_scope_depth = prev_function_scope_depth;
        self.scoped_static_super_base_alias = prev_super_alias;
        self.scoped_static_super_direct_access = prev_super_direct;
        self.scoped_static_super_index_alias = prev_super_index_alias;
        self.scoped_static_super_index_value_access = prev_super_index_value;
        self.ctx.emit_await_as_yield = saved_yield;
        self.insert_function_body_hoisted_temps_at(generator_hoist_anchor);
        self.hoisted_assignment_temps = outer_hoisted_assignment_temps;
        self.hoisted_assignment_value_temps = outer_hoisted_assignment_value_temps;
        self.hoisted_for_of_temps = outer_hoisted_for_of_temps;

        self.decrease_indent();
        self.write("});");
        self.write_line();
        self.decrease_indent();
        self.write("}");
    }
}
