impl<'a> Printer<'a> {
    fn emit_recovered_jsx_unary_trailing_less_than(
        &mut self,
        statement: &Node,
        expression: NodeIndex,
    ) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let Some(expr_node) = self.arena.get(expression) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION {
            return false;
        }
        let Some(unary) = self.arena.get_unary_expr(expr_node) else {
            return false;
        };
        let Some(operand_node) = self.arena.get(unary.operand) else {
            return false;
        };
        if operand_node.kind != syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT {
            return false;
        }

        let Ok(source) =
            crate::safe_slice::slice(text, statement.pos as usize, statement.end as usize)
        else {
            return false;
        };
        let recovered_source = format!("{}< <", super::super::get_operator_text(unary.operator));
        if source.trim() != recovered_source {
            return false;
        }

        self.write(" <");
        true
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

    pub(in crate::emitter) fn emit_leading_directive_prologue_statements(
        &mut self,
        statements: &[NodeIndex],
        block_close_pos: u32,
    ) -> usize {
        let mut emitted_count = 0;
        for (stmt_i, &stmt_idx) in statements.iter().enumerate() {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                break;
            };
            if !self.is_directive_prologue_statement(stmt_node) {
                break;
            }

            let actual_start = self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
            if let Some(text) = self.source_text {
                while self.comment_emit_idx < self.all_comments.len() {
                    let c_end = self.all_comments[self.comment_emit_idx].end;
                    if c_end > actual_start {
                        break;
                    }
                    let c_pos = self.all_comments[self.comment_emit_idx].pos;
                    let c_trailing = self.all_comments[self.comment_emit_idx].has_trailing_new_line;
                    if let Ok(comment_text) =
                        safe_slice::slice(text, c_pos as usize, c_end as usize)
                    {
                        self.write_comment_with_reindent(comment_text, Some(c_pos));
                        if c_trailing {
                            self.write_line();
                        } else if comment_text.starts_with("/*") {
                            self.pending_block_comment_space = true;
                        }
                    }
                    self.comment_emit_idx += 1;
                }
            }

            let before_emit_len = self.writer.len();
            self.emit(stmt_idx);
            if self.writer.len() > before_emit_len && !self.writer.is_at_line_start() {
                let upper_bound = statements
                    .get(stmt_i + 1)
                    .and_then(|&next_idx| self.arena.get(next_idx))
                    .map_or(block_close_pos, |next_node| next_node.pos);
                let token_end = self.find_token_end_before_trivia(stmt_node.pos, upper_bound);
                let max_pos = if stmt_i + 1 >= statements.len() {
                    block_close_pos
                } else {
                    upper_bound
                };
                self.emit_trailing_comments_before(token_end, max_pos);
                self.write_line();
            }
            emitted_count += 1;
        }
        emitted_count
    }

    fn is_directive_prologue_statement(&self, node: &Node) -> bool {
        node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
            && self
                .arena
                .get_expression_statement(node)
                .and_then(|stmt| self.arena.get(stmt.expression))
                .is_some_and(|expr| expr.is_string_literal())
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
        let capped_range_end = self
            .trailing_comment_scan_max_pos
            .map_or(range_end, |cap| cap.min(range_end));
        let stmt_end = std::cmp::min(capped_range_end as usize, bytes.len());
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
                if let Some(max_pos) = self.trailing_comment_scan_max_pos
                    && comment.pos >= max_pos
                {
                    break;
                }
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
