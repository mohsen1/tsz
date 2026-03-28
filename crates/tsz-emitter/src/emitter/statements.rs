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

        // Compute the block's closing `}` position so the last statement's
        // trailing comment scan doesn't overshoot into comments belonging to
        // the closing brace line (same pattern as namespace IIFE emitter).
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

        // Block-level using-declaration lowering for non-ES5 targets below ES2025.
        // When a block contains `using`/`await using` declarations, tsc wraps ALL
        // statements in the block inside a single try/catch/finally, not just the
        // using declarations. We detect this here and set up the env + try wrapper.
        let block_using_lowered =
            if !self.ctx.target_es5 && !self.ctx.options.target.supports_es2025() {
                self.block_has_using_declarations(&block.statements)
            } else {
                false
            };
        let prev_block_using_env = self.block_using_env.take();
        let block_using_names: Option<(String, String, String, bool)> = if block_using_lowered {
            let using_async = self.block_has_await_using(&block.statements);
            let (env_name, error_name, result_name) = self.next_disposable_env_names();

            // Block-level using: tsc uses `const` for the __addDisposableResource calls
            // inside the try block (no var hoisting needed since the entire try/catch/finally
            // is at the same block scope level).
            self.write("const ");
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
                            let comment_text =
                                crate::safe_slice::slice(text, c_pos as usize, c_end as usize);
                            self.write_comment_with_reindent(comment_text, Some(c_pos));
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

        // Lower `using`/`await using` declarations for non-ES5 targets below ES2025.
        // When block_using_env is set, the block-level try/catch is already active,
        // so we just emit `const x = __addDisposableResource(env, expr, async)`.
        // When not set (standalone), emit the full try/catch per-statement.
        let using_is_lowered = has_using_declaration && !self.ctx.options.target.supports_es2025();
        if using_is_lowered && !self.ctx.target_es5 {
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

        // VariableStatement.declarations contains a VARIABLE_DECLARATION_LIST
        // Emit the declaration list (which handles the let/const/var keyword)
        for &decl_list_idx in &var_stmt.declarations.nodes {
            self.emit(decl_list_idx);
        }
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
        let using_async = (flags & node_flags::AWAIT_USING) == node_flags::AWAIT_USING;
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

        // When a function/object expression appears at the start of a statement, it needs
        // wrapping parentheses: `function` would be parsed as a declaration, and `{` as a
        // block. We use a leftmost-expression walker that follows the left chain through
        // call/property-access/element-access and unwraps type assertions (which are erased
        // in JS output) to find the actual leading token.
        // e.g., `<unknown>function() {}();` → `(function () { })();`
        // e.g., `<unknown>{foo() {}}.foo();` → `({ foo() { } }.foo());`
        let needs_parens = if let Some(expr_node) = self.arena.get(expr_stmt.expression) {
            let leftmost = self
                .leftmost_expression_kind_after_erasure(expr_stmt.expression)
                .unwrap_or(expr_node.kind);
            leftmost == syntax_kind_ext::FUNCTION_EXPRESSION
                || leftmost == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
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
        } else {
            self.emit(expr_stmt.expression);
        }
        self.ctx.flags.in_statement_expression = prev_stmt_expr;
        self.map_trailing_semicolon(node);
        self.write_semicolon();
        self.emit_trailing_comment_after_semicolon(node);
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
                _ => break,
            }
        }
        let Some(callee_node) = self.arena.get(callee_idx) else {
            return false;
        };
        callee_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || callee_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
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
                let comment_text =
                    safe_slice::slice(text, comment.pos as usize, comment.end as usize);
                if !comment_text.is_empty() {
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

    pub(super) fn emit_while_statement(&mut self, node: &Node) {
        let Some(loop_stmt) = self.arena.get_loop(node) else {
            return;
        };

        // ES5: Check if closures capture body-scoped let/const variables
        if self.ctx.target_es5 {
            let body_info =
                super::es5::loop_capture::collect_loop_body_vars(self.arena, loop_stmt.statement);
            if !body_info.block_scoped_vars.is_empty()
                && let Some(capture_info) = super::es5::loop_capture::check_loop_needs_capture(
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

    pub(super) fn emit_for_statement(&mut self, node: &Node) {
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
            let body_info =
                super::es5::loop_capture::collect_loop_body_vars(self.arena, loop_stmt.statement);
            if (!init_vars.is_empty() || !body_info.block_scoped_vars.is_empty())
                && let Some(capture_info) = super::es5::loop_capture::check_loop_needs_capture(
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
            let prev_stmt = self.ctx.flags.in_statement_expression;
            self.ctx.flags.in_statement_expression = true;
            self.emit(loop_stmt.incrementor);
            self.ctx.flags.in_statement_expression = prev_stmt;
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
            // Check if catch variable has object rest that needs ES2018 lowering.
            let needs_rest_lowering = self.ctx.needs_es2018_lowering
                && !self.ctx.target_es5
                && self.catch_var_has_object_rest(catch.variable_declaration);

            if needs_rest_lowering
                && let Some(pattern_idx) = self.catch_var_pattern_idx(catch.variable_declaration)
            {
                let temp = self.get_temp_var_name();
                self.write(" ");
                self.map_token_after(node.pos, node.end, b'(');
                self.write("(");
                self.write(&temp);
                self.write(")");

                // Emit the block with preamble injected
                self.write(" {");
                self.write_line();
                self.increase_indent();

                // Emit rest preamble
                self.write("var ");
                self.emit_object_rest_var_decl(pattern_idx, NodeIndex::NONE, Some(&temp));
                self.write(";");
                self.write_line();

                // Emit the original block body
                if let Some(block_node) = self.arena.get(catch.block)
                    && let Some(block) = self.arena.get_block(block_node)
                {
                    for &stmt in &block.statements.nodes {
                        self.emit(stmt);
                        self.write_line();
                    }
                }

                self.decrease_indent();
                self.write("}");
                return;
            }

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

    /// Check if a catch clause variable declaration has an object rest pattern.
    fn catch_var_has_object_rest(&self, var_decl_idx: NodeIndex) -> bool {
        let Some(var_node) = self.arena.get(var_decl_idx) else {
            return false;
        };
        let Some(var_decl) = self.arena.get_variable_declaration(var_node) else {
            return false;
        };
        self.pattern_has_object_rest(var_decl.name)
    }

    /// Get the binding pattern index from a catch clause variable declaration.
    fn catch_var_pattern_idx(&self, var_decl_idx: NodeIndex) -> Option<NodeIndex> {
        let var_node = self.arena.get(var_decl_idx)?;
        let var_decl = self.arena.get_variable_declaration(var_node)?;
        Some(var_decl.name)
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

    pub(super) fn emit_default_clause(&mut self, node: &Node) {
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

    pub(super) fn emit_break_statement(&mut self, node: &Node) {
        self.write("break");
        if let Some(jump) = self.arena.get_jump_data(node)
            && jump.label.is_some()
        {
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

    pub(super) fn emit_continue_statement(&mut self, node: &Node) {
        self.write("continue");
        if let Some(jump) = self.arena.get_jump_data(node)
            && jump.label.is_some()
        {
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
    fn find_semicolon_pos_in_range(&self, start: u32, end: u32) -> Option<u32> {
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

    pub(super) fn emit_labeled_statement(&mut self, node: &Node) {
        let Some(labeled) = self.arena.get_labeled_statement(node) else {
            return;
        };

        self.emit(labeled.label);
        self.write(": ");
        let before = self.writer.len();
        self.emit(labeled.statement);
        // If the labeled body was completely erased (e.g. const enum, interface),
        // emit `;` to produce a valid empty statement.
        if self.writer.len() == before {
            self.write(";");
        }
    }

    pub(super) fn emit_do_statement(&mut self, node: &Node) {
        let Some(loop_stmt) = self.arena.get_loop(node) else {
            return;
        };

        // ES5: Check if closures capture body-scoped let/const variables
        if self.ctx.target_es5 {
            let body_info =
                super::es5::loop_capture::collect_loop_body_vars(self.arena, loop_stmt.statement);
            if !body_info.block_scoped_vars.is_empty()
                && let Some(capture_info) = super::es5::loop_capture::check_loop_needs_capture(
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
        let using_async = (flags & node_flags::AWAIT_USING) == node_flags::AWAIT_USING;
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
        let using_async = (flags & node_flags::AWAIT_USING) == node_flags::AWAIT_USING;
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
    fn block_has_using_declarations(&self, statements: &NodeList) -> bool {
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
    fn block_has_await_using(&self, statements: &NodeList) -> bool {
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
    fn emit_using_addresource_only(
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

        // Block-level using: tsc emits `const d1 = __addDisposableResource(env, expr, false)`
        // inside the try block. The `const` is kept because the try block is at the same
        // block scope level as the using declaration.
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
}

#[cfg(test)]
#[path = "../../tests/statements.rs"]
mod tests;
