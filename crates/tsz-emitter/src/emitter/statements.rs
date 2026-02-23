use super::{Printer, get_trailing_comment_ranges};
use crate::safe_slice;
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
        // Reset the flag so nested blocks (for/if/while inside this function)
        // are not treated as function body blocks.
        self.emitting_function_body_block = false;

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
                if is_function_body_block {
                    // tsc suppresses trailing comments on function/method/arrow body
                    // opening braces.  For empty bodies the comments sit between { and },
                    // so we skip same-line-as-brace comments and preserve original format.
                    self.skip_trailing_same_line_comments(node.pos, closing_brace_pos);
                    // After skipping same-line comments, check if there are still inner
                    // comments on subsequent lines (e.g., a comment-only function body
                    // like `foo() {\n    //return 4;\n}`). tsc preserves these.
                    let has_remaining_comments = self
                        .all_comments
                        .get(self.comment_emit_idx)
                        .is_some_and(|c| c.end <= closing_brace_pos);
                    if self.is_single_line(node) && !has_remaining_comments {
                        self.map_opening_brace(node);
                        self.write("{ }");
                    } else if has_remaining_comments {
                        self.map_opening_brace(node);
                        self.write_with_end_marker("{");
                        self.write_line();
                        self.increase_indent();
                        self.emit_comments_before_pos(closing_brace_pos);
                        self.decrease_indent();
                        self.map_closing_brace(node);
                        self.write_with_end_marker("}");
                    } else {
                        self.map_opening_brace(node);
                        self.write_with_end_marker("{");
                        self.write_line();
                        self.map_closing_brace(node);
                        self.write_with_end_marker("}");
                    }
                } else {
                    self.map_opening_brace(node);
                    self.write_with_end_marker("{");
                    self.write_line();
                    self.increase_indent();
                    self.emit_comments_before_pos(closing_brace_pos);
                    self.decrease_indent();
                    self.map_closing_brace(node);
                    self.write_with_end_marker("}");
                }
            } else if self.is_single_line(node) {
                // Single-line empty block: { }
                self.map_opening_brace(node);
                self.write("{ }");
            } else {
                // Multi-line empty block (has newlines in source): {\n}
                // TypeScript preserves multi-line format even for empty blocks
                self.map_opening_brace(node);
                self.write_with_end_marker("{");
                self.write_line();
                self.map_closing_brace(node);
                self.write_with_end_marker("}");
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
            self.map_opening_brace(node);
            self.write("{ ");
            self.emit(block.statements.nodes[0]);
            self.map_closing_brace(node);
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
        // Map opening `{` to its source position
        self.map_opening_brace(node);
        self.write_with_end_marker("{");
        // Emit trailing comments on the same line as `{` before moving to the next line.
        // For example: `if (cond) { // comment` should keep `// comment` on the brace line.
        // tsc does NOT emit trailing comments on function/method/arrow body opening braces —
        // only on control-flow blocks (if/for/while/try/catch). Function body comments are
        // conceptually part of the signature (which may include erased type annotations), so
        // they are dropped to match tsc behavior.
        if !self.ctx.options.remove_comments
            && let Some(text) = self.source_text
        {
            let bytes = text.as_bytes();
            let start = node.pos as usize;
            let end = (node.end as usize).min(bytes.len());
            if let Some(offset) = bytes[start..end].iter().position(|&b| b == b'{') {
                let brace_end = (start + offset + 1) as u32;
                if is_function_body_block {
                    // Suppress (skip) same-line comments on function body `{` without
                    // emitting them. Advance comment_emit_idx past these comments so
                    // they don't leak as leading comments on the first statement.
                    self.skip_trailing_same_line_comments(brace_end, node.end);
                } else {
                    self.emit_trailing_comments(brace_end);
                }
            }
        }
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
                let defer_for_of_comments = stmt_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
                    && self.should_defer_for_of_comments(stmt_node);
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
                            let comment_text =
                                crate::safe_slice::slice(text, c_pos as usize, c_end as usize);
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
            // Only add newline if something was actually emitted and we're not
            // already at line start (e.g. class with lowered static fields already
            // wrote a trailing newline after the last `ClassName.field = value;`).
            if self.writer.len() > before_len && !self.writer.is_at_line_start() {
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
        // Map closing `}` to its source position for accurate debugger stepping
        self.map_closing_brace(node);
        self.write_with_end_marker("}");
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
        if self
            .arena
            .has_modifier(&var_stmt.modifiers, SyntaxKind::DeclareKeyword)
        {
            self.skip_comments_for_erased_node(node);
            return;
        }

        let is_exported = self.ctx.is_commonjs()
            && self
                .arena
                .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
            && !self.ctx.module_state.has_export_assignment;
        let is_default = self
            .arena
            .has_modifier(&var_stmt.modifiers, SyntaxKind::DefaultKeyword);

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
        self.map_trailing_semicolon(node);
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
                if decl.initializer.is_some() {
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
        self.map_trailing_semicolon(node);
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

        let then_is_block = self
            .arena
            .get(if_stmt.then_statement)
            .is_some_and(|n| n.kind == syntax_kind_ext::BLOCK);

        // TSC always puts non-block then-statements on their own indented line,
        // e.g., `if (cond)\n    return;`. Block then-statements stay on the same
        // line: `if (cond) { ... }`.
        let then_is_multiline_in_source = !then_is_block;

        self.write("if (");
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
            self.write_line();
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
            if else_is_erased && !else_is_if {
                self.write("else");
                self.write_line();
                self.increase_indent();
                self.emit(if_stmt.else_statement);
                self.write(";");
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

    pub(super) fn emit_while_statement(&mut self, node: &Node) {
        let Some(loop_stmt) = self.arena.get_loop(node) else {
            return;
        };

        self.write("while (");
        self.emit(loop_stmt.condition);
        // Map closing `)` — scan backward from body start
        if let Some(body_node) = self.arena.get(loop_stmt.statement) {
            self.map_closing_paren_backward(node.pos, body_node.pos);
        }
        self.write(")");
        self.emit_loop_body(loop_stmt.statement);
    }

    pub(super) fn emit_for_statement(&mut self, node: &Node) {
        let Some(loop_stmt) = self.arena.get_loop(node) else {
            return;
        };

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
                let s1 = semis.first().map(|&p| {
                    crate::output::source_writer::source_position_from_offset(text, p as u32)
                });
                let s2 = semis.get(1).map(|&p| {
                    crate::output::source_writer::source_position_from_offset(text, p as u32)
                });
                (s1, s2)
            } else {
                (None, None)
            }
        };

        self.write("for (");
        self.emit(loop_stmt.initializer);
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
            self.emit(loop_stmt.incrementor);
        }
        // Map closing `)` — scan backward from body start
        if let Some(body_node) = self.arena.get(loop_stmt.statement) {
            self.map_closing_paren_backward(node.pos, body_node.pos);
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
        // Map closing `)` — scan backward from body start
        if let Some(body_node) = self.arena.get(for_in_of.statement) {
            self.map_closing_paren_backward(node.pos, body_node.pos);
        }
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
        // Map closing `)` — scan backward from body start
        if let Some(body_node) = self.arena.get(for_in_of.statement) {
            self.map_closing_paren_backward(node.pos, body_node.pos);
        }
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
            self.map_trailing_semicolon(node);
            self.write_semicolon();
            return;
        };

        self.write("return");
        if ret.expression.is_some() {
            self.write(" ");
            self.emit_expression(ret.expression);
        }
        self.map_trailing_semicolon(node);
        self.write_semicolon();
    }

    // =========================================================================
    // Additional Statements
    // =========================================================================

    pub(super) fn emit_throw_statement(&mut self, node: &Node) {
        // ThrowStatement uses ReturnData (same structure)
        let Some(throw_data) = self.arena.get_return_statement(node) else {
            self.write("throw");
            self.map_trailing_semicolon(node);
            self.write_semicolon();
            return;
        };

        self.write("throw ");
        self.emit(throw_data.expression);
        self.map_trailing_semicolon(node);
        self.write_semicolon();
    }

    pub(super) fn emit_try_statement(&mut self, node: &Node) {
        let Some(try_stmt) = self.arena.get_try(node) else {
            return;
        };

        self.write("try ");
        self.emit(try_stmt.try_block);

        if try_stmt.catch_clause.is_some() {
            self.write_line();
            self.emit(try_stmt.catch_clause);
        }

        if try_stmt.finally_block.is_some() {
            self.write_line();
            // Map the `finally` keyword to its source position
            // The keyword is between the catch block end and finally block start
            if let Some(finally_node) = self.arena.get(try_stmt.finally_block) {
                let search_start = if try_stmt.catch_clause.is_some() {
                    self.arena
                        .get(try_stmt.catch_clause)
                        .map_or(node.pos, |n| n.end)
                } else {
                    self.arena
                        .get(try_stmt.try_block)
                        .map_or(node.pos, |n| n.end)
                };
                self.map_token_after_skipping_whitespace(search_start, finally_node.pos);
            }
            self.write("finally ");
            self.emit(try_stmt.finally_block);
        }
    }

    pub(super) fn emit_catch_clause(&mut self, node: &Node) {
        let Some(catch) = self.arena.get_catch_clause(node) else {
            return;
        };

        self.write("catch");

        if catch.variable_declaration.is_some() {
            self.write(" ");
            // Map the `(` to its source position
            self.map_token_after(node.pos, node.end, b'(');
            self.write("(");
            self.emit(catch.variable_declaration);
            self.write(")");
        } else if self.ctx.needs_es2019_lowering {
            self.write(" (_unused)");
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
        // Map closing `)` — scan forward from expression end
        if let Some(expr_node) = self.arena.get(switch.expression) {
            self.map_token_after(expr_node.end, node.end, b')');
        }
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

    pub(super) fn emit_case_clause(&mut self, node: &Node) {
        let Some(clause) = self.arena.get_case_clause(node) else {
            return;
        };

        self.write("case ");
        self.emit(clause.expression);
        // Map the `:` after the case expression
        let label_end = self.arena.get(clause.expression).map_or(0, |n| n.end);
        self.map_token_after(label_end, node.end, b':');
        self.write(":");

        // Use expression end position for same-line detection
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
            && jump.label.is_some()
        {
            self.write(" ");
            self.emit(jump.label);
        }
        self.map_trailing_semicolon(node);
        self.write_semicolon();
    }

    pub(super) fn emit_continue_statement(&mut self, node: &Node) {
        self.write("continue");
        if let Some(jump) = self.arena.get_jump_data(node)
            && jump.label.is_some()
        {
            self.write(" ");
            self.emit(jump.label);
        }
        self.map_trailing_semicolon(node);
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
        // Map closing `)` — scan backward from node end (past `;`)
        self.map_closing_paren_backward(node.pos, node.end);
        self.write(")");
        self.map_trailing_semicolon(node);
        self.write_semicolon();
    }

    pub(super) fn emit_debugger_statement(&mut self, node: &Node) {
        self.write("debugger");
        self.map_trailing_semicolon(node);
        self.write_semicolon();
    }

    pub(super) fn emit_with_statement(&mut self, node: &Node) {
        let Some(with_stmt) = self.arena.get_with_statement(node) else {
            return;
        };

        self.write("with (");
        self.emit(with_stmt.expression);
        // Map closing `)` — scan backward from body start
        if let Some(body_node) = self.arena.get(with_stmt.then_statement) {
            self.map_closing_paren_backward(node.pos, body_node.pos);
        }
        self.write(") ");
        self.emit(with_stmt.then_statement);
    }
}

#[cfg(test)]
mod tests {
    use crate::output::printer::{PrintOptions, Printer};
    use tsz_parser::ParserState;

    /// Case clause with a single non-block statement on the same source line
    /// should be emitted on one line: `case true: return "true";`
    #[test]
    fn case_clause_same_line_non_block_statement() {
        let source = r#"function f(x: boolean) {
    switch (x) {
        case true: return "true";
        case false: return "false";
    }
}"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains(r#"case true: return "true";"#),
            "Case clause with single statement on same line should stay on one line.\nOutput:\n{output}"
        );
        assert!(
            output.contains(r#"case false: return "false";"#),
            "Case clause with single statement on same line should stay on one line.\nOutput:\n{output}"
        );
    }

    /// Case clause with a statement on a different line should be indented normally.
    #[test]
    fn case_clause_multiline_stays_indented() {
        let source = r#"function f(x: number) {
    switch (x) {
        case 1:
            return "one";
        case 2:
            return "two";
    }
}"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        // Should NOT be on same line
        assert!(
            !output.contains("case 1: return"),
            "Case clause with statement on next line should remain multi-line.\nOutput:\n{output}"
        );
        assert!(
            output.contains("case 1:\n"),
            "Case clause should have newline after colon.\nOutput:\n{output}"
        );
    }

    /// Default clause with same-line statement should also be emitted on one line.
    #[test]
    fn default_clause_same_line_statement() {
        let source = r#"function f(x: number) {
    switch (x) {
        case 1: return "one";
        default: return "other";
    }
}"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains(r#"default: return "other";"#),
            "Default clause with single statement on same line should stay on one line.\nOutput:\n{output}"
        );
    }

    /// Case clause with a block on the same line should still work (existing behavior).
    #[test]
    fn case_clause_same_line_block_statement() {
        let source = r#"function f(x: number) {
    switch (x) {
        case 0: { break; }
        default: break;
    }
}"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("case 0: {"),
            "Case clause with block on same line should stay on one line.\nOutput:\n{output}"
        );
    }

    #[test]
    fn ts_check_comment_preserved_in_output() {
        let source = "// @ts-check\nvar x = 1;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("// @ts-check"),
            "// @ts-check directive should be preserved in output.\nOutput:\n{output}"
        );
    }

    #[test]
    fn ts_nocheck_comment_preserved_in_output() {
        let source = "// @ts-nocheck\nvar x = 1;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("// @ts-nocheck"),
            "// @ts-nocheck directive should be preserved in output.\nOutput:\n{output}"
        );
    }

    #[test]
    fn test_at_directive_comments_preserved() {
        // tsc preserves all source-level `// @` comments in JS output.
        // The test harness strips actual test directives from the baseline
        // source before the emitter sees them, so any `// @` comment
        // in the source is a legitimate comment to preserve.
        let source = "// @target: esnext\nvar x = 1;\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("// @target"),
            "// @target directive should be preserved in output (tsc preserves all source comments).\nOutput:\n{output}"
        );
    }

    #[test]
    fn test_ts_ignore_directive_preserved() {
        // // @ts-ignore is a runtime directive that tsc preserves.
        let source = "// @ts-ignore\nvar x: number = 'hello';\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("// @ts-ignore"),
            "// @ts-ignore directive should be preserved in output.\nOutput:\n{output}"
        );
    }

    #[test]
    fn test_ts_expect_error_directive_preserved() {
        // // @ts-expect-error is a runtime directive that tsc preserves.
        let source = "// @ts-expect-error\nvar x: number = 'hello';\n";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("// @ts-expect-error"),
            "// @ts-expect-error directive should be preserved in output.\nOutput:\n{output}"
        );
    }

    /// Comments before case/default clauses should appear before the label,
    /// not inside the clause body. tsc emits:
    ///   // comment
    ///   case X:
    /// not:
    ///   case X:
    ///       // comment
    #[test]
    fn case_clause_leading_comment_before_label() {
        let source = r#"function f(x: number) {
    switch (x) {
        // First case
        case 0:
            return "zero";
        // Second case
        case 1:
            return "one";
    }
}"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        // Comment must appear BEFORE the case keyword, not after.
        // The case clause is indented 2 levels (8 spaces) inside function + switch.
        assert!(
            output.contains("// First case\n        case 0:"),
            "Leading comment should appear before 'case 0:', not inside the body.\nOutput:\n{output}"
        );
        assert!(
            output.contains("// Second case\n        case 1:"),
            "Leading comment should appear before 'case 1:', not inside the body.\nOutput:\n{output}"
        );
    }

    /// Comment before default clause should appear before 'default:', not inside the body.
    #[test]
    fn default_clause_leading_comment_before_label() {
        let source = r#"function f(x: number) {
    switch (x) {
        case 0:
            return "zero";
        // Fallback
        default:
            return "other";
    }
}"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("// Fallback\n        default:"),
            "Leading comment should appear before 'default:', not inside the body.\nOutput:\n{output}"
        );
    }

    /// Trailing comment on opening `{` of a block should stay on the same line.
    /// e.g. `if (cond) { // comment` should NOT become `if (cond) {\n    // comment`.
    #[test]
    fn trailing_comment_on_opening_brace_if_statement() {
        let source = r#"function f(x: string) {
    if (typeof x === "Object") { // comparison is OK
        console.log(x);
    }
}"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("{ // comparison is OK"),
            "Trailing comment should stay on the same line as opening brace.\nOutput:\n{output}"
        );
    }

    /// Trailing comment on opening `{` of a for-in loop body block.
    #[test]
    fn trailing_comment_on_opening_brace_for_in() {
        let source = r#"function f(x: object) {
    for (const key in x) { // iterate
        console.log(key);
    }
}"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("{ // iterate"),
            "Trailing comment should stay on the same line as opening brace.\nOutput:\n{output}"
        );
    }

    /// tsc drops trailing comments on function body opening `{`.
    /// `function foo(x: number) { // comment` should emit `function foo(x) {` (no comment).
    #[test]
    fn function_body_brace_comment_suppressed() {
        let source = r#"function foo(x: number) { // param comment
    return x;
}"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            !output.contains("// param comment"),
            "Trailing comment on function body `{{` should be suppressed.\nOutput:\n{output}"
        );
        assert!(
            output.contains("return x;"),
            "Function body should still be emitted.\nOutput:\n{output}"
        );
    }

    /// tsc drops trailing comments on method body opening `{`, but preserves
    /// trailing comments on control-flow blocks inside the method.
    #[test]
    fn method_body_brace_comment_suppressed_but_inner_block_preserved() {
        let source = r#"class C {
    foo(_i: number, ...rest) { // error
        if (true) { // ok
            var _i = 10;
        }
    }
}"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            !output.contains("{ // error"),
            "Trailing comment on method body `{{` should be suppressed.\nOutput:\n{output}"
        );
        assert!(
            output.contains("{ // ok"),
            "Trailing comment on if-block `{{` should be preserved.\nOutput:\n{output}"
        );
    }

    /// tsc drops trailing comments on arrow function body opening `{`.
    #[test]
    fn arrow_function_body_brace_comment_suppressed() {
        let source = r#"const fn = (x: number) => { // arrow comment
    return x;
};"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            !output.contains("// arrow comment"),
            "Trailing comment on arrow function body `{{` should be suppressed.\nOutput:\n{output}"
        );
    }

    /// Empty function body with trailing comment on `{` should suppress the comment.
    /// tsc: `function f4(_i, ...rest) {\n}` (comment dropped)
    #[test]
    fn empty_function_body_brace_comment_suppressed() {
        let source = "function f4(_i: any, ...rest) { // error\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            !output.contains("// error"),
            "Trailing comment on empty function body `{{` should be suppressed.\nOutput:\n{output}"
        );
    }

    /// Empty method body with comment should also be suppressed.
    #[test]
    fn empty_method_body_brace_comment_suppressed() {
        let source = "class C {\n    foo() { // comment\n    }\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            !output.contains("// comment"),
            "Trailing comment on empty method body `{{` should be suppressed.\nOutput:\n{output}"
        );
    }

    /// Control-flow empty blocks should still preserve comments.
    #[test]
    fn empty_if_block_comment_preserved() {
        let source = "function f() {\n    if (true) { // keep this\n    }\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("// keep this"),
            "Trailing comment on control-flow empty block should be preserved.\nOutput:\n{output}"
        );
    }

    /// Empty method body with inner comment on a DIFFERENT line from `{` should
    /// preserve the comment.  tsc: `foo() {\n    //return 4;\n}`
    /// (This is distinct from same-line comments on `{` which ARE suppressed.)
    #[test]
    fn empty_method_body_inner_comment_on_next_line_preserved() {
        let source = "class Foo {\n    foo(): number {\n        //return 4;\n    }\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("//return 4;"),
            "Inner comment on a different line from `{{` in an empty method body \
             should be preserved (tsc preserves these).\nOutput:\n{output}"
        );
    }

    /// Empty constructor body with inner comment on a different line should
    /// preserve the comment.  tsc: `constructor(x) {\n    // comment\n}`
    #[test]
    fn empty_constructor_body_inner_comment_preserved() {
        let source = "class Foo {\n    constructor(x: any) {\n        // WScript.Echo(\"test\");\n    }\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("// WScript.Echo"),
            "Inner comment in empty constructor body should be preserved.\nOutput:\n{output}"
        );
    }

    /// Single-line empty function body with same-line block comment should still
    /// suppress the comment.  tsc: `bar1() { }` (comment dropped)
    #[test]
    fn empty_method_body_single_line_comment_still_suppressed() {
        let source = "class A {\n    bar1() { /*WScript.Echo(\"bar1\");*/ }\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            !output.contains("WScript"),
            "Same-line block comment in single-line empty method body should be \
             suppressed (tsc drops these).\nOutput:\n{output}"
        );
    }
}
