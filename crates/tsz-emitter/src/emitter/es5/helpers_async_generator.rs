//! ES5 async generator emission support.

use super::super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn next_async_generator_inner_name(&mut self, base: &str) -> String {
        loop {
            let count = self
                .async_generator_inner_name_counts
                .entry(base.to_string())
                .and_modify(|count| *count += 1)
                .or_insert(1);
            let candidate = format!("{base}_{count}");
            if !self.file_identifiers.contains(&candidate) {
                return candidate;
            }
        }
    }

    pub(in crate::emitter) fn emit_async_generator_es5_inner_function(
        &mut self,
        inner_name: Option<String>,
        params: &[NodeIndex],
        body: NodeIndex,
        include_params: bool,
    ) {
        use crate::transforms::async_es5_ir::AsyncES5Transformer;
        use crate::transforms::ir_printer::IRPrinter;

        let mut transformer = AsyncES5Transformer::new(self.arena);
        if let Some(text) = self.source_text {
            transformer.set_source_text(text);
        }
        let ir = transformer.transform_async_generator_inner_function(
            inner_name,
            params,
            body,
            include_params,
        );
        let mut printer = IRPrinter::with_arena(self.arena);
        printer.set_transforms(self.transforms.clone());
        printer.set_target_es5(true);
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
    }

    pub(in crate::emitter) fn emit_async_generator_es5_function_wrapper(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
        outer_name: &str,
        inner_name: Option<String>,
    ) {
        let move_params_to_generator =
            self.async_generator_params_need_forwarding(&func.parameters.nodes);
        if outer_name.is_empty() {
            self.write("function (");
        } else {
            self.write("function ");
            self.write(outer_name);
            self.write("(");
        }
        if move_params_to_generator {
            self.emit_async_outer_parameter_placeholders(&func.parameters.nodes);
        } else {
            self.emit_function_parameters_js(&func.parameters.nodes);
        }
        self.write(") {");
        self.write_line();
        self.increase_indent();
        self.write("return ");
        self.write_helper("__asyncGenerator");
        self.write("(this, arguments, ");
        self.emit_async_generator_es5_inner_function(
            inner_name,
            &func.parameters.nodes,
            func.body,
            move_params_to_generator,
        );
        self.write(");");
        self.write_line();
        self.decrease_indent();
        self.write("}");
    }

    pub(in crate::emitter) fn emit_async_generator_es5_object_method_property(
        &mut self,
        property_name: &str,
        params: &[NodeIndex],
        body: NodeIndex,
    ) {
        let move_params_to_generator = self.async_generator_params_need_forwarding(params);
        self.write(property_name);
        self.write(": function (");
        if move_params_to_generator {
            self.emit_async_outer_parameter_placeholders(params);
        } else {
            self.emit_function_parameters_js(params);
        }
        self.write(") {");
        self.write_line();
        self.increase_indent();
        self.write("return ");
        self.write_helper("__asyncGenerator");
        self.write("(this, arguments, ");
        let inner_name = (!property_name.is_empty()).then(|| format!("{property_name}_1"));
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
    }

    pub(in crate::emitter) fn emit_async_generator_es5_object_method_value(
        &mut self,
        property_name: &str,
        params: &[NodeIndex],
        body: NodeIndex,
    ) {
        let move_params_to_generator = self.async_generator_params_need_forwarding(params);
        self.write("function (");
        if move_params_to_generator {
            self.emit_async_outer_parameter_placeholders(params);
        } else {
            self.emit_function_parameters_js(params);
        }
        self.write(") {");
        self.write_line();
        self.increase_indent();
        self.write("return ");
        self.write_helper("__asyncGenerator");
        self.write("(this, arguments, ");
        let inner_name = (!property_name.is_empty()).then(|| format!("{property_name}_1"));
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
    }

    /// Emit an async generator function lowered to `__asyncGenerator` wrapper.
    /// `async function* f() { ... }` becomes:
    /// `function f() { return __asyncGenerator(this, arguments, function* f_1() { ... }); }`
    pub(in crate::emitter) fn emit_async_generator_lowered(
        &mut self,
        func: &tsz_parser::parser::node::FunctionData,
        func_name: &str,
    ) {
        self.push_temp_scope();
        let move_params_to_generator =
            self.async_generator_params_need_forwarding(&func.parameters.nodes);
        let inner_name =
            (!func_name.is_empty()).then(|| self.next_async_generator_inner_name(func_name));
        if self.ctx.target_es5 {
            self.emit_async_generator_es5_function_wrapper(func, func_name, inner_name);
            self.pop_temp_scope();
            return;
        }
        let body_is_empty_single_line = func.body.is_some()
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
        let body_is_single_line = func.body.is_some()
            && self
                .arena
                .get(func.body)
                .is_some_and(|n| self.is_single_line(n));

        // function name(params) {
        if func_name.is_empty() {
            self.write("function (");
        } else {
            self.write("function ");
            self.write(func_name);
            self.write("(");
        }
        if move_params_to_generator {
            self.emit_async_outer_parameter_placeholders(&func.parameters.nodes);
        } else {
            self.emit_function_parameters_js(&func.parameters.nodes);
        }
        self.write(") {");
        if body_is_empty_single_line || body_is_single_line {
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
                self.emit_function_parameters_js(&func.parameters.nodes);
            }
            self.ctx.emit_await_as_yield_await = saved_await;
            self.write(") {");
            if !body_is_empty_single_line {
                let saved_await = self.ctx.emit_await_as_yield_await;
                self.ctx.emit_await_as_yield_await = true;
                self.function_scope_depth += 1;
                if !self.pending_object_rest_params.is_empty() {
                    self.write(" ");
                    self.emit_pending_object_rest_param_preamble(true);
                }
                if let Some(body_node) = self.arena.get(func.body)
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
                self.ctx.emit_await_as_yield_await = saved_await;
            }
            self.write(" }); }");
            self.pop_temp_scope();
            return;
        }
        self.write_line();
        self.increase_indent();

        // return __asyncGenerator(this, arguments, function* name_1() {
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
            self.emit_function_parameters_js(&func.parameters.nodes);
        }
        self.ctx.emit_await_as_yield_await = saved_await;
        self.write(") {");
        self.write_line();
        self.increase_indent();
        let generator_hoist_byte_offset = self.writer.len();
        let generator_hoist_line = self.writer.current_line();
        let hoisted_assignment_start = self.hoisted_assignment_temps.len();
        let hoisted_for_of_start = self.hoisted_for_of_temps.len();
        let hoisted_value_start = self.hoisted_assignment_value_temps.len();

        // Set flag so `await expr` emits as `yield __await(expr)`
        let saved = self.ctx.emit_await_as_yield_await;
        self.ctx.emit_await_as_yield_await = true;
        self.function_scope_depth += 1;

        if !self.pending_object_rest_params.is_empty() {
            self.emit_pending_object_rest_param_preamble(false);
        }

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

        self.function_scope_depth -= 1;
        self.ctx.emit_await_as_yield_await = saved;
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

        self.decrease_indent();
        self.write("});");
        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.pop_temp_scope();
    }

    pub(in crate::emitter) fn async_generator_params_need_forwarding(
        &self,
        params: &[NodeIndex],
    ) -> bool {
        params.iter().copied().any(|p| {
            let Some(node) = self.arena.get(p) else {
                return false;
            };
            let Some(param) = self.arena.get_parameter(node) else {
                return false;
            };
            if param.initializer.is_some() {
                return true;
            }
            self.arena
                .get(param.name)
                .is_some_and(|name_node| name_node.kind != SyntaxKind::Identifier as u16)
        })
    }
}
