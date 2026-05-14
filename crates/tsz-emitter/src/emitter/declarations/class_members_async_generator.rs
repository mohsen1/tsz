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
        let super_property_names = if body_is_empty_single_line {
            Vec::new()
        } else {
            crate::transforms::emit_utils::collect_async_method_super_property_names(
                self.arena, body,
            )
        };
        let super_alias = if super_property_names.is_empty() {
            None
        } else {
            Some(std::sync::Arc::<str>::from("_super"))
        };

        self.write(" {");
        if self.ctx.target_es5 {
            self.write_line();
            self.increase_indent();
            self.write("return ");
            self.write_helper("__asyncGenerator");
            self.write("(this, arguments, ");
            let inner_name = (!method_name.is_empty()).then(|| format!("{method_name}_1"));
            self.emit_async_generator_es5_inner_function(
                inner_name,
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

        if body_is_empty_single_line || body_is_single_line {
            if !super_property_names.is_empty() {
                self.write(" const _super = Object.create(null, {");
                self.write_line();
                self.increase_indent();
                for (i, name) in super_property_names.iter().enumerate() {
                    self.write(name);
                    self.write(": { get: () => super.");
                    self.write(name);
                    self.write(" }");
                    if i + 1 < super_property_names.len() {
                        self.write(",");
                    }
                    self.write_line();
                }
                self.decrease_indent();
                self.write("});");
            }
            self.write(" return ");
            self.write_helper("__asyncGenerator");
            self.write("(this, arguments, function* ");
            if !method_name.is_empty() {
                self.write(&method_name);
                self.write("_1");
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
                self.ctx.emit_await_as_yield_await = true;
                if let Some(alias) = super_alias {
                    self.scoped_static_super_base_alias = Some(alias);
                    self.scoped_static_super_direct_access = true;
                }
                self.function_scope_depth += 1;
                if !self.pending_object_rest_params.is_empty() {
                    self.write(" ");
                    self.emit_pending_object_rest_param_preamble(true);
                }
                if let Some(body_node) = self.arena.get(body)
                    && let Some(block) = self.arena.get_block(body_node)
                {
                    for &stmt in &block.statements.nodes {
                        self.write(" ");
                        self.emit(stmt);
                    }
                }
                self.function_scope_depth -= 1;
                self.scoped_static_super_base_alias = prev_super_alias;
                self.scoped_static_super_direct_access = prev_super_direct;
                self.ctx.emit_await_as_yield_await = saved_await;
            }
            self.write(" }); }");
            return;
        }

        self.write_line();
        self.increase_indent();

        if !super_property_names.is_empty() {
            self.write("const _super = Object.create(null, {");
            self.write_line();
            self.increase_indent();
            for (i, name) in super_property_names.iter().enumerate() {
                self.write(name);
                self.write(": { get: () => super.");
                self.write(name);
                self.write(" }");
                if i + 1 < super_property_names.len() {
                    self.write(",");
                }
                self.write_line();
            }
            self.decrease_indent();
            self.write("});");
            self.write_line();
        }

        self.write("return ");
        self.write_helper("__asyncGenerator");
        self.write("(this, arguments, function* ");
        if !method_name.is_empty() {
            self.write(&method_name);
            self.write("_1");
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
        if let Some(alias) = super_alias {
            self.scoped_static_super_base_alias = Some(alias);
            self.scoped_static_super_direct_access = true;
        }
        self.function_scope_depth += 1;

        if !self.pending_object_rest_params.is_empty() {
            self.emit_pending_object_rest_param_preamble(false);
        }

        if let Some(body_node) = self.arena.get(body)
            && let Some(block) = self.arena.get_block(body_node)
        {
            for &stmt in &block.statements.nodes {
                self.emit(stmt);
                self.write_line();
            }
        }

        self.function_scope_depth -= 1;
        self.scoped_static_super_base_alias = prev_super_alias;
        self.scoped_static_super_direct_access = prev_super_direct;
        self.ctx.emit_await_as_yield_await = saved;

        self.decrease_indent();
        self.write("});");
        self.write_line();
        self.decrease_indent();
        self.write("}");
    }
}
