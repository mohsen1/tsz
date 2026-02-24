use super::{Printer, get_trailing_comment_ranges};
use crate::safe_slice;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::node_flags;
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

        // Single-line blocks: tsc only preserves single-line formatting for
        // function/method/arrow body blocks.  Control-flow blocks (for, while,
        // if, do, try, etc.) are always expanded to multi-line, even when the
        // source was single-line.
        // (Forced multi-line when we need to inject `var _this = this;`.)
        let is_single_statement = block.statements.nodes.len() == 1;
        let should_emit_single_line = is_single_statement
            && self.is_single_line(node)
            && !needs_this_capture
            && is_function_body_block
            && self.hoisted_assignment_value_temps.is_empty()
            && self.hoisted_for_of_temps.is_empty();

        if should_emit_single_line {
            if is_function_body_block {
                self.ctx.block_scope_state.enter_function_scope();
            } else {
                self.ctx.block_scope_state.enter_scope();
            }
            self.map_opening_brace(node);
            self.write("{ ");
            let var_insert_pos = if is_function_body_block {
                Some(self.writer.len())
            } else {
                None
            };
            self.emit(block.statements.nodes[0]);
            // Inject hoisted temp vars inline for single-line function bodies.
            // Temps like `_a` are created during emit (e.g. optional chaining lowering),
            // so we insert `var _a; ` at the position right after `{ `.
            if let Some(byte_offset) = var_insert_pos
                && !self.hoisted_assignment_temps.is_empty()
            {
                let var_decl = format!("var {}; ", self.hoisted_assignment_temps.join(", "));
                self.writer.insert_at(byte_offset, &var_decl);
                self.hoisted_assignment_temps.clear();
            }
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

        // Compute the block's closing `}` position so the last statement's
        // trailing comment scan doesn't overshoot into comments belonging to
        // the closing brace line (same pattern as namespace IIFE emitter).
        let block_close_pos = self
            .source_text
            .map(|text| {
                let bytes = text.as_bytes();
                let end = (node.end as usize).min(bytes.len());
                let mut pos = end;
                while pos > 0 {
                    pos -= 1;
                    if bytes[pos] == b'}' {
                        return pos as u32;
                    }
                }
                node.end
            })
            .unwrap_or(node.end);

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
                            } else if comment_text.starts_with("/*") {
                                self.pending_block_comment_space = true;
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
                    // For the last statement, cap trailing comment scan at the block's
                    // closing `}` to avoid stealing comments that belong on the closing
                    // brace line (same pattern as namespace IIFE emitter).
                    let is_last = stmt_i + 1 >= stmts.len();
                    if is_last {
                        self.emit_trailing_comments_before(token_end, block_close_pos);
                    } else {
                        self.emit_trailing_comments(token_end);
                    }
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

        // Emit comments between the last statement and the closing `}`.
        // tsc preserves comments like `// trailing note` that appear on lines
        // after the last statement but before the block's closing brace.
        // Without this, such comments leak outside the block.
        if !self.ctx.options.remove_comments
            && let Some(text) = self.source_text
        {
            let bytes = text.as_bytes();
            let end = (node.end as usize).min(bytes.len());
            // Scan backwards from node.end to find the closing `}`
            let mut closing_brace_pos = end;
            let mut i = end;
            while i > 0 {
                i -= 1;
                if bytes[i] == b'}' {
                    closing_brace_pos = i;
                    break;
                }
            }
            // Emit any comments that end before the closing brace position
            while self.comment_emit_idx < self.all_comments.len() {
                let c = &self.all_comments[self.comment_emit_idx];
                if (c.end as usize) <= closing_brace_pos {
                    let comment_text =
                        crate::safe_slice::slice(text, c.pos as usize, c.end as usize);
                    let c_trailing = c.has_trailing_new_line;
                    let c_pos = c.pos;
                    self.write_comment_with_reindent(comment_text, Some(c_pos));
                    if c_trailing {
                        self.write_line();
                    }
                    self.comment_emit_idx += 1;
                } else {
                    break;
                }
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

        let has_using_declaration = var_stmt.declarations.nodes.iter().any(|decl_list_idx| {
            self.arena
                .get(*decl_list_idx)
                .is_some_and(|decl_list| (decl_list.flags as u32 & node_flags::USING) != 0)
        });

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
        // Skip the semicolon only when `using` declarations are being lowered
        // (targets below ES2025). At ES2025+, `using` passes through as-is and
        // needs a normal trailing semicolon like var/let/const.
        let using_is_lowered = has_using_declaration && !self.ctx.options.target.supports_es2025();
        if !using_is_lowered {
            self.map_trailing_semicolon(node);
            self.write_semicolon();
        }

        // Emit trailing comments (e.g., var x = 1; // comment).
        // Use a bounded scan range that excludes erased type annotations.
        // For `var v: { (...); // comment }`, the backward `;` scan must
        // not find semicolons inside the erased type annotation.
        let effective_end = self.variable_statement_effective_end(&var_stmt.declarations);
        self.emit_trailing_comment_after_semicolon_in_range(node.pos, effective_end);

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

    /// Compute the source position at the end of the last emitted content for
    /// a variable statement, excluding erased type annotations. This prevents
    /// `emit_trailing_comment_after_semicolon` from finding semicolons inside
    /// erased type annotations (e.g., `var v: { (x: number); // comment }`).
    fn variable_statement_effective_end(&self, declarations: &NodeList) -> u32 {
        // Walk the declaration list to find the last variable declaration's
        // name or initializer end position.
        let mut effective_end = 0u32;
        for &decl_list_idx in &declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            // Use the full node end as baseline
            effective_end = effective_end.max(decl_list_node.end);

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
                // If the declaration has a type annotation but no initializer,
                // use the name's end as the effective boundary (the type annotation
                // is erased and its semicolons should not be scanned).
                if decl.type_annotation.is_some()
                    && decl.initializer.is_none()
                    && let Some(name_node) = self.arena.get(decl.name)
                {
                    // Use the name's end, not the full declaration end
                    effective_end = name_node.end;
                }
            }
        }
        effective_end
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

        // Suppress bare `declare;` expression statements that are artifacts of the parser
        // not recognizing `declare` as a modifier before certain keywords (e.g.,
        // `declare import a = b;`, `declare export function f() {}`). We distinguish
        // these from legitimate `declare;` expressions (where `declare` is a variable)
        // by checking the source text: if `declare` is immediately followed by a keyword
        // on the same line (no newline/semicolon between), it was meant as a modifier.
        if let Some(expr_node) = self.arena.get(expr_stmt.expression)
            && let Some(ident) = self.arena.get_identifier(expr_node)
            && ident.escaped_text == "declare"
            && self.is_declare_modifier_artifact(node)
        {
            self.skip_comments_for_erased_node(node);
            return;
        }

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
        self.emit_trailing_comment_after_semicolon_in_range(node.pos, node.end);
    }

    /// Like `emit_trailing_comment_after_semicolon` but with an explicit scan range.
    /// Use this when the node's full range includes erased content (e.g., type
    /// annotations with semicolons inside) that should not be scanned.
    pub(super) fn emit_trailing_comment_after_semicolon_in_range(
        &mut self,
        range_start: u32,
        range_end: u32,
    ) {
        if self.ctx.options.remove_comments {
            return;
        }

        let Some(text) = self.source_text else {
            return;
        };

        let bytes = text.as_bytes();
        let stmt_end = std::cmp::min(range_end as usize, bytes.len());
        let stmt_start = range_start as usize;

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
            self.emit_trailing_comment_after_semicolon(node);
            return;
        };

        self.write("return");
        if ret.expression.is_some() {
            self.write(" ");
            self.emit_expression(ret.expression);
        }
        self.map_trailing_semicolon(node);
        self.write_semicolon();
        self.emit_trailing_comment_after_semicolon(node);
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
            self.emit_trailing_comment_after_semicolon(node);
            return;
        };

        self.write("throw ");
        self.emit(throw_data.expression);
        self.map_trailing_semicolon(node);
        self.write_semicolon();
        self.emit_trailing_comment_after_semicolon(node);
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
            // ES2019 optional catch binding: generate a unique temp name like tsc does
            // (e.g., _a, _b, _c) instead of a hardcoded name.
            let name = self.make_unique_name();
            self.write(" (");
            self.write(&name);
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
        self.emit_trailing_comment_after_semicolon(node);
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
        self.emit_trailing_comment_after_semicolon(node);
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
        self.emit_trailing_comment_after_semicolon(node);
    }

    pub(super) fn emit_debugger_statement(&mut self, node: &Node) {
        self.write("debugger");
        self.map_trailing_semicolon(node);
        self.write_semicolon();
        self.emit_trailing_comment_after_semicolon(node);
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
#[path = "../../tests/statements.rs"]
mod tests;
