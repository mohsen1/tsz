//! Special Expression Emission Module
//!
//! This module handles emission of special expressions like yield, await, spread,
//! and decorators that don't fit neatly into other categories.

use super::Printer;
use tracing::warn;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    // =========================================================================
    // Yield and Await
    // =========================================================================

    /// Emit a yield expression: yield or yield* or yield value
    pub(super) fn emit_yield_expression(&mut self, node: &Node) {
        // YieldExpression is stored with UnaryExprDataEx (expression + asterisk_token)
        let Some(unary) = self.arena.get_unary_expr_ex(node) else {
            self.write("yield");
            return;
        };

        // Emit comments that occur before the `yield` keyword itself.
        // This preserves cases like `/*comment*/ yield 1`.
        let keyword_pos = self.skip_trivia_forward(node.pos, node.end);
        let (had_leading_comment, _, leading_comment_had_newline) =
            self.emit_comments_in_range(node.pos, keyword_pos, true, false);
        if (!had_leading_comment && self.comment_just_before_pos(keyword_pos))
            || (had_leading_comment && !leading_comment_had_newline)
        {
            self.write(" ");
        }

        self.write("yield");
        let after_yield_pos = keyword_pos.saturating_add(5);

        if unary.asterisk_token {
            let Some(expr_node) = self.arena.get(unary.expression) else {
                // TypeScript emits `yield* ;` (with space) when yield* has no expression
                if self.ctx.flags.in_generator {
                    self.write("*");
                } else {
                    self.write(" *");
                }
                self.write(" ");
                return;
            };

            let star_pos = self.source_text.map_or(after_yield_pos, |text| {
                let text_end = std::cmp::min(expr_node.end, text.len() as u32);
                self.skip_trivia_forward(after_yield_pos, text_end)
            });

            let (has_star_comment, _, _) =
                self.emit_comments_in_range(after_yield_pos, star_pos, true, false);
            if self.ctx.flags.in_generator || has_star_comment {
                self.write("*");
            } else {
                self.write(" *");
            }

            let expr_start = star_pos.saturating_add(1);
            let expr_pos = expr_node.pos;
            let (has_expression_comment, _, expression_comment_had_newline) =
                self.emit_comments_in_range(expr_start, expr_pos, true, false);
            if has_expression_comment {
                if !expression_comment_had_newline {
                    self.write(" ");
                }
            } else if !self.is_expression_parenthesized(expr_node) || self.ctx.flags.in_generator {
                self.write(" ");
            }
            self.emit_expression(unary.expression);
        } else {
            let Some(expr_node) = self.arena.get(unary.expression) else {
                return;
            };

            let (has_expression_comment, _, expression_comment_had_newline) =
                self.emit_comments_in_range(after_yield_pos, expr_node.pos, true, false);
            if has_expression_comment {
                if !expression_comment_had_newline {
                    self.write(" ");
                }
            } else if !self.is_expression_parenthesized(expr_node) || self.ctx.flags.in_generator {
                self.write(" ");
            }
            self.emit_expression(unary.expression);
        }
    }

    fn comment_just_before_pos(&self, pos: u32) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };

        let Some(prev_idx) = self.comment_emit_idx.checked_sub(1) else {
            return false;
        };
        let Some(prev_comment) = self.all_comments.get(prev_idx) else {
            return false;
        };

        if prev_comment.end > pos || prev_comment.has_trailing_new_line {
            return false;
        }

        let bytes = text.as_bytes();
        let mut cursor = prev_comment.end as usize;
        let end = std::cmp::min(pos as usize, bytes.len());
        while cursor < end {
            match bytes[cursor] {
                b' ' | b'\t' => {
                    cursor += 1;
                }
                _ => {
                    return false;
                }
            }
        }

        true
    }

    const fn is_expression_parenthesized(&self, node: &Node) -> bool {
        node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
    }

    /// Emit an await expression: await value
    pub(super) fn emit_await_expression(&mut self, node: &Node) {
        // AwaitExpression is stored with UnaryExprDataEx
        let emit_as_yield = self.ctx.emit_await_as_yield
            || (self.ctx.needs_async_lowering && self.function_scope_depth > 0);
        let Some(unary) = self.arena.get_unary_expr_ex(node) else {
            self.write(if self.ctx.emit_await_as_yield_await || emit_as_yield {
                "yield"
            } else {
                "await"
            });
            return;
        };

        // Async generator lowering: `await expr` → `yield __await(expr)`
        if self.ctx.emit_await_as_yield_await {
            self.write("yield ");
            self.write_helper("__await");
            self.write("(");
            self.emit_expression(unary.expression);
            self.write(")");
            return;
        }

        // For ES2015/ES2016 async lowering, emit yield instead of await.
        // When yield replaces await inside a binary expression, we need parens
        // because yield has lower precedence than binary operators.
        // e.g., `await p || a` → `(yield p) || a` (not `yield p || a`)
        let needs_yield_parens =
            emit_as_yield && self.ctx.flags.in_binary_operand && unary.expression.is_some();

        let keyword = if emit_as_yield { "yield" } else { "await" };

        if needs_yield_parens {
            self.write("(");
        }

        // Find keyword position in source to compute comment range
        let keyword_pos = self.skip_trivia_forward(node.pos, node.end);
        self.write(keyword);
        let after_keyword_pos = keyword_pos.saturating_add(keyword.len() as u32);

        if unary.expression.is_none() {
            // Preserve malformed syntax parity for missing await operands:
            // emit `await ` / `yield ` without synthesizing `void 0`.
            self.write(" ");
            if needs_yield_parens {
                self.write(")");
            }
            return;
        }

        // Emit any comments between the keyword and the operand expression.
        // Preserve original spacing: if source has no space (e.g. `await(x)`
        // where await is used as an identifier call), don't add one.
        let expr_node = self.arena.get(unary.expression);
        if let Some(expr_node) = expr_node {
            let (has_comment, _, comment_had_newline) =
                self.emit_comments_in_range(after_keyword_pos, expr_node.pos, true, false);
            if has_comment {
                if !comment_had_newline {
                    self.write(" ");
                }
            } else {
                // Check if source had a space between keyword and expression
                let source_had_space = self.source_text.is_none_or(|text| {
                    let kw_end = after_keyword_pos as usize;
                    kw_end >= text.len() || text.as_bytes()[kw_end] != b'('
                });
                if source_had_space {
                    self.write(" ");
                }
            }
        } else {
            self.write(" ");
        }

        self.emit_expression(unary.expression);
        if needs_yield_parens {
            self.write(")");
        }
    }

    // =========================================================================
    // Spread Elements
    // =========================================================================

    /// Emit a spread element: ...expr
    ///
    /// When the operand has a leading inline comment (e.g., `.../** @type */ (x)`),
    /// tsc inserts a space between `...` and the comment and suppresses the
    /// normal space after the comment: `... /** @type */(x)`.
    pub(super) fn emit_spread_element(&mut self, node: &Node) {
        let Some(spread) = self.arena.get_spread(node) else {
            self.write("...");
            return;
        };

        self.write("...");
        // tsc separates `...` from a leading inline block comment with a space
        // and suppresses the normal post-comment space so the paren sits
        // right against the closing `*/`:
        //   `.../** @type {T} */(x)` → `... /** @type {T} */(x)`
        if let Some(expr_node) = self.arena.get(spread.expression)
            && self.has_pending_comment_before(expr_node.pos)
        {
            self.write(" ");
            self.emit_comments_before_pos(expr_node.pos);
            // Suppress the automatic space after block comment — the
            // spread already inserted its own separator space above.
            self.pending_block_comment_space = false;
        }
        self.emit_expression(spread.expression);
    }

    // =========================================================================
    // Decorators
    // =========================================================================

    /// Emit a decorator: @expression
    ///
    /// For ES6+ targets, decorators are emitted as native `@expression` syntax.
    /// For ES5 targets, decorators should be downleveled using the `__decorate` helper,
    /// but this requires class-level coordination. Standalone decorator emission
    /// in ES5 mode is handled here by emitting a warning comment.
    ///
    /// Note: Full decorator ES5 lowering is handled at the class level by collecting
    /// decorators and emitting `__decorate([...], Class, "member", descriptor)` calls
    /// after the class definition.
    pub(super) fn emit_decorator(&mut self, node: &Node) {
        let Some(decorator) = self.arena.get_decorator(node) else {
            return;
        };

        // In ES5 mode, standalone decorator nodes should not be emitted directly.
        // Decorator lowering is handled at the class level via __decorate helper.
        // If we encounter a decorator here in ES5 mode, emit a warning comment.
        if self.ctx.target_es5 {
            // Get decorator expression text for the warning
            let decorator_text = self.get_decorator_text(decorator.expression);
            warn!(
                decorator = %decorator_text,
                "Decorator skipped in ES5 mode - decorator lowering not fully implemented"
            );
            self.write("/* @");
            self.write(&decorator_text);
            self.write(" - ES5 decorator lowering not implemented */");
            return;
        }

        // ES6+ native decorator syntax
        self.write("@");
        self.emit(decorator.expression);
    }

    /// Maximum recursion depth for decorator text extraction to prevent stack overflow
    const MAX_DECORATOR_TEXT_DEPTH: u32 = 10;

    /// Get a string representation of a decorator expression for diagnostics
    fn get_decorator_text(&self, expr_idx: tsz_parser::parser::NodeIndex) -> String {
        self.get_decorator_text_with_depth(expr_idx, 0)
    }

    /// Get a string representation of a decorator expression with depth limiting
    fn get_decorator_text_with_depth(
        &self,
        expr_idx: tsz_parser::parser::NodeIndex,
        depth: u32,
    ) -> String {
        // Prevent unbounded recursion for deeply nested decorator expressions
        if depth > Self::MAX_DECORATOR_TEXT_DEPTH {
            return "...".to_string();
        }

        if expr_idx.is_none() {
            return "unknown".to_string();
        }

        let Some(expr_node) = self.arena.get(expr_idx) else {
            return "unknown".to_string();
        };

        // Handle common cases: identifier, call expression
        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(ident) = self.arena.get_identifier(expr_node) {
                    return ident.escaped_text.clone();
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(expr_node) {
                    let callee_text =
                        self.get_decorator_text_with_depth(call.expression, depth + 1);
                    return format!("{callee_text}(...)");
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(expr_node) {
                    let obj_text = self.get_decorator_text_with_depth(access.expression, depth + 1);
                    let prop_text =
                        self.get_decorator_text_with_depth(access.name_or_argument, depth + 1);
                    return format!("{obj_text}.{prop_text}");
                }
            }
            _ => {}
        }

        "expression".to_string()
    }
}
