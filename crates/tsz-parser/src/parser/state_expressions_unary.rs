use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::interner::IdentText;

/// Parser state - unary, postfix, await, and yield expression parsing
use super::state::{CONTEXT_FLAG_ARROW_PARAMETERS, ParserState};
use crate::parser::{
    NodeIndex,
    node::{IdentifierData, UnaryExprData, UnaryExprDataEx},
    syntax_kind_ext,
};
use tsz_scanner::{SyntaxKind, keyword_text_len};

impl ParserState {
    // Parse unary expression
    pub(crate) fn parse_unary_expression(&mut self) -> NodeIndex {
        match self.token() {
            SyntaxKind::PlusToken
            | SyntaxKind::MinusToken
            | SyntaxKind::TildeToken
            | SyntaxKind::ExclamationToken
            | SyntaxKind::PlusPlusToken
            | SyntaxKind::MinusMinusToken => {
                let start_pos = self.token_pos();
                let operator = self.token() as u16;
                let is_update_operator = operator == SyntaxKind::PlusPlusToken as u16
                    || operator == SyntaxKind::MinusMinusToken as u16;
                self.next_token();
                if is_update_operator {
                    match self.token() {
                        // TSC recovers `++delete foo.bar`, `++++y`, `++\n++y`
                        // by treating the outer `++`/`--` as a unary with a
                        // missing operand and leaving the inner unary
                        // (`delete …`, `++y`, …) for the next statement, so
                        // the JS emitter prints the bare `++;` followed by
                        // the inner expression statement. tsc reaches the
                        // same shape via `parsePrimaryExpression`'s default
                        // `parseIdentifier(Expression_expected)` branch,
                        // which emits TS1109 at the offender without
                        // consuming it.
                        SyntaxKind::DeleteKeyword
                        | SyntaxKind::PlusPlusToken
                        | SyntaxKind::MinusMinusToken => {
                            self.parse_error_at(
                                self.token_pos(),
                                self.token_end().saturating_sub(self.token_pos()),
                                "Expression expected.",
                                diagnostic_codes::EXPRESSION_EXPECTED,
                            );
                            // End the unary expression at the offender's
                            // start so the next statement begins at the
                            // unconsumed token.
                            let end_pos = self.token_pos();
                            return self.arena.add_unary_expr(
                                syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
                                start_pos,
                                end_pos,
                                UnaryExprData {
                                    operator,
                                    operand: NodeIndex::NONE,
                                },
                            );
                        }
                        // TS1109: ++await and --await are invalid because await
                        // expressions are not valid left-hand-side expressions
                        // for increment/decrement.
                        SyntaxKind::AwaitKeyword => {
                            self.error_expression_expected();
                            // In async context, parse the full await expression
                            // (including operand like `42`) so tokens are consumed
                            // and no spurious TS1005 follows.
                            if self.in_async_context() {
                                let operand = self.parse_unary_expression();
                                let end_pos = self.token_end();
                                return self.arena.add_unary_expr(
                                    syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
                                    start_pos,
                                    end_pos,
                                    UnaryExprData { operator, operand },
                                );
                            }
                        }
                        _ => {}
                    }
                }
                // For prefix ++/-- (update operators), parse only a
                // LeftHandSideExpression as the operand — matching tsc's
                // parseUpdateExpression which calls
                // parseLeftHandSideExpressionOrHigher, NOT
                // parseUnaryExpressionOrHigher.  This prevents `--x--`
                // from being parsed as `--(x--)`.  Instead, `--x` is one
                // expression statement, and the trailing `--;` triggers
                // TS1005 (';' expected) + TS1109 (Expression expected).
                //
                // For other prefix unary operators (+, -, ~, !, typeof,
                // void, delete), the operand is still a full
                // UnaryExpression.
                let operand = if is_update_operator {
                    self.parse_left_hand_side_expression()
                } else {
                    self.parse_unary_expression()
                };
                if operand.is_none() {
                    // When a prefix unary operator has no operand, emit TS1109 at
                    // the current position unconditionally. tsc emits this via
                    // parsePrimaryExpression's default case -> createMissingNode,
                    // which uses only exact-position dedup (no distance-based
                    // suppression). Bypass should_report_error() so a prior
                    // TS1005 at the operator itself (e.g. `,` expected at `~` in
                    // `var a = q~;`) does not swallow the distinct missing-operand
                    // error. parse_error_at already dedupes at the same position,
                    // so this won't double up when the recursive call already
                    // reported at the same token.
                    self.parse_error_at_current_token(
                        "Expression expected.",
                        diagnostic_codes::EXPRESSION_EXPECTED,
                    );
                }
                let end_pos = self.token_end();

                self.arena.add_unary_expr(
                    syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
                    start_pos,
                    end_pos,
                    UnaryExprData { operator, operand },
                )
            }
            // `*` is only a binary operator (multiplication, etc.). Fall through to
            // the default path so `parse_primary_expression`'s `is_binary_operator`
            // branch reports TS1109 and returns a missing LHS without advancing,
            // matching tsc's `parsePrimaryExpression -> createMissingNode` flow.
            // The outer `parse_binary_expression_chain` then consumes `*` as a
            // binary operator, which is the correct tree shape for recovery
            // (e.g. `import type defer * as ns1 from "./a";` parses `* as`
            // as a binary expression and produces `;' expected` on `ns1`,
            // matching tsc).
            SyntaxKind::TypeOfKeyword | SyntaxKind::VoidKeyword | SyntaxKind::DeleteKeyword => {
                let start_pos = self.token_pos();
                let operator = self.token() as u16;
                self.next_token();
                let operand = self.parse_unary_expression();
                if operand.is_none() {
                    // Emit TS1109 for incomplete unary expression: typeof[missing], void[missing], delete[missing]
                    self.error_expression_expected();
                }
                let end_pos = self.token_end();

                self.arena.add_unary_expr(
                    syntax_kind_ext::PREFIX_UNARY_EXPRESSION,
                    start_pos,
                    end_pos,
                    UnaryExprData { operator, operand },
                )
            }
            SyntaxKind::AwaitKeyword => self.parse_await_expression(),
            SyntaxKind::YieldKeyword => self.parse_yield_expression(),
            _ => self.parse_postfix_expression(),
        }
    }

    pub(crate) fn parse_await_expression(&mut self) -> NodeIndex {
        // Check if 'await' is followed by an expression
        let snapshot = self.scanner.save_state();
        let current_token = self.current_token;
        self.next_token(); // consume 'await'
        let next_token = self.token();
        self.scanner.restore_state(snapshot);
        self.current_token = current_token;

        let has_following_expression = !matches!(
            next_token,
            SyntaxKind::SemicolonToken
                | SyntaxKind::CloseBracketToken
                | SyntaxKind::CommaToken
                | SyntaxKind::ColonToken
                | SyntaxKind::EqualsGreaterThanToken
                | SyntaxKind::CloseParenToken
                | SyntaxKind::EndOfFileToken
                | SyntaxKind::CloseBraceToken
        );

        // In static block context with a following expression, but NOT in an async context
        // (i.e., directly in the static block, not in a nested async function),
        // emit TS18037 and parse as await expression for correct AST structure
        if self.in_static_block_context() && !self.in_async_context() && has_following_expression {
            self.parse_error_at_current_token(
                "'await' expression cannot be used inside a class static block.",
                diagnostic_codes::AWAIT_EXPRESSION_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK,
            );
            // Fall through to parse as await expression
        } else if !self.in_async_context()
            && has_following_expression
            && !self.in_parameter_default_context()
            && (next_token != SyntaxKind::OpenParenToken || !self.in_function_body_context())
        {
            // Parse as await expression - the checker will emit TS1308
            // (not TS1359 from the parser) to match TSC behavior
        } else if self.in_async_context()
            && self.in_parameter_default_context()
            && has_following_expression
        {
            // Note: TS2524 ('await' expressions cannot be used in a parameter initializer)
            // is emitted by the checker, not the parser, matching TSC behavior.
            // Fall through to parse as await expression for error recovery
        } else if !self.in_async_context() {
            // NOT in async context - 'await' should be treated as identifier
            // In parameter default context of non-async functions, 'await' is a valid identifier
            if self.in_parameter_default_context() && !has_following_expression {
                // Parse 'await' as regular identifier in parameter defaults of non-async functions
                let start_pos = self.token_pos();
                let end_pos = self.token_end(); // capture end before consuming
                let atom = self.scanner.get_token_atom();
                self.next_token(); // consume the await token
                return self.arena.add_identifier(
                    SyntaxKind::Identifier as u16,
                    start_pos,
                    end_pos,
                    crate::parser::node::IdentifierData {
                        atom,
                        escaped_text: IdentText::from("await"),
                        original_text: None,
                    },
                );
            }

            // Outside async context or in other contexts, check if await is used as a bare expression
            // If followed by tokens that can't start an expression, report "Expression expected"
            // Examples where await is a reserved identifier but invalid as expression:
            //   await;  // Error: TS1359 in static blocks (reserved word)
            //   await (1);  // Error: Expression expected (in static blocks)
            //   async (a = await => x) => {}  // Error: Expression expected (before arrow)

            // Special case: Don't emit TS1109 for 'await' in computed property names like { [await]: foo }
            // In this context, 'await' is used as an identifier and CloseBracketToken is expected
            let is_computed_property_context = next_token == SyntaxKind::CloseBracketToken;
            // Special case: Don't emit TS1109 for 'await' when followed by colon (labeled statement)
            // The labeled statement parser will emit TS1109 (Expression expected) in static blocks
            let is_label_context = next_token == SyntaxKind::ColonToken;

            if !has_following_expression
                && !is_computed_property_context
                && !is_label_context
                && self.in_static_block_context()
            {
                // In static blocks, tsc treats `await` as a keyword and
                // emits TS1109 at the token AFTER `await` (the missing
                // operand position), matching await-expression parsing.
                let start_pos = self.token_pos();
                self.next_token(); // consume `await`
                self.error_expression_expected();
                let end_pos = self.token_end();
                return self.arena.add_unary_expr_ex(
                    syntax_kind_ext::AWAIT_EXPRESSION,
                    start_pos,
                    end_pos,
                    UnaryExprDataEx {
                        expression: NodeIndex::NONE,
                        asterisk_token: false,
                    },
                );
            }
            // Outside static blocks and async contexts, 'await' without a following
            // expression is a valid identifier (e.g., inside nested function bodies
            // within static blocks, or in non-module script code). Don't emit TS1109;
            // fall through to parse as identifier via parse_postfix_expression().

            // Fall through to parse as identifier/postfix expression
            return self.parse_postfix_expression();
        }

        // In async context, parse as await expression
        let start_pos = self.token_pos();
        self.consume_keyword(); // TS1260 check for await keyword with escapes

        // In parameter-default context, `await =>` reports a missing operand.
        //
        // In arrow function parameters (`CONTEXT_FLAG_ARROW_PARAMETERS`):
        //   Emit TS1109 at the `await` keyword and do NOT consume `=>`.
        //   The parameter-list recovery will then emit TS1005 "',' expected"
        //   at `=>`, giving the code set {TS1005, TS1109} matching tsc.
        //   Example: `async (a = await => await) => {}` → TS1109 + TS1005.
        //
        // In regular function parameters (no arrow context):
        //   Emit TS1109 at `=>` and consume `=>` + following token for recovery.
        //   Example: `async function foo(a = await => await) {}` → only TS1109.
        if self.in_parameter_default_context() && self.is_token(SyntaxKind::EqualsGreaterThanToken)
        {
            let in_arrow_params = (self.context_flags & CONTEXT_FLAG_ARROW_PARAMETERS) != 0;
            if in_arrow_params {
                // Emit TS1109 at await position (different from =>) to avoid
                // position-based dedup with the TS1005 from parameter list.
                self.parse_error_at(
                    start_pos,
                    keyword_text_len(SyntaxKind::AwaitKeyword),
                    "Expression expected.",
                    diagnostic_codes::EXPRESSION_EXPECTED,
                );
            } else {
                // Regular function: emit at => and consume for recovery
                self.error_expression_expected();
                self.next_token(); // consume `=>`
                if !self.is_token(SyntaxKind::CloseParenToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    self.next_token(); // skip arrow body token
                }
            }
            let end_pos = self.token_end();
            return self.arena.add_unary_expr_ex(
                syntax_kind_ext::AWAIT_EXPRESSION,
                start_pos,
                end_pos,
                UnaryExprDataEx {
                    expression: NodeIndex::NONE,
                    asterisk_token: false,
                },
            );
        }

        // Unlike return/throw, `await` does NOT participate in ASI
        // for its operand. `await\n1` parses as `await 1`, not `await; 1;`.
        // Only emit TS1109 when the next token truly can't start an expression
        // (`;`, `)`, `}`, EOF, etc.), not when there's a line break before a valid expr.
        if !self.is_expression_start() {
            self.error_expression_expected();
        }

        let expression = self.parse_unary_expression();
        let end_pos = self.token_end();

        self.arena.add_unary_expr_ex(
            syntax_kind_ext::AWAIT_EXPRESSION,
            start_pos,
            end_pos,
            UnaryExprDataEx {
                expression,
                asterisk_token: false,
            },
        )
    }

    pub(crate) fn parse_yield_expression(&mut self) -> NodeIndex {
        if self.in_class_member_name()
            && !self.in_generator_context()
            && !self.is_computed_class_member_yield_expression()
        {
            return self.parse_identifier_name();
        }

        // Check if 'yield' is followed by a token that disambiguates
        // between yield-expression and yield-as-identifier.
        let snapshot = self.scanner.save_state();
        let current_token = self.current_token;
        self.next_token(); // consume 'yield'

        // For non-generator context: tsc only parses yield as a yield expression
        // when the next token on the same line is an identifier, keyword, or literal.
        // This matches tsc's `nextTokenIsIdentifierOrKeywordOrLiteralOnSameLine`.
        // e.g., `yield foo;` → yield expression (TS1163)
        // e.g., `yield(foo);` → identifier + call (checker emits TS1212)
        // e.g., `yield * x;` → identifier * x (checker emits TS1212)
        let next_is_ident_keyword_or_literal_on_same_line =
            !self.scanner.has_preceding_line_break()
                && (crate::parser::parse_rules::is_identifier_or_keyword(self.token())
                    || matches!(
                        self.token(),
                        SyntaxKind::NumericLiteral
                            | SyntaxKind::BigIntLiteral
                            | SyntaxKind::StringLiteral
                    ));

        self.scanner.restore_state(snapshot);
        self.current_token = current_token;

        // Outside a generator context: use tsc's disambiguation rule.
        // Only parse as yield expression (for TS1163 error recovery) when the
        // next token on the same line is an identifier, keyword, or literal.
        // Otherwise parse as an identifier (the checker will emit TS1212 in
        // strict mode for `yield` as a reserved word).
        if !self.in_generator_context() && next_is_ident_keyword_or_literal_on_same_line {
            // Still parse as a yield expression (so emit is unchanged), but
            // skip the grammar diagnostic when this subtree belongs to a class
            // member recovered after a misplaced `case`/`default` clause: tsc
            // keeps that member yet does not run the yield grammar check on it.
            if !self.suppress_recovered_clause_member_yield_grammar {
                self.parse_error_at_current_token(
                    "A 'yield' expression is only allowed in a generator body.",
                    diagnostic_codes::A_YIELD_EXPRESSION_IS_ONLY_ALLOWED_IN_A_GENERATOR_BODY,
                );
            }
            // Fall through to parse as yield expression
        } else if !self.in_generator_context() {
            // Outside a generator context and next token is not identifier/keyword/
            // literal on same line — 'yield' is a regular identifier.
            // e.g., `yield(foo)` → call expression, `yield * x` → multiplication,
            //        `function f(yield = yield) {}` → identifier
            let start_pos = self.token_pos();
            let end_pos = self.token_end();
            let atom = self.scanner.get_token_atom();
            self.next_token();
            return self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                start_pos,
                end_pos,
                IdentifierData {
                    atom,
                    escaped_text: IdentText::from("yield"),
                    original_text: None,
                },
            );
        }

        let start_pos = self.token_pos();

        // Note: TS2523 ('yield' expressions cannot be used in a parameter initializer)
        // is emitted by the checker, not the parser, matching TSC behavior.

        self.consume_keyword(); // TS1260 check for yield keyword with escapes

        // Check for yield* (delegate yield)
        let asterisk_token = self.parse_optional(SyntaxKind::AsteriskToken);

        // Parse the expression (may be empty for bare yield)
        let expression = if !self.scanner.has_preceding_line_break()
            && !self.is_token(SyntaxKind::SemicolonToken)
            && !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::CloseParenToken)
            && !self.is_token(SyntaxKind::CloseBracketToken)
            && !self.is_token(SyntaxKind::ColonToken)
            && !self.is_token(SyntaxKind::CommaToken)
            && !self.is_token(SyntaxKind::EqualsGreaterThanToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            self.parse_assignment_expression()
        } else {
            NodeIndex::NONE
        };

        // yield * requires an expression (TS1109: Expression expected)
        if asterisk_token && expression.is_none() {
            self.error_expression_expected();
        }

        let end_pos = self.token_end();

        self.arena.add_unary_expr_ex(
            syntax_kind_ext::YIELD_EXPRESSION,
            start_pos,
            end_pos,
            UnaryExprDataEx {
                expression,
                asterisk_token,
            },
        )
    }

    // Parse postfix expression
    pub(crate) fn parse_postfix_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut expr = self.parse_left_hand_side_expression();

        // Handle postfix operators
        if !self.scanner.has_preceding_line_break()
            && (self.is_token(SyntaxKind::PlusPlusToken)
                || self.is_token(SyntaxKind::MinusMinusToken))
        {
            let operator = self.token() as u16;
            let end_pos = self.token_end();
            self.next_token();

            expr = self.arena.add_unary_expr(
                syntax_kind_ext::POSTFIX_UNARY_EXPRESSION,
                start_pos,
                end_pos,
                UnaryExprData {
                    operator,
                    operand: expr,
                },
            );
        }

        expr
    }
}
