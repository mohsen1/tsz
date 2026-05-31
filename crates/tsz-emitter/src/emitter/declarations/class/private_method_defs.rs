use super::super::super::Printer;
use crate::emitter::core::{PrivateAccessorDef, PrivateMethodDef};
use std::sync::Arc;
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_private_method_function_def(
        &mut self,
        def: &PrivateMethodDef,
        private_member_def_needs_class_alias: bool,
        class_value_alias: Option<&str>,
        class_name: &str,
    ) {
        self.write(&def.var_name);
        self.write(" = ");
        if def.is_async
            && def.is_generator
            && (self.ctx.options.target as u32) < (ScriptTarget::ES2018 as u32)
        {
            let prev_self_alias = self.scoped_class_expression_self_alias.clone();
            if private_member_def_needs_class_alias
                && let Some(alias) = class_value_alias
                && !class_name.is_empty()
            {
                self.scoped_class_expression_self_alias =
                    Some((Arc::<str>::from(class_name), Arc::<str>::from(alias)));
            }
            self.emit_private_async_generator_function(def);
            self.scoped_class_expression_self_alias = prev_self_alias;
            return;
        }
        if def.is_async
            && !def.is_generator
            && (self.ctx.options.target as u32) < (ScriptTarget::ES2017 as u32)
        {
            let prev_self_alias = self.scoped_class_expression_self_alias.clone();
            if private_member_def_needs_class_alias
                && let Some(alias) = class_value_alias
                && !class_name.is_empty()
            {
                self.scoped_class_expression_self_alias =
                    Some((Arc::<str>::from(class_name), Arc::<str>::from(alias)));
            }
            self.emit_async_function_es5_body(
                &def.var_name,
                &def.params,
                def.body,
                "this",
                NodeIndex::NONE,
            );
            self.scoped_class_expression_self_alias = prev_self_alias;
            return;
        }
        if def.is_async {
            self.write("async ");
        }
        self.write("function");
        if def.is_generator {
            self.write("*");
        }
        self.write(" ");
        self.write(&def.var_name);
        self.write("(");
        self.function_scope_depth += 1;
        self.emit_function_parameters_js(&def.params);
        self.write(") ");

        let prev_self_alias = self.scoped_class_expression_self_alias.clone();
        if private_member_def_needs_class_alias
            && let Some(alias) = class_value_alias
            && !class_name.is_empty()
        {
            self.scoped_class_expression_self_alias =
                Some((Arc::<str>::from(class_name), Arc::<str>::from(alias)));
        }
        let prev_emitting_function_body_block = self.emitting_function_body_block;
        self.emitting_function_body_block = true;
        let prev_pending_function_body_parameters = std::mem::replace(
            &mut self.pending_function_body_parameters,
            def.params.clone(),
        );
        self.ctx.block_scope_state.enter_scope();
        self.push_temp_scope();
        let prev_declared = std::mem::take(&mut self.declared_namespace_names);
        self.prepare_logical_assignment_value_temps(def.body);
        let prev_in_generator = self.ctx.flags.in_generator;
        self.ctx.flags.in_generator = def.is_generator;
        self.emit(def.body);
        self.ctx.flags.in_generator = prev_in_generator;
        self.declared_namespace_names = prev_declared;
        self.pop_temp_scope();
        self.ctx.block_scope_state.exit_scope();
        self.pending_function_body_parameters = prev_pending_function_body_parameters;
        self.emitting_function_body_block = prev_emitting_function_body_block;
        self.function_scope_depth -= 1;
        self.scoped_class_expression_self_alias = prev_self_alias;
    }

    pub(in crate::emitter) fn emit_private_accessor_function_def(
        &mut self,
        def: &PrivateAccessorDef,
        private_member_def_needs_class_alias: bool,
        class_value_alias: Option<&str>,
        class_name: &str,
    ) {
        self.write(&def.var_name);
        self.write(" = ");
        let params = def.param.into_iter().collect::<Vec<_>>();
        if def.is_async
            && let Some(body) = def.body
            && (self.ctx.options.target as u32) < (ScriptTarget::ES2017 as u32)
        {
            let prev_self_alias = self.scoped_class_expression_self_alias.clone();
            if private_member_def_needs_class_alias
                && let Some(alias) = class_value_alias
                && !class_name.is_empty()
            {
                self.scoped_class_expression_self_alias =
                    Some((Arc::<str>::from(class_name), Arc::<str>::from(alias)));
            }
            self.emit_async_function_es5_body(
                &def.var_name,
                &params,
                body,
                "this",
                NodeIndex::NONE,
            );
            self.scoped_class_expression_self_alias = prev_self_alias;
            return;
        }
        self.write("function ");
        self.write(&def.var_name);
        self.write("(");
        self.function_scope_depth += 1;
        if let Some(param_idx) = def.param
            && let Some(param_node) = self.arena.get(param_idx)
            && let Some(param_data) = self.arena.get_parameter(param_node)
        {
            self.emit(param_data.name);
        }
        self.write(") ");

        let prev_self_alias = self.scoped_class_expression_self_alias.clone();
        if private_member_def_needs_class_alias
            && let Some(alias) = class_value_alias
            && !class_name.is_empty()
        {
            self.scoped_class_expression_self_alias =
                Some((Arc::<str>::from(class_name), Arc::<str>::from(alias)));
        }
        let prev_emitting_function_body_block = self.emitting_function_body_block;
        self.emitting_function_body_block = true;
        let prev_pending_function_body_parameters = std::mem::replace(
            &mut self.pending_function_body_parameters,
            def.param.into_iter().collect(),
        );
        self.ctx.block_scope_state.enter_scope();
        self.push_temp_scope();
        let prev_declared = std::mem::take(&mut self.declared_namespace_names);
        if let Some(body) = def.body {
            self.prepare_logical_assignment_value_temps(body);
            self.emit_single_line_block(body);
        } else {
            self.write("{ }");
        }
        self.declared_namespace_names = prev_declared;
        self.pop_temp_scope();
        self.ctx.block_scope_state.exit_scope();
        self.pending_function_body_parameters = prev_pending_function_body_parameters;
        self.emitting_function_body_block = prev_emitting_function_body_block;
        self.function_scope_depth -= 1;
        self.scoped_class_expression_self_alias = prev_self_alias;
    }

    fn emit_private_async_generator_function(&mut self, def: &PrivateMethodDef) {
        self.push_temp_scope();
        let move_params_to_generator = self.async_params_need_generator_forwarding(&def.params);
        let inner_name = self.next_async_generator_inner_name(&def.var_name);
        self.write("function ");
        self.write(&def.var_name);
        self.write("(");
        if move_params_to_generator {
            self.emit_async_outer_parameter_placeholders(&def.params);
        } else {
            self.emit_function_parameters_js(&def.params);
        }
        self.write(") {");

        if self.ctx.target_es5 {
            self.write_line();
            self.increase_indent();
            self.write("return ");
            self.write_helper("__asyncGenerator");
            self.write("(this, arguments, ");
            self.emit_async_generator_es5_inner_function(
                Some(inner_name),
                &def.params,
                def.body,
                move_params_to_generator,
            );
            self.write(");");
            self.write_line();
            self.decrease_indent();
            self.write("}");
            self.pop_temp_scope();
            return;
        }

        let body_is_single_line = self
            .arena
            .get(def.body)
            .is_some_and(|node| self.is_single_line(node));
        if body_is_single_line {
            self.write(" return ");
            self.write_helper("__asyncGenerator");
            self.write("(this, arguments, function* ");
            self.write(&inner_name);
            self.write("(");
            if move_params_to_generator {
                let saved = self.ctx.emit_await_as_yield_await;
                self.ctx.emit_await_as_yield_await = true;
                self.emit_function_parameters_js(&def.params);
                self.ctx.emit_await_as_yield_await = saved;
            }
            self.write(") {");
            self.emit_private_async_generator_body_statements(def.body, true);
            self.write(" }); }");
            self.pop_temp_scope();
            return;
        }

        self.write_line();
        self.increase_indent();
        self.write("return ");
        self.write_helper("__asyncGenerator");
        self.write("(this, arguments, function* ");
        self.write(&inner_name);
        self.write("(");
        if move_params_to_generator {
            let saved = self.ctx.emit_await_as_yield_await;
            self.ctx.emit_await_as_yield_await = true;
            self.emit_function_parameters_js(&def.params);
            self.ctx.emit_await_as_yield_await = saved;
        }
        self.write(") {");
        self.write_line();
        self.increase_indent();
        self.emit_private_async_generator_body_statements(def.body, false);
        self.decrease_indent();
        self.write("});");
        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.pop_temp_scope();
    }

    fn emit_private_async_generator_body_statements(&mut self, body: NodeIndex, inline: bool) {
        let saved = self.ctx.emit_await_as_yield_await;
        self.ctx.emit_await_as_yield_await = true;
        self.function_scope_depth += 1;
        if let Some(body_node) = self.arena.get(body)
            && let Some(block) = self.arena.get_block(body_node)
        {
            let statements = block.statements.clone();
            if inline {
                for &stmt in &statements.nodes {
                    self.write(" ");
                    self.emit_private_async_generator_statement(stmt);
                }
            } else if !self.emit_statement_list_with_using_scope(&statements) {
                for &stmt in &statements.nodes {
                    self.emit_private_async_generator_statement(stmt);
                    self.write_line();
                }
            }
        }
        self.function_scope_depth -= 1;
        self.ctx.emit_await_as_yield_await = saved;
    }

    fn emit_private_async_generator_statement(&mut self, stmt: NodeIndex) {
        let Some(node) = self.arena.get(stmt) else {
            return;
        };
        if node.kind != syntax_kind_ext::RETURN_STATEMENT {
            self.emit(stmt);
            return;
        }
        let Some(ret) = self.arena.get_return_statement(node) else {
            self.emit(stmt);
            return;
        };
        self.write("return");
        if ret.expression.is_some() {
            self.write(" yield ");
            self.write_helper("__await");
            self.write("(");
            self.emit_expression(ret.expression);
            self.write(")");
        }
        self.write_semicolon();
        self.emit_trailing_comment_after_semicolon(node);
    }
}
