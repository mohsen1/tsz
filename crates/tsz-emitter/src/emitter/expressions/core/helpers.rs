use super::super::super::Printer;
use tsz_parser::parser::{NodeIndex, node::Node, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(super) fn new_expression_has_explicit_parens(
        &self,
        node: &Node,
        callee: tsz_parser::parser::NodeIndex,
    ) -> bool {
        let Some(source) = self.source_text else {
            return false;
        };

        let Some(callee_node) = self.arena.get(callee) else {
            return false;
        };

        let bytes = source.as_bytes();
        let mut i = callee_node.end as usize;
        let end = std::cmp::min(node.end as usize, source.len());

        while i < end {
            match bytes[i] {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    i += 1;
                }
                b'/' if i + 1 < end && bytes[i + 1] == b'/' => {
                    // Line comment: skip to end of line
                    while i < end && bytes[i] != b'\n' {
                        i += 1;
                    }
                }
                b'/' if i + 1 < end && bytes[i + 1] == b'*' => {
                    // Block comment: skip to closing */
                    i += 2;
                    while i + 1 < end && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                        i += 1;
                    }
                    if i + 1 < end {
                        i += 2;
                    }
                }
                b'(' => return true,
                _ => return false,
            }
        }
        false
    }

    pub(in crate::emitter) fn emit_parenthesized(&mut self, node: &Node) {
        let Some(paren) = self.arena.get_parenthesized(node) else {
            return;
        };

        // If the inner expression is a type assertion/as/satisfies/instantiation expression,
        // the parens were only needed for the TS syntax (e.g., `(<Type>x).foo`).
        // In JS emit, the type assertion is stripped, making the parens unnecessary
        // UNLESS the underlying expression (after unwrapping type assertions) is:
        //   - An object literal (block ambiguity)
        //   - A binary/complex expression (operator precedence would change)
        if let Some(inner) = self.arena.get(paren.expression)
            && (inner.kind == syntax_kind_ext::TYPE_ASSERTION
                || inner.kind == syntax_kind_ext::AS_EXPRESSION
                || inner.kind == syntax_kind_ext::SATISFIES_EXPRESSION
                || inner.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS)
        {
            let unwrapped_kind = self.unwrap_type_assertion_kind(paren.expression);
            // Only strip parens if the unwrapped expression is a simple primary that
            // cannot change meaning without parens: identifiers, property access,
            // element access, `this`, template literals, literals, class/function expr.
            let can_strip = matches!(
                unwrapped_kind,
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
                    // CALL_EXPRESSION can strip parens unless in new-callee position:
                    // `(x() as T).foo` → `x().foo` is fine, but `new (x() as T)` must keep
                    // parens because `new x()` has different semantics (constructs `x`).
                    || (k == syntax_kind_ext::CALL_EXPRESSION && !self.paren_in_new_callee)
                    // NEW_EXPRESSION can strip parens only when NOT in access position:
                    // `(new a)` → `new a` is fine, but `(new a).b` must keep parens
                    // because `new a.b` has different semantics (constructs `a.b`).
                    || (k == syntax_kind_ext::NEW_EXPRESSION && !self.paren_in_access_position)
                    // FunctionExpression and ClassExpression can strip parens when NOT in
                    // expression statement position (where they'd be ambiguous with declarations).
                    // `let a = (<T>function foo() {})` → `let a = function foo() {}`
                    || ((k == syntax_kind_ext::FUNCTION_EXPRESSION
                        || k == syntax_kind_ext::CLASS_EXPRESSION)
                        && !self.ctx.flags.paren_leftmost_function_or_object
                        && (!self.paren_in_access_position || self.paren_is_direct_call_callee))
            );

            if can_strip {
                // Before stripping parens, check if there are comments between
                // the `(` and the inner expression. tsc preserves parens when a
                // comment exists inside them for SOME cases, but NOT when the
                // inner expression is a type assertion/as/satisfies that will be
                // erased. In the erasure case, tsc strips both parens and type
                // syntax, hoisting the comment before the expression:
                //   `(/* TODO */ expr as T)` → `/* TODO */ expr`
                //   `(/* TODO */ expr satisfies T)` → `/* TODO */ expr`
                let actual_inner_start = self.skip_trivia_forward(inner.pos, inner.pos + 2048);
                let has_inner_comment = if actual_inner_start > node.pos {
                    self.all_comments
                        .iter()
                        .any(|c| c.pos >= node.pos && c.end <= actual_inner_start)
                } else {
                    false
                };
                if !has_inner_comment {
                    self.emit(paren.expression);
                    return;
                }
                // When there IS a comment but the inner expression is a type
                // assertion/as/satisfies that will be erased, still strip the
                // parens — UNLESS the comment introduces a newline (e.g., a
                // line comment `// ...`). A newline between a keyword like
                // `yield` and its operand triggers ASI, so the parens must be
                // preserved to keep the operand attached:
                //   `yield (// comment\n a as any)` → `yield (\n// comment\n a)`
                //   `yield (/* ok */ a as any)`     → `yield /* ok */ a`
                if inner.kind == syntax_kind_ext::TYPE_ASSERTION
                    || inner.kind == syntax_kind_ext::AS_EXPRESSION
                    || inner.kind == syntax_kind_ext::SATISFIES_EXPRESSION
                {
                    let has_newline_comment = self.all_comments.iter().any(|c| {
                        c.pos >= node.pos && c.end <= actual_inner_start && c.has_trailing_new_line
                    });
                    if !has_newline_comment && !self.parenthesized_span_has_newline(node) {
                        self.emit(paren.expression);
                        return;
                    }
                    // Fall through to emit with parens preserved
                }
                // Fall through to emit with parens preserved (non-type-erasure case)
            }

            // Check if the unwrapped expression is already parenthesized
            if self.type_assertion_result_is_parenthesized(paren.expression) {
                self.emit(paren.expression);
                return;
            }
            // Fall through to emit with parens preserved
        }

        // If the inner expression is another ParenExpr wrapping a type assertion/instantiation,
        // and the inner paren would be stripped during emit (because the unwrapped expression
        // is simple), then the outer parens are also redundant.
        if let Some(inner) = self.arena.get(paren.expression)
            && inner.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(inner_paren) = self.arena.get_parenthesized(inner)
            && let Some(inner_inner) = self.arena.get(inner_paren.expression)
            && (inner_inner.kind == syntax_kind_ext::TYPE_ASSERTION
                || inner_inner.kind == syntax_kind_ext::AS_EXPRESSION
                || inner_inner.kind == syntax_kind_ext::SATISFIES_EXPRESSION
                || inner_inner.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS)
        {
            // Check if the inner paren would strip its own parens (object literal or simple expr)
            if self.type_assertion_wraps_object_literal(inner_paren.expression) {
                self.emit(paren.expression);
                return;
            }
            // Also strip if the unwrapped expression is simple and the inner paren will be elided
            let unwrapped_kind = self.unwrap_type_assertion_kind(inner_paren.expression);
            let inner_can_strip = matches!(
                unwrapped_kind,
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
                    || (k == syntax_kind_ext::CALL_EXPRESSION && !self.paren_in_new_callee)
                    || (k == syntax_kind_ext::NEW_EXPRESSION && !self.paren_in_access_position)
            );
            if inner_can_strip {
                // Inner paren will be stripped, so our outer paren is redundant
                self.emit(paren.expression);
                return;
            }
        }

        // Emit comments that appear BEFORE the `(` in source text.
        // JSDoc type casts like `/** @type {T} */(expr)` have the comment
        // before the opening paren — tsc preserves this placement.
        // Comments AFTER the `(` (e.g., `( /* Preserve */ j = f())`) stay inside.
        let open_paren_source_pos = if let Some(text) = self.source_text {
            let bytes = text.as_bytes();
            let mut pos = node.pos as usize;
            let limit = std::cmp::min(node.end as usize, bytes.len());
            let mut found = None;
            while pos < limit {
                match bytes[pos] {
                    b'(' => {
                        found = Some(pos as u32);
                        break;
                    }
                    b'/' if pos + 1 < limit && bytes[pos + 1] == b'*' => {
                        pos += 2;
                        while pos + 1 < limit && !(bytes[pos] == b'*' && bytes[pos + 1] == b'/') {
                            pos += 1;
                        }
                        if pos + 1 < limit {
                            pos += 2;
                        }
                    }
                    b'/' if pos + 1 < limit && bytes[pos + 1] == b'/' => {
                        while pos < limit && bytes[pos] != b'\n' {
                            pos += 1;
                        }
                    }
                    _ => pos += 1,
                }
            }
            found
        } else {
            None
        };
        if let Some(paren_pos) = open_paren_source_pos
            && self.has_pending_comment_before(paren_pos)
        {
            self.emit_comments_before_pos(paren_pos);
            // Leave pending_block_comment_space intact — tsc's printer
            // normalizes block comment spacing: `/** ... */(x)` becomes
            // `/** ... */ (x)` with a space before the paren.
        }
        self.write("(");
        // Emit inline comments between `(` and inner expression
        // (e.g., `( /* Preserve */j = f())`)
        // When the comment has a trailing newline (line comment `// ...`),
        // emit a newline after `(` so that the output matches tsc:
        //   `yield (\n// comment\na)` instead of `yield ( // comment\na)`
        if let Some(inner_node) = self.arena.get(paren.expression)
            && self.has_pending_comment_before(inner_node.pos)
        {
            let actual_inner_start =
                self.skip_trivia_forward(inner_node.pos, inner_node.pos + 2048);
            let inserted_same_line_separator = if self
                .has_newline_comment_in_range(node.pos, inner_node.pos)
                || self.source_range_has_newline(node.pos, actual_inner_start)
            {
                self.write_line();
                false
            } else {
                self.write(" ");
                true
            };
            self.emit_comments_before_pos(inner_node.pos);
            if inserted_same_line_separator {
                self.pending_block_comment_space = false;
            }
        }
        // The explicit parens already provide grouping, so clear the
        // "needs parens" flags to avoid double-parenthesization when the
        // inner expression is a downlevel optional chain, nullish coalescing,
        // or yield-from-await in binary operand.
        let prev_optional = self.ctx.flags.optional_chain_needs_parens;
        let prev_nullish = self.ctx.flags.nullish_coalescing_needs_parens;
        let prev_in_binary = self.ctx.flags.in_binary_operand;
        // Likewise, clear the "self-parenthesize Function/Object literal"
        // flag: the explicit source paren already disambiguates the IIFE
        // (`(function(){})()`), so the inner FunctionExpression /
        // ObjectLiteralExpression must not add another wrapping pair.
        // Without this, `(<any>function foo() {})()` emits as
        // `((function foo() {}))()` instead of `(function foo() {})()`.
        let prev_paren_leftmost = self.ctx.flags.paren_leftmost_function_or_object;
        self.ctx.flags.optional_chain_needs_parens = false;
        self.ctx.flags.nullish_coalescing_needs_parens = false;
        self.ctx.flags.in_binary_operand = false;
        self.ctx.flags.paren_leftmost_function_or_object = false;
        self.emit(paren.expression);
        self.ctx.flags.in_binary_operand = prev_in_binary;
        self.ctx.flags.optional_chain_needs_parens = prev_optional;
        self.ctx.flags.nullish_coalescing_needs_parens = prev_nullish;
        self.ctx.flags.paren_leftmost_function_or_object = prev_paren_leftmost;
        self.write(")");
    }

    fn parenthesized_span_has_newline(&self, node: &Node) -> bool {
        self.source_range_has_newline(node.pos, node.end)
    }

    fn source_range_has_newline(&self, start: u32, end: u32) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let start = std::cmp::min(start as usize, text.len());
        let end = std::cmp::min(end as usize, text.len());
        start < end
            && text.as_bytes()[start..end]
                .iter()
                .any(|b| matches!(b, b'\n' | b'\r'))
    }

    /// Unwrap type assertion chain and return the kind of the underlying expression.
    pub(in crate::emitter) fn unwrap_type_assertion_kind(&self, mut idx: NodeIndex) -> Option<u16> {
        loop {
            let node = self.arena.get(idx)?;
            match node.kind {
                k if k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
                {
                    if let Some(ta) = self.arena.get_type_assertion(node) {
                        idx = ta.expression;
                    } else {
                        return Some(node.kind);
                    }
                }
                k if k == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS => {
                    if let Some(data) = self.arena.get_expr_type_args(node) {
                        idx = data.expression;
                    } else {
                        return Some(node.kind);
                    }
                }
                _ => return Some(node.kind),
            }
        }
    }

    /// Check if unwrapping a type assertion chain yields a parenthesized expression.
    /// Used to detect redundant outer parens: `((<Error>({...})))` → the type
    /// assertion wraps `({...})` which is already parenthesized, so outer parens
    /// are redundant.
    fn type_assertion_result_is_parenthesized(&self, mut idx: NodeIndex) -> bool {
        // Unwrap type assertions to find the underlying expression
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
                k if k == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS => {
                    if let Some(data) = self.arena.get_expr_type_args(node) {
                        idx = data.expression;
                    } else {
                        return false;
                    }
                }
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => return true,
                _ => return false,
            }
        }
    }

    /// Walk the leftmost chain of an expression, unwrapping type assertions,
    /// and return the kind of the deepest leftmost expression after erasure.
    /// This traces through `CallExpression`, `PropertyAccessExpression`,
    /// `ElementAccessExpression`, `TaggedTemplateExpression`, `NonNullExpression`,
    /// and type assertions to find what will actually appear at the start of
    /// the emitted expression.
    pub(in crate::emitter) fn leftmost_expression_kind_after_erasure(
        &self,
        mut idx: NodeIndex,
    ) -> Option<u16> {
        loop {
            let node = self.arena.get(idx)?;
            match node.kind {
                // Type assertions are erased — follow inner expression
                k if k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
                {
                    if let Some(ta) = self.arena.get_type_assertion(node) {
                        idx = ta.expression;
                    } else {
                        return Some(node.kind);
                    }
                }
                // Call/New have a left-side "expression" field
                k if k == syntax_kind_ext::CALL_EXPRESSION => {
                    if let Some(call) = self.arena.get_call_expr(node) {
                        idx = call.expression;
                    } else {
                        return Some(node.kind);
                    }
                }
                // Property/Element access have a left-side "expression" field
                k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
                {
                    if let Some(acc) = self.arena.get_access_expr(node) {
                        idx = acc.expression;
                    } else {
                        return Some(node.kind);
                    }
                }
                // NonNull: expr!
                k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                    if let Some(expr) = self.arena.get_unary_expr_ex(node) {
                        idx = expr.expression;
                    } else {
                        return Some(node.kind);
                    }
                }
                k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                    if let Some(tt) = self.arena.get_tagged_template(node) {
                        idx = tt.tag;
                    } else {
                        return Some(node.kind);
                    }
                }
                // ParenthesizedExpression wrapping a type assertion: parens stripped.
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    if let Some(paren) = self.arena.get_parenthesized(node) {
                        if let Some(inner) = self.arena.get(paren.expression)
                            && (inner.kind == syntax_kind_ext::TYPE_ASSERTION
                                || inner.kind == syntax_kind_ext::AS_EXPRESSION
                                || inner.kind == syntax_kind_ext::SATISFIES_EXPRESSION
                                || inner.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS)
                        {
                            idx = paren.expression;
                        } else {
                            return Some(node.kind);
                        }
                    } else {
                        return Some(node.kind);
                    }
                }
                // Terminal — return this kind
                _ => return Some(node.kind),
            }
        }
    }

    pub(in crate::emitter) fn emit_type_assertion_expression(&mut self, node: &Node) {
        let Some(assertion) = self.arena.get_type_assertion(node) else {
            self.write("void 0");
            return;
        };
        // Emit comments in the erased type assertion region before the inner expression.
        // For `<T>expr` (TypeAssertion): comments inside `<T>` are before the expression.
        if let Some(expr_node) = self.arena.get(assertion.expression) {
            self.emit_comments_before_pos(expr_node.pos);
        }
        self.emit_expression(assertion.expression);
        // For `expr as T` and `expr satisfies T`: skip comments inside the erased
        // type annotation so they don't leak into subsequent output.
        if (node.kind == syntax_kind_ext::AS_EXPRESSION
            || node.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
            && !self.ctx.options.remove_comments
            && assertion.type_node.is_some()
            && let Some(type_node) = self.arena.get(assertion.type_node)
        {
            self.skip_comments_in_range(type_node.pos, type_node.end);
        }
    }

    pub(in crate::emitter) fn emit_non_null_expression(&mut self, node: &Node) {
        let Some(unary) = self.arena.get_unary_expr_ex(node) else {
            self.write("void 0");
            return;
        };
        // Emit comments before the inner expression.
        if let Some(expr_node) = self.arena.get(unary.expression) {
            self.emit_comments_before_pos(expr_node.pos);
        }
        self.emit_expression(unary.expression);
        // Trailing comments are preserved by the statement emitter.
    }

    pub(in crate::emitter) fn emit_conditional(&mut self, node: &Node) {
        let Some(cond) = self.arena.get_conditional_expr(node) else {
            return;
        };

        let prev = self.ctx.flags.in_binary_operand;
        self.ctx.flags.in_binary_operand = true;

        // Detect newlines between the three operands in source to preserve
        // multiline ternary formatting (tsc preserves these line breaks).
        let (newline_before_question, newline_before_colon) =
            self.detect_conditional_newlines(cond.condition, cond.when_true, cond.when_false);

        // When lowering optional chains or nullish coalescing in the condition
        // (e.g., `o?.b ? 1 : 0` → `(o === null ... : o.b) ? 1 : 0`,
        //  `(a ?? 'foo') ? 1 : 2` → `(a !== null && a !== void 0 ? a : 'foo') ? 1 : 2`),
        // the ternary must be wrapped in parens to avoid ambiguity with
        // the outer conditional's `?`.
        let prev_optional = self.ctx.flags.optional_chain_needs_parens;
        let prev_nullish = self.ctx.flags.nullish_coalescing_needs_parens;
        self.ctx.flags.optional_chain_needs_parens = true;
        self.ctx.flags.nullish_coalescing_needs_parens = true;
        self.emit(cond.condition);
        self.ctx.flags.optional_chain_needs_parens = prev_optional;
        self.ctx.flags.nullish_coalescing_needs_parens = prev_nullish;
        // The true/false branches of a conditional don't need yield parens
        // because the ternary operator has very low precedence.
        self.ctx.flags.in_binary_operand = false;

        if newline_before_question {
            // Check if `?` is on the condition line (Case A) or the next line (Case B).
            // Case A: `a ?\n    b` → `?` before newline → emit `a ?` then newline + indent
            // Case B: `a\n    ? b` → `?` after newline → emit `a` then newline + indent + `?`
            let question_on_condition_line =
                self.question_on_condition_line(cond.condition, cond.when_true);
            if question_on_condition_line {
                // Case A: `?` trails on condition line — e.g.:
                //   var v = a ?
                //       b : c;
                self.write(" ?");
                self.write_line();
                self.increase_indent();
                self.emit(cond.when_true);
                if newline_before_colon {
                    let colon_on_true_line =
                        self.colon_on_true_line(cond.when_true, cond.when_false);
                    if colon_on_true_line {
                        // `:` trails on when_true line: `b :\n    c`
                        self.write(" :");
                        self.write_line();
                        self.emit(cond.when_false);
                    } else {
                        // `:` on new line: `b\n    : c`
                        self.write_line();
                        self.write(": ");
                        self.emit(cond.when_false);
                    }
                } else {
                    self.write(" : ");
                    self.emit(cond.when_false);
                }
                self.decrease_indent();
            } else {
                // Case B: `?` starts on new line — e.g.:
                //   var v = a
                //       ? b
                //       : c;
                self.write_line();
                self.increase_indent();
                self.write("? ");
                self.emit(cond.when_true);
                if newline_before_colon {
                    self.write_line();
                    self.write(": ");
                } else {
                    self.write(" : ");
                }
                self.emit(cond.when_false);
                self.decrease_indent();
            }
        } else if newline_before_colon {
            self.write(" ? ");
            self.emit(cond.when_true);
            let colon_on_new_line = !self.colon_on_true_line(cond.when_true, cond.when_false);
            if colon_on_new_line {
                // Newline before `:` — e.g.:
                //   var v = a ? b
                //       : c;
                self.write_line();
                self.increase_indent();
                self.write(": ");
                self.emit(cond.when_false);
                self.decrease_indent();
            } else {
                // `:` trailing on same line, alternate on next — e.g.:
                //   var v = a ? b :
                //       c;
                self.write(" :");
                self.write_line();
                self.increase_indent();
                self.emit(cond.when_false);
                self.decrease_indent();
            }
        } else {
            self.write(" ? ");
            self.emit(cond.when_true);
            self.write(" : ");
            self.emit(cond.when_false);
        }

        self.ctx.flags.in_binary_operand = prev;
    }

    /// Detect whether the source text has newlines between the parts of a
    /// conditional expression. Returns (`newline_before_question`, `newline_before_colon`).
    ///
    /// The parser's node.end extends past trailing trivia, so we scan the
    /// source text from condition.end forward, tracking ternary nesting depth,
    /// to find the OUTER `?` and `:` positions reliably.
    fn detect_conditional_newlines(
        &self,
        condition: NodeIndex,
        when_true: NodeIndex,
        _when_false: NodeIndex,
    ) -> (bool, bool) {
        let Some(text) = self.source_text else {
            return (false, false);
        };
        let cond_node = self.arena.get(condition);
        let true_node = self.arena.get(when_true);

        // For the `?`: check if there's a newline between the condition's actual
        // token end and when_true.pos.  The parser's condition.end can overshoot
        // past trivia AND the `?` token (since scanner.pos includes lookahead).
        // Use find_token_end_before_trivia with the `?` position as the upper
        // bound to get the condition's true last-token end.
        let newline_before_question = match (cond_node, true_node) {
            (Some(c), Some(t)) => {
                let range_end = std::cmp::min(t.pos as usize, text.len());
                let range_start = std::cmp::min(c.pos as usize, text.len());
                if range_start >= range_end {
                    false
                } else {
                    // Find the `?` scanning backward from the when_true node
                    let bytes = text.as_bytes();
                    let mut q_pos = None;
                    let mut j = range_end;
                    while j > range_start {
                        j -= 1;
                        if bytes[j] == b'?' {
                            q_pos = Some(j);
                            break;
                        }
                    }
                    if let Some(qp) = q_pos {
                        // Get the actual end of the condition content (before `?`)
                        let cond_end = self.find_token_end_before_trivia(c.pos, qp as u32) as usize;
                        let cond_end = std::cmp::min(cond_end, text.len());
                        cond_end < range_end && text[cond_end..range_end].contains('\n')
                    } else {
                        text[range_start..range_end].contains('\n')
                    }
                }
            }
            _ => false,
        };

        // For the `:`: find it by scanning backward from when_false.pos, then
        // check for a newline between the when_true content and the `:` position.
        let false_node = self.arena.get(_when_false);
        let newline_before_colon = match (true_node, false_node) {
            (Some(t), Some(f)) => {
                let bytes = text.as_bytes();
                let f_pos = std::cmp::min(f.pos as usize, bytes.len());
                // Find `:` scanning backward from when_false.pos
                let mut colon_pos = None;
                let mut j = f_pos;
                while j > t.pos as usize {
                    j -= 1;
                    if bytes[j] == b':' {
                        colon_pos = Some(j);
                        break;
                    }
                }
                if let Some(cp) = colon_pos {
                    // Get actual end of when_true content (before `:`)
                    let true_end = self.find_token_end_before_trivia(t.pos, cp as u32) as usize;
                    let true_end = std::cmp::min(true_end, text.len());
                    true_end < f_pos && text[true_end..f_pos].contains('\n')
                } else {
                    false
                }
            }
            _ => false,
        };

        (newline_before_question, newline_before_colon)
    }

    /// Check whether the `?` token in a conditional expression is on the
    /// condition's line (before the newline) or on the next line.
    /// Returns `true` for `a ?\n  b` (Case A), `false` for `a\n  ? b` (Case B).
    fn question_on_condition_line(&self, condition: NodeIndex, when_true: NodeIndex) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let Some(c) = self.arena.get(condition) else {
            return false;
        };
        let Some(t) = self.arena.get(when_true) else {
            return false;
        };
        let bytes = text.as_bytes();
        // Scan backward from when_true.pos to find the `?` operator
        let t_pos = std::cmp::min(t.pos as usize, bytes.len());
        let c_pos = c.pos as usize;
        let mut q_pos = None;
        let mut j = t_pos;
        while j > c_pos {
            j -= 1;
            if bytes[j] == b'?' {
                q_pos = Some(j);
                break;
            }
        }
        let Some(qp) = q_pos else { return false };
        // Get actual end of condition content, then check for newline between it and `?`
        let cond_end = self.find_token_end_before_trivia(c.pos, qp as u32) as usize;
        let cond_end = std::cmp::min(cond_end, bytes.len());
        // `?` is on condition line if NO newline between cond_end and `?`
        !text[cond_end..qp].contains('\n')
    }

    /// Check whether the `:` token in a conditional expression is on the
    /// `when_true` expression's line (before the newline), as in `b :\n  c`.
    /// Returns `true` for trailing colon: `b :\n  c`.
    /// Returns `false` for leading colon: `b\n  : c`.
    fn colon_on_true_line(&self, when_true: NodeIndex, when_false: NodeIndex) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let Some(t) = self.arena.get(when_true) else {
            return false;
        };
        let Some(f) = self.arena.get(when_false) else {
            return false;
        };
        let bytes = text.as_bytes();
        // Scan backward from when_false.pos to find the `:` operator
        let f_pos = std::cmp::min(f.pos as usize, bytes.len());
        let t_pos = t.pos as usize;
        let mut colon_pos = None;
        let mut j = f_pos;
        while j > t_pos {
            j -= 1;
            if bytes[j] == b':' {
                colon_pos = Some(j);
                break;
            }
        }
        let Some(cp) = colon_pos else { return false };
        // Get actual end of when_true content, then check for newline between it and `:`
        let true_end = self.find_token_end_before_trivia(t.pos, cp as u32) as usize;
        let true_end = std::cmp::min(true_end, bytes.len());
        // `:` is on true line if NO newline between true_end and `:`
        !text[true_end..cp].contains('\n')
    }
}
