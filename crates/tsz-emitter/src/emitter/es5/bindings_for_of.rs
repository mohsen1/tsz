//! ES5 for-of binding and loop lowering helpers.

use super::super::Printer;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::ForInOfData;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn preallocate_nested_iterator_return_temps(
        &mut self,
        stmt_idx: NodeIndex,
    ) {
        self.visit_for_of_return_temp_prealloc(stmt_idx);
    }

    pub(in crate::emitter) fn visit_for_of_return_temp_prealloc(&mut self, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::FOR_OF_STATEMENT {
            if let Some(for_in_of) = self.arena.get_for_in_of(node) {
                if !self.reserved_iterator_return_temps.contains_key(&idx) {
                    let temp = self.get_temp_var_name();
                    self.reserved_iterator_return_temps.insert(idx, temp);
                }
                self.visit_for_of_return_temp_prealloc(for_in_of.statement);
            }
            return;
        }

        match node.kind {
            k if k == syntax_kind_ext::BLOCK || k == syntax_kind_ext::CASE_BLOCK => {
                if let Some(block) = self.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        self.visit_for_of_return_temp_prealloc(stmt);
                    }
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.arena.get_if_statement(node) {
                    self.visit_for_of_return_temp_prealloc(if_stmt.then_statement);
                    self.visit_for_of_return_temp_prealloc(if_stmt.else_statement);
                }
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_stmt) = self.arena.get_try(node) {
                    self.visit_for_of_return_temp_prealloc(try_stmt.try_block);
                    self.visit_for_of_return_temp_prealloc(try_stmt.catch_clause);
                    self.visit_for_of_return_temp_prealloc(try_stmt.finally_block);
                }
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_clause) = self.arena.get_catch_clause(node) {
                    self.visit_for_of_return_temp_prealloc(catch_clause.block);
                }
            }
            k if k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT =>
            {
                if let Some(loop_data) = self.arena.get_loop(node) {
                    self.visit_for_of_return_temp_prealloc(loop_data.statement);
                } else if let Some(for_in_of) = self.arena.get_for_in_of(node) {
                    self.visit_for_of_return_temp_prealloc(for_in_of.statement);
                }
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(sw) = self.arena.get_switch(node) {
                    self.visit_for_of_return_temp_prealloc(sw.case_block);
                }
            }
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                if let Some(clause) = self.arena.get_case_clause(node) {
                    for &stmt in &clause.statements.nodes {
                        self.visit_for_of_return_temp_prealloc(stmt);
                    }
                }
            }
            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.arena.get_labeled_statement(node) {
                    self.visit_for_of_return_temp_prealloc(labeled.statement);
                }
            }
            k if k == syntax_kind_ext::WITH_STATEMENT => {
                if let Some(with_stmt) = self.arena.get_with_statement(node) {
                    self.visit_for_of_return_temp_prealloc(with_stmt.then_statement);
                }
            }
            _ => {}
        }
    }

    /// Emit for-of using simple array indexing (default, --downlevelIteration disabled)
    ///
    /// Transforms:
    /// ```typescript
    /// for (const item of arr) { body }
    /// ```
    /// Into:
    /// ```javascript
    /// for (var _i = 0, arr_1 = arr; _i < arr_1.length; _i++) {
    ///     var item = arr_1[_i];
    ///     body
    /// }
    /// ```
    /// Note: This only works for arrays, not for Sets, Maps, Strings, or Generators.
    pub(in crate::emitter) fn emit_for_of_statement_es5_array_indexing(
        &mut self,
        for_in_of: &ForInOfData,
    ) {
        // Simple array indexing pattern (default, no --downlevelIteration):
        // for (var _i = 0, arr_1 = arr; _i < arr_1.length; _i++) {
        //     var v = arr_1[_i];
        //     <body>
        // }
        //
        // TypeScript uses a single global name generator:
        // - First for-of gets `_i` as index name (special case)
        // - All other temp names come from the global counter (_a, _b, _c, ...)
        // - Named arrays use `<name>_1` (doesn't consume from counter)
        // - Names are checked against all identifiers in the source file

        // CRITICAL: Pre-register the loop variable BEFORE emitting the initialization expression
        // This ensures that references to shadowed variables in the array initializer get renamed.
        // For example: `for (let v of [v])` where inner v shadows outer v
        // We need to register inner v as v_1 BEFORE emitting [v] so the reference becomes [v_1]
        self.ctx.block_scope_state.enter_scope();
        self.pre_register_for_of_loop_variable(for_in_of.initializer);

        // Generate index name: first for-of gets `_i`, subsequent ones use global counter
        let index_name = if !self.first_for_of_emitted {
            self.first_for_of_emitted = true;
            let candidate = "_i".to_string();
            if self.file_identifiers.contains(&candidate)
                || self.generated_temp_names.contains(&candidate)
            {
                let name = self.make_unique_name();
                self.ctx.block_scope_state.reserve_name(name.clone());
                name
            } else {
                self.generated_temp_names.insert(candidate.clone());
                self.ctx.block_scope_state.reserve_name(candidate.clone());
                candidate
            }
        } else {
            let name = self.make_unique_name();
            self.ctx.block_scope_state.reserve_name(name.clone());
            name
        };

        // For assignment-pattern for-of with object/array literals, tsc allocates
        // destructuring temps before choosing the array temp in the loop header.
        // Reserve those temps now so later lowering reuses them in order.
        let reserve_count = self.estimate_for_of_assignment_temp_reserve(for_in_of.initializer);
        if reserve_count > 0 {
            self.preallocate_temp_names(reserve_count);
        }

        // Derive array name from expression:
        // - Simple identifier `arr` -> `arr_1`, `arr_2`, etc. (doesn't consume counter)
        // - Complex expression -> `_a`, `_b`, etc. (from global counter)
        let array_name = if let Some(expr_node) = self.arena.get(for_in_of.expression) {
            if expr_node.is_identifier() {
                if let Some(ident) = self.arena.get_identifier(expr_node) {
                    let name = self.arena.resolve_identifier_text(ident).to_string();
                    // Try incrementing suffixes: name_1, name_2, name_3, ...
                    let mut found = None;
                    for suffix in 1..=100 {
                        let candidate = format!("{name}_{suffix}");
                        if !self.file_identifiers.contains(&candidate)
                            && !self.generated_temp_names.contains(&candidate)
                        {
                            found = Some(candidate);
                            break;
                        }
                    }
                    if let Some(candidate) = found {
                        self.generated_temp_names.insert(candidate.clone());
                        // Reserve this name in block scope state to prevent variable shadowing conflicts
                        self.ctx.block_scope_state.reserve_name(candidate.clone());
                        candidate
                    } else {
                        let name = self.make_unique_name_fresh();
                        self.ctx.block_scope_state.reserve_name(name.clone());
                        name
                    }
                } else {
                    let name = self.make_unique_name_fresh();
                    self.ctx.block_scope_state.reserve_name(name.clone());
                    name
                }
            } else {
                let name = self.make_unique_name_fresh();
                self.ctx.block_scope_state.reserve_name(name.clone());
                name
            }
        } else {
            let name = self.make_unique_name_fresh();
            self.ctx.block_scope_state.reserve_name(name.clone());
            name
        };

        // Check if the for-of initializer is a `using` declaration that needs dispose lowering.
        let using_info = if !self.ctx.options.target.supports_es2025() {
            self.for_of_initializer_using_info(for_in_of.initializer)
        } else {
            None
        };

        let capture_context = if using_info.is_none() {
            let init_vars = self.collect_for_of_iteration_var_names(for_in_of.initializer);
            let body_info =
                super::loop_capture::collect_loop_body_vars(self.arena, for_in_of.statement);
            if (!init_vars.is_empty() || !body_info.block_scoped_vars.is_empty())
                && let Some(capture_info) = super::loop_capture::check_loop_needs_capture(
                    self.arena,
                    for_in_of.statement,
                    &init_vars,
                    &body_info.block_scoped_vars,
                )
            {
                let init_var_set: std::collections::HashSet<&str> =
                    init_vars.iter().map(String::as_str).collect();
                let param_vars: Vec<String> = capture_info
                    .captured_vars
                    .iter()
                    .filter(|v| init_var_set.contains(v.as_str()))
                    .cloned()
                    .collect();
                let loop_fn_name = self.ctx.block_scope_state.next_loop_function_name();
                self.emit_loop_function(
                    &loop_fn_name,
                    &param_vars,
                    for_in_of.statement,
                    &body_info,
                    &init_vars,
                );
                self.write_line();
                if !body_info.var_decl_names.is_empty() {
                    self.write("var ");
                    for (i, name) in body_info.var_decl_names.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }
                        self.write(name);
                    }
                    self.write(";");
                    self.write_line();
                }
                Some((loop_fn_name, param_vars, body_info))
            } else {
                None
            }
        } else {
            None
        };

        self.write("for (var ");
        self.write(&index_name);
        self.write(" = 0, ");
        self.write(&array_name);
        self.write(" = ");
        self.emit_expression(for_in_of.expression);
        self.write("; ");
        self.write(&index_name);
        self.write(" < ");
        self.write(&array_name);
        self.write(".length; ");
        self.write(&index_name);
        self.write("++) ");

        self.write("{");
        self.write_line();
        self.increase_indent();

        // Scope was already entered above (before emitting the initialization expression)

        if let Some((var_name, using_async)) = using_info {
            // ES5 for-of with `using`: emit temp binding + dispose wrapper
            // Emit: var d1_1 = _b[_i];
            let (env_name, error_name, result_name) = self.next_disposable_env_names();
            let temp_var_name = format!("{}_{}", var_name, self.next_disposable_env_id - 1);
            self.generated_temp_names.insert(temp_var_name.clone());

            self.write("var ");
            self.write(&temp_var_name);
            self.write(" = ");
            self.write(&array_name);
            self.write("[");
            self.write(&index_name);
            self.write("];");
            self.write_line();

            // Emit dispose wrapper
            self.write("var ");
            self.write(&env_name);
            self.write(" = { stack: [], error: void 0, hasError: false };");
            self.write_line();
            self.write("try {");
            self.write_line();
            self.increase_indent();
            self.write("var ");
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
                self.write("var ");
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
                self.write("await ");
                self.write(&result_name);
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
            self.emit_for_of_value_binding_array_es5(
                for_in_of.initializer,
                &array_name,
                &index_name,
            );
            self.write_line();

            // Emit the loop body
            if let Some((loop_fn_name, param_vars, body_info)) = &capture_context {
                self.emit_loop_call(loop_fn_name, param_vars, body_info);
                self.write_line();
            } else {
                self.emit_for_of_body(for_in_of.statement);
            }
        }

        // Exit the loop body scope
        self.ctx.block_scope_state.exit_scope();

        self.decrease_indent();
        self.write("}");
    }

    pub(in crate::emitter) fn estimate_for_of_assignment_temp_reserve(
        &self,
        initializer: NodeIndex,
    ) -> usize {
        let Some(init_node) = self.arena.get(initializer) else {
            return 0;
        };
        match init_node.kind {
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                if let Some(lit) = self.arena.get_literal_expr(init_node)
                    && lit.elements.nodes.len() > 1
                {
                    // One extracted source temp + per-property default temps.
                    let mut defaults = 0usize;
                    for &elem_idx in &lit.elements.nodes {
                        let Some(elem_node) = self.arena.get(elem_idx) else {
                            continue;
                        };
                        if elem_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                            && let Some(prop) = self.arena.get_property_assignment(elem_node)
                            && let Some(value_node) = self.arena.get(prop.initializer)
                            && value_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                            && let Some(bin) = self.arena.get_binary_expr(value_node)
                            && bin.operator_token == SyntaxKind::EqualsToken as u16
                        {
                            defaults += 1;
                        }
                    }
                    return 1 + defaults;
                }
                0
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if let Some(lit) = self.arena.get_literal_expr(init_node)
                    && lit.elements.nodes.len() > 1
                {
                    // One extracted source temp. Additional nested/default temps are emitted
                    // later and use normal allocation.
                    return 1;
                }
                0
            }
            _ => 0,
        }
    }

    /// Emit the for-of loop body (common logic for both array and iterator modes)
    pub(in crate::emitter) fn emit_for_of_body(&mut self, statement: NodeIndex) {
        if let Some(stmt_node) = self.arena.get(statement) {
            if stmt_node.kind == tsz_parser::parser::syntax_kind_ext::BLOCK {
                // If body is a block, emit its statements directly (unwrap the block)
                if let Some(block) = self.arena.get_block(stmt_node) {
                    for &stmt_idx in &block.statements.nodes {
                        self.emit(stmt_idx);
                        self.write_line();
                    }
                }
            } else {
                self.emit(statement);
                self.write_line();
            }
        }
    }

    /// Emit value binding for iterator protocol: `var item = _a.value;`
    pub(in crate::emitter) fn emit_for_of_value_binding_iterator_es5(
        &mut self,
        initializer: NodeIndex,
        result_name: &str,
    ) {
        if initializer.is_none() {
            return;
        }

        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };

        if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            self.write("var ");
            if let Some(decl_list) = self.arena.get_variable(init_node) {
                let mut first = true;
                for &decl_idx in &decl_list.declarations.nodes {
                    if let Some(decl_node) = self.arena.get(decl_idx)
                        && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                    {
                        // Check if name is a binding pattern (array or object destructuring)
                        if self.is_binding_pattern(decl.name) {
                            // For downlevelIteration with binding patterns, use __read
                            // Transform: var [a = 0, b = 1] = _c.value
                            // Into: var _d = __read(_c.value, 2), _e = _d[0], a = _e === void 0 ? 0 : _e, ...
                            self.emit_es5_destructuring_with_read(
                                decl.name,
                                &format!("{result_name}.value"),
                                &mut first,
                            );
                        } else {
                            if !first {
                                self.write(", ");
                            }
                            first = false;
                            // Simple identifier binding
                            self.emit(decl.name);
                            self.write(" = ");
                            self.write(result_name);
                            self.write(".value");
                        }
                    }
                }
            }
            self.write_semicolon();
        } else if self.is_binding_pattern(initializer) {
            self.write("var ");
            let mut first = true;
            self.emit_es5_destructuring_from_value(
                initializer,
                &format!("{result_name}.value"),
                &mut first,
            );
            self.write_semicolon();
        } else {
            self.emit_expression(initializer);
            self.write(" = ");
            self.write(result_name);
            self.write(".value");
            self.write_semicolon();
        }
    }

    /// Emit value binding for async iterator protocol from an already extracted value.
    /// Uses direct assignment (no `__read`) for `for await...of` downleveling.
    pub(in crate::emitter) fn emit_for_of_value_binding_iterator_es5_async(
        &mut self,
        initializer: NodeIndex,
        value_name: &str,
    ) {
        if initializer.is_none() {
            return;
        }

        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };

        if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            let flags = init_node.flags as u32;
            let keyword = if self.ctx.target_es5 {
                "var"
            } else if flags & tsz_parser::parser::node_flags::CONST != 0 {
                "const"
            } else if flags & tsz_parser::parser::node_flags::LET != 0 {
                "let"
            } else {
                "var"
            };
            self.write(keyword);
            self.write(" ");
            if let Some(decl_list) = self.arena.get_variable(init_node) {
                let mut first = true;
                for &decl_idx in &decl_list.declarations.nodes {
                    if let Some(decl_node) = self.arena.get(decl_idx)
                        && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                    {
                        if self.is_binding_pattern(decl.name) {
                            let mut first = true;
                            self.emit_es5_destructuring_from_value(
                                decl.name, value_name, &mut first,
                            );
                        } else {
                            if !first {
                                self.write(", ");
                            }
                            first = false;
                            self.emit(decl.name);
                            self.write(" = ");
                            self.write(value_name);
                        }
                    }
                }
            }
            self.write_semicolon();
        } else if self.is_binding_pattern(initializer) {
            self.write("var ");
            let mut first = true;
            self.emit_es5_destructuring_from_value(initializer, value_name, &mut first);
            self.write_semicolon();
        } else {
            self.emit_expression(initializer);
            self.write(" = ");
            self.write(value_name);
            self.write_semicolon();
        }
    }

    /// Pre-register loop variables before emitting the for-of initialization expression.
    /// This ensures that references to outer variables with the same name get properly renamed.
    ///
    /// For example: `for (let v of [v])` where inner v shadows outer v
    /// We register inner v as `v_1`, so when we emit [v], it becomes [`v_1`]
    ///
    /// Note: Only registers variables from `VARIABLE_DECLARATION_LIST` nodes (e.g., `for (let v of ...)`).
    /// Bare identifiers (e.g., `for (v of ...)`) are assignment targets, not declarations, so they don't
    /// create new variables and shouldn't be pre-registered.
    pub(in crate::emitter) fn pre_register_for_of_loop_variable(&mut self, initializer: NodeIndex) {
        if initializer.is_none() {
            return;
        }

        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };

        // Only handle variable declaration list: `for (let v of ...)`
        // Do NOT handle bare identifiers: `for (v of ...)` - those are assignments, not declarations
        // Note: Pre-register for both var and let/const in for-of loops because loop
        // temporaries (e.g., a_1 for array copy) create naming conflicts that must be avoided.
        if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            && let Some(decl_list) = self.arena.get_variable(init_node)
        {
            for &decl_idx in &decl_list.declarations.nodes {
                if let Some(decl_node) = self.arena.get(decl_idx)
                    && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                {
                    self.pre_register_binding_name(decl.name);
                }
            }
        }
        // Note: We explicitly do NOT pre-register for the else case (bare identifiers or patterns)
        // because those are assignment targets, not declarations
    }

    /// Pre-register a binding name (identifier or pattern) in the current scope
    pub(in crate::emitter) fn pre_register_binding_name(&mut self, name_idx: NodeIndex) {
        if name_idx.is_none() {
            return;
        }

        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        // Simple identifier: register it directly
        if name_node.is_identifier() {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                let original_name = self.arena.resolve_identifier_text(ident);
                self.ctx.block_scope_state.register_variable(original_name);
            }
        }
        // Destructuring patterns: extract all binding identifiers
        else if matches!(
            name_node.kind,
            syntax_kind_ext::ARRAY_BINDING_PATTERN | syntax_kind_ext::OBJECT_BINDING_PATTERN
        ) && let Some(pattern) = self.arena.get_binding_pattern(name_node)
        {
            for &elem_idx in &pattern.elements.nodes {
                if let Some(elem_node) = self.arena.get(elem_idx)
                    && let Some(elem) = self.arena.get_binding_element(elem_node)
                {
                    self.pre_register_binding_name(elem.name);
                }
            }
        }
    }

    /// Pre-register a var binding name. Uses `register_var_declaration` which allows
    /// same-scope redeclarations but renames for parent-scope conflicts.
    pub(in crate::emitter) fn pre_register_var_binding_name(&mut self, name_idx: NodeIndex) {
        if name_idx.is_none() {
            return;
        }

        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        if name_node.is_identifier() {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                let original_name = self.arena.resolve_identifier_text(ident);
                self.ctx
                    .block_scope_state
                    .register_var_declaration(original_name);
            }
        } else if matches!(
            name_node.kind,
            syntax_kind_ext::ARRAY_BINDING_PATTERN | syntax_kind_ext::OBJECT_BINDING_PATTERN
        ) {
            if let Some(pattern) = self.arena.get_binding_pattern(name_node) {
                for &elem_idx in &pattern.elements.nodes {
                    if let Some(elem_node) = self.arena.get(elem_idx)
                        && let Some(elem) = self.arena.get_binding_element(elem_node)
                    {
                        self.pre_register_var_binding_name(elem.name);
                    }
                }
            }
            if let Some(pattern) = self.arena.get_binding_pattern(name_node) {
                for &elem_idx in &pattern.elements.nodes {
                    if let Some(elem_node) = self.arena.get(elem_idx)
                        && let Some(elem) = self.arena.get_binding_element(elem_node)
                    {
                        self.pre_register_var_binding_name(elem.name);
                    }
                }
            }
        }
    }

    /// Emit variable binding for array-indexed for-of pattern:
    /// `var v = _a[_i];`
    pub(in crate::emitter) fn emit_for_of_value_binding_array_es5(
        &mut self,
        initializer: NodeIndex,
        array_name: &str,
        index_name: &str,
    ) {
        if initializer.is_none() {
            return;
        }

        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };

        let element_expr = format!("{array_name}[{index_name}]");

        if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            self.write("var ");
            if let Some(decl_list) = self.arena.get_variable(init_node) {
                // Empty declaration list (e.g. `for (var of X)` where `of` is parsed
                // as the keyword, leaving zero declarators).  tsc still emits a temp
                // variable assignment inside the loop body, so we do the same.
                if decl_list.declarations.nodes.is_empty() {
                    let temp = self.get_temp_var_name();
                    self.write(&temp);
                    self.write(" = ");
                    self.write(&element_expr);
                    self.write_semicolon();
                    return;
                }
                let mut first = true;
                // `for...of` allows only a single declaration. If the source
                // has multiple (`for (var a, b of X)`), that is a syntax error
                // and tsc only processes the first declaration. Match that.
                for &decl_idx in decl_list.declarations.nodes.iter().take(1) {
                    if let Some(decl_node) = self.arena.get(decl_idx)
                        && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                    {
                        if self.is_binding_pattern(decl.name) {
                            if let Some(pattern_node) = self.arena.get(decl.name) {
                                // Object patterns: for single-property patterns, use element_expr
                                // directly. For multi-property, create a temp.
                                if pattern_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
                                    let (obj_count, obj_rest) =
                                        self.count_effective_bindings(pattern_node);
                                    if obj_count <= 1 && !obj_rest {
                                        // Single property: var nameA = robots_1[_i].name
                                        self.emit_es5_destructuring_pattern_direct(
                                            pattern_node,
                                            &element_expr,
                                            &mut first,
                                        );
                                    } else {
                                        // Multi property: var _p = robots_1[_o], nameA = _p.name, skillA = _p.skill
                                        let temp_name = self.get_temp_var_name();
                                        if !first {
                                            self.write(", ");
                                        }
                                        first = false;
                                        self.write(&temp_name);
                                        self.write(" = ");
                                        self.write(&element_expr);
                                        self.emit_es5_destructuring_pattern(
                                            pattern_node,
                                            &temp_name,
                                        );
                                    }
                                    continue;
                                }

                                let (effective_count, has_rest) =
                                    self.count_effective_bindings(pattern_node);

                                // Single element at index 0: inline as name = arr[idx][0]
                                if effective_count == 1
                                    && !has_rest
                                    && self.try_emit_single_inline_from_expr(
                                        pattern_node,
                                        &element_expr,
                                        &mut first,
                                    )
                                {
                                    continue;
                                }

                                // Rest-only: inline as name = arr[idx].slice(0)
                                if effective_count == 0
                                    && has_rest
                                    && self.try_emit_rest_only_from_expr(
                                        pattern_node,
                                        &element_expr,
                                        &mut first,
                                    )
                                {
                                    continue;
                                }

                                // Multi-binding or complex: create temp and lower
                                // e.g., var [, nameA] = robots_1[_i] → var _a = robots_1[_i], nameA = _a[1]
                                let temp_name = self.get_temp_var_name();
                                if !first {
                                    self.write(", ");
                                }
                                first = false;
                                self.write(&temp_name);
                                self.write(" = ");
                                self.write(&element_expr);
                                self.emit_es5_destructuring_pattern(pattern_node, &temp_name);
                            }
                        } else {
                            if !first {
                                self.write(", ");
                            }
                            first = false;

                            // Handle variable shadowing: get the pre-registered renamed name
                            // (variable was already registered in pre_register_for_of_loop_variable)
                            if let Some(ident_node) = self.arena.get(decl.name) {
                                if ident_node.is_identifier() {
                                    if let Some(ident) = self.arena.get_identifier(ident_node) {
                                        let original_name =
                                            self.arena.resolve_identifier_text(ident);
                                        let emitted_name = self
                                            .ctx
                                            .block_scope_state
                                            .get_emitted_name(original_name)
                                            .unwrap_or_else(|| original_name.to_string());
                                        self.write(&emitted_name);
                                    } else {
                                        self.emit(decl.name);
                                    }
                                } else {
                                    self.emit(decl.name);
                                }
                            } else {
                                self.emit(decl.name);
                            }

                            self.write(" = ");
                            self.write(&element_expr);
                        }
                    }
                }
            }
            self.write_semicolon();
        } else if init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
            || init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        {
            // Assignment destructuring pattern in for-of: {name: nameA} or [, nameA]
            // Lower to: nameA = element_expr.name or nameA = element_expr[1]
            let mut first = true;
            match init_node.kind {
                k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                    if let Some(lit) = self.arena.get_literal_expr(init_node) {
                        let elem_count = lit.elements.nodes.len();
                        if elem_count > 1 {
                            // Multi-element: need temp
                            let temp = self.make_unique_name_hoisted_assignment();
                            self.write(&temp);
                            self.write(" = ");
                            self.write(&element_expr);
                            first = false;
                            self.emit_assignment_array_destructuring(
                                &lit.elements.nodes,
                                &temp,
                                &mut first,
                                None,
                            );
                        } else {
                            // Single element: inline
                            self.emit_assignment_array_destructuring(
                                &lit.elements.nodes,
                                &element_expr,
                                &mut first,
                                None,
                            );
                        }
                    }
                }
                _ => {
                    // Object pattern
                    if let Some(lit) = self.arena.get_literal_expr(init_node) {
                        let elem_count = lit.elements.nodes.len();
                        if elem_count > 1 {
                            let temp = self.make_unique_name_hoisted_assignment();
                            self.write(&temp);
                            self.write(" = ");
                            self.write(&element_expr);
                            first = false;
                            self.emit_assignment_object_destructuring(
                                &lit.elements.nodes,
                                &temp,
                                &mut first,
                            );
                        } else {
                            self.emit_assignment_object_destructuring(
                                &lit.elements.nodes,
                                &element_expr,
                                &mut first,
                            );
                        }
                    }
                }
            }
            self.write_semicolon();
        } else {
            self.emit_expression(initializer);
            self.write(" = ");
            self.write(&element_expr);
            self.write_semicolon();
        }
    }
}
