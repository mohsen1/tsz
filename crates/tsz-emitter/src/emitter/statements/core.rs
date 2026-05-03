use super::super::{ModuleKind, Printer, get_trailing_comment_ranges};
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

    pub(in crate::emitter) fn emit_block(&mut self, node: &Node, idx: NodeIndex) {
        let Some(block) = self.arena.get_block(node) else {
            return;
        };
        let is_function_body_block = self.emitting_function_body_block;
        // Reset the flag so nested blocks (for/if/while inside this function)
        // are not treated as function body blocks.
        self.emitting_function_body_block = false;

        // For non-function-body blocks (bare `{}`, if/for/while bodies),
        // save/restore declared_namespace_names so that block-scoped `let`
        // declarations for enums/namespaces in sibling blocks don't leak.
        // Function body blocks already get this from emit_function_declaration etc.
        let prev_declared_ns = if !is_function_body_block {
            Some(self.declared_namespace_names.clone())
        } else {
            None
        };

        // Check if this block needs `var _this = this;` injection
        let this_capture_name: Option<String> = self
            .transforms
            .this_capture_name(idx)
            .map(std::string::ToString::to_string);
        let needs_this_capture = this_capture_name.is_some();

        // Empty blocks: check for comments inside and preserve original format
        if block.statements.nodes.is_empty() && !needs_this_capture {
            // Find the actual closing `}` position (not node.end which includes trailing trivia)
            let closing_brace_end = self.find_block_closing_brace_end(node);
            let closing_brace_pos = closing_brace_end.saturating_sub(1);
            let opening_brace_pos = self.find_block_opening_brace_pos(node).unwrap_or(node.pos);
            if is_function_body_block && !self.ctx.options.remove_comments {
                while self.comment_emit_idx < self.all_comments.len()
                    && self.all_comments[self.comment_emit_idx].end <= opening_brace_pos
                {
                    self.comment_emit_idx += 1;
                }
            }
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
                    // For control-flow blocks (if/for/while/try/catch), emit
                    // trailing same-line comments on the `{` line before the
                    // newline, matching tsc behavior:
                    //   `if (cond) { // comment\n}`
                    if let Some(text) = self.source_text {
                        let bytes = text.as_bytes();
                        let start = node.pos as usize;
                        let end = (node.end as usize).min(bytes.len());
                        if let Some(offset) = bytes[start..end].iter().position(|&b| b == b'{') {
                            let brace_end = (start + offset + 1) as u32;
                            self.emit_trailing_comments(brace_end);
                        }
                    }
                    let has_remaining_comments = self
                        .all_comments
                        .get(self.comment_emit_idx)
                        .is_some_and(|c| c.end <= closing_brace_pos);
                    if has_remaining_comments {
                        self.write_line();
                        self.increase_indent();
                        self.emit_comments_before_pos(closing_brace_pos);
                        self.decrease_indent();
                    } else {
                        self.write_line();
                    }
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
        // tsc preserves the single-line format for function body blocks regardless
        // of statement count, as long as the source was single-line.
        // (Forced multi-line when we need to inject `var _this = this;`.)
        let should_emit_single_line = !block.statements.nodes.is_empty()
            && self.is_single_line(node)
            && !needs_this_capture
            && is_function_body_block
            && self.hoisted_assignment_value_temps.is_empty()
            && self.hoisted_for_of_temps.is_empty()
            && self.pending_object_rest_params.is_empty();

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
            for (si, &stmt_idx) in block.statements.nodes.iter().enumerate() {
                if si > 0 {
                    self.write(" ");
                }
                self.emit(stmt_idx);
            }
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
        // Compute the block's closing `}` position so comment scans don't
        // overshoot into comments belonging to the closing brace line.
        // Use find_token_end_before_trivia which correctly skips `}` inside
        // comments (e.g., `//}` in commented-out code).
        let block_close_pos = {
            let token_end = self.find_token_end_before_trivia(node.pos, node.end);
            // find_token_end_before_trivia returns position AFTER the `}`,
            // we want the position OF `}` so comments ending at `}` are excluded.
            if token_end > node.pos {
                token_end - 1
            } else {
                node.end
            }
        };
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
                    let scan_end = block
                        .statements
                        .nodes
                        .first()
                        .and_then(|&stmt_idx| self.arena.get(stmt_idx))
                        .map_or(block_close_pos, |stmt_node| stmt_node.pos);
                    self.emit_trailing_comments_before(brace_end, scan_end);
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

        // Inject object rest parameter destructuring preamble for ES2018 lowering.
        // e.g., `function f(_a, b) { var { a } = _a, rest = __rest(_a, ["a"]); ... }`
        if is_function_body_block && !self.pending_object_rest_params.is_empty() {
            let rest_params: Vec<(String, NodeIndex)> =
                std::mem::take(&mut self.pending_object_rest_params);
            for (temp_name, pattern_idx) in &rest_params {
                self.write("var ");
                self.emit_object_rest_var_decl(*pattern_idx, NodeIndex::NONE, Some(temp_name));
                self.write(";");
                self.write_line();
            }
        }

        let hoisted_var_byte_offset = if is_function_body_block {
            Some((self.writer.len(), self.writer.current_line()))
        } else {
            None
        };

        // Block-level using-declaration lowering below ES2025.
        // When a block contains `using`/`await using` declarations, tsc wraps ALL
        // statements in the block inside a single try/catch/finally, not just the
        // using declarations. We detect this here and set up the env + try wrapper.
        let block_using_lowered = if !self.ctx.options.target.supports_es2025() {
            self.block_has_using_declarations(&block.statements)
        } else {
            false
        };
        let prev_block_using_env = self.block_using_env.take();
        let block_using_names: Option<(String, String, String, bool)> = if block_using_lowered {
            let using_async = self.block_has_await_using(&block.statements);
            let (env_name, error_name, result_name) = self.next_disposable_env_names();
            let env_decl_keyword = if self.ctx.target_es5 { "var" } else { "const" };

            // Block-level using: tsc uses `const` for the __addDisposableResource calls
            // inside the try block (no var hoisting needed since the entire try/catch/finally
            // is at the same block scope level).
            self.write(env_decl_keyword);
            self.write(" ");
            self.write(&env_name);
            self.write(" = { stack: [], error: void 0, hasError: false };");
            self.write_line();
            self.write("try {");
            self.write_line();
            self.increase_indent();
            self.block_using_env = Some((env_name.clone(), using_async));
            Some((env_name, error_name, result_name, using_async))
        } else {
            None
        };

        // Pre-collect statement indices so we can look up the next statement's
        // position as an upper bound for trailing comment scanning. Our parser sets
        // stmt_node.end past the statement boundary into the next statement's tokens,
        // so using next_stmt.pos as the scan limit prevents over-scanning.
        let stmts: Vec<NodeIndex> = block.statements.nodes.to_vec();
        for (stmt_i, &stmt_idx) in stmts.iter().enumerate() {
            // Save state before leading comments so we can undo them if the
            // statement produces no output (e.g., namespace alias import or
            // CJS export var with no initializer).
            let pre_comment_writer_len = self.writer.len();
            let pre_comment_idx = self.comment_emit_idx;

            // Emit leading comments before this statement
            if let Some(stmt_node) = self.arena.get(stmt_idx) {
                // When a statement is erased (interface, type alias, declare, etc.),
                // tsc also erases the comments in its leading trivia. Skip both
                // leading and trailing comments so they don't leak to the next
                // non-erased statement.
                if self.is_erased_statement(stmt_node) {
                    let actual_start = self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
                    // Skip leading comments (before the statement's token start)
                    while self.comment_emit_idx < self.all_comments.len() {
                        if self.all_comments[self.comment_emit_idx].end <= actual_start {
                            self.comment_emit_idx += 1;
                        } else {
                            break;
                        }
                    }
                    // Skip trailing same-line comments (on the same line as the erased
                    // statement's last token), matching tsc behavior.
                    let scan_end = stmts
                        .get(stmt_i + 1)
                        .and_then(|&next_idx| self.arena.get(next_idx))
                        .map_or(stmt_node.end, |next_node| next_node.pos);
                    let stmt_token_end = self.find_token_end_before_trivia(stmt_node.pos, scan_end);
                    if let Some(text) = self.source_text {
                        let bytes = text.as_bytes();
                        let mut pos = stmt_token_end as usize;
                        while pos < bytes.len() && bytes[pos] != b'\n' && bytes[pos] != b'\r' {
                            pos += 1;
                        }
                        let line_end = pos as u32;
                        while self.comment_emit_idx < self.all_comments.len() {
                            if self.all_comments[self.comment_emit_idx].end <= line_end {
                                self.comment_emit_idx += 1;
                            } else {
                                break;
                            }
                        }
                    }
                    continue;
                }

                let defer_for_of_comments = stmt_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
                    && self.should_defer_for_of_comments(stmt_node);
                // Skip past ALL trivia (whitespace + comments) to find the
                // actual first token position.  Leading comments whose `c_end`
                // falls before this position are emitted here.
                let actual_start = self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
                if !defer_for_of_comments && let Some(text) = self.source_text {
                    while self.comment_emit_idx < self.all_comments.len() {
                        let c_end = self.all_comments[self.comment_emit_idx].end;
                        // Only emit if the comment ends before the statement starts
                        if c_end <= actual_start {
                            let c_pos = self.all_comments[self.comment_emit_idx].pos;
                            let c_trailing =
                                self.all_comments[self.comment_emit_idx].has_trailing_new_line;
                            if let Ok(comment_text) =
                                crate::safe_slice::slice(text, c_pos as usize, c_end as usize)
                            {
                                self.write_comment_with_reindent(comment_text, Some(c_pos));
                                if c_trailing {
                                    self.write_line();
                                } else if comment_text.starts_with("/*") {
                                    self.pending_block_comment_space = true;
                                }
                            }
                            self.comment_emit_idx += 1;
                        } else {
                            break;
                        }
                    }
                }
            }

            let before_emit_len = self.writer.len();
            self.emit(stmt_idx);
            let emitted_output = self.writer.len() > before_emit_len;
            // Only add newline if something was actually emitted and we're not
            // already at line start (e.g. class with lowered static fields already
            // wrote a trailing newline after the last `ClassName.field = value;`).
            if emitted_output && !self.writer.is_at_line_start() {
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
                    // Cap trailing comment scan so we don't steal comments that
                    // belong to the next statement or the block's closing `}`.
                    // For the last statement, cap at the block's closing brace.
                    // For non-last statements, cap at the next statement's pos
                    // to prevent consuming comments past a same-line boundary
                    // (e.g. `function f() { }; // comment` — the comment belongs
                    // to the `;` statement, not the function declaration).
                    let is_last = stmt_i + 1 >= stmts.len();
                    let max_pos = if is_last {
                        block_close_pos
                    } else {
                        upper_bound
                    };
                    self.emit_trailing_comments_before(token_end, max_pos);
                }
                self.write_line();
            } else if !emitted_output {
                // Statement produced no output (e.g., namespace alias `import a = M`
                // with no runtime value, or CJS export var with no initializer).
                // Undo any leading comments we emitted before it, then consume
                // trailing same-line comments so they don't leak to the next
                // statement's leading comment emission.
                if self.writer.len() > pre_comment_writer_len {
                    self.writer.truncate(pre_comment_writer_len);
                    self.comment_emit_idx = pre_comment_idx;
                }
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    let actual_start = self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
                    // Skip leading comments
                    while self.comment_emit_idx < self.all_comments.len() {
                        if self.all_comments[self.comment_emit_idx].end <= actual_start {
                            self.comment_emit_idx += 1;
                        } else {
                            break;
                        }
                    }
                    // Skip trailing same-line comments
                    let scan_end = stmts
                        .get(stmt_i + 1)
                        .and_then(|&next_idx| self.arena.get(next_idx))
                        .map_or(stmt_node.end, |next_node| next_node.pos);
                    let stmt_token_end = self.find_token_end_before_trivia(stmt_node.pos, scan_end);
                    if let Some(text) = self.source_text {
                        let bytes = text.as_bytes();
                        let mut pos = stmt_token_end as usize;
                        while pos < bytes.len() && bytes[pos] != b'\n' && bytes[pos] != b'\r' {
                            pos += 1;
                        }
                        let line_end = pos as u32;
                        while self.comment_emit_idx < self.all_comments.len() {
                            if self.all_comments[self.comment_emit_idx].end <= line_end {
                                self.comment_emit_idx += 1;
                            } else {
                                break;
                            }
                        }
                    }
                }
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

        // Close the block-level using try/catch/finally if active
        if let Some((env_name, error_name, result_name, using_async)) = block_using_names {
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
                // tsc emits: const result_N = __disposeResources(env_N);
                //            if (result_N) await result_N;
                // (inside __awaiter generator, `await` becomes `yield`)
                let await_kw = if self.ctx.emit_await_as_yield {
                    "yield"
                } else {
                    "await"
                };
                self.write(if self.ctx.target_es5 { "var" } else { "const" });
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
            // Restore previous block_using_env
            self.block_using_env = prev_block_using_env;
        } else {
            self.block_using_env = prev_block_using_env;
        }

        // Emit comments between the last statement and the closing `}`.
        // tsc preserves these comments inside the block at the block's
        // indentation level for both function bodies and control-flow blocks.
        if !self.ctx.options.remove_comments {
            self.emit_comments_before_pos(block_close_pos);
        }
        self.decrease_indent();
        self.map_closing_brace(node);
        self.write_with_end_marker("}");
        self.ctx.block_scope_state.exit_scope();
        // Restore declared_namespace_names for non-function blocks to prevent
        // block-scoped let declarations from leaking to sibling blocks.
        if let Some(prev) = prev_declared_ns {
            self.declared_namespace_names = prev;
        }
        // Trailing comments after the block's closing brace are handled by
        // the calling context (class member loop, statement loop, etc.)
    }

    pub(in crate::emitter) fn emit_function_body_hoisted_temps(&mut self) {
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

    pub(in crate::emitter) fn emit_variable_statement(&mut self, node: &Node) {
        let Some(var_stmt) = self.arena.get_variable(node) else {
            return;
        };
        let deferred_export_bindings = self.deferred_local_export_bindings.clone();

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

        if self.emit_recovered_ambiguous_generic_assertion_variable_statement(node) {
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

        // When `export =` is present, exported variables with no initializers
        // whose names are already in commonjs_exported_var_names are redundant
        // (the preamble emitted `exports.X = void 0;`). Skip the bare `var X;`.
        if self.ctx.is_commonjs()
            && self.ctx.module_state.has_export_assignment
            && self.all_declarations_lack_initializer(&var_stmt.declarations)
            && self.all_declaration_names_in_exported_set(&var_stmt.declarations)
        {
            return;
        }

        // Collect declaration names for export assignment
        let export_names: Vec<String> = if is_exported {
            self.collect_variable_names(&var_stmt.declarations)
        } else {
            Vec::new()
        };

        let is_es_module_export = self.ctx.target_es5
            && matches!(
                self.ctx.options.module,
                ModuleKind::ES2015 | ModuleKind::ESNext
            )
            && self
                .arena
                .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword);
        if is_es_module_export && self.emit_es5_empty_binding_pattern_export(&var_stmt.declarations)
        {
            return;
        }

        // Lower `using`/`await using` declarations below ES2025.
        // When block_using_env is set, the block-level try/catch is already active,
        // so we just emit `const/var x = __addDisposableResource(env, expr, async)`.
        // When not set (standalone), emit the full try/catch per-statement.
        let using_is_lowered = has_using_declaration && !self.ctx.options.target.supports_es2025();
        if using_is_lowered {
            if let Some((ref env_name, using_async)) = self.block_using_env.clone() {
                // Block-level try/catch is active — just emit the __addDisposableResource calls
                for &decl_list_idx in &var_stmt.declarations.nodes {
                    if let Some(decl_list_node) = self.arena.get(decl_list_idx)
                        && let Some(decl_list) = self.arena.get_variable(decl_list_node)
                    {
                        let flags = decl_list_node.flags as u32;
                        if (flags & node_flags::USING) != 0 {
                            self.emit_using_addresource_only(decl_list, env_name, using_async);
                        } else {
                            self.emit(decl_list_idx);
                            self.write_semicolon();
                        }
                    }
                }
            } else {
                // No block-level wrapper — emit full try/catch per-statement (legacy path)
                for &decl_list_idx in &var_stmt.declarations.nodes {
                    if let Some(decl_list_node) = self.arena.get(decl_list_idx)
                        && let Some(decl_list) = self.arena.get_variable(decl_list_node)
                    {
                        let flags = decl_list_node.flags as u32;
                        if (flags & node_flags::USING) != 0 {
                            self.emit_using_declaration_lowered(decl_list, flags);
                        } else {
                            self.emit(decl_list_idx);
                            self.write_semicolon();
                        }
                    }
                }
            }
            return;
        }

        if self.in_system_execute_body
            && self.function_scope_depth == 0
            && !self.in_namespace_iife
            && let Some(bindings) = deferred_export_bindings.as_ref()
            && !bindings.is_empty()
            && !self
                .arena
                .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
        {
            let mut lowered_system_exports = Vec::new();
            let mut can_lower_as_assignments = true;

            for &decl_list_idx in &var_stmt.declarations.nodes {
                let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                    can_lower_as_assignments = false;
                    break;
                };
                let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                    can_lower_as_assignments = false;
                    break;
                };
                for &decl_idx in &decl_list.declarations.nodes {
                    let Some(decl_node) = self.arena.get(decl_idx) else {
                        can_lower_as_assignments = false;
                        break;
                    };
                    let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                        can_lower_as_assignments = false;
                        break;
                    };
                    let Some(name_node) = self.arena.get(decl.name) else {
                        can_lower_as_assignments = false;
                        break;
                    };
                    if name_node.kind != SyntaxKind::Identifier as u16 || decl.initializer.is_none()
                    {
                        can_lower_as_assignments = false;
                        break;
                    }
                    let local_name = self.get_identifier_text_idx(decl.name);
                    let Some(export_name) = bindings.get(&local_name).cloned() else {
                        can_lower_as_assignments = false;
                        break;
                    };
                    lowered_system_exports.push((local_name, export_name, decl.initializer));
                }
                if !can_lower_as_assignments {
                    break;
                }
            }

            if can_lower_as_assignments && !lowered_system_exports.is_empty() {
                let mut first = true;
                for (local_name, export_name, init_idx) in lowered_system_exports {
                    if !first {
                        self.write_line();
                    }
                    self.write(&local_name);
                    self.write(" = ");
                    self.emit(init_idx);
                    self.write(";");
                    self.write_line();
                    self.write_export_binding_start(&export_name);
                    self.write(&local_name);
                    self.write_export_binding_end();
                    self.system_folded_export_names.insert(local_name);
                    first = false;
                }
                return;
            }
        }

        // VariableStatement.declarations contains a VARIABLE_DECLARATION_LIST
        // Emit the declaration list (which handles the let/const/var keyword)
        for &decl_list_idx in &var_stmt.declarations.nodes {
            self.emit(decl_list_idx);
        }
        let recovered_async_arrow_return = self.recovered_async_arrow_return_name(node);
        let recovered_bare_arrow_return = self.recovered_bare_arrow_return_name(node);
        let recovered_arrow_return = recovered_async_arrow_return
            .as_ref()
            .or(recovered_bare_arrow_return.as_ref());
        if !using_is_lowered {
            if let Some(return_name) = recovered_arrow_return {
                self.write(", ");
                self.write(return_name);
            }
            if let Some(last_end) =
                self.variable_statement_last_emitted_declaration_end(&var_stmt.declarations)
            {
                let effective_end = self.variable_statement_effective_end(&var_stmt.declarations);
                if let Some(semi_after) =
                    self.find_declaration_semicolon_after(last_end, effective_end)
                {
                    let comment_end = semi_after.saturating_sub(1);
                    self.emit_comments_in_range(last_end, comment_end, true, false);
                }
            }
            self.map_trailing_semicolon(node);
            self.write_semicolon();
        }

        // Emit trailing comments (e.g., var x = 1; // comment).
        // Use a bounded scan range that excludes erased type annotations.
        // For `var v: { (...); // comment }`, the backward `;` scan must
        // not find semicolons inside the erased type annotation.
        let effective_end = self.variable_statement_effective_end(&var_stmt.declarations);
        self.emit_trailing_comment_after_semicolon_in_range(node.pos, effective_end);
        self.emit_recovered_malformed_arrow_block_after_variable_statement(
            node,
            recovered_async_arrow_return.is_some(),
        );
        self.emit_recovered_typeof_member_call_after_variable_statement(node);

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

        if !is_exported
            && self.function_scope_depth == 0
            && !self.in_namespace_iife
            && let Some(bindings) = deferred_export_bindings.as_ref()
            && !bindings.is_empty()
        {
            let mut deferred_names = Vec::new();
            for &decl_list_idx in &var_stmt.declarations.nodes {
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
                    if decl.initializer.is_none() {
                        continue;
                    }
                    let Some(name_node) = self.arena.get(decl.name) else {
                        continue;
                    };
                    let Some(ident) = self.arena.get_identifier(name_node) else {
                        continue;
                    };
                    deferred_names.push(ident.escaped_text.clone());
                }
            }

            for local_name in deferred_names {
                let Some(export_name) = bindings.get(&local_name) else {
                    continue;
                };
                self.write_line();
                self.write_export_binding_start(export_name);
                self.write(&local_name);
                self.write_export_binding_end();
                if self.in_system_execute_body {
                    self.system_folded_export_names.insert(local_name);
                }
            }
        }
    }

    fn emit_recovered_malformed_arrow_block_after_variable_statement(
        &mut self,
        node: &Node,
        recovered_async_arrow_return: bool,
    ) {
        let Some(text) = self.source_text else {
            return;
        };
        let bytes = text.as_bytes();
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        if start >= bytes.len() {
            return;
        }

        let mut line_end = start;
        while line_end < bytes.len() && bytes[line_end] != b'\n' && bytes[line_end] != b'\r' {
            line_end += 1;
        }

        let Ok(line) = std::str::from_utf8(&bytes[start..line_end]) else {
            return;
        };

        if line.contains("= @") && line.contains("=>") {
            let Some(arrow_rel) = line.find("=>") else {
                return;
            };
            let after_arrow = start + arrow_rel + 2;
            let Some(open_rel) = bytes[after_arrow..line_end].iter().position(|&b| b == b'{')
            else {
                return;
            };
            let open = after_arrow + open_rel;
            let mut pos = open + 1;
            while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            if bytes.get(pos) == Some(&b'}') {
                self.write_line();
                self.write("{");
                self.write_line();
                self.write("}");
            }
            return;
        }

        if recovered_async_arrow_return {
            self.write_line();
            self.write_semicolon();
            self.write_line();
            self.write("{");
            self.write_line();
            self.write("}");
            return;
        }

        let Some(arrow_rel) = line.find("): =>").or_else(|| line.find("):=>")) else {
            return;
        };

        // Parser recovery for `var v = (a): => { }` ends the variable statement
        // before the recovered empty block. TSC still emits that block as a
        // separate statement after the `var`.
        let after_arrow = start + arrow_rel + line[arrow_rel..].find("=>").unwrap_or(0) + 2;
        let Some(open_rel) = bytes[after_arrow..line_end].iter().position(|&b| b == b'{') else {
            return;
        };
        let open = after_arrow + open_rel;
        let mut pos = open + 1;
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if bytes.get(pos) != Some(&b'}') {
            return;
        }

        self.write_line();
        self.write("{");
        self.write_line();
        self.write("}");
        self.write_line();
        self.write_semicolon();
    }

    fn emit_recovered_typeof_member_call_after_variable_statement(&mut self, node: &Node) {
        let Some(text) = self.source_text else {
            return;
        };
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        let end = std::cmp::min(node.end as usize, text.len());
        if start >= end {
            return;
        }
        let segment = &text[start..end];
        let Some(typeof_rel) = segment.find(".typeof(") else {
            return;
        };
        let open = start + typeof_rel + ".typeof".len();
        let Some(close) = self.find_matching_source_paren(open, end) else {
            return;
        };
        let argument = text[open + 1..close].trim();
        if argument.is_empty() {
            return;
        }

        self.write_line();
        self.write("typeof (");
        self.write(argument);
        self.write(");");
    }

    fn find_matching_source_paren(&self, open: usize, limit: usize) -> Option<usize> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        if bytes.get(open) != Some(&b'(') {
            return None;
        }

        let mut depth = 1u32;
        let mut i = open + 1;
        while i < limit && i < bytes.len() {
            match bytes[i] {
                b'\'' | b'"' | b'`' => {
                    i = self.skip_quoted_source_text(i, limit);
                    continue;
                }
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    fn skip_quoted_source_text(&self, quote_start: usize, limit: usize) -> usize {
        let Some(text) = self.source_text else {
            return quote_start + 1;
        };
        let bytes = text.as_bytes();
        let quote = bytes[quote_start];
        let mut i = quote_start + 1;
        while i < limit && i < bytes.len() {
            if bytes[i] == b'\\' {
                i = (i + 2).min(limit);
                continue;
            }
            if bytes[i] == quote {
                return i + 1;
            }
            i += 1;
        }
        i
    }

    fn recovered_async_arrow_return_name(&self, node: &Node) -> Option<String> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        if start >= bytes.len() {
            return None;
        }

        let mut line_end = start;
        while line_end < bytes.len() && bytes[line_end] != b'\n' && bytes[line_end] != b'\r' {
            line_end += 1;
        }

        let line = std::str::from_utf8(&bytes[start..line_end]).ok()?;
        if !line.contains("async") || !line.contains("= await =>") {
            return None;
        }

        let colon = line.find("):")? + 2;
        let arrow = line[colon..].find("=>")? + colon;
        let return_type = line[colon..arrow].trim();
        let name: String = return_type
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
            .collect();
        if name.is_empty() { None } else { Some(name) }
    }

    fn recovered_bare_arrow_return_name(&self, node: &Node) -> Option<String> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        if start >= bytes.len() {
            return None;
        }

        let mut line_end = start;
        while line_end < bytes.len() && bytes[line_end] != b'\n' && bytes[line_end] != b'\r' {
            line_end += 1;
        }

        let line = std::str::from_utf8(&bytes[start..line_end]).ok()?;
        let equals = line.find('=')?;
        let arrow = line[equals..].find("=>")? + equals;
        let colon = line[equals..arrow].rfind(':')? + equals;
        let arrow_head = line[equals + 1..colon].trim();
        if arrow_head.is_empty()
            || !arrow_head
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
        {
            return None;
        }

        let return_type = line[colon + 1..arrow].trim();
        let name: String = return_type
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
            .collect();
        if name.is_empty() { None } else { Some(name) }
    }

    fn emit_recovered_ambiguous_generic_assertion_variable_statement(
        &mut self,
        node: &Node,
    ) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let bytes = text.as_bytes();
        let start = self.skip_trivia_forward(node.pos, node.end) as usize;
        if start >= bytes.len() {
            return false;
        }

        let mut line_end = start;
        while line_end < bytes.len() && bytes[line_end] != b'\n' && bytes[line_end] != b'\r' {
            line_end += 1;
        }

        let Ok(line) = std::str::from_utf8(&bytes[start..line_end]) else {
            return false;
        };
        let Some((head, recovered)) = Self::recovered_ambiguous_generic_assertion_parts(line)
        else {
            return false;
        };

        self.write(&head);
        self.write_line();
        self.write(&recovered);
        true
    }

    fn recovered_ambiguous_generic_assertion_parts(line: &str) -> Option<(String, String)> {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("var ") || !trimmed.contains("= <<") {
            return None;
        }

        let eq = line.find('=')?;
        let before_eq = line[..eq].trim_end();
        let mut rest = line[eq + 1..].trim_start();
        if !rest.starts_with("<<") {
            return None;
        }
        rest = &rest[2..];

        let type_param_end = rest.find('>')?;
        let type_param = rest[..type_param_end].trim();
        rest = rest[type_param_end + 1..].trim_start();
        if !rest.starts_with('(') {
            return None;
        }
        rest = &rest[1..];

        let param_end = rest.find(')')?;
        let param = rest[..param_end].split(':').next()?.trim();
        rest = rest[param_end + 1..].trim_start();
        if !rest.starts_with("=>") {
            return None;
        }
        rest = rest[2..].trim_start();

        let return_type_end = rest.find('>')?;
        let return_type = rest[..return_type_end].trim();
        rest = rest[return_type_end + 1..].trim_start();

        let (callee, trailing_comment) = if let Some(comment_start) = rest.find("//") {
            (
                rest[..comment_start].trim().trim_end_matches(';').trim(),
                Some(rest[comment_start..].trim()),
            )
        } else {
            (rest.trim().trim_end_matches(';').trim(), None)
        };
        if type_param.is_empty() || param.is_empty() || return_type.is_empty() || callee.is_empty()
        {
            return None;
        }

        let head = format!("{before_eq} =  << {type_param} > ({param}), {return_type};");
        let recovered = if let Some(comment) = trailing_comment {
            format!("{return_type} > {callee}; {comment}")
        } else {
            format!("{return_type} > {callee};")
        };
        Some((head, recovered))
    }

    /// Lower `using`/`await using` declarations for non-ES5 targets (ES2015+).
    /// Transforms:
    ///   `using d = expr;`
    /// Into:
    ///   `var d;`
    ///   `const env_1 = { stack: [], error: void 0, hasError: false };`
    ///   `try { d = __addDisposableResource(env_1, expr, false); }`
    ///   `catch (e_1) { env_1.error = e_1; env_1.hasError = true; }`
    ///   `finally { __disposeResources(env_1); }`
    fn emit_using_declaration_lowered(
        &mut self,
        decl_list: &tsz_parser::parser::node::VariableData,
        flags: u32,
    ) {
        let using_async = node_flags::is_await_using(flags);
        let (env_name, error_name, result_name) = self.next_disposable_env_names();

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

        // Hoist `var` declarations before the try block — variables must remain
        // accessible after the try/catch/finally completes.
        if !initialized_decls.is_empty() {
            let mut var_names = Vec::new();
            for &decl_idx in &initialized_decls {
                if let Some(decl_node) = self.arena.get(decl_idx)
                    && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                {
                    self.collect_binding_names(decl.name, &mut var_names);
                }
            }
            if !var_names.is_empty() {
                self.write("var ");
                self.write(&var_names.join(", "));
                self.write(";");
                self.write_line();
            }
        }

        self.write("const ");
        self.write(&env_name);
        self.write(" = { stack: [], error: void 0, hasError: false };");
        self.write_line();

        self.write("try {");
        self.write_line();
        self.increase_indent();

        // Emit assignments (no `const`/`let` prefix — vars are hoisted above)
        if !initialized_decls.is_empty() {
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
            // tsc emits: const result_N = __disposeResources(env_N);
            //            if (result_N) await result_N;
            // (inside __awaiter generator, `await` becomes `yield`)
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
                    effective_end = self
                        .find_declaration_semicolon_after(name_node.end, decl_node.end)
                        .unwrap_or(name_node.end);
                }
            }
        }
        effective_end
    }

    fn variable_statement_last_emitted_declaration_end(
        &self,
        declarations: &NodeList,
    ) -> Option<u32> {
        let mut last_end = None;
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
                if let Some(init_node) = self.arena.get(decl.initializer) {
                    last_end = Some(init_node.end);
                } else if let Some(name_node) = self.arena.get(decl.name) {
                    last_end = Some(name_node.end);
                }
            }
        }
        last_end
    }

    fn find_declaration_semicolon_after(&self, start: u32, end: u32) -> Option<u32> {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let mut i = std::cmp::min(start as usize, bytes.len());
        let limit = std::cmp::min(end as usize, bytes.len());
        let mut depth = 0i32;
        while i < limit {
            match bytes[i] {
                b'{' | b'(' | b'[' | b'<' => {
                    depth += 1;
                    i += 1;
                }
                b'}' | b')' | b']' | b'>' => {
                    depth -= 1;
                    i += 1;
                }
                b';' if depth == 0 => return Some((i + 1) as u32),
                b'/' if i + 1 < limit && bytes[i + 1] == b'/' => {
                    while i < limit && bytes[i] != b'\n' && bytes[i] != b'\r' {
                        i += 1;
                    }
                }
                b'/' if i + 1 < limit && bytes[i + 1] == b'*' => {
                    i += 2;
                    while i + 1 < limit && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                        i += 1;
                    }
                    i = std::cmp::min(i + 2, limit);
                }
                b'\'' | b'"' | b'`' => {
                    let quote = bytes[i];
                    i += 1;
                    while i < limit {
                        if bytes[i] == b'\\' {
                            i = std::cmp::min(i + 2, limit);
                        } else if bytes[i] == quote {
                            i += 1;
                            break;
                        } else {
                            i += 1;
                        }
                    }
                }
                _ => i += 1,
            }
        }
        None
    }

    /// Check if all variable declarations in a declaration list lack initializers
    pub(in crate::emitter) fn all_declarations_lack_initializer(
        &self,
        declarations: &NodeList,
    ) -> bool {
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

    /// Check if all declared names in a variable declaration list are present
    /// in the `commonjs_exported_var_names` set (already handled by the CJS
    /// preamble `exports.X = void 0;`).
    pub(in crate::emitter) fn all_declaration_names_in_exported_set(
        &self,
        declarations: &NodeList,
    ) -> bool {
        let names = self.collect_variable_names(declarations);
        !names.is_empty()
            && names
                .iter()
                .all(|n| self.commonjs_exported_var_names.contains(n))
    }

    /// Collect variable names from a declaration list for `CommonJS` export
    pub(in crate::emitter) fn collect_variable_names(
        &self,
        declarations: &NodeList,
    ) -> Vec<String> {
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

    fn emit_es5_empty_binding_pattern_export(&mut self, declarations: &NodeList) -> bool {
        let mut initializers = Vec::new();

        for &decl_list_idx in &declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                return false;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                return false;
            };

            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    return false;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    return false;
                };
                if !self.binding_pattern_is_empty(decl.name) || decl.initializer.is_none() {
                    return false;
                }
                initializers.push(decl.initializer);
            }
        }

        if initializers.is_empty() {
            return false;
        }

        for (index, initializer) in initializers.iter().copied().enumerate() {
            if index > 0 {
                self.write_line();
            }
            let source_temp = self.make_unique_name_hoisted();
            let export_temp = self.make_unique_name();
            self.write("export var ");
            self.write(&export_temp);
            self.write(" = ");
            self.write(&source_temp);
            self.write(" = ");
            self.emit(initializer);
            self.write_semicolon();
        }
        true
    }

    pub(in crate::emitter) fn collect_binding_names(
        &self,
        name_idx: NodeIndex,
        names: &mut Vec<String>,
    ) {
        if name_idx.is_none() {
            return;
        }

        let Some(node) = self.arena.get(name_idx) else {
            return;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            if let Some(id) = self.arena.get_identifier(node) {
                // Use original_text (preserving unicode escapes) when available,
                // falling back to escaped_text. TSC preserves unicode escapes
                // in CJS export assignments (exports.\u0078 = \u0078;).
                let text = id
                    .original_text
                    .as_deref()
                    .unwrap_or(&id.escaped_text)
                    .to_string();
                names.push(text);
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

    pub(in crate::emitter) fn collect_binding_names_from_element(
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

    pub(in crate::emitter) fn emit_expression_statement(&mut self, node: &Node) {
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

        if self.emit_invalid_prefix_await_expression_statement(node, expr_stmt.expression) {
            return;
        }

        // When a function/object expression appears at the start of a statement, it needs
        // wrapping parentheses: `function` would be parsed as a declaration, and `{` as a
        // block. We use a leftmost-expression walker that follows the left chain through
        // call/property-access/element-access and unwraps type assertions (which are erased
        // in JS output) to find the actual leading token.
        // e.g., `<unknown>function() {}();` → `(function () { })();`
        // e.g., `<unknown>{foo() {}}.foo();` → `({ foo() { } }.foo());`
        //
        // EXCEPTION: when the outer expression is itself a ParenthesizedExpression that
        // will survive emit (e.g., `(<any>{a:0})`), its own surviving parens already
        // disambiguate the leading `{`/`function` token. Adding another pair here would
        // produce double parens like `(({a:0}))`. Skip the wrapping in that case —
        // `emit_parenthesized` will print `({a:0})`.
        let needs_parens = if let Some(expr_node) = self.arena.get(expr_stmt.expression) {
            let leftmost = self
                .leftmost_expression_kind_after_erasure(expr_stmt.expression)
                .unwrap_or(expr_node.kind);
            let leftmost_needs_parens = leftmost == syntax_kind_ext::FUNCTION_EXPRESSION
                || leftmost == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || (self.ctx.target_es5 && expr_node.kind == syntax_kind_ext::ARROW_FUNCTION);
            leftmost_needs_parens && !self.outer_paren_will_survive_emit(expr_stmt.expression)
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

        let prev_stmt_expr = self.ctx.flags.in_statement_expression;
        self.ctx.flags.in_statement_expression = true;
        if needs_parens {
            // TSC special case: when the expression (after type erasure) is a
            // CallExpression whose direct callee is a function/object expression,
            // wrap only the callee — producing `(function(){})()` instead of
            // `(function(){}())`.
            if self.is_call_with_function_or_object_callee(expr_stmt.expression) {
                self.ctx.flags.paren_leftmost_function_or_object = true;
                self.emit(expr_stmt.expression);
                self.ctx.flags.paren_leftmost_function_or_object = false;
            } else {
                self.write("(");
                self.emit(expr_stmt.expression);
                self.write(")");
            }
        } else if self.emit_import_type_arguments_statement_expression(expr_stmt.expression) {
            // Handled above: `import<T>;` erases to `import;`, while the same
            // expression in value position still uses the generic
            // ExpressionWithTypeArguments paren path.
        } else {
            self.emit(expr_stmt.expression);
        }
        self.ctx.flags.in_statement_expression = prev_stmt_expr;
        self.map_trailing_semicolon(node);
        if !self.output_ends_with_semicolon() {
            self.write_semicolon();
        }
        self.emit_trailing_comment_after_semicolon(node);
    }

    fn emit_import_type_arguments_statement_expression(&mut self, expression: NodeIndex) -> bool {
        let Some(expr_node) = self.arena.get(expression) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS {
            return false;
        }
        let Some(data) = self.arena.get_expr_type_args(expr_node) else {
            return false;
        };
        let Some(inner) = self.arena.get(data.expression) else {
            return false;
        };
        if inner.kind != SyntaxKind::ImportKeyword as u16 {
            return false;
        }

        self.emit(data.expression);
        if !self.ctx.options.remove_comments
            && let Some(type_arguments) = data.type_arguments.as_ref()
        {
            for ta_idx in &type_arguments.nodes {
                if let Some(ta_node) = self.arena.get(*ta_idx) {
                    self.skip_comments_in_range(ta_node.pos, ta_node.end);
                }
            }
        }
        true
    }

    fn emit_invalid_prefix_await_expression_statement(
        &mut self,
        statement: &Node,
        expression: NodeIndex,
    ) -> bool {
        let Some(expr_node) = self.arena.get(expression) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            return false;
        }
        let Some(unary) = self.arena.get_unary_expr(expr_node) else {
            return false;
        };
        if unary.operator != SyntaxKind::PlusPlusToken as u16
            && unary.operator != SyntaxKind::MinusMinusToken as u16
        {
            return false;
        }
        let Some(operand_node) = self.arena.get(unary.operand) else {
            return false;
        };
        if operand_node.kind != syntax_kind_ext::AWAIT_EXPRESSION {
            return false;
        }

        self.write(super::super::get_operator_text(unary.operator));
        self.write_semicolon();
        self.write_line();

        let prev_stmt_expr = self.ctx.flags.in_statement_expression;
        self.ctx.flags.in_statement_expression = true;
        self.emit(unary.operand);
        self.ctx.flags.in_statement_expression = prev_stmt_expr;

        self.map_trailing_semicolon(statement);
        self.write_semicolon();
        self.emit_trailing_comment_after_semicolon(statement);
        true
    }

    /// Check if an expression (after skipping type assertions) is a `CallExpression`
    /// whose direct callee (after skipping type assertions) is a `FunctionExpression`
    /// or `ObjectLiteralExpression`. Used for TSC-style IIFE parenthesization.
    fn is_call_with_function_or_object_callee(&self, mut idx: NodeIndex) -> bool {
        // Skip type assertions
        loop {
            let Some(node) = self.arena.get(idx) else {
                return false;
            };
            match node.kind {
                k if k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
                {
                    if let Some(ta) = self.arena.get_type_assertion(node) {
                        idx = ta.expression;
                    } else {
                        return false;
                    }
                }
                _ => break,
            }
        }
        // Check if it's a CallExpression
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }
        let Some(call) = self.arena.get_call_expr(node) else {
            return false;
        };
        // Skip type assertions on the callee
        let mut callee_idx = call.expression;
        loop {
            let Some(callee_node) = self.arena.get(callee_idx) else {
                return false;
            };
            match callee_node.kind {
                k if k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
                {
                    if let Some(ta) = self.arena.get_type_assertion(callee_node) {
                        callee_idx = ta.expression;
                    } else {
                        return false;
                    }
                }
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    if let Some(paren) = self.arena.get_parenthesized(callee_node)
                        && let Some(inner) = self.arena.get(paren.expression)
                        && (inner.kind == syntax_kind_ext::TYPE_ASSERTION
                            || inner.kind == syntax_kind_ext::AS_EXPRESSION
                            || inner.kind == syntax_kind_ext::SATISFIES_EXPRESSION
                            || inner.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS)
                    {
                        callee_idx = paren.expression;
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
        let Some(callee_node) = self.arena.get(callee_idx) else {
            return false;
        };
        callee_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || callee_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
    }

    /// Returns `true` when the given expression node is a `ParenthesizedExpression`
    /// whose outer `(...)` will survive emit — that is, the inner expression is a
    /// type assertion whose unwrapped target is *not* in the can-strip set used by
    /// `emit_parenthesized`.
    ///
    /// Used by `emit_expression_statement` to avoid double-wrapping when the source
    /// already has parens that disambiguate the leading `{` / `function` token:
    /// `(<any>{a:0});` should emit `({ a: 0 });`, not `(({ a: 0 }));`.
    ///
    /// The check is intentionally conservative: it only returns `true` for the
    /// specific shape `(<TypeAssertion or as/satisfies>{ObjectLiteral|FunctionExpression|...})`
    /// where the surviving paren wraps a leading-token-ambiguous primary. Other
    /// `ParenthesizedExpression`s (e.g., wrapping an assignment, comma, or arrow)
    /// are not considered, because their wrapping behavior is different and
    /// already covered by other rules.
    fn outer_paren_will_survive_emit(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
            return false;
        }
        let Some(paren) = self.arena.get_parenthesized(node) else {
            return false;
        };
        let Some(inner) = self.arena.get(paren.expression) else {
            return false;
        };
        // Only handle the type-assertion-erasure shape: `(<T>x)` / `(x as T)` /
        // `(x satisfies T)`. Without an erased assertion, the outer paren is
        // either redundant in source or already handled by other rules.
        let is_type_erasure = inner.kind == syntax_kind_ext::TYPE_ASSERTION
            || inner.kind == syntax_kind_ext::AS_EXPRESSION
            || inner.kind == syntax_kind_ext::SATISFIES_EXPRESSION
            || inner.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS;
        if !is_type_erasure {
            return false;
        }
        let unwrapped = self.unwrap_type_assertion_kind(paren.expression);
        // Mirror the `can_strip` set in `emit_parenthesized`. If the unwrapped kind
        // is NOT strippable, the outer paren survives emit and provides leading-
        // token disambiguation, so the statement-level wrap is redundant.
        let can_strip = matches!(
            unwrapped,
            Some(k) if k == SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                || k == SyntaxKind::ThisKeyword as u16
                || k == SyntaxKind::SuperKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::RegularExpressionLiteral as u16
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::NON_NULL_EXPRESSION
                || k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                || k == syntax_kind_ext::CALL_EXPRESSION
                || k == syntax_kind_ext::NEW_EXPRESSION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::CLASS_EXPRESSION
        );
        !can_strip
    }

    /// Emit trailing comments after a semicolon. Scans backward through the
    /// entire node range to find the semicolon, allowing it to work even when
    /// node.end is past the newline (at the start of the next statement).
    pub(in crate::emitter) fn emit_trailing_comment_after_semicolon(&mut self, node: &Node) {
        self.emit_trailing_comment_after_semicolon_in_range(node.pos, node.end);
    }

    /// Like `emit_trailing_comment_after_semicolon` but with an explicit scan range.
    /// Use this when the node's full range includes erased content (e.g., type
    /// annotations with semicolons inside) that should not be scanned.
    pub(in crate::emitter) fn emit_trailing_comment_after_semicolon_in_range(
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

        // Scan forwards and keep the last outermost semicolon within this node's range.
        // This still ignores semicolons nested inside blocks/object literals, but it
        // does not get confused when node.end extends onto later `}` lines after the
        // statement's own trailing comment (e.g. `break; // done` inside `switch`).
        let mut semi_pos = None;
        let mut depth: i32 = 0;
        let mut i = stmt_start;
        while i < stmt_end {
            match bytes[i] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                b';' if depth == 0 => {
                    semi_pos = Some(i + 1);
                }
                _ => {}
            }
            i += 1;
        }

        if let Some(pos) = semi_pos {
            let comments = get_trailing_comment_ranges(text, pos);
            for comment in comments {
                self.write_space();
                if let Ok(comment_text) =
                    safe_slice::slice(text, comment.pos as usize, comment.end as usize)
                    && !comment_text.is_empty()
                {
                    self.write_comment_with_reindent(comment_text, Some(comment.pos));
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
}
