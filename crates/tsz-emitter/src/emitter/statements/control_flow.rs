use super::super::{Printer, get_trailing_comment_ranges};
use tsz_parser::parser::node::Node;
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

struct RecoveredSwitchClass {
    header: String,
    inline_body: Option<String>,
}

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_if_statement(&mut self, node: &Node) {
        let Some(if_stmt) = self.arena.get_if_statement(node) else {
            return;
        };

        let then_is_block = self
            .arena
            .get(if_stmt.then_statement)
            .is_some_and(|n| n.kind == syntax_kind_ext::BLOCK);

        // TSC always puts non-block then-statements on their own indented line,
        // e.g., `if (cond)\n    return;`. Block then-statements stay on the same
        // line: `if (cond) { ... }`.
        let then_is_multiline_in_source = !then_is_block;

        self.write("if (");
        // tsc inserts a space between `if (` and a leading inline block
        // comment on the condition, then suppresses the post-comment
        // space so the next token sits flush against `*/`. e.g.
        //   `if ( /** @type {T} */(a))` (tsc)  vs
        //   `if (/** @type {T} */ (a))` (tsz before this fix).
        // Mirrors the spread-element pattern in special_expressions.rs.
        if let Some(expr_node) = self.arena.get(if_stmt.expression)
            && self.has_pending_comment_before(expr_node.pos)
        {
            self.write(" ");
            self.emit_comments_before_pos(expr_node.pos);
            self.pending_block_comment_space = false;
        }
        self.emit(if_stmt.expression);
        // Map closing `)` — scan backward from then-statement start
        if let Some(then_node) = self.arena.get(if_stmt.then_statement) {
            self.map_closing_paren_backward(node.pos, then_node.pos);
        }
        self.write(")");
        if then_is_multiline_in_source {
            self.write_line();
            if !then_is_block {
                self.increase_indent();
            }
        } else {
            self.write(" ");
        }
        let before_then = self.writer.len();
        self.emit(if_stmt.then_statement);
        // If the then-statement was completely erased (e.g. const enum),
        // emit `;` to produce a valid empty statement.
        if self.writer.len() == before_then {
            self.write(";");
        }
        if then_is_multiline_in_source && !then_is_block {
            self.decrease_indent();
        }

        if if_stmt.else_statement.is_some() {
            // Emit leading comments before the `else` keyword. These are
            // comments that appear between the then block's `}` and the
            // `else`, e.g., `} // All non-winged beasts\n else {`.
            if let Some(else_node) = self.arena.get(if_stmt.else_statement) {
                let actual_start = self.skip_trivia_forward(else_node.pos, else_node.end);
                // Emit trailing comments on the then block's closing line first
                if let Some(then_node) = self.arena.get(if_stmt.then_statement) {
                    let then_token_end =
                        self.find_token_end_before_trivia(then_node.pos, actual_start);
                    self.emit_trailing_comments_before(then_token_end, actual_start);
                }
            }
            self.write_line();
            // Emit any leading block comments before `else` on their own lines
            if let Some(else_node) = self.arena.get(if_stmt.else_statement) {
                let actual_start = self.skip_trivia_forward(else_node.pos, else_node.end);
                self.emit_comments_before_pos(actual_start);
            }
            // Map the `else` keyword to its source position
            if let Some(then_node) = self.arena.get(if_stmt.then_statement)
                && let Some(else_node) = self.arena.get(if_stmt.else_statement)
            {
                self.map_token_after_skipping_whitespace(then_node.end, else_node.pos);
            }
            // Check if the else body is erased (e.g. const enum).
            // We need to detect this before emitting to format the empty
            // statement correctly on a new indented line.
            let else_is_erased = self
                .arena
                .get(if_stmt.else_statement)
                .is_some_and(|n| self.is_erased_statement(n));
            let else_is_if = self
                .arena
                .get(if_stmt.else_statement)
                .is_some_and(|n| n.kind == syntax_kind_ext::IF_STATEMENT);
            let else_is_block = self
                .arena
                .get(if_stmt.else_statement)
                .is_some_and(|n| n.kind == syntax_kind_ext::BLOCK);
            if else_is_erased && !else_is_if {
                self.write("else");
                self.write_line();
                self.increase_indent();
                self.emit(if_stmt.else_statement);
                self.write(";");
                self.decrease_indent();
            } else if !else_is_if && !else_is_block {
                // Non-block, non-if else body: put on new indented line,
                // e.g., `else\n    return;` — matching tsc behavior.
                self.write("else");
                self.write_line();
                self.increase_indent();
                self.emit(if_stmt.else_statement);
                self.decrease_indent();
            } else {
                self.write("else ");
                let before_else = self.writer.len();
                self.emit(if_stmt.else_statement);
                if self.writer.len() == before_else {
                    self.write(";");
                }
            }
        }
    }

    pub(in crate::emitter) fn emit_while_statement(&mut self, node: &Node) {
        let Some(loop_stmt) = self.arena.get_loop(node) else {
            return;
        };

        // ES5: Check if closures capture body-scoped let/const variables
        if self.ctx.target_es5 {
            let body_info = super::super::es5::loop_capture::collect_loop_body_vars(
                self.arena,
                loop_stmt.statement,
            );
            if !body_info.block_scoped_vars.is_empty()
                && let Some(capture_info) =
                    super::super::es5::loop_capture::check_loop_needs_capture(
                        self.arena,
                        loop_stmt.statement,
                        &[],
                        &body_info.block_scoped_vars,
                    )
            {
                self.emit_while_statement_with_capture(node, loop_stmt, &capture_info, &body_info);
                return;
            }
        }

        self.write("while (");
        self.emit(loop_stmt.condition);
        // Map closing `)` — scan backward from body start
        if let Some(body_node) = self.arena.get(loop_stmt.statement) {
            self.map_closing_paren_backward(node.pos, body_node.pos);
        }
        self.write(")");
        self.emit_loop_body(loop_stmt.statement);
    }

    pub(in crate::emitter) fn emit_for_statement(&mut self, node: &Node) {
        let Some(loop_stmt) = self.arena.get_loop(node) else {
            return;
        };

        // Check if the for initializer has `using` that needs lowering.
        // `for (using d1 = expr, d2 = expr2;;) { body }`
        // becomes:
        // `{ const env_1 = ...; try { const d1 = __addDisposable(env_1, expr, false), d2 = ...; for (;;) { body } } catch ... finally ... }`
        if !self.ctx.target_es5
            && !self.ctx.options.target.supports_es2025()
            && self.for_initializer_has_using(loop_stmt.initializer)
        {
            self.emit_for_with_using_lowering(node, loop_stmt);
            return;
        }

        // ES5: Check if closures capture loop variables (let/const) —
        // if so, emit the _loop_N IIFE pattern instead of a plain for-loop.
        // Capture can happen with let/const from the initializer OR the body.
        if self.ctx.target_es5 {
            let init_vars = self.collect_for_initializer_let_const_vars(loop_stmt.initializer);
            let body_info = super::super::es5::loop_capture::collect_loop_body_vars(
                self.arena,
                loop_stmt.statement,
            );
            if (!init_vars.is_empty() || !body_info.block_scoped_vars.is_empty())
                && let Some(capture_info) =
                    super::super::es5::loop_capture::check_loop_needs_capture(
                        self.arena,
                        loop_stmt.statement,
                        &init_vars,
                        &body_info.block_scoped_vars,
                    )
            {
                self.emit_for_statement_with_capture(
                    node,
                    loop_stmt,
                    &capture_info,
                    &init_vars,
                    &body_info,
                );
                return;
            }
        }

        let hoisted_initializer_exports =
            self.deferred_exported_var_initializers(loop_stmt.initializer);
        if hoisted_initializer_exports.len() == 1 {
            let (local_name, export_name, init_idx) = &hoisted_initializer_exports[0];
            if !self.in_system_execute_body {
                self.write("var ");
            }
            self.write(local_name);
            self.write(" = ");
            self.emit(*init_idx);
            self.write(";");
            self.write_line();
            self.write_export_binding_start(export_name);
            self.write(local_name);
            self.write_export_binding_end();
            if self.in_system_execute_body {
                self.system_folded_export_names.insert(local_name.clone());
            }
            self.write_line();
        }

        // Pre-locate both `;` positions in the for-header by scanning from
        // the statement start to the body start.  The parser often includes the
        // `;` inside the preceding node's range, so per-child scans miss them.
        let (semi1_src, semi2_src) = {
            let body_start = self
                .arena
                .get(loop_stmt.statement)
                .map_or(node.end, |n| n.pos);
            if let Some(text) = self.source_text_for_map() {
                let bytes = text.as_bytes();
                let start = node.pos as usize;
                let end = (body_start as usize).min(bytes.len());
                let mut semis = Vec::new();
                for (i, &b) in bytes[start..end].iter().enumerate() {
                    if b == b';' {
                        semis.push(start + i);
                    }
                }
                let s1 = semis
                    .first()
                    .and_then(|&p| self.fast_source_position(p as u32));
                let s2 = semis
                    .get(1)
                    .and_then(|&p| self.fast_source_position(p as u32));
                (s1, s2)
            } else {
                (None, None)
            }
        };

        self.write("for (");
        if hoisted_initializer_exports.len() == 1 {
            // The exported `var` initializer was emitted immediately before the loop
            // so the export assignment observes the initialized value.
        } else if self.in_system_execute_body {
            self.emit_for_initializer_strip_var(loop_stmt.initializer);
        } else if self.emit_async_generator_shadow_for_initializer(loop_stmt.initializer) {
            // Rewritten from `var x = y` to `x = y`; the declaration is hoisted
            // at the top of the generated async generator body.
        } else {
            self.emit(loop_stmt.initializer);
        }
        // Map first `;` in for-header
        self.pending_source_pos = semi1_src;
        self.write(";");
        if loop_stmt.condition.is_some() {
            self.write(" ");
            self.emit(loop_stmt.condition);
        }
        // Map second `;` in for-header
        self.pending_source_pos = semi2_src;
        self.write(";");
        if loop_stmt.incrementor.is_some() {
            self.write(" ");
            let prev_stmt = self.ctx.flags.in_statement_expression;
            self.ctx.flags.in_statement_expression = true;
            self.emit(loop_stmt.incrementor);
            self.ctx.flags.in_statement_expression = prev_stmt;
        }
        let recovered_empty_header_comment =
            self.recovered_empty_for_header_body_comment(node, loop_stmt);
        if let Some((comment_pos, comment_end, comment_has_trailing_newline)) =
            recovered_empty_header_comment
        {
            if let Some(text) = self.source_text
                && let Ok(comment_text) =
                    crate::safe_slice::slice(text, comment_pos as usize, comment_end as usize)
            {
                self.write(" ");
                self.write_comment_with_reindent(comment_text, Some(comment_pos));
                if comment_has_trailing_newline {
                    self.write_line();
                }
            }
        }

        // Map closing `)` — scan backward from body start
        if let Some(body_node) = self.arena.get(loop_stmt.statement) {
            self.map_closing_paren_backward(node.pos, body_node.pos);
        }
        self.write(")");
        self.emit_loop_body(loop_stmt.statement);
    }

    fn recovered_empty_for_header_body_comment(
        &self,
        node: &Node,
        loop_stmt: &tsz_parser::parser::node::LoopData,
    ) -> Option<(u32, u32, bool)> {
        if loop_stmt.initializer.is_some()
            || loop_stmt.condition.is_some()
            || loop_stmt.incrementor.is_some()
        {
            return None;
        }

        let text = self.source_text?;
        let body_node = self.arena.get(loop_stmt.statement)?;
        if body_node.kind != syntax_kind_ext::BLOCK {
            return None;
        }

        let bytes = text.as_bytes();
        let search_start = body_node.pos as usize;
        let search_end = (body_node.end as usize).min(bytes.len());
        let brace_pos = bytes
            .get(search_start..search_end)?
            .iter()
            .position(|&b| b == b'{')
            .map(|offset| search_start + offset)?;

        let header_start = node.pos as usize;
        let header_end = brace_pos.min(bytes.len());
        if bytes.get(header_start..header_end)?.contains(&b';') {
            return None;
        }

        let comment = get_trailing_comment_ranges(text, brace_pos + 1)
            .into_iter()
            .next()?;
        Some((comment.pos, comment.end, comment.has_trailing_newline))
    }

    pub(in crate::emitter) fn emit_for_in_statement(&mut self, node: &Node) {
        let Some(for_in_of) = self.arena.get_for_in_of(node) else {
            return;
        };
        let initializer_exports = self
            .deferred_exported_var_iteration_bindings(for_in_of.initializer, for_in_of.statement);

        self.write("for (");
        // In System modules, `var` declarations are hoisted to the module scope,
        // so `for (var key in ...)` becomes `for (key in ...)`.
        if self.ctx.target_es5
            && self.emit_for_in_missing_destructuring_initializer_es5(for_in_of.initializer)
        {
            // The recovery path emitted the initializer.
        } else if self.in_system_execute_body {
            self.emit_for_initializer_strip_var(for_in_of.initializer);
        } else if self.emit_async_generator_shadow_for_in_of_initializer(for_in_of.initializer) {
            // Rewritten from `var x in y` to `x in y`; the declaration is hoisted
            // at the top of the generated async generator body.
        } else {
            self.emit(for_in_of.initializer);
        }
        self.write(" in ");
        self.emit(for_in_of.expression);
        // Map closing `)` — scan backward from body start
        if let Some(body_node) = self.arena.get(for_in_of.statement) {
            self.map_closing_paren_backward(node.pos, body_node.pos);
        }
        self.write(")");
        if initializer_exports.is_empty() {
            self.emit_loop_body(for_in_of.statement);
        } else {
            self.emit_loop_body_with_deferred_exports(for_in_of.statement, &initializer_exports);
        }
    }

    fn emit_for_in_missing_destructuring_initializer_es5(
        &mut self,
        initializer: NodeIndex,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return false;
        }

        let Some(decl_list) = self.arena.get_variable(init_node) else {
            return false;
        };

        let mut pattern_decls = Vec::new();
        for &decl_idx in &decl_list.declarations.nodes {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                return false;
            };
            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                return false;
            };
            if !self.is_binding_pattern(decl.name) || decl.initializer.is_some() {
                return false;
            }
            pattern_decls.push(decl.name);
        }

        if pattern_decls.is_empty() {
            return false;
        }

        self.write("var ");
        let mut first = true;
        for pattern_idx in pattern_decls {
            let Some(pattern_node) = self.arena.get(pattern_idx) else {
                continue;
            };
            let temp_name = self.get_temp_var_name();
            if !first {
                self.write(", ");
            }
            first = false;
            self.write(&temp_name);
            self.write(" = void 0");
            self.emit_es5_destructuring_pattern(pattern_node, &temp_name);
        }
        true
    }

    pub(in crate::emitter) fn emit_for_of_statement(&mut self, node: &Node) {
        let Some(for_in_of) = self.arena.get_for_in_of(node) else {
            return;
        };
        let initializer_exports = self
            .deferred_exported_var_iteration_bindings(for_in_of.initializer, for_in_of.statement);

        // Check if the for-of initializer has `using` that needs lowering.
        if !self.ctx.target_es5
            && !self.ctx.options.target.supports_es2025()
            && let Some(using_info) = self.for_of_initializer_using_info(for_in_of.initializer)
        {
            self.emit_for_of_with_using_lowering(node, for_in_of, using_info);
            return;
        }

        // Check if the for-of initializer has object rest that needs ES2018 lowering.
        if self.ctx.needs_es2018_lowering
            && !self.ctx.target_es5
            && let Some(pattern_idx) =
                self.for_of_assignment_initializer_has_object_rest(for_in_of.initializer)
        {
            self.emit_for_of_assignment_with_rest_lowering(node, for_in_of, pattern_idx);
            return;
        }

        if self.ctx.needs_es2018_lowering
            && !self.ctx.target_es5
            && let Some(rest_info) = self.for_of_has_object_rest(for_in_of.initializer)
        {
            self.emit_for_of_with_rest_lowering(node, for_in_of, rest_info);
            return;
        }

        self.write("for ");
        if for_in_of.await_modifier {
            self.write("await ");
        }
        self.write("(");
        if self.in_system_execute_body {
            self.emit_for_initializer_strip_var(for_in_of.initializer);
        } else if self.emit_async_generator_shadow_for_in_of_initializer(for_in_of.initializer) {
            // Rewritten from `var x of y` to `x of y`; the declaration is hoisted
            // at the top of the generated async generator body.
        } else {
            self.emit(for_in_of.initializer);
        }
        self.write(" of ");
        self.emit(for_in_of.expression);
        // Map closing `)` — scan backward from body start
        if let Some(body_node) = self.arena.get(for_in_of.statement) {
            self.map_closing_paren_backward(node.pos, body_node.pos);
        }
        self.write(")");
        if initializer_exports.is_empty() {
            self.emit_loop_body(for_in_of.statement);
        } else {
            self.emit_loop_body_with_deferred_exports(for_in_of.statement, &initializer_exports);
        }
    }

    /// Check if a for-of initializer has an object rest pattern.
    fn for_of_has_object_rest(&self, initializer: NodeIndex) -> Option<(String, NodeIndex)> {
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return None;
        }
        let decl_list = self.arena.get_variable(init_node)?;
        if decl_list.declarations.nodes.len() != 1 {
            return None;
        }
        let decl_idx = decl_list.declarations.nodes[0];
        let decl_node = self.arena.get(decl_idx)?;
        let decl = self.arena.get_variable_declaration(decl_node)?;
        if !self.pattern_has_object_rest(decl.name) {
            return None;
        }
        let flags = init_node.flags as u32;
        let keyword = if flags & node_flags::LET != 0 {
            "let"
        } else if flags & node_flags::CONST != 0 {
            "const"
        } else {
            "var"
        };
        Some((keyword.to_string(), decl.name))
    }

    fn for_of_assignment_initializer_has_object_rest(
        &self,
        initializer: NodeIndex,
    ) -> Option<NodeIndex> {
        let init_node = self.arena.get(initializer)?;
        if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return None;
        }
        self.assignment_pattern_has_object_rest(initializer)
            .then_some(initializer)
    }

    /// Emit a for-of with object rest lowering.
    fn emit_for_of_with_rest_lowering(
        &mut self,
        node: &Node,
        for_in_of: &tsz_parser::parser::node::ForInOfData,
        rest_info: (String, NodeIndex),
    ) {
        let (keyword, pattern_idx) = rest_info;
        let temp = self.get_temp_var_name();

        self.write("for ");
        if for_in_of.await_modifier {
            self.write("await ");
        }
        self.write("(");
        self.write(&keyword);
        self.write(" ");
        self.write(&temp);
        self.write(" of ");
        self.emit(for_in_of.expression);
        if let Some(body_node) = self.arena.get(for_in_of.statement) {
            self.map_closing_paren_backward(node.pos, body_node.pos);
        }
        self.write(") {");
        self.write_line();
        self.increase_indent();

        // Emit the destructuring preamble
        self.write(&keyword);
        self.write(" ");
        self.emit_object_rest_var_decl(pattern_idx, NodeIndex::NONE, Some(&temp));
        self.write(";");
        self.write_line();

        // Emit the original body statements
        if let Some(body_node) = self.arena.get(for_in_of.statement) {
            if body_node.kind == syntax_kind_ext::BLOCK {
                if let Some(block) = self.arena.get_block(body_node) {
                    for &stmt in &block.statements.nodes {
                        self.emit(stmt);
                        self.write_line();
                    }
                }
            } else {
                self.emit(for_in_of.statement);
                self.write_line();
            }
        }

        self.decrease_indent();
        self.write("}");
    }

    fn emit_for_of_assignment_with_rest_lowering(
        &mut self,
        node: &Node,
        for_in_of: &tsz_parser::parser::node::ForInOfData,
        pattern_idx: NodeIndex,
    ) {
        let reserve_count = self.native_for_of_object_rest_assignment_temp_count(pattern_idx);
        if reserve_count > 0 {
            self.preallocate_assignment_temps(reserve_count);
        }
        let temp = self.get_temp_var_name();

        self.write("for ");
        if for_in_of.await_modifier {
            self.write("await ");
        }
        self.write("(let ");
        self.write(&temp);
        self.write(" of ");
        self.emit(for_in_of.expression);
        if let Some(body_node) = self.arena.get(for_in_of.statement) {
            self.map_closing_paren_backward(node.pos, body_node.pos);
        }
        self.write(") {");
        self.write_line();
        self.increase_indent();

        let needs_assignment_parens = self
            .arena
            .get(pattern_idx)
            .is_none_or(|pattern| pattern.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION);
        if needs_assignment_parens {
            self.write("(");
        }
        self.emit_assignment_object_rest_destructuring_from_source(pattern_idx, &temp);
        if needs_assignment_parens {
            self.write(")");
        }
        self.write_semicolon();
        self.write_line();

        if let Some(body_node) = self.arena.get(for_in_of.statement) {
            if body_node.kind == syntax_kind_ext::BLOCK {
                if let Some(block) = self.arena.get_block(body_node) {
                    for &stmt in &block.statements.nodes {
                        self.emit(stmt);
                        self.write_line();
                    }
                }
            } else {
                self.emit(for_in_of.statement);
                self.write_line();
            }
        }

        self.decrease_indent();
        self.write("}");
    }

    fn native_for_of_object_rest_assignment_temp_count(&self, pattern_idx: NodeIndex) -> usize {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return 0;
        };
        if pattern_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return 0;
        }
        let Some(lit) = self.arena.get_literal_expr(pattern_node) else {
            return 0;
        };
        lit.elements
            .nodes
            .iter()
            .filter(|&&elem_idx| self.assignment_pattern_has_object_rest(elem_idx))
            .count()
    }

    /// Emit a loop body statement. If the body is a block, emit it inline.
    /// If it's a single statement, put it on a new indented line (matching tsc behavior).
    /// Emit a for/for-in/for-of initializer, stripping `var` if the
    /// initializer is a `var` declaration list (because the var is hoisted
    /// in System modules). `let`/`const` are emitted normally.
    fn emit_for_initializer_strip_var(&mut self, initializer: NodeIndex) {
        let Some(init_node) = self.arena.get(initializer) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            self.emit(initializer);
            return;
        }
        // Only strip `var`, not `let`/`const`
        let is_var = !node_flags::is_let_or_const(init_node.flags as u32);
        if !is_var {
            self.emit(initializer);
            return;
        }
        // Emit just the variable names (without `var` keyword)
        let Some(decl_list) = self.arena.get_variable(init_node) else {
            self.emit(initializer);
            return;
        };
        for (i, &decl_idx) in decl_list.declarations.nodes.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            self.emit(decl.name);
        }
    }

    fn deferred_exported_var_initializers(
        &self,
        initializer: NodeIndex,
    ) -> Vec<(String, String, NodeIndex)> {
        if self.function_scope_depth != 0 || self.in_namespace_iife {
            return Vec::new();
        }
        let Some(bindings) = self.deferred_local_export_bindings.as_ref() else {
            return Vec::new();
        };
        let Some(init_node) = self.arena.get(initializer) else {
            return Vec::new();
        };
        if init_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST
            || node_flags::is_let_or_const(init_node.flags as u32)
        {
            return Vec::new();
        }
        let Some(decl_list) = self.arena.get_variable(init_node) else {
            return Vec::new();
        };
        let mut exports = Vec::new();
        for &decl_idx in &decl_list.declarations.nodes {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                return Vec::new();
            };
            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                return Vec::new();
            };
            if decl.initializer.is_none() {
                return Vec::new();
            }
            let init_idx = decl.initializer;
            let Some(name_node) = self.arena.get(decl.name) else {
                return Vec::new();
            };
            let Some(ident) = self.arena.get_identifier(name_node) else {
                return Vec::new();
            };
            let local_name = ident.escaped_text.clone();
            let Some(export_name) = bindings.get(&local_name).cloned() else {
                return Vec::new();
            };
            exports.push((local_name, export_name, init_idx));
        }
        exports
    }

    fn deferred_exported_var_iteration_bindings(
        &self,
        initializer: NodeIndex,
        body: NodeIndex,
    ) -> Vec<(String, String)> {
        if self.function_scope_depth != 0 || self.in_namespace_iife {
            return Vec::new();
        }
        let Some(body_node) = self.arena.get(body) else {
            return Vec::new();
        };
        if body_node.kind == syntax_kind_ext::BLOCK {
            return Vec::new();
        }
        let Some(bindings) = self.deferred_local_export_bindings.as_ref() else {
            return Vec::new();
        };
        let Some(init_node) = self.arena.get(initializer) else {
            return Vec::new();
        };
        if init_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST
            || node_flags::is_let_or_const(init_node.flags as u32)
        {
            return Vec::new();
        }
        let Some(decl_list) = self.arena.get_variable(init_node) else {
            return Vec::new();
        };
        let mut exports = Vec::new();
        for &decl_idx in &decl_list.declarations.nodes {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                return Vec::new();
            };
            let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                return Vec::new();
            };
            if decl.initializer.is_some() {
                return Vec::new();
            }
            let Some(name_node) = self.arena.get(decl.name) else {
                return Vec::new();
            };
            let Some(ident) = self.arena.get_identifier(name_node) else {
                return Vec::new();
            };
            let local_name = ident.escaped_text.clone();
            let Some(export_name) = bindings.get(&local_name).cloned() else {
                return Vec::new();
            };
            exports.push((local_name, export_name));
        }
        exports
    }

    fn emit_loop_body_with_deferred_exports(
        &mut self,
        body: NodeIndex,
        exports: &[(String, String)],
    ) {
        self.write(" {");
        self.write_line();
        self.increase_indent();
        for (local_name, export_name) in exports {
            self.write_export_binding_start(export_name);
            self.write(local_name);
            self.write_export_binding_end();
            if self.in_system_execute_body {
                self.system_folded_export_names.insert(local_name.clone());
            }
            self.write_line();
        }
        self.emit(body);
        self.write_line();
        self.decrease_indent();
        self.write("}");
    }

    fn emit_loop_body(&mut self, body: NodeIndex) {
        let is_block = self
            .arena
            .get(body)
            .is_some_and(|n| n.kind == syntax_kind_ext::BLOCK);
        if is_block {
            self.write(" ");
            self.emit(body);
        } else {
            self.write_line();
            self.increase_indent();
            let before = self.writer.len();
            self.emit(body);
            // If the body was completely erased (e.g. const enum, interface),
            // emit `;` to produce a valid empty statement.
            if self.writer.len() == before {
                self.write(";");
            }
            self.decrease_indent();
        }
    }

    pub(in crate::emitter) fn emit_return_statement(&mut self, node: &Node) {
        let Some(ret) = self.arena.get_return_statement(node) else {
            self.write("return");
            self.map_trailing_semicolon(node);
            self.write_semicolon();
            self.emit_trailing_comment_after_semicolon(node);
            return;
        };

        self.write("return");
        if ret.expression.is_some() {
            self.write(" ");
            if !self.try_emit_object_literal_es5_return_expression(ret.expression) {
                self.emit_expression(ret.expression);
            }
        }
        self.map_trailing_semicolon(node);
        self.write_semicolon();
        self.emit_trailing_comment_after_semicolon(node);
    }

    // =========================================================================
    // Additional Statements
    // =========================================================================

    pub(in crate::emitter) fn emit_throw_statement(&mut self, node: &Node) {
        // ThrowStatement uses ReturnData (same structure)
        let Some(throw_data) = self.arena.get_return_statement(node) else {
            self.write("throw");
            self.map_trailing_semicolon(node);
            self.write_semicolon();
            self.emit_trailing_comment_after_semicolon(node);
            return;
        };

        self.write("throw ");
        self.emit(throw_data.expression);
        self.map_trailing_semicolon(node);
        self.write_semicolon();
        self.emit_trailing_comment_after_semicolon(node);
    }

    pub(in crate::emitter) fn emit_switch_statement(&mut self, node: &Node) {
        let Some(switch) = self.arena.get_switch(node) else {
            return;
        };

        self.write("switch (");
        self.emit(switch.expression);
        // Map closing `)` — scan forward from expression end
        if let Some(expr_node) = self.arena.get(switch.expression) {
            self.map_token_after(expr_node.end, node.end, b')');
        }
        self.write(") ");
        // case_block is a NodeIndex pointing to a CaseBlock node
        self.emit(switch.case_block);
        if let Some(recovered_class) =
            self.recovered_class_after_unterminated_empty_switch(node, switch.case_block)
        {
            self.write_line();
            self.write("class ");
            self.write(&recovered_class.header);
            self.write(" {");
            self.write_line();
            if let Some(body) = recovered_class.inline_body {
                self.increase_indent();
                self.write(&body);
                if !body.ends_with(';') {
                    self.write(";");
                }
                self.decrease_indent();
                self.write_line();
            }
            self.write("}");
        }
    }

    fn recovered_class_after_unterminated_empty_switch(
        &self,
        node: &Node,
        case_block_idx: NodeIndex,
    ) -> Option<RecoveredSwitchClass> {
        let case_block_node = self.arena.get(case_block_idx)?;
        let case_block = self.arena.blocks.get(case_block_node.data_index as usize)?;
        if !case_block.statements.nodes.is_empty() {
            return None;
        }

        let text = self.source_text?;
        let start = std::cmp::min(node.pos as usize, text.len());
        let end = std::cmp::min(node.end as usize, text.len());
        let source = text.get(start..end)?;
        for line in source.lines() {
            let line = line.trim_start();
            let Some(rest) = line.strip_prefix("class ") else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|&ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
                .collect();
            if name.is_empty() {
                continue;
            }

            if rest[name.len()..].trim_start().starts_with('{') {
                return Some(RecoveredSwitchClass {
                    header: name,
                    inline_body: None,
                });
            }

            let Some(open_brace) = rest.find('{') else {
                continue;
            };
            let Some(close_brace) = rest[open_brace + 1..].rfind('}') else {
                continue;
            };
            let header = rest[..open_brace].trim_end().to_string();
            let inline_body = rest[open_brace + 1..open_brace + 1 + close_brace]
                .trim()
                .to_string();
            if !header.is_empty() && !inline_body.is_empty() {
                return Some(RecoveredSwitchClass {
                    header,
                    inline_body: Some(inline_body),
                });
            }
        }
        None
    }

    pub(in crate::emitter) fn emit_case_block(&mut self, node: &Node) {
        if !node.has_data() || node.kind != syntax_kind_ext::CASE_BLOCK {
            return;
        }
        let Some(case_block) = self.arena.blocks.get(node.data_index as usize) else {
            return;
        };

        self.map_opening_brace(node);
        self.write_with_end_marker("{");
        self.write_line();
        self.increase_indent();

        for &clause_idx in &case_block.statements.nodes {
            // Emit leading comments before each case/default clause.
            // Without this, comments between clauses get attached to the
            // first statement INSIDE the clause body instead of appearing
            // before the case/default label.
            if let Some(clause_node) = self.arena.get(clause_idx) {
                let actual_start = self.skip_trivia_forward(clause_node.pos, clause_node.end);
                self.emit_comments_before_pos(actual_start);
            }
            self.emit(clause_idx);
        }

        self.decrease_indent();
        self.map_closing_brace(node);
        self.write_with_end_marker("}");
    }

    pub(in crate::emitter) fn emit_case_clause(&mut self, node: &Node) {
        let Some(clause) = self.arena.get_case_clause(node) else {
            return;
        };

        self.write("case ");
        self.emit(clause.expression);
        // Map the `:` after the case expression
        let label_end = self.arena.get(clause.expression).map_or(0, |n| n.end);
        self.map_token_after(label_end, node.end, b':');
        self.write(":");

        // Emit trailing comments on the same source line as `case X:`
        // e.g., `case 0: // Zero` should keep the comment on the same line.
        // Cap the scan at the first statement's pos to avoid consuming
        // comments that trail statements in the case body.
        let colon_end = self
            .find_char_after(label_end, node.end, b':')
            .map_or(label_end, |p| p + 1);
        let first_stmt_pos = clause
            .statements
            .nodes
            .first()
            .and_then(|&idx| self.arena.get(idx))
            .map_or(node.end, |n| n.pos);
        self.emit_trailing_comments_before(colon_end, first_stmt_pos);

        // Use expression end position for same-line detection
        self.emit_case_clause_body(&clause.statements, label_end);
    }

    pub(in crate::emitter) fn emit_default_clause(&mut self, node: &Node) {
        let Some(clause) = self.arena.get_case_clause(node) else {
            return;
        };

        self.write("default:");

        // Emit trailing comments on the same source line as `default:`
        // Cap the scan at the first statement's pos to avoid consuming
        // comments that trail statements in the clause body.
        let colon_end = self
            .find_char_after(node.pos, node.end, b':')
            .map_or(node.pos + 8, |p| p + 1);
        let first_stmt_pos = clause
            .statements
            .nodes
            .first()
            .and_then(|&idx| self.arena.get(idx))
            .map_or(node.end, |n| n.pos);
        self.emit_trailing_comments_before(colon_end, first_stmt_pos);

        // Use node pos + "default" length for same-line detection
        self.emit_case_clause_body(&clause.statements, node.pos + 8);
    }

    fn emit_case_clause_body(&mut self, statements: &NodeList, label_end: u32) {
        // If single statement on the same line as the case/default label in source,
        // emit it on the same line (e.g., `case 0: { ... }` or `case true: return x;`)
        if statements.nodes.len() == 1
            && let Some(stmt_node) = self.arena.get(statements.nodes[0])
            && self.is_on_same_source_line(label_end, stmt_node.pos)
        {
            self.write(" ");
            self.emit(statements.nodes[0]);
            if !self.writer.is_at_line_start() {
                self.write_line();
            }
            return;
        }

        self.write_line();
        self.increase_indent();

        let stmts = &statements.nodes;
        for (stmt_i, &stmt) in stmts.iter().enumerate() {
            // Emit leading comments before this statement.
            // Use skip_trivia_forward to get the actual token start (past comments),
            // so that emit_comments_before_pos can pick up comments in the leading trivia.
            if let Some(stmt_node) = self.arena.get(stmt) {
                let actual_start = self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
                self.emit_comments_before_pos(actual_start);
            }
            self.emit(stmt);
            if !self.writer.is_at_line_start() {
                // Emit trailing same-line comments (e.g., `var x = z[1]; // comment`).
                let (stmt_pos, upper_bound) = {
                    let cur = self.arena.get(stmt);
                    let next_pos = stmts
                        .get(stmt_i + 1)
                        .and_then(|&next_idx| self.arena.get(next_idx))
                        .map(|n| n.pos);
                    if let Some(sn) = cur {
                        (sn.pos, next_pos.unwrap_or(sn.end))
                    } else {
                        (0, 0)
                    }
                };
                if upper_bound > 0 {
                    let token_end = self.find_token_end_before_trivia(stmt_pos, upper_bound);
                    self.emit_trailing_comments_before(token_end, upper_bound);
                }
                self.write_line();
            }
        }

        self.decrease_indent();
    }

    /// Check if two source positions are on the same line
    fn is_on_same_source_line(&self, pos1: u32, pos2: u32) -> bool {
        if let Some(text) = self.source_text {
            let start = std::cmp::min(pos1 as usize, text.len());
            let end = std::cmp::min(pos2 as usize, text.len());
            if start > end {
                return false;
            }
            !text[start..end].contains('\n')
        } else {
            false
        }
    }

    pub(in crate::emitter) fn emit_break_statement(&mut self, node: &Node) {
        self.write("break");
        if let Some(jump) = self.arena.get_jump_data(node)
            && jump.label.is_some()
        {
            if self.is_static_block_await_identifier(jump.label) {
                self.write(" ;");
                self.write_line();
                self.emit(jump.label);
                self.write(" ;");
                self.emit_trailing_comment_after_semicolon(node);
                return;
            }
            self.write(" ");
            // Emit inline comments between keyword and label (e.g., `break /*c*/ label`)
            if let Some(label_node) = self.arena.get(jump.label) {
                self.emit_comments_before_pos(label_node.pos);
            }
            self.emit(jump.label);
            // Emit inline comments between label and semicolon (e.g., `break foo /*c*/;`)
            if let Some(label_node) = self.arena.get(jump.label) {
                // Limit the scan range to the semicolon position (not node.end,
                // which may extend past the `;` into the next statement's trivia
                // due to how parse_break_statement sets end_pos).
                let range_end = self
                    .find_semicolon_pos_in_range(label_node.end, node.end)
                    .unwrap_or(label_node.end);
                self.emit_comments_in_range(label_node.end, range_end, true, false);
            }
        }
        self.map_trailing_semicolon(node);
        self.write_semicolon();
        self.emit_trailing_comment_after_semicolon(node);
    }

    pub(in crate::emitter) fn emit_continue_statement(&mut self, node: &Node) {
        self.write("continue");
        if let Some(jump) = self.arena.get_jump_data(node)
            && jump.label.is_some()
        {
            if self.is_static_block_await_identifier(jump.label) {
                self.write(" ;");
                self.write_line();
                self.emit(jump.label);
                self.write(" ;");
                self.emit_trailing_comment_after_semicolon(node);
                return;
            }
            self.write(" ");
            // Emit inline comments between keyword and label (e.g., `continue /*c*/ label`)
            if let Some(label_node) = self.arena.get(jump.label) {
                self.emit_comments_before_pos(label_node.pos);
            }
            self.emit(jump.label);
            // Emit inline comments between label and semicolon (e.g., `continue foo /*c*/;`)
            if let Some(label_node) = self.arena.get(jump.label) {
                // Limit the scan range to the semicolon position (not node.end,
                // which may extend past the `;` into the next statement's trivia
                // due to how parse_continue_statement sets end_pos).
                let range_end = self
                    .find_semicolon_pos_in_range(label_node.end, node.end)
                    .unwrap_or(label_node.end);
                self.emit_comments_in_range(label_node.end, range_end, true, false);
            }
        }
        self.map_trailing_semicolon(node);
        self.write_semicolon();
        self.emit_trailing_comment_after_semicolon(node);
    }

    /// Find the position of the first `;` in the source text between `start` and `end`.
    /// Returns the position right after the `;` (exclusive end) or `None` if no `;` found.
    pub(in crate::emitter) fn find_semicolon_pos_in_range(
        &self,
        start: u32,
        end: u32,
    ) -> Option<u32> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let s = start as usize;
        let e = (end as usize).min(bytes.len());
        for (i, &byte) in bytes[s..e].iter().enumerate() {
            if byte == b';' {
                return Some((s + i) as u32);
            }
        }
        None
    }

    pub(in crate::emitter) fn emit_labeled_statement(&mut self, node: &Node) {
        let Some(labeled) = self.arena.get_labeled_statement(node) else {
            return;
        };

        if self.ctx.emit_await_as_yield && self.get_identifier_text_idx(labeled.label) == "await" {
            self.write("yield ;");
            self.write_line();
            self.emit(labeled.statement);
            return;
        }

        if self.is_static_block_await_identifier(labeled.label) {
            self.emit(labeled.label);
            self.write(" ;");
            self.write_line();
            if let (Some(label_node), Some(stmt_node)) = (
                self.arena.get(labeled.label),
                self.arena.get(labeled.statement),
            ) {
                self.skip_trailing_same_line_comments(label_node.end, stmt_node.pos);
            }
            if self.emit_static_block_await_labeled_jump_recovery(labeled.statement) {
                return;
            }
            self.emit(labeled.statement);
            return;
        }

        self.emit(labeled.label);
        self.write(": ");
        if (self.ctx.is_commonjs() || self.in_system_execute_body)
            && self.labeled_body_is_initializerless_export_variable(labeled.statement)
        {
            self.write(";");
            return;
        }
        if self.labeled_body_needs_block(labeled.statement) {
            self.write("{");
            self.write_line();
            self.increase_indent();
            self.emit(labeled.statement);
            if !self.writer.is_at_line_start() {
                self.write_line();
            }
            self.decrease_indent();
            self.write("}");
            return;
        }
        let before = self.writer.len();
        self.emit(labeled.statement);
        // If the labeled body was completely erased (e.g. const enum, interface),
        // emit `;` to produce a valid empty statement.
        if self.writer.len() == before {
            self.write(";");
        }
    }

    fn labeled_body_needs_block(&self, stmt_idx: NodeIndex) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        if stmt_node.kind != syntax_kind_ext::ENUM_DECLARATION {
            return false;
        }
        let Some(enum_decl) = self.arena.get_enum(stmt_node) else {
            return false;
        };
        if self.arena.is_declare(&enum_decl.modifiers) {
            return false;
        }
        !self
            .arena
            .has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword)
            || self.ctx.options.preserve_const_enums
    }

    fn labeled_body_is_initializerless_export_variable(&self, stmt_idx: NodeIndex) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        let variable_node = if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
            let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                return false;
            };
            if !self
                .arena
                .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
            {
                return false;
            }
            stmt_node
        } else if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            let Some(export_decl) = self.arena.get_export_decl(stmt_node) else {
                return false;
            };
            if export_decl.module_specifier.is_some() {
                return false;
            }
            let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
                return false;
            };
            if clause_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                return false;
            }
            clause_node
        } else {
            return false;
        };

        self.arena
            .get_variable(variable_node)
            .is_some_and(|var_stmt| self.all_declarations_lack_initializer(&var_stmt.declarations))
    }

    pub(in crate::emitter) fn emit_do_statement(&mut self, node: &Node) {
        let Some(loop_stmt) = self.arena.get_loop(node) else {
            return;
        };

        // ES5: Check if closures capture body-scoped let/const variables
        if self.ctx.target_es5 {
            let body_info = super::super::es5::loop_capture::collect_loop_body_vars(
                self.arena,
                loop_stmt.statement,
            );
            if !body_info.block_scoped_vars.is_empty()
                && let Some(capture_info) =
                    super::super::es5::loop_capture::check_loop_needs_capture(
                        self.arena,
                        loop_stmt.statement,
                        &[],
                        &body_info.block_scoped_vars,
                    )
            {
                self.emit_do_statement_with_capture(node, loop_stmt, &capture_info, &body_info);
                return;
            }
        }

        self.write("do");
        let body_is_block = self
            .arena
            .get(loop_stmt.statement)
            .is_some_and(|n| n.kind == syntax_kind_ext::BLOCK);
        if body_is_block {
            self.write(" ");
            self.emit(loop_stmt.statement);
            self.write(" ");
        } else {
            self.write_line();
            self.increase_indent();
            let before = self.writer.len();
            self.emit(loop_stmt.statement);
            // If the body was completely erased (e.g. const enum, interface),
            // emit `;` to produce a valid empty statement.
            if self.writer.len() == before {
                self.write(";");
            }
            self.decrease_indent();
            self.write_line();
        }
        self.write("while (");
        self.emit(loop_stmt.condition);
        // Map closing `)` — scan backward from node end (past `;`)
        self.map_closing_paren_backward(node.pos, node.end);
        self.write(")");
        self.map_trailing_semicolon(node);
        self.write_semicolon();
        self.emit_trailing_comment_after_semicolon(node);
    }

    pub(in crate::emitter) fn emit_debugger_statement(&mut self, node: &Node) {
        self.write("debugger");
        self.map_trailing_semicolon(node);
        self.write_semicolon();
        self.emit_trailing_comment_after_semicolon(node);
    }

    pub(in crate::emitter) fn emit_with_statement(&mut self, node: &Node) {
        let Some(with_stmt) = self.arena.get_with_statement(node) else {
            return;
        };

        self.write("with (");
        self.emit(with_stmt.expression);
        // Map closing `)` — scan backward from body start
        if let Some(body_node) = self.arena.get(with_stmt.then_statement) {
            self.map_closing_paren_backward(node.pos, body_node.pos);
        }
        self.write(")");
        let body_is_block = self
            .arena
            .get(with_stmt.then_statement)
            .is_some_and(|n| n.kind == syntax_kind_ext::BLOCK);
        if body_is_block {
            self.write(" ");
            self.emit(with_stmt.then_statement);
        } else {
            self.write_line();
            self.increase_indent();
            let before = self.writer.len();
            self.emit(with_stmt.then_statement);
            // If the body was completely erased (e.g. const enum, interface),
            // emit `;` to produce a valid empty statement.
            if self.writer.len() == before {
                self.write(";");
            }
            self.decrease_indent();
        }
    }

    /// Check if a for-statement initializer is a `using` declaration list.
    fn for_initializer_has_using(&self, initializer: NodeIndex) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return false;
        }
        (init_node.flags as u32 & node_flags::USING) != 0
    }

    /// Get info about a for-of initializer that has `using`: returns the variable
    /// name and whether it's `await using`.
    /// For `for (using d of items)`, the initializer is a `VariableDeclarationList`
    /// with one declaration `d` (no initializer in for-of context).
    pub(in crate::emitter) fn for_of_initializer_using_info(
        &self,
        initializer: NodeIndex,
    ) -> Option<(String, bool)> {
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return None;
        }
        let flags = init_node.flags as u32;
        if (flags & node_flags::USING) == 0 {
            return None;
        }
        let using_async = node_flags::is_await_using(flags);
        let decl_list = self.arena.get_variable(init_node)?;
        if decl_list.declarations.nodes.len() != 1 {
            return None;
        }
        let decl_idx = decl_list.declarations.nodes[0];
        let decl_node = self.arena.get(decl_idx)?;
        let decl = self.arena.get_variable_declaration(decl_node)?;
        let name_node = self.arena.get(decl.name)?;
        let ident = self.arena.get_identifier(name_node)?;
        Some((ident.escaped_text.clone(), using_async))
    }

    /// Emit `for (using d of items) { body }` with dispose lowering.
    /// Transforms to:
    /// ```js
    /// for (const d_1 of items) {
    ///     const env_1 = { stack: [], error: void 0, hasError: false };
    ///     try {
    ///         const d = __addDisposableResource(env_1, d_1, false);
    ///         // ... body statements
    ///     }
    ///     catch (e_1) { env_1.error = e_1; env_1.hasError = true; }
    ///     finally { __disposeResources(env_1); }
    /// }
    /// ```
    fn emit_for_of_with_using_lowering(
        &mut self,
        node: &Node,
        for_in_of: &tsz_parser::parser::node::ForInOfData,
        using_info: (String, bool),
    ) {
        let (var_name, using_async) = using_info;
        let (env_name, error_name, result_name) = self.next_disposable_env_names();
        // Generate a temp name based on original: d1 -> d1_1 (uses the env counter)
        let temp_name = format!("{}_{}", var_name, self.next_disposable_env_id - 1);
        self.generated_temp_names.insert(temp_name.clone());

        self.write("for ");
        if for_in_of.await_modifier {
            self.write("await ");
        }
        self.write("(const ");
        self.write(&temp_name);
        self.write(" of ");
        self.emit(for_in_of.expression);
        // Map closing `)` — scan backward from body start
        if let Some(body_node) = self.arena.get(for_in_of.statement) {
            self.map_closing_paren_backward(node.pos, body_node.pos);
        }
        self.write(") {");
        self.write_line();
        self.increase_indent();

        // Emit: const env_1 = { stack: [], error: void 0, hasError: false };
        self.write("const ");
        self.write(&env_name);
        self.write(" = { stack: [], error: void 0, hasError: false };");
        self.write_line();

        // Emit: try {
        self.write("try {");
        self.write_line();
        self.increase_indent();

        // Emit: const d = __addDisposableResource(env_1, d_1, false);
        self.write("const ");
        self.write(&var_name);
        self.write(" = ");
        self.write_helper("__addDisposableResource");
        self.write("(");
        self.write(&env_name);
        self.write(", ");
        self.write(&temp_name);
        self.write(", ");
        self.write(if using_async { "true" } else { "false" });
        self.write(");");
        self.write_line();

        // Emit the original loop body statements (unwrap the block)
        if let Some(body_node) = self.arena.get(for_in_of.statement) {
            if body_node.kind == syntax_kind_ext::BLOCK {
                if let Some(block) = self.arena.get_block(body_node) {
                    for &stmt in &block.statements.nodes {
                        self.emit(stmt);
                        if !self.writer.is_at_line_start() {
                            self.write_line();
                        }
                    }
                }
            } else {
                self.emit(for_in_of.statement);
                if !self.writer.is_at_line_start() {
                    self.write_line();
                }
            }
        }

        // Close try
        self.decrease_indent();
        self.write("}");
        self.write_line();

        // Emit catch
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

        // Emit finally
        self.write("finally {");
        self.write_line();
        self.increase_indent();
        if using_async {
            let await_kw = if self.ctx.emit_await_as_yield {
                "yield"
            } else {
                "await"
            };
            self.write("const ");
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
            self.write(await_kw);
            self.write(" ");
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

        // Close outer for loop body
        self.decrease_indent();
        self.write("}");
    }

    /// Emit `for (using d1 = expr, d2 = expr2;;) { body }` with dispose lowering.
    /// Transforms to:
    /// ```js
    /// {
    ///     const env_1 = { stack: [], error: void 0, hasError: false };
    ///     try {
    ///         const d1 = __addDisposableResource(env_1, expr, false), d2 = ...;
    ///         for (;;) { body }
    ///     }
    ///     catch (e_1) { env_1.error = e_1; env_1.hasError = true; }
    ///     finally { __disposeResources(env_1); }
    /// }
    /// ```
    fn emit_for_with_using_lowering(
        &mut self,
        node: &Node,
        loop_stmt: &tsz_parser::parser::node::LoopData,
    ) {
        let init_node = self.arena.get(loop_stmt.initializer).unwrap();
        let flags = init_node.flags as u32;
        let using_async = node_flags::is_await_using(flags);
        let decl_list = self.arena.get_variable(init_node).unwrap();
        let (env_name, error_name, result_name) = self.next_disposable_env_names();

        // Emit wrapping block: {
        self.write("{");
        self.write_line();
        self.increase_indent();

        // Emit: const env_1 = { stack: [], error: void 0, hasError: false };
        self.write("const ");
        self.write(&env_name);
        self.write(" = { stack: [], error: void 0, hasError: false };");
        self.write_line();

        // Emit: try {
        self.write("try {");
        self.write_line();
        self.increase_indent();

        // Emit: const d1 = __addDisposableResource(env_1, expr, false), d2 = ...;
        let initialized_decls: Vec<_> = decl_list
            .declarations
            .nodes
            .iter()
            .copied()
            .filter(|&decl_idx| {
                self.arena
                    .get(decl_idx)
                    .and_then(|n| self.arena.get_variable_declaration(n))
                    .is_some_and(|d| d.initializer.is_some())
            })
            .collect();

        if !initialized_decls.is_empty() {
            self.write("const ");
            for (i, &decl_idx) in initialized_decls.iter().enumerate() {
                if let Some(decl_node) = self.arena.get(decl_idx)
                    && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                {
                    self.emit(decl.name);
                    self.write(" = ");
                    self.write_helper("__addDisposableResource");
                    self.write("(");
                    self.write(&env_name);
                    self.write(", ");
                    self.emit(decl.initializer);
                    self.write(", ");
                    self.write(if using_async { "true" } else { "false" });
                    self.write(")");
                    if i + 1 < initialized_decls.len() {
                        self.write(", ");
                    }
                }
            }
            self.write(";");
            self.write_line();
        }

        // Emit the for loop with no initializer: for (;;) { body }
        self.write("for (");
        // No initializer
        // Emit condition and incrementor (both should be None for `using` in for-init)
        self.write(";");
        if loop_stmt.condition.is_some() {
            self.write(" ");
            self.emit(loop_stmt.condition);
        }
        self.write(";");
        if loop_stmt.incrementor.is_some() {
            self.write(" ");
            self.emit(loop_stmt.incrementor);
        }
        // Map closing `)` — scan backward from body start
        if let Some(body_node) = self.arena.get(loop_stmt.statement) {
            self.map_closing_paren_backward(node.pos, body_node.pos);
        }
        self.write(")");
        self.emit_loop_body(loop_stmt.statement);
        self.write_line();

        // Close try
        self.decrease_indent();
        self.write("}");
        self.write_line();

        // Emit catch
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

        // Emit finally
        self.write("finally {");
        self.write_line();
        self.increase_indent();
        if using_async {
            let await_kw = if self.ctx.emit_await_as_yield {
                "yield"
            } else {
                "await"
            };
            self.write("const ");
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
            self.write(await_kw);
            self.write(" ");
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

        // Close wrapping block
        self.decrease_indent();
        self.write("}");
    }

    /// Check if a statement list contains any `using`/`await using` declarations.
    pub(in crate::emitter) fn block_has_using_declarations(&self, statements: &NodeList) -> bool {
        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                    continue;
                };
                for &decl_list_idx in &var_stmt.declarations.nodes {
                    if let Some(decl_list_node) = self.arena.get(decl_list_idx)
                        && (decl_list_node.flags as u32 & node_flags::USING) != 0
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if a statement list contains any `await using` declarations.
    pub(in crate::emitter) fn block_has_await_using(&self, statements: &NodeList) -> bool {
        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                    continue;
                };
                for &decl_list_idx in &var_stmt.declarations.nodes {
                    if let Some(decl_list_node) = self.arena.get(decl_list_idx)
                        && (decl_list_node.flags as u32 & node_flags::AWAIT_USING)
                            == node_flags::AWAIT_USING
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Emit just the `__addDisposableResource` calls for a using declaration,
    /// without the try/catch/finally wrapper (used when block-level wrapping is active).
    pub(crate) fn emit_using_addresource_only(
        &mut self,
        decl_list: &tsz_parser::parser::node::VariableData,
        env_name: &str,
        using_async: bool,
    ) {
        let initialized_decls: Vec<_> = decl_list
            .declarations
            .nodes
            .iter()
            .copied()
            .filter(|&decl_idx| {
                self.arena
                    .get(decl_idx)
                    .and_then(|n| self.arena.get_variable_declaration(n))
                    .is_some_and(|d| d.initializer.is_some())
            })
            .collect();

        // Block-level using: tsc emits `const/var d1 = __addDisposableResource(env, expr, false)`
        // inside the try block. Uses `var` for ES5, `const` otherwise.
        if !initialized_decls.is_empty() {
            let kw = if self.ctx.target_es5 { "var" } else { "const" };
            self.write(kw);
            self.write(" ");
            for (i, &decl_idx) in initialized_decls.iter().enumerate() {
                if let Some(decl_node) = self.arena.get(decl_idx)
                    && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                {
                    self.emit(decl.name);
                    self.write(" = ");
                    self.write_helper("__addDisposableResource");
                    self.write("(");
                    self.write(env_name);
                    self.write(", ");
                    self.emit(decl.initializer);
                    self.write(", ");
                    self.write(if using_async { "true" } else { "false" });
                    self.write(")");
                    if i + 1 < initialized_decls.len() {
                        self.write(", ");
                    }
                }
            }
            self.write(";");
        }
    }

    fn emit_static_block_await_labeled_jump_recovery(&mut self, stmt_idx: NodeIndex) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        let jump_keyword = if stmt_node.kind == syntax_kind_ext::BREAK_STATEMENT {
            "break"
        } else if stmt_node.kind == syntax_kind_ext::CONTINUE_STATEMENT {
            "continue"
        } else {
            return false;
        };
        if !self.static_block_jump_source_has_await_label(stmt_node, jump_keyword) {
            return false;
        }

        self.write(jump_keyword);
        self.write(" ;");
        true
    }

    fn static_block_jump_source_has_await_label(
        &self,
        stmt_node: &Node,
        jump_keyword: &str,
    ) -> bool {
        if !self.ctx.flags.in_class_static_block {
            return false;
        }
        if self
            .arena
            .get_jump_data(stmt_node)
            .is_some_and(|jump| jump.label.is_some())
        {
            return false;
        }
        let Some(text) = self.source_text else {
            return false;
        };
        let start = stmt_node.pos as usize;
        if start >= text.len() {
            return false;
        }
        let line_end = text[start..]
            .find('\n')
            .map_or(text.len(), |offset| start + offset);
        let Ok(line) = crate::safe_slice::slice(text, start, line_end) else {
            return false;
        };
        let Some(rest) = line.trim_start().strip_prefix(jump_keyword) else {
            return false;
        };
        let rest = rest.trim_start();
        rest.starts_with("await")
            && rest["await".len()..]
                .chars()
                .next()
                .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '$')
    }
}
