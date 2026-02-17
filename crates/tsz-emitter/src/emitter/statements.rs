use super::{Printer, get_trailing_comment_ranges};
use crate::printer::safe_slice;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    // =========================================================================
    // Statements
    // =========================================================================

    pub(super) fn emit_block(&mut self, node: &Node, idx: NodeIndex) {
        let Some(block) = self.arena.get_block(node) else {
            return;
        };
        let is_function_body_block = self.emitting_function_body_block;

        // Check if this block needs `var _this = this;` injection
        let this_capture_name: Option<String> = self
            .transforms
            .this_capture_name(idx)
            .map(std::string::ToString::to_string);
        let needs_this_capture = this_capture_name.is_some();

        // Empty blocks: check for comments inside and preserve original format
        if block.statements.nodes.is_empty() && !needs_this_capture {
            // Find the actual closing `}` position (not node.end which includes trailing trivia)
            let closing_brace_pos = self.find_token_end_before_trivia(node.pos, node.end);
            // Check if there are comments inside the block (between { and })
            let has_inner_comments = !self.ctx.options.remove_comments
                && self
                    .all_comments
                    .get(self.comment_emit_idx)
                    .is_some_and(|c| c.end <= closing_brace_pos);
            if has_inner_comments {
                self.write("{");
                self.write_line();
                self.increase_indent();
                self.emit_comments_before_pos(closing_brace_pos);
                self.decrease_indent();
                self.write("}");
            } else if self.is_single_line(node) {
                // Single-line empty block: { }
                self.write("{ }");
            } else {
                // Multi-line empty block (has newlines in source): {\n}
                // TypeScript preserves multi-line format even for empty blocks
                self.write("{");
                self.write_line();
                self.write("}");
            }
            // Trailing comments are handled by the calling context
            // (class member loop, statement loop, etc.)
            return;
        }

        // Single-line blocks: preserve single-line formatting from source.
        // tsc only emits single-line blocks when the original source was single-line.
        // It never collapses multi-line blocks to single lines.
        // (But not when we need to inject `var _this = this;` — that forces multi-line.)
        let is_single_statement = block.statements.nodes.len() == 1;
        let should_emit_single_line = is_single_statement
            && self.is_single_line(node)
            && !needs_this_capture
            && (!is_function_body_block
                || (self.hoisted_assignment_value_temps.is_empty()
                    && self.hoisted_for_of_temps.is_empty()));

        if should_emit_single_line {
            if is_function_body_block {
                self.ctx.block_scope_state.enter_function_scope();
            } else {
                self.ctx.block_scope_state.enter_scope();
            }
            self.write("{ ");
            self.emit(block.statements.nodes[0]);
            self.write(" }");
            self.ctx.block_scope_state.exit_scope();
            // Trailing comments are handled by the calling context
            return;
        }

        if is_function_body_block {
            self.ctx.block_scope_state.enter_function_scope();
        } else {
            self.ctx.block_scope_state.enter_scope();
        }
        self.write("{");
        self.write_line();
        self.increase_indent();

        // Inject `var _this = this;` at the start of the block for arrow function _this capture
        if let Some(ref capture_name) = this_capture_name {
            self.write("var ");
            self.write(capture_name);
            self.write(" = this;");
            self.write_line();
        }
        let hoisted_var_byte_offset = if is_function_body_block {
            Some((self.writer.len(), self.writer.current_line()))
        } else {
            None
        };

        // Pre-collect statement indices so we can look up the next statement's
        // position as an upper bound for trailing comment scanning. Our parser sets
        // stmt_node.end past the statement boundary into the next statement's tokens,
        // so using next_stmt.pos as the scan limit prevents over-scanning.
        let stmts: Vec<NodeIndex> = block.statements.nodes.to_vec();
        for (stmt_i, &stmt_idx) in stmts.iter().enumerate() {
            // Emit leading comments before this statement
            if let Some(stmt_node) = self.arena.get(stmt_idx) {
                let defer_for_of_comments = self.ctx.target_es5
                    && self.ctx.options.downlevel_iteration
                    && stmt_node.kind == syntax_kind_ext::FOR_OF_STATEMENT;
                // Only skip whitespace, not comments - comments inside expressions
                // should be handled by expression emitters
                let actual_start = self.skip_whitespace_forward(stmt_node.pos, stmt_node.end);
                if !defer_for_of_comments && let Some(text) = self.source_text {
                    while self.comment_emit_idx < self.all_comments.len() {
                        let c_end = self.all_comments[self.comment_emit_idx].end;
                        // Only emit if the comment ends before the statement starts
                        if c_end <= actual_start {
                            let c_pos = self.all_comments[self.comment_emit_idx].pos;
                            let c_trailing =
                                self.all_comments[self.comment_emit_idx].has_trailing_new_line;
                            let comment_text = crate::printer::safe_slice::slice(
                                text,
                                c_pos as usize,
                                c_end as usize,
                            );
                            self.write(comment_text);
                            if c_trailing {
                                self.write_line();
                            }
                            self.comment_emit_idx += 1;
                        } else {
                            break;
                        }
                    }
                }
            }

            let before_len = self.writer.len();
            self.emit(stmt_idx);
            // Only add newline if something was actually emitted
            if self.writer.len() > before_len {
                // Emit trailing same-line comments (e.g. `foo(); // comment`).
                // Use the next statement's pos as the scan upper bound: stmt_node.end
                // extends into the next statement, which would cause
                // find_token_end_before_trivia to return a position inside the next
                // statement and incorrectly skip between-statement comments.
                let (stmt_pos, upper_bound) = {
                    let cur = self.arena.get(stmt_idx);
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
                    self.emit_trailing_comments(token_end);
                }
                self.write_line();
            }
        }

        if let Some((byte_offset, line_no)) = hoisted_var_byte_offset {
            let indent = " ".repeat(self.writer.indent_width() as usize);
            let mut ref_vars = Vec::new();
            ref_vars.extend(self.hoisted_assignment_temps.iter().cloned());
            ref_vars.extend(self.hoisted_for_of_temps.iter().cloned());

            if !ref_vars.is_empty() {
                let var_decl = format!("{}var {};", indent, ref_vars.join(", "));
                self.writer.insert_line_at(byte_offset, line_no, &var_decl);
            }

            if !self.hoisted_assignment_value_temps.is_empty() {
                let var_decl = format!(
                    "{}var {};",
                    indent,
                    self.hoisted_assignment_value_temps.join(", ")
                );
                self.writer.insert_line_at(byte_offset, line_no, &var_decl);
            }
        }

        self.decrease_indent();
        self.write("}");
        self.ctx.block_scope_state.exit_scope();
        // Trailing comments after the block's closing brace are handled by
        // the calling context (class member loop, statement loop, etc.)
    }

    pub(super) fn emit_function_body_hoisted_temps(&mut self) {
        if !self.hoisted_assignment_value_temps.is_empty() {
            self.write("var ");
            self.write(&self.hoisted_assignment_value_temps.join(", "));
            self.write(";");
            self.write_line();
        }

        let mut ref_vars = Vec::new();
        ref_vars.extend(self.hoisted_assignment_temps.iter().cloned());
        ref_vars.extend(self.hoisted_for_of_temps.iter().cloned());

        if !ref_vars.is_empty() {
            self.write("var ");
            self.write(&ref_vars.join(", "));
            self.write(";");
            self.write_line();
        }
    }

    pub(super) fn emit_variable_statement(&mut self, node: &Node) {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return;
        };

        // Skip ambient declarations (declare var/let/const)
        if self.has_declare_modifier(&var_stmt.modifiers) {
            self.skip_comments_for_erased_node(node);
            return;
        }

        let is_exported = self.ctx.is_commonjs()
            && self.has_export_modifier(&var_stmt.modifiers)
            && !self.ctx.module_state.has_export_assignment;
        let is_default = self.has_default_modifier(&var_stmt.modifiers);

        // For CommonJS exported variables with no initializers, skip the
        // declaration entirely. The preamble `exports.X = void 0;` already
        // handles the export, and no local `var` is needed.
        if is_exported && self.all_declarations_lack_initializer(&var_stmt.declarations) {
            return;
        }

        // Collect declaration names for export assignment
        let export_names: Vec<String> = if is_exported {
            self.collect_variable_names(&var_stmt.declarations)
        } else {
            Vec::new()
        };

        // VariableStatement.declarations contains a VARIABLE_DECLARATION_LIST
        // Emit the declaration list (which handles the let/const/var keyword)
        for &decl_list_idx in &var_stmt.declarations.nodes {
            self.emit(decl_list_idx);
        }
        self.write_semicolon();

        // Emit trailing comments (e.g., var x = 1; // comment)
        self.emit_trailing_comment_after_semicolon(node);

        // CommonJS: emit exports.X = X; after the declaration
        if is_exported && !export_names.is_empty() {
            self.write_line();
            if is_default && export_names.len() == 1 {
                // export default const x = ... -> exports.default = x;
                self.write("exports.default = ");
                self.write(&export_names[0]);
                self.write(";");
            } else {
                // export const x = ..., y = ...; -> exports.x = x; exports.y = y;
                for name in &export_names {
                    self.write("exports.");
                    self.write(name);
                    self.write(" = ");
                    self.write(name);
                    self.write(";");
                    self.write_line();
                }
            }
        }
    }

    /// Check if all variable declarations in a declaration list lack initializers
    pub(super) fn all_declarations_lack_initializer(&self, declarations: &NodeList) -> bool {
        for &decl_list_idx in &declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };

            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                if !decl.initializer.is_none() {
                    return false;
                }
            }
        }
        true
    }

    /// Collect variable names from a declaration list for `CommonJS` export
    pub(super) fn collect_variable_names(&self, declarations: &NodeList) -> Vec<String> {
        let mut names = Vec::new();
        for &decl_list_idx in &declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };

            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                self.collect_binding_names(decl.name, &mut names);
            }
        }
        names
    }

    pub(super) fn collect_binding_names(&self, name_idx: NodeIndex, names: &mut Vec<String>) {
        if name_idx.is_none() {
            return;
        }

        let Some(node) = self.arena.get(name_idx) else {
            return;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            if let Some(id) = self.arena.get_identifier(node) {
                names.push(id.escaped_text.clone());
            }
            return;
        }

        match node.kind {
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    for &elem_idx in &pattern.elements.nodes {
                        self.collect_binding_names_from_element(elem_idx, names);
                    }
                }
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                if let Some(elem) = self.arena.get_binding_element(node) {
                    self.collect_binding_names(elem.name, names);
                }
            }
            _ => {}
        }
    }

    pub(super) fn collect_binding_names_from_element(
        &self,
        elem_idx: NodeIndex,
        names: &mut Vec<String>,
    ) {
        if elem_idx.is_none() {
            return;
        }

        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };

        if let Some(elem) = self.arena.get_binding_element(elem_node) {
            self.collect_binding_names(elem.name, names);
        }
    }

    pub(super) fn emit_expression_statement(&mut self, node: &Node) {
        let Some(expr_stmt) = self.arena.get_expression_statement(node) else {
            return;
        };

        // When a function expression appears as a statement, it needs wrapping parentheses
        // to distinguish it from a function declaration. This includes:
        // - Arrow functions transpiled to ES5 function expressions
        // - Regular function expressions
        let needs_parens = if let Some(expr_node) = self.arena.get(expr_stmt.expression) {
            expr_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || (self.ctx.target_es5 && expr_node.kind == syntax_kind_ext::ARROW_FUNCTION)
        } else {
            false
        };
        let needs_legacy_asterisk_padding = self
            .arena
            .get(expr_stmt.expression)
            .and_then(|expr_node| self.arena.get_unary_expr(expr_node))
            .is_some_and(|unary| unary.operator == SyntaxKind::AsteriskToken as u16);

        if needs_legacy_asterisk_padding {
            self.write_space();
        }

        if needs_parens {
            self.write("(");
        }
        self.emit(expr_stmt.expression);
        if needs_parens {
            self.write(")");
        }
        self.write_semicolon();
        self.emit_trailing_comment_after_semicolon(node);
    }

    /// Emit trailing comments after a semicolon. Scans backward through the
    /// entire node range to find the semicolon, allowing it to work even when
    /// node.end is past the newline (at the start of the next statement).
    pub(super) fn emit_trailing_comment_after_semicolon(&mut self, node: &Node) {
        if self.ctx.options.remove_comments {
            return;
        }

        let Some(text) = self.source_text else {
            return;
        };

        let bytes = text.as_bytes();
        let stmt_end = std::cmp::min(node.end as usize, bytes.len());
        let stmt_start = node.pos as usize;

        // Scan backwards to find the semicolon within this node's range
        let mut semi_pos = None;
        let mut i = stmt_end;
        while i > stmt_start {
            i -= 1;
            if bytes[i] == b';' {
                semi_pos = Some(i + 1);
                break;
            }
        }

        if let Some(pos) = semi_pos {
            let comments = get_trailing_comment_ranges(text, pos);
            for comment in comments {
                self.write_space();
                let comment_text =
                    safe_slice::slice(text, comment.pos as usize, comment.end as usize);
                if !comment_text.is_empty() {
                    self.write(comment_text);
                }
                // Advance the global comment index past this comment so it
                // won't be emitted again by the end-of-file comment sweep.
                while self.comment_emit_idx < self.all_comments.len() {
                    let c = &self.all_comments[self.comment_emit_idx];
                    if c.pos >= comment.pos && c.end <= comment.end {
                        self.comment_emit_idx += 1;
                        break;
                    } else if c.end > comment.end {
                        break;
                    }
                    self.comment_emit_idx += 1;
                }
            }
        }
    }

    pub(super) fn emit_if_statement(&mut self, node: &Node) {
        let Some(if_stmt) = self.arena.get_if_statement(node) else {
            return;
        };

        let then_is_multiline_in_source = self.source_text.is_some_and(|text| {
            let cond_end = self
                .arena
                .get(if_stmt.expression)
                .map_or(node.pos as usize, |n| n.end as usize);
            let then_start = self
                .arena
                .get(if_stmt.then_statement)
                .map_or(node.end as usize, |n| n.pos as usize);
            if cond_end >= then_start || then_start > text.len() {
                return false;
            }
            text[cond_end..then_start].contains('\n')
        }) || self.source_text.is_some_and(|text| {
            let Some(then_node) = self.arena.get(if_stmt.then_statement) else {
                return false;
            };
            if then_node.kind == syntax_kind_ext::BLOCK {
                return false;
            }
            let start = then_node.pos as usize;
            let end = then_node.end as usize;
            if start >= end || end > text.len() {
                return false;
            }
            text[start..end].contains("...")
        }) || (!self.is_single_line(node)
            && self
                .arena
                .get(if_stmt.then_statement)
                .is_some_and(|n| n.kind != syntax_kind_ext::BLOCK))
            || self
                .arena
                .get(if_stmt.then_statement)
                .is_some_and(|n| n.pos >= n.end && n.kind != syntax_kind_ext::BLOCK);

        self.write("if (");
        self.emit(if_stmt.expression);
        self.write(")");
        let then_is_block = self
            .arena
            .get(if_stmt.then_statement)
            .is_some_and(|n| n.kind == syntax_kind_ext::BLOCK);
        if then_is_multiline_in_source {
            self.write_line();
            if !then_is_block {
                self.increase_indent();
            }
        } else {
            self.write(" ");
        }
        self.emit(if_stmt.then_statement);
        if then_is_multiline_in_source && !then_is_block {
            self.decrease_indent();
        }

        if !if_stmt.else_statement.is_none() {
            self.write_line();
            self.write("else ");
            self.emit(if_stmt.else_statement);
        }
    }

    pub(super) fn emit_while_statement(&mut self, node: &Node) {
        let Some(loop_stmt) = self.arena.get_loop(node) else {
            return;
        };

        self.write("while (");
        self.emit(loop_stmt.condition);
        self.write(")");
        self.emit_loop_body(loop_stmt.statement);
    }

    pub(super) fn emit_for_statement(&mut self, node: &Node) {
        let Some(loop_stmt) = self.arena.get_loop(node) else {
            return;
        };

        self.write("for (");
        self.emit(loop_stmt.initializer);
        self.write(";");
        if !loop_stmt.condition.is_none() {
            self.write(" ");
            self.emit(loop_stmt.condition);
        }
        self.write(";");
        if !loop_stmt.incrementor.is_none() {
            self.write(" ");
            self.emit(loop_stmt.incrementor);
        }
        self.write(")");
        self.emit_loop_body(loop_stmt.statement);
    }

    pub(super) fn emit_for_in_statement(&mut self, node: &Node) {
        let Some(for_in_of) = self.arena.get_for_in_of(node) else {
            return;
        };

        self.write("for (");
        self.emit(for_in_of.initializer);
        self.write(" in ");
        self.emit(for_in_of.expression);
        self.write(")");
        self.emit_loop_body(for_in_of.statement);
    }

    pub(super) fn emit_for_of_statement(&mut self, node: &Node) {
        let Some(for_in_of) = self.arena.get_for_in_of(node) else {
            return;
        };

        self.write("for ");
        if for_in_of.await_modifier {
            self.write("await ");
        }
        self.write("(");
        self.emit(for_in_of.initializer);
        self.write(" of ");
        self.emit(for_in_of.expression);
        self.write(")");
        self.emit_loop_body(for_in_of.statement);
    }

    /// Emit a loop body statement. If the body is a block, emit it inline.
    /// If it's a single statement, put it on a new indented line (matching tsc behavior).
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
            self.emit(body);
            self.decrease_indent();
        }
    }

    pub(super) fn emit_return_statement(&mut self, node: &Node) {
        let Some(ret) = self.arena.get_return_statement(node) else {
            self.write("return");
            self.write_semicolon();
            return;
        };

        self.write("return");
        if !ret.expression.is_none() {
            self.write(" ");
            self.emit_expression(ret.expression);
        }
        self.write_semicolon();
    }

    // =========================================================================
    // Additional Statements
    // =========================================================================

    pub(super) fn emit_throw_statement(&mut self, node: &Node) {
        // ThrowStatement uses ReturnData (same structure)
        let Some(throw_data) = self.arena.get_return_statement(node) else {
            self.write("throw");
            self.write_semicolon();
            return;
        };

        self.write("throw ");
        self.emit(throw_data.expression);
        self.write_semicolon();
    }

    pub(super) fn emit_try_statement(&mut self, node: &Node) {
        let Some(try_stmt) = self.arena.get_try(node) else {
            return;
        };

        self.write("try ");
        self.emit(try_stmt.try_block);

        if !try_stmt.catch_clause.is_none() {
            self.write_line();
            self.emit(try_stmt.catch_clause);
        }

        if !try_stmt.finally_block.is_none() {
            self.write_line();
            self.write("finally ");
            self.emit(try_stmt.finally_block);
        }
    }

    pub(super) fn emit_catch_clause(&mut self, node: &Node) {
        let Some(catch) = self.arena.get_catch_clause(node) else {
            return;
        };

        self.write("catch");

        if !catch.variable_declaration.is_none() {
            self.write(" (");
            self.emit(catch.variable_declaration);
            self.write(")");
        }

        self.write(" ");
        self.emit(catch.block);
    }

    pub(super) fn emit_switch_statement(&mut self, node: &Node) {
        let Some(switch) = self.arena.get_switch(node) else {
            return;
        };

        self.write("switch (");
        self.emit(switch.expression);
        self.write(") ");
        // case_block is a NodeIndex pointing to a CaseBlock node
        self.emit(switch.case_block);
    }

    pub(super) fn emit_case_block(&mut self, node: &Node) {
        if !node.has_data() || node.kind != syntax_kind_ext::CASE_BLOCK {
            return;
        }
        let Some(case_block) = self.arena.blocks.get(node.data_index as usize) else {
            return;
        };

        self.write("{");
        self.write_line();
        self.increase_indent();

        for &clause_idx in &case_block.statements.nodes {
            self.emit(clause_idx);
        }

        self.decrease_indent();
        self.write("}");
    }

    pub(super) fn emit_case_clause(&mut self, node: &Node) {
        let Some(clause) = self.arena.get_case_clause(node) else {
            return;
        };

        self.write("case ");
        self.emit(clause.expression);
        self.write(":");

        // Use expression end position for same-line detection
        let label_end = self.arena.get(clause.expression).map_or(0, |n| n.end);
        self.emit_case_clause_body(&clause.statements, label_end);
    }

    pub(super) fn emit_default_clause(&mut self, node: &Node) {
        let Some(clause) = self.arena.get_case_clause(node) else {
            return;
        };

        self.write("default:");

        // Use node pos + "default" length for same-line detection
        self.emit_case_clause_body(&clause.statements, node.pos + 8);
    }

    fn emit_case_clause_body(&mut self, statements: &NodeList, label_end: u32) {
        // If single block statement and the block is on the same line as the
        // case/default label in source, emit it on the same line (e.g., `case 0: {`)
        if statements.nodes.len() == 1
            && let Some(stmt_node) = self.arena.get(statements.nodes[0])
            && stmt_node.kind == syntax_kind_ext::BLOCK
            && self.is_on_same_source_line(label_end, stmt_node.pos)
        {
            self.write(" ");
            self.emit(statements.nodes[0]);
            self.write_line();
            return;
        }

        self.write_line();
        self.increase_indent();

        for &stmt in &statements.nodes {
            // Emit leading comments before this statement.
            // Use skip_trivia_forward to get the actual token start (past comments),
            // so that emit_comments_before_pos can pick up comments in the leading trivia.
            if let Some(stmt_node) = self.arena.get(stmt) {
                let actual_start = self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
                self.emit_comments_before_pos(actual_start);
            }
            self.emit(stmt);
            self.write_line();
        }

        self.decrease_indent();
    }

    /// Check if two source positions are on the same line
    fn is_on_same_source_line(&self, pos1: u32, pos2: u32) -> bool {
        if let Some(text) = self.source_text {
            let start = std::cmp::min(pos1 as usize, text.len());
            let end = std::cmp::min(pos2 as usize, text.len());
            !text[start..end].contains('\n')
        } else {
            false
        }
    }

    pub(super) fn emit_break_statement(&mut self, node: &Node) {
        self.write("break");
        if let Some(jump) = self.arena.get_jump_data(node)
            && !jump.label.is_none()
        {
            self.write(" ");
            self.emit(jump.label);
        }
        self.write_semicolon();
    }

    pub(super) fn emit_continue_statement(&mut self, node: &Node) {
        self.write("continue");
        if let Some(jump) = self.arena.get_jump_data(node)
            && !jump.label.is_none()
        {
            self.write(" ");
            self.emit(jump.label);
        }
        self.write_semicolon();
    }

    pub(super) fn emit_labeled_statement(&mut self, node: &Node) {
        let Some(labeled) = self.arena.get_labeled_statement(node) else {
            return;
        };

        self.emit(labeled.label);
        self.write(": ");
        self.emit(labeled.statement);
    }

    pub(super) fn emit_do_statement(&mut self, node: &Node) {
        let Some(loop_stmt) = self.arena.get_loop(node) else {
            return;
        };

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
            self.emit(loop_stmt.statement);
            self.decrease_indent();
            self.write_line();
        }
        self.write("while (");
        self.emit(loop_stmt.condition);
        self.write(")");
        self.write_semicolon();
    }

    pub(super) fn emit_debugger_statement(&mut self) {
        self.write("debugger");
        self.write_semicolon();
    }

    pub(super) fn emit_with_statement(&mut self, node: &Node) {
        let Some(with_stmt) = self.arena.get_with_statement(node) else {
            return;
        };

        self.write("with (");
        self.emit(with_stmt.expression);
        self.write(") ");
        self.emit(with_stmt.then_statement);
    }
}
