//! ES5 destructuring - for-of array indexing and assignment destructuring.

use super::{Printer, is_valid_identifier_name};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{ForInOfData, Node};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(super) fn preallocate_nested_iterator_return_temps(&mut self, stmt_idx: NodeIndex) {
        self.visit_for_of_return_temp_prealloc(stmt_idx);
    }

    pub(super) fn visit_for_of_return_temp_prealloc(&mut self, idx: NodeIndex) {
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
    pub(super) fn emit_for_of_statement_es5_array_indexing(&mut self, for_in_of: &ForInOfData) {
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
            if expr_node.kind == SyntaxKind::Identifier as u16 {
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

        self.emit_for_of_value_binding_array_es5(for_in_of.initializer, &array_name, &index_name);
        self.write_line();

        // Emit the loop body
        self.emit_for_of_body(for_in_of.statement);

        // Exit the loop body scope
        self.ctx.block_scope_state.exit_scope();

        self.decrease_indent();
        self.write("}");
    }

    pub(super) fn estimate_for_of_assignment_temp_reserve(&self, initializer: NodeIndex) -> usize {
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
    pub(super) fn emit_for_of_body(&mut self, statement: NodeIndex) {
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
    pub(super) fn emit_for_of_value_binding_iterator_es5(
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

    /// Emit value binding for async iterator protocol: `var item = _a.value;`
    /// Uses direct `.value` access (no `__read`) for `for await...of` downleveling.
    pub(super) fn emit_for_of_value_binding_iterator_es5_async(
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
                        if self.is_binding_pattern(decl.name) {
                            let mut first = true;
                            self.emit_es5_destructuring_from_value(
                                decl.name,
                                &format!("{result_name}.value"),
                                &mut first,
                            );
                        } else {
                            if !first {
                                self.write(", ");
                            }
                            first = false;
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

    /// Pre-register loop variables before emitting the for-of initialization expression.
    /// This ensures that references to outer variables with the same name get properly renamed.
    ///
    /// For example: `for (let v of [v])` where inner v shadows outer v
    /// We register inner v as `v_1`, so when we emit [v], it becomes [`v_1`]
    ///
    /// Note: Only registers variables from `VARIABLE_DECLARATION_LIST` nodes (e.g., `for (let v of ...)`).
    /// Bare identifiers (e.g., `for (v of ...)`) are assignment targets, not declarations, so they don't
    /// create new variables and shouldn't be pre-registered.
    pub(super) fn pre_register_for_of_loop_variable(&mut self, initializer: NodeIndex) {
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
    pub(super) fn pre_register_binding_name(&mut self, name_idx: NodeIndex) {
        if name_idx.is_none() {
            return;
        }

        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        // Simple identifier: register it directly
        if name_node.kind == SyntaxKind::Identifier as u16 {
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
    pub(super) fn pre_register_var_binding_name(&mut self, name_idx: NodeIndex) {
        if name_idx.is_none() {
            return;
        }

        let Some(name_node) = self.arena.get(name_idx) else {
            return;
        };

        if name_node.kind == SyntaxKind::Identifier as u16 {
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
    pub(super) fn emit_for_of_value_binding_array_es5(
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
                let mut first = true;
                for &decl_idx in &decl_list.declarations.nodes {
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
                                if ident_node.kind == SyntaxKind::Identifier as u16 {
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

    // =========================================================================
    // Assignment destructuring lowering (ES5)
    // Lowers: [, nameA] = expr  →  nameA = expr[1]
    //         { name: nameA } = expr  →  nameA = expr.name
    // =========================================================================

    /// Count the total number of elements (including holes) in an array destructuring pattern.
    /// TypeScript creates a temp for non-identifier sources when there are 2+ elements
    /// (including holes). With exactly 1 element (no holes), it inlines the source.
    const fn count_array_destructuring_elements(&self, elements: &[NodeIndex]) -> usize {
        elements.len()
    }

    /// Lower an assignment destructuring pattern to ES5.
    /// Called from `emit_binary_expression` when left side is array/object literal.
    pub(super) fn emit_assignment_destructuring_es5(
        &mut self,
        left_node: &Node,
        right_idx: NodeIndex,
    ) {
        // Determine if right side is a simple identifier (can be accessed directly)
        let is_simple = self
            .arena
            .get(right_idx)
            .is_some_and(|n| n.kind == SyntaxKind::Identifier as u16);

        // Count elements to determine if we need a temp for complex sources.
        // TypeScript creates a temp for non-identifier sources when there are 2+ elements
        // (including holes). With exactly 1 element (no holes), it inlines the source.
        let element_count = if is_simple {
            0 // doesn't matter for identifiers
        } else {
            match left_node.kind {
                k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                    if let Some(lit) = self.arena.get_literal_expr(left_node) {
                        self.count_array_destructuring_elements(&lit.elements.nodes)
                    } else {
                        2 // fallback: assume needs temp
                    }
                }
                k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                    if let Some(pattern) = self.arena.get_binding_pattern(left_node) {
                        self.count_array_destructuring_elements(&pattern.elements.nodes)
                    } else {
                        2
                    }
                }
                k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => 2,
                _ => 2, // object patterns always need temp for now
            }
        };

        // For complex sources (function calls, array literals), we only need a temp
        // if the pattern requires multiple accesses. Single-access patterns can
        // inline the source expression directly.
        let needs_temp = !is_simple && element_count > 1;

        let source_name = if is_simple {
            self.get_identifier_text(right_idx)
        } else if needs_temp {
            let temp = self.make_unique_name_hoisted_assignment();
            self.write(&temp);
            self.write(" = ");
            self.emit(right_idx);
            temp
        } else {
            // Single access: use empty string as source_name marker,
            // and we'll inline the right_idx expression at the access point
            String::new()
        };

        let use_inline_source = !is_simple && !needs_temp;
        let mut first = !needs_temp;

        match left_node.kind {
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if let Some(lit) = self.arena.get_literal_expr(left_node) {
                    self.emit_assignment_array_destructuring(
                        &lit.elements.nodes,
                        &source_name,
                        &mut first,
                        use_inline_source.then_some(right_idx),
                    );
                }
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN
                || k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION =>
            {
                if let Some(elements) = self.get_binding_or_literal_elements(left_node) {
                    match left_node.kind {
                        k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                            self.emit_assignment_array_destructuring(
                                &elements,
                                &source_name,
                                &mut first,
                                use_inline_source.then_some(right_idx),
                            );
                        }
                        k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                            || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION =>
                        {
                            self.emit_assignment_object_destructuring(
                                &elements,
                                &source_name,
                                &mut first,
                            );
                        }
                        _ => {}
                    }
                } else {
                    self.emit_node_default(left_node, right_idx);
                }
            }
            _ => {
                // Fallback: emit as-is
                self.emit_node_default(left_node, right_idx);
            }
        }
    }

    /// Emit lowered array assignment destructuring.
    /// `[, nameA, [primaryB, secondaryB]] = source` →
    /// `nameA = source[1], _a = source[2], primaryB = _a[0], secondaryB = _a[1]`
    ///
    /// When `inline_source` is Some, the source expression is emitted inline
    /// instead of using the `source` string. Used when only one access is needed.
    pub(super) fn emit_assignment_array_destructuring(
        &mut self,
        elements: &[NodeIndex],
        source: &str,
        first: &mut bool,
        inline_source: Option<NodeIndex>,
    ) {
        for (i, &elem_idx) in elements.iter().enumerate() {
            if elem_idx.is_none() {
                continue;
            }
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };

            // Check for spread element: [...rest]
            if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                if let Some(spread) = self.arena.get_spread(elem_node) {
                    self.emit_assignment_separator(first);
                    let target_node = self.arena.get(spread.expression);
                    if let Some(tn) = target_node {
                        if tn.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            || tn.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                            || tn.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                            || tn.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        {
                            // Nested destructuring on rest
                            let temp = self.make_unique_name_hoisted_assignment();
                            self.write(&temp);
                            self.write(" = ");
                            if let Some(inline_src) = inline_source {
                                self.emit(inline_src);
                            } else {
                                self.write(source);
                            }
                            self.write(".slice(");
                            self.write_usize(i);
                            self.write(")");
                            self.emit_assignment_nested_destructuring(
                                spread.expression,
                                &temp,
                                first,
                            );
                        } else {
                            self.emit(spread.expression);
                            self.write(" = ");
                            if let Some(inline_src) = inline_source {
                                self.emit(inline_src);
                            } else {
                                self.write(source);
                            }
                            self.write(".slice(");
                            self.write_usize(i);
                            self.write(")");
                        }
                    }
                }
                continue;
            }

            // Check if element has a default value (BinaryExpression with =)
            if elem_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(bin) = self.arena.get_binary_expr(elem_node)
                && bin.operator_token == SyntaxKind::EqualsToken as u16
            {
                // Element with default: target = source[i] === void 0 ? default : source[i]
                let target_node = self.arena.get(bin.left);
                let is_nested = target_node.is_some_and(|n| {
                    n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                        || n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                        || n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                });

                if is_nested {
                    let extract_temp = self.make_unique_name_hoisted_assignment();
                    let default_temp = self.make_unique_name_hoisted_assignment();
                    self.emit_assignment_separator(first);
                    self.write(&extract_temp);
                    self.write(" = ");
                    if let Some(inline_src) = inline_source {
                        self.emit(inline_src);
                    } else {
                        self.write(source);
                    }
                    self.write("[");
                    self.write_usize(i);
                    self.write("], ");
                    self.write(&default_temp);
                    self.write(" = ");
                    self.write(&extract_temp);
                    self.write(" === void 0 ? ");
                    self.emit(bin.right);
                    self.write(" : ");
                    self.write(&extract_temp);
                    self.emit_assignment_nested_destructuring(bin.left, &default_temp, first);
                } else {
                    let temp = self.make_unique_name_hoisted_assignment();
                    self.emit_assignment_separator(first);
                    self.write(&temp);
                    self.write(" = ");
                    if let Some(inline_src) = inline_source {
                        self.emit(inline_src);
                    } else {
                        self.write(source);
                    }
                    self.write("[");
                    self.write_usize(i);
                    self.write("], ");
                    self.emit(bin.left);
                    self.write(" = ");
                    self.write(&temp);
                    self.write(" === void 0 ? ");
                    self.emit(bin.right);
                    self.write(" : ");
                    self.write(&temp);
                }
                continue;
            }

            // Check for nested array/object destructuring
            if elem_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || elem_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || elem_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                || elem_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            {
                let temp = self.make_unique_name_hoisted_assignment();
                self.emit_assignment_separator(first);
                self.write(&temp);
                self.write(" = ");
                if let Some(inline_src) = inline_source {
                    self.emit(inline_src);
                } else {
                    self.write(source);
                }
                self.write("[");
                self.write_usize(i);
                self.write("]");
                self.emit_assignment_nested_destructuring(elem_idx, &temp, first);
                continue;
            }

            // Simple identifier target
            self.emit_assignment_separator(first);
            self.emit(elem_idx);
            self.write(" = ");
            if let Some(inline_src) = inline_source {
                self.emit(inline_src);
            } else {
                self.write(source);
            }
            self.write("[");
            self.write_usize(i);
            self.write("]");
        }
    }

    /// Emit lowered object assignment destructuring.
    /// `{ name: nameA, skill: skillA } = source` →
    /// `nameA = source.name, skillA = source.skill`
    pub(super) fn emit_assignment_object_destructuring(
        &mut self,
        elements: &[NodeIndex],
        source: &str,
        first: &mut bool,
    ) {
        for &elem_idx in elements {
            if elem_idx.is_none() {
                continue;
            }
            let Some(elem_node) = self.arena.get(elem_idx) else {
                continue;
            };

            match elem_node.kind {
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    if let Some(prop) = self.arena.get_property_assignment(elem_node) {
                        let key_text = self.get_property_key_text(prop.name);
                        let key = key_text.unwrap_or_default();

                        // Check if value is a nested pattern
                        let value_node = self.arena.get(prop.initializer);
                        let is_nested = value_node.is_some_and(|n| {
                            n.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                                || n.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                || n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        });

                        if is_nested {
                            let temp = self.make_unique_name_hoisted_assignment();
                            self.emit_assignment_separator(first);
                            self.write(&temp);
                            self.write(" = ");
                            self.emit_object_key_access(source, &key);
                            self.emit_assignment_nested_destructuring(
                                prop.initializer,
                                &temp,
                                first,
                            );
                        } else {
                            // Check for default value: { name: nameA = "default" }
                            let value_bin = value_node.and_then(|n| {
                                if n.kind == syntax_kind_ext::BINARY_EXPRESSION {
                                    self.arena.get_binary_expr(n)
                                } else {
                                    None
                                }
                            });
                            if let Some(bin) = value_bin
                                && bin.operator_token == SyntaxKind::EqualsToken as u16
                            {
                                let temp = self.make_unique_name_hoisted_assignment();
                                self.emit_assignment_separator(first);
                                self.write(&temp);
                                self.write(" = ");
                                self.emit_object_key_access(source, &key);
                                self.write(", ");
                                self.emit(bin.left);
                                self.write(" = ");
                                self.write(&temp);
                                self.write(" === void 0 ? ");
                                self.emit(bin.right);
                                self.write(" : ");
                                self.write(&temp);
                                continue;
                            }
                            self.emit_assignment_separator(first);
                            self.emit(prop.initializer);
                            self.write(" = ");
                            self.emit_object_key_access(source, &key);
                        }
                    }
                }
                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    // { name } → name = source.name
                    if let Some(shorthand) = self.arena.get_shorthand_property(elem_node) {
                        let name = self.get_identifier_text(shorthand.name);
                        self.emit_assignment_separator(first);
                        self.write(&name);
                        self.write(" = ");
                        self.emit_object_key_access(source, &name);
                    }
                }
                k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => {
                    // { ...rest } → rest = __rest(source, ["prop1", "prop2"])
                    if let Some(spread) = self.arena.get_spread(elem_node) {
                        self.emit_assignment_separator(first);
                        self.emit(spread.expression);
                        self.write(" = __rest(");
                        self.write(source);
                        self.write(", [");
                        // Collect non-rest property names
                        let mut prop_first = true;
                        for &other_idx in elements {
                            if other_idx == elem_idx {
                                continue;
                            }
                            if let Some(other_node) = self.arena.get(other_idx) {
                                let key = match other_node.kind {
                                    k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                                        .arena
                                        .get_property_assignment(other_node)
                                        .and_then(|p| self.get_property_key_text(p.name)),
                                    k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                                        self.arena
                                            .get_shorthand_property(other_node)
                                            .map(|s| self.get_identifier_text(s.name))
                                    }
                                    _ => None,
                                };
                                if let Some(k) = key {
                                    if !prop_first {
                                        self.write(", ");
                                    }
                                    self.write("\"");
                                    self.write(&k);
                                    self.write("\"");
                                    prop_first = false;
                                }
                            }
                        }
                        self.write("])");
                    }
                }
                _ => {}
            }
        }
    }

    /// Helper to emit nested destructuring from a source name.
    pub(super) fn emit_assignment_nested_destructuring(
        &mut self,
        pattern_idx: NodeIndex,
        source: &str,
        first: &mut bool,
    ) {
        let Some(node) = self.arena.get(pattern_idx) else {
            return;
        };
        match node.kind {
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                if let Some(lit) = self.arena.get_literal_expr(node) {
                    self.emit_assignment_array_destructuring(
                        &lit.elements.nodes,
                        source,
                        first,
                        None,
                    );
                }
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                if let Some(lit) = self.arena.get_literal_expr(node) {
                    self.emit_assignment_object_destructuring(&lit.elements.nodes, source, first);
                }
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    self.emit_assignment_array_destructuring(
                        &pattern.elements.nodes,
                        source,
                        first,
                        None,
                    );
                }
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    self.emit_assignment_object_destructuring(
                        &pattern.elements.nodes,
                        source,
                        first,
                    );
                }
            }
            _ => {}
        }
    }

    pub(super) fn emit_object_key_access(&mut self, source: &str, key: &str) {
        if is_valid_identifier_name(key) {
            self.write(source);
            self.write(".");
            self.write(key);
        } else {
            self.write(source);
            self.write("[\"");
            self.write(&key.replace('\\', "\\\\").replace('\"', "\\\""));
            self.write("\"]");
        }
    }

    pub(super) fn get_binding_or_literal_elements(&self, node: &Node) -> Option<Vec<NodeIndex>> {
        self.arena
            .get_literal_expr(node)
            .map(|lit| lit.elements.nodes.to_vec())
            .or_else(|| {
                self.arena
                    .get_binding_pattern(node)
                    .map(|pattern| pattern.elements.nodes.to_vec())
            })
    }

    /// Emit separator for assignment destructuring (`, ` between parts).
    pub(super) fn emit_assignment_separator(&mut self, first: &mut bool) {
        if !*first {
            self.write(", ");
        }
        *first = false;
    }

    /// Get property key text from a property name node.
    pub(super) fn get_property_key_text(&self, name_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(name_idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            Some(self.get_identifier_text(name_idx))
        } else if node.kind == SyntaxKind::StringLiteral as u16 {
            // For string keys like { "name": value }
            self.get_string_literal_text(name_idx)
        } else if node.kind == SyntaxKind::NumericLiteral as u16 {
            self.get_numeric_literal_text(name_idx)
        } else {
            None
        }
    }

    pub(super) fn get_string_literal_text(&self, idx: NodeIndex) -> Option<String> {
        let source = self.source_text?;
        let node = self.arena.get(idx)?;
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let end = node.end as usize;
        let text = &source[start..end];
        // Strip quotes
        if text.len() >= 2 && (text.starts_with('"') || text.starts_with('\'')) {
            Some(text[1..text.len() - 1].to_string())
        } else {
            Some(text.to_string())
        }
    }

    pub(super) fn get_numeric_literal_text(&self, idx: NodeIndex) -> Option<String> {
        let source = self.source_text?;
        let node = self.arena.get(idx)?;
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let end = node.end as usize;
        Some(source[start..end].to_string())
    }
}
