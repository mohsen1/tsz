impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_for_of_statement_es5_async_iterator(
        &mut self,
        for_of_idx: NodeIndex,
        for_in_of: &ForInOfData,
    ) {
        let counter = self.ctx.destructuring_state.for_of_counter;

        // TypeScript's variable naming pattern:
        // - Simple identifier expression `arr`: iterator=arr_1, result=arr_1_1
        // - Complex expression: iterator=_d, result=_e (generic temps)
        // - Top-level hoisted: _a (done), e_N (error), _b (return), _c (value)
        // - For loop: _d (guard)
        // Catch: e_N_1 (error value, not pre-declared)
        let error_container_name = format!("e_{}", counter + 1);
        let system_temp_plan = if self.in_system_execute_body && self.temp_scope_stack.is_empty() {
            self.system_for_await_temp_plans.remove(&for_of_idx)
        } else {
            None
        };
        let loop_done_name = system_temp_plan.as_ref().map_or_else(
            || self.get_temp_var_name(),
            |plan| plan.loop_done_name.clone(),
        );
        let return_temp_name = system_temp_plan.as_ref().map_or_else(
            || {
                self.reserved_iterator_return_temps
                    .remove(&for_of_idx)
                    .unwrap_or_else(|| self.get_temp_var_name())
            },
            |plan| plan.return_temp_name.clone(),
        ); // _a, _b, ...
        let is_nested_iterator_for_of = self.iterator_for_of_depth > 0;
        self.iterator_for_of_depth += 1;

        // Reserve return temps for nested iterator for-of loops in this body before
        // allocating this loop's iterator/result vars.
        self.preallocate_nested_iterator_return_temps(for_in_of.statement);

        let value_temp_name = system_temp_plan.as_ref().map_or_else(
            || self.get_temp_var_name(),
            |plan| plan.value_temp_name.clone(),
        );
        let loop_guard_name = if system_temp_plan.is_some() {
            loop_done_name.clone()
        } else {
            self.get_temp_var_name()
        };
        let (loop_iterator_name, loop_result_name) = if let Some(expr_node) =
            self.arena.get(for_in_of.expression)
            && expr_node.is_identifier()
            && let Some(ident) = self.arena.get_identifier(expr_node)
        {
            let base = self.arena.resolve_identifier_text(ident).to_string();
            let mut iter_name = None;
            for suffix in 1..=100 {
                let candidate = format!("{base}_{suffix}");
                if !self.file_identifiers.contains(&candidate)
                    && !self.generated_temp_names.contains(&candidate)
                {
                    iter_name = Some(candidate);
                    break;
                }
            }
            if let Some(iter_name) = iter_name {
                self.generated_temp_names.insert(iter_name.clone());
                let result_name = format!("{iter_name}_1");
                self.generated_temp_names.insert(result_name.clone());
                (iter_name, result_name)
            } else {
                let a = self.get_temp_var_name();
                let b = self.get_temp_var_name();
                (a, b)
            }
        } else {
            let a = self.get_temp_var_name();
            let b = self.get_temp_var_name();
            (a, b)
        };
        let catch_error_name = format!("e_{}_1", counter + 1);

        self.ctx.destructuring_state.for_of_counter += 1;
        // tsc hoists the for-await loop-init temps (guard/iterator/result) into
        // the enclosing async body's `var` group IFF a preceding `for-of` /
        // `for-await-of` loop exists earlier in the same async body; otherwise
        // it keeps them inline in `for (var _d = true, it = __asyncValues(x),
        // res; ...)`. See `body_has_for_of_before_for_await` for the rule.
        let hoist_loop_init_temps = self.body_has_for_of_before_for_await(for_of_idx);

        // Hoist done/error/return/value temps to the top of the source file scope.
        self.hoisted_for_of_temps.push(loop_done_name.clone());
        self.hoisted_for_of_temps.push(error_container_name.clone());
        self.hoisted_for_of_temps.push(return_temp_name.clone());
        self.hoisted_for_of_temps.push(value_temp_name.clone());
        if hoist_loop_init_temps {
            self.hoisted_for_of_temps.push(loop_guard_name.clone());
            self.hoisted_for_of_temps.push(loop_iterator_name.clone());
            self.hoisted_for_of_temps.push(loop_result_name.clone());
        }

        // try block
        self.write("try {");
        self.write_line();
        self.increase_indent();

        // Leading comments for downlevel for-await-of are deferred by statement emitters
        // and emitted here so they stay attached to the transformed loop body.
        if let Some(for_of_node) = self.arena.get(for_of_idx) {
            let actual_start = self.skip_trivia_forward(for_of_node.pos, for_of_node.end);
            self.emit_comments_before_pos(actual_start);
        }

        // If this for-await-of is the body of a `LabeledStatement`, the label
        // attaches to this inner lowered `for` loop (the actual iteration
        // statement), not the wrapping `try`. Otherwise `continue <label>` would
        // target the `try` and produce non-runnable JavaScript.
        self.emit_downlevel_for_await_loop_label(for_of_idx);

        // for (var _d = true, iterable_1 = __asyncValues(iterable), iterable_1_1; iterable_1_1 = [await/yield/yield __await(...)] iterable_1.next(), _a = iterable_1_1.done, !_a; _d = true) {
        // Inside generated async generator bodies, tsc hoists these loop-init temps
        // into the generator's var group instead of redeclaring them inline.
        self.write("for (");
        if !hoist_loop_init_temps {
            self.write("var ");
        }
        self.write(&loop_guard_name);
        self.write(" = true, ");
        self.write(&loop_iterator_name);
        self.write(" = ");
        if is_nested_iterator_for_of {
            self.write("(");
            self.write(&error_container_name);
            self.write(" = void 0, ");
            self.write_helper("__asyncValues");
            self.write("(");
            self.emit_expression(for_in_of.expression);
            self.write("))");
        } else {
            self.write_helper("__asyncValues");
            self.write("(");
            self.emit_expression(for_in_of.expression);
            self.write(")");
        }
        if !hoist_loop_init_temps {
            self.write(", ");
            self.write(&loop_result_name);
        }
        self.write("; ");
        self.write(&loop_result_name);
        self.write(" = ");
        self.emit_for_await_implicit_await_prefix();
        self.write(&loop_iterator_name);
        self.write(".next()");
        self.emit_for_await_implicit_await_suffix();
        self.write(", ");
        self.write(&loop_done_name);
        self.write(" = ");
        self.write(&loop_result_name);
        self.write(".done, !");
        self.write(&loop_done_name);
        self.write("; ");
        self.write(&loop_guard_name);
        self.write(" = true) {");
        self.write_line();
        self.increase_indent();

        // Enter a new scope for the loop body to track variable shadowing
        self.ctx.block_scope_state.enter_scope();

        // Pre-register loop variables before emitting (needed for shadowing)
        // Note: We only pre-register for VARIABLE_DECLARATION_LIST nodes, not assignment targets
        self.pre_register_for_of_loop_variable(for_in_of.initializer);

        // Check if the initializer is a `using` declaration that needs dispose lowering.
        let using_info = if !self.ctx.options.target.supports_es2025() {
            crate::transforms::emit_utils::for_of_using_info(self.arena, for_in_of.initializer)
        } else {
            None
        };

        if let Some(using_info) = using_info {
            // For `using` in for-await-of with async iterator lowering:
            // Emit: _c = _f.value;
            // Then: _d = false;
            // Then: const d1_1 = _c;
            // Then: const env = ...; try { const d1 = __addDisposable(env, d1_1, ...); body } catch/finally
            let var_name = if using_info.recovered_missing_binding {
                self.get_temp_var_name()
            } else {
                using_info.binding_name
            };
            let using_async = using_info.using_async;
            let value_temp = loop_result_name.clone();

            // Emit value assignment to the temp already reserved with the loop temps.
            let value_assign_temp = value_temp_name.clone();
            self.write(&value_assign_temp);
            self.write(" = ");
            self.write(&value_temp);
            self.write(".value;");
            self.write_line();
            self.write(&loop_guard_name);
            self.write(" = false;");
            self.write_line();

            // Register the outer for-await-of error container name so that
            // next_disposable_env_names doesn't collide with it.
            self.generated_temp_names
                .insert(error_container_name.clone());

            // Generate temp name for the renamed variable: d1 -> d1_1.
            // The surrounding for-await transform already owns e_1, but tsc
            // still uses env_1/result_1 for the resource region and only
            // bumps the catch variable to e_2.
            let (env_name, error_name, result_name, env_id) =
                self.next_disposable_env_names_allowing_error_gap();
            let temp_var_name = format!("{var_name}_{env_id}");
            self.generated_temp_names.insert(temp_var_name.clone());

            // Determine if we use const or var based on target
            let kw = if self.ctx.target_es5 { "var" } else { "const" };

            // Emit: const d1_1 = _c;
            self.write(kw);
            self.write(" ");
            self.write(&temp_var_name);
            self.write(" = ");
            self.write(&value_assign_temp);
            self.write(";");
            self.write_line();

            // Emit dispose wrapper: const env = ...; try { const d1 = __addDisposable(env, d1_1, false); body } catch/finally
            self.write(kw);
            self.write(" ");
            self.write(&env_name);
            self.write(" = { stack: [], error: void 0, hasError: false };");
            self.write_line();
            self.write("try {");
            self.write_line();
            self.increase_indent();

            self.write(kw);
            self.write(" ");
            self.write(&var_name);
            self.write(" = ");
            self.write_helper("__addDisposableResource");
            self.write("(");
            self.write(&env_name);
            self.write(", ");
            self.write(&temp_var_name);
            self.write(", ");
            self.write(if using_async { "true" } else { "false" });
            self.write(");");
            self.write_line();

            // Emit body
            self.emit_for_of_body(for_in_of.statement);

            self.decrease_indent();
            self.write("}");
            self.write_line();
            self.write("catch (");
            self.write(&error_name);
            self.write(") {");
            self.write_line();
            self.increase_indent();
            self.write(&env_name);
            self.write(".error = ");
            self.write(&error_name);
            self.write(";");
            self.write_line();
            self.write(&env_name);
            self.write(".hasError = true;");
            self.write_line();
            self.decrease_indent();
            self.write("}");
            self.write_line();
            self.write("finally {");
            self.write_line();
            self.increase_indent();
            if using_async {
                self.write(kw);
                self.write(" ");
                self.write(&result_name);
                self.write(" = ");
                self.write_helper("__disposeResources");
                self.write("(");
                self.write(&env_name);
                self.write(");");
                self.write_line();
                self.write("if (");
                self.write(&result_name);
                self.write(")");
                self.write_line();
                self.increase_indent();
                self.emit_for_await_implicit_await_prefix();
                self.write(&result_name);
                self.emit_for_await_implicit_await_suffix();
                self.write(";");
                self.write_line();
                self.decrease_indent();
            } else {
                self.write_helper("__disposeResources");
                self.write("(");
                self.write(&env_name);
                self.write(");");
                self.write_line();
            }
            self.decrease_indent();
            self.write("}");
            self.write_line();
        } else {
            // Normal (non-using) path
            self.write(&value_temp_name);
            self.write(" = ");
            self.write(&loop_result_name);
            self.write(".value;");
            self.write_line();
            self.write(&loop_guard_name);
            self.write(" = false;");
            self.write_line();
            self.emit_for_of_value_binding_iterator_es5_async(
                for_in_of.initializer,
                &value_temp_name,
            );
            self.write_line();

            // Emit the loop body
            self.emit_for_of_body(for_in_of.statement);
        }

        // Exit the loop body scope
        self.ctx.block_scope_state.exit_scope();

        self.decrease_indent();
        self.write("}");
        self.write_line();

        self.decrease_indent();
        self.write("}");
        self.write_line();

        // catch block
        self.write("catch (");
        self.write(&catch_error_name);
        self.write(") { ");
        self.write(&error_container_name);
        self.write(" = { error: ");
        self.write(&catch_error_name);
        self.write(" }; }");
        self.write_line();

        // finally block
        self.write("finally {");
        self.write_line();
        self.increase_indent();

        self.write("try {");
        self.write_line();
        self.increase_indent();

        // Cleanup: if (!_e && !_d && (_a = _b.return)) [await/yield/yield __await(...)] _a.call(_b);
        self.write("if (!");
        self.write(&loop_guard_name);
        self.write(" && !");
        self.write(&loop_done_name);
        self.write(" && (");
        self.write(&return_temp_name);
        self.write(" = ");
        self.write(&loop_iterator_name);
        self.write(".return)) ");
        self.emit_for_await_implicit_await_prefix();
        self.write(&return_temp_name);
        self.write(".call(");
        self.write(&loop_iterator_name);
        self.write(")");
        self.emit_for_await_implicit_await_suffix();
        self.write(";");

        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.write_line();

        self.write("finally { if (");
        self.write(&error_container_name);
        self.write(") throw ");
        self.write(&error_container_name);
        self.write(".error; }");

        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.iterator_for_of_depth = self.iterator_for_of_depth.saturating_sub(1);
    }
}
