//! Parser state - expression parsing methods

use super::state::{CONTEXT_FLAG_ASYNC, CONTEXT_FLAG_GENERATOR, ParserState};
use crate::interner::Atom;
use crate::parser::{NodeIndex, NodeList, node::*, node_flags, syntax_kind_ext};
use crate::scanner::SyntaxKind;
use crate::scanner_impl::TokenFlags;

impl ParserState {
    // =========================================================================
    // Parse Methods - Expressions
    // =========================================================================

    /// Parse an expression (including comma operator)
    pub fn parse_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut left = self.parse_assignment_expression();

        // Handle comma operator: expr, expr, expr
        // Comma expressions create a sequence, returning the last value
        while self.is_token(SyntaxKind::CommaToken) {
            self.next_token(); // consume comma
            let right = self.parse_assignment_expression();
            if right.is_none() {
                // Emit TS1109 for trailing comma or missing expression: expr, [missing]
                self.error_expression_expected();
                break; // Exit loop to prevent cascading errors
            }
            let end_pos = self.token_end();

            left = self.arena.add_binary_expr(
                syntax_kind_ext::BINARY_EXPRESSION,
                start_pos,
                end_pos,
                BinaryExprData {
                    left,
                    operator_token: SyntaxKind::CommaToken as u16,
                    right,
                },
            );
        }

        left
    }

    /// Parse assignment expression
    pub(crate) fn parse_assignment_expression(&mut self) -> NodeIndex {
        // Check for arrow function first (including async arrow)
        if self.is_start_of_arrow_function() {
            // Check if it's an async arrow function
            // Note: `async => x` is a NON-async arrow where 'async' is the parameter name
            // `async x => x` or `async (x) => x` are async arrow functions
            if self.is_token(SyntaxKind::AsyncKeyword) {
                // Need to distinguish:
                // - `async => expr` (non-async, 'async' is param)
                // - `async x => expr` or `async (x) => expr` (async arrow)
                if self.look_ahead_is_simple_arrow_function() {
                    // async => expr - treat 'async' as identifier parameter
                    return self.parse_arrow_function_expression_with_async(false);
                }
                return self.parse_async_arrow_function_expression();
            }
            return self.parse_arrow_function_expression_with_async(false);
        }

        // Start at precedence 2 to skip comma operator (precedence 1)
        // Comma expressions are only valid in certain contexts (e.g., for loop)
        self.parse_binary_expression(2)
    }

    /// Parse async arrow function: async (x) => ... or async x => ...
    pub(crate) fn parse_async_arrow_function_expression(&mut self) -> NodeIndex {
        self.parse_expected(SyntaxKind::AsyncKeyword);
        self.parse_arrow_function_expression_with_async(true)
    }

    /// Check if we're at the start of an arrow function
    pub(crate) fn is_start_of_arrow_function(&mut self) -> bool {
        match self.token() {
            // (params) => ...
            SyntaxKind::OpenParenToken => self.look_ahead_is_arrow_function(),
            // async could be:
            // 1. async (x) => ... or async x => ... (async arrow function)
            // 2. async => ... (non-async arrow where 'async' is parameter name)
            SyntaxKind::AsyncKeyword => {
                // Check if 'async' is immediately followed by '=>'
                // If so, it's 'async' used as parameter name, not async modifier
                if self.look_ahead_is_simple_arrow_function() {
                    // async => expr - treat as simple arrow with 'async' as param
                    true
                } else {
                    // Check for async (x) => ... or async x => ...
                    self.look_ahead_is_arrow_function_after_async()
                }
            }
            // <T>(x) => ... (generic arrow function)
            SyntaxKind::LessThanToken => self.look_ahead_is_generic_arrow_function(),
            _ => self.is_identifier_or_keyword() && self.look_ahead_is_simple_arrow_function(),
        }
    }

    /// Look ahead to see if < starts a generic arrow function: <T>(x) => or <T, U>() =>
    pub(crate) fn look_ahead_is_generic_arrow_function(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip <
        self.next_token();

        // Skip type parameters until we find >
        let mut depth = 1;
        while depth > 0 && !self.is_token(SyntaxKind::EndOfFileToken) {
            if self.is_token(SyntaxKind::LessThanToken) {
                depth += 1;
            } else if self.is_token(SyntaxKind::GreaterThanToken) {
                depth -= 1;
            }
            self.next_token();
        }

        // After >, should have (
        if !self.is_token(SyntaxKind::OpenParenToken) {
            self.scanner.restore_state(snapshot);
            self.current_token = current;
            return false;
        }

        // Now check if this is an arrow function
        let result = self.look_ahead_is_arrow_function();

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Look ahead after async to see if it's an arrow function: async (x) => or async x => or async <T>(x) =>
    ///
    /// ASI Rule: If there's a line break after 'async', it's NOT an async arrow function.
    /// The line break prevents 'async' from being treated as a modifier.
    /// Example: `async\nx => x` parses as `async; (x => x);` not as an async arrow function.
    pub(crate) fn look_ahead_is_arrow_function_after_async(&mut self) -> bool {
        // IMPORTANT: Check for line break BEFORE consuming 'async'
        // If there's a line break after 'async', it cannot be an async arrow function
        if self.scanner.has_preceding_line_break() {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip 'async'
        self.next_token();

        let result = match self.token() {
            // async (params) => ...
            SyntaxKind::OpenParenToken => self.look_ahead_is_arrow_function(),
            // async x => ...
            SyntaxKind::Identifier => self.look_ahead_is_simple_arrow_function(),
            // async <T>(x) => ... (generic async arrow)
            SyntaxKind::LessThanToken => self.look_ahead_is_generic_arrow_function(),
            _ => false,
        };

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Look ahead to see if ( starts an arrow function: () => or (x) => or (x, y) =>
    ///
    /// ASI Rule: If there's a line break between ) and =>, it's NOT an arrow function.
    /// Example: `(x)\n=> y` should NOT be parsed as an arrow function.
    pub(crate) fn look_ahead_is_arrow_function(&mut self) -> bool {
        // IMPORTANT: If we're inside the 'true' branch of a conditional expression (a ? [here] : b),
        // a following ':' belongs to the conditional, NOT to an arrow function return type.
        // This prevents "stealing" the colon from the enclosing conditional.
        if (self.context_flags & crate::parser::state::CONTEXT_FLAG_IN_CONDITIONAL_TRUE) != 0 {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip (
        self.next_token();

        // Empty params: () => or (): type =>
        if self.is_token(SyntaxKind::CloseParenToken) {
            self.next_token();
            // Check for line break before =>
            let has_line_break = self.scanner.has_preceding_line_break();
            let is_arrow = if has_line_break {
                // Line break before => means this is not an arrow function (ASI applies)
                false
            } else if self.is_token(SyntaxKind::ColonToken) {
                let saved_arena_len = self.arena.nodes.len();
                let saved_diagnostics_len = self.parse_diagnostics.len();

                self.next_token();
                let _ = self.parse_return_type();
                let result = !self.scanner.has_preceding_line_break()
                    && (self.is_token(SyntaxKind::EqualsGreaterThanToken)
                        || self.is_token(SyntaxKind::OpenBraceToken));

                self.arena.nodes.truncate(saved_arena_len);
                self.parse_diagnostics.truncate(saved_diagnostics_len);

                result
            } else {
                // Check for => or { (error recovery: user forgot =>)
                self.is_token(SyntaxKind::EqualsGreaterThanToken)
                    || self.is_token(SyntaxKind::OpenBraceToken)
            };
            self.scanner.restore_state(snapshot);
            self.current_token = current;
            return is_arrow;
        }

        // Skip to matching ) to check for =>
        let mut depth = 1;
        while depth > 0 && !self.is_token(SyntaxKind::EndOfFileToken) {
            if self.is_token(SyntaxKind::OpenParenToken) {
                depth += 1;
            } else if self.is_token(SyntaxKind::CloseParenToken) {
                depth -= 1;
            }
            self.next_token();
        }

        // Check for line break before =>
        let has_line_break = self.scanner.has_preceding_line_break();

        // Check for optional return type annotation
        let is_arrow = if has_line_break {
            // Line break before => means this is not an arrow function (ASI applies)
            false
        } else if self.is_token(SyntaxKind::ColonToken) {
            let saved_arena_len = self.arena.nodes.len();
            let saved_diagnostics_len = self.parse_diagnostics.len();

            self.next_token();
            let _ = self.parse_return_type();
            let result = !self.scanner.has_preceding_line_break()
                && (self.is_token(SyntaxKind::EqualsGreaterThanToken)
                    || self.is_token(SyntaxKind::OpenBraceToken));

            self.arena.nodes.truncate(saved_arena_len);
            self.parse_diagnostics.truncate(saved_diagnostics_len);

            result
        } else {
            // Check for => or { (error recovery: user forgot =>)
            self.is_token(SyntaxKind::EqualsGreaterThanToken)
                || self.is_token(SyntaxKind::OpenBraceToken)
        };
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_arrow
    }

    /// Look ahead to see if identifier is followed by => (simple arrow function)
    ///
    /// ASI Rule: If there's a line break between the identifier and =>, it's NOT an arrow function.
    /// Example: `x\n=> y` should NOT be parsed as an arrow function.
    pub(crate) fn look_ahead_is_simple_arrow_function(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip identifier
        self.next_token();

        // Check if => is immediately after identifier (no line break)
        // If there's a line break, ASI applies and this is not an arrow function
        let is_arrow = !self.scanner.has_preceding_line_break()
            && self.is_token(SyntaxKind::EqualsGreaterThanToken);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_arrow
    }

    /// Parse arrow function expression: (params) => body or x => body or <T>(x) => body
    pub(crate) fn parse_arrow_function_expression_with_async(
        &mut self,
        is_async: bool,
    ) -> NodeIndex {
        let start_pos = self.token_pos();

        // Set async context BEFORE parsing parameters
        // This is important for correctly handling 'await' in parameter defaults:
        // - `async (a = await) => {}` should emit TS1109 (Expression expected)
        // - TSC sets async context for the entire async function scope including parameters
        let saved_flags = self.context_flags;
        if is_async {
            self.context_flags |= CONTEXT_FLAG_ASYNC;
        }

        // Parse optional type parameters: <T, U extends Foo>
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        // Parse parameters
        let parameters = if self.is_token(SyntaxKind::OpenParenToken) {
            // Parenthesized parameter list: (a, b) =>
            self.parse_expected(SyntaxKind::OpenParenToken);
            let params = self.parse_parameter_list();
            self.parse_expected(SyntaxKind::CloseParenToken);
            params
        } else {
            // Single identifier parameter: x => or async => (where async is used as identifier)
            let param_start = self.token_pos();
            // Use parse_identifier_name to allow keywords like 'async' as parameter names
            let name = self.parse_identifier_name();
            let param_end = self.token_end();

            let param = self.arena.add_parameter(
                syntax_kind_ext::PARAMETER,
                param_start,
                param_end,
                crate::parser::node::ParameterData {
                    modifiers: None,
                    dot_dot_dot_token: false,
                    name,
                    question_token: false,
                    type_annotation: NodeIndex::NONE,
                    initializer: NodeIndex::NONE,
                },
            );
            self.make_node_list(vec![param])
        };

        // Parse optional return type annotation (supports type predicates: x is T)
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        // Recovery: Handle missing fat arrow - common typo: (a, b) { return a; }
        // If we see { immediately after parameters/return type, the user forgot =>
        if self.is_token(SyntaxKind::OpenBraceToken) {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token("'=>' expected.", diagnostic_codes::TOKEN_EXPECTED);
            // Don't consume the {, just continue to body parsing
            // The arrow is logically present but missing
        } else {
            // Normal case: expect =>
            self.parse_expected(SyntaxKind::EqualsGreaterThanToken);
        }

        // Async context was already set at the start of this function for parameter parsing
        // and remains set for body parsing

        // Parse body (block or expression)
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            // Check if next token starts a statement but not an expression
            // This catches cases like `() => var x` where `{` was expected
            if self.is_statement_start() && !self.is_expression_start() {
                self.error_token_expected("{");
            }
            self.parse_assignment_expression()
        };

        // Restore context flags
        self.context_flags = saved_flags;

        let end_pos = self.token_end();

        self.arena.add_function(
            syntax_kind_ext::ARROW_FUNCTION,
            start_pos,
            end_pos,
            FunctionData {
                modifiers: None,
                is_async,
                asterisk_token: false,
                name: NodeIndex::NONE,
                type_parameters,
                parameters,
                type_annotation,
                body,
                equals_greater_than_token: true,
            },
        )
    }

    /// Parse type parameters: <T, U extends Foo, V = DefaultType>
    pub(crate) fn parse_type_parameters(&mut self) -> NodeList {
        let mut params = Vec::new();

        self.parse_expected(SyntaxKind::LessThanToken);

        // Check for empty type parameter list: <>
        // TypeScript reports TS1098: "Type parameter list cannot be empty"
        if self.is_token(SyntaxKind::GreaterThanToken) {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Type parameter list cannot be empty.",
                diagnostic_codes::TYPE_PARAMETER_LIST_CANNOT_BE_EMPTY,
            );
        }

        while !self.is_greater_than_or_compound() && !self.is_token(SyntaxKind::EndOfFileToken) {
            let param = self.parse_type_parameter();
            params.push(param);

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        self.parse_expected_greater_than();

        self.make_node_list(params)
    }

    /// Parse a single type parameter: T or T extends U or T = Default or T extends U = Default
    /// Also supports modifiers: `const T`, `in T`, `out T`, `in out T`, `const in T`, etc.
    pub(crate) fn parse_type_parameter(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse optional modifiers: const, in, out (TypeScript 4.7+ variance, 5.0+ const)
        let modifiers = self.parse_type_parameter_modifiers();

        // Parse the type parameter name
        let name = self.parse_identifier();

        // Parse optional constraint: extends SomeType
        let constraint = if self.parse_optional(SyntaxKind::ExtendsKeyword) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        // Parse optional default: = DefaultType
        let default = if self.parse_optional(SyntaxKind::EqualsToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();

        self.arena.add_type_parameter(
            syntax_kind_ext::TYPE_PARAMETER,
            start_pos,
            end_pos,
            crate::parser::node::TypeParameterData {
                modifiers,
                name,
                constraint,
                default,
            },
        )
    }

    /// Parse type parameter modifiers: `const`, `in`, `out`
    fn parse_type_parameter_modifiers(&mut self) -> Option<NodeList> {
        let mut modifiers = Vec::new();

        loop {
            match self.token() {
                SyntaxKind::ConstKeyword => {
                    let pos = self.token_pos();
                    let end = self.token_end();
                    self.next_token();
                    modifiers.push(
                        self.arena
                            .add_token(SyntaxKind::ConstKeyword as u16, pos, end),
                    );
                }
                SyntaxKind::InKeyword => {
                    let pos = self.token_pos();
                    let end = self.token_end();
                    self.next_token();
                    modifiers.push(self.arena.add_token(SyntaxKind::InKeyword as u16, pos, end));
                }
                SyntaxKind::OutKeyword => {
                    let pos = self.token_pos();
                    let end = self.token_end();
                    self.next_token();
                    modifiers.push(
                        self.arena
                            .add_token(SyntaxKind::OutKeyword as u16, pos, end),
                    );
                }
                _ => break,
            }
        }

        if modifiers.is_empty() {
            None
        } else {
            Some(self.make_node_list(modifiers))
        }
    }

    /// Parse binary expression with precedence climbing
    pub(crate) fn parse_binary_expression(&mut self, min_precedence: u8) -> NodeIndex {
        // Check recursion limit for deeply nested expressions
        if !self.enter_recursion() {
            return NodeIndex::NONE;
        }

        let start_pos = self.token_pos();
        let mut left = self.parse_unary_expression();

        loop {
            // Try to rescan > as >>, >>>, >=, >>=, >>>= for binary operators
            let op = if self.is_token(SyntaxKind::GreaterThanToken) {
                self.try_rescan_greater_token()
            } else {
                self.token()
            };
            let precedence = self.get_operator_precedence(op);

            if precedence == 0 || precedence < min_precedence {
                break;
            }

            if op == SyntaxKind::AsKeyword || op == SyntaxKind::SatisfiesKeyword {
                left = self.parse_as_or_satisfies_expression(left, start_pos);
                continue;
            }

            let operator_token = op as u16;
            self.next_token();

            // Handle conditional expression
            if op == SyntaxKind::QuestionToken {
                // Set flag to indicate we're parsing the 'true' branch of a conditional
                // This prevents arrow function lookahead from stealing the ':' that belongs to this conditional
                let saved_flags = self.context_flags;
                self.context_flags |= crate::parser::state::CONTEXT_FLAG_IN_CONDITIONAL_TRUE;

                let mut when_true = self.parse_assignment_expression();

                // Restore flags after parsing the true branch
                self.context_flags = saved_flags;
                if when_true.is_none() {
                    // Emit TS1109 for incomplete conditional expression: condition ? [missing]
                    self.error_expression_expected();
                    // Create placeholder for missing true branch
                    when_true = self.create_missing_expression();
                }
                self.parse_expected(SyntaxKind::ColonToken);
                let mut when_false = self.parse_assignment_expression();
                if when_false.is_none() {
                    // Emit TS1109 for incomplete conditional expression: condition ? true : [missing]
                    self.error_expression_expected();
                    // Create placeholder for missing false branch
                    when_false = self.create_missing_expression();
                }
                let end_pos = self.token_end();

                left = self.arena.add_conditional_expr(
                    syntax_kind_ext::CONDITIONAL_EXPRESSION,
                    start_pos,
                    end_pos,
                    ConditionalExprData {
                        condition: left,
                        when_true,
                        when_false,
                    },
                );
            } else {
                // Right associativity for assignment and exponentiation
                // For assignment operators, use parse_assignment_expression to allow arrow functions on RHS
                let is_assignment = matches!(
                    op,
                    SyntaxKind::EqualsToken
                        | SyntaxKind::PlusEqualsToken
                        | SyntaxKind::MinusEqualsToken
                        | SyntaxKind::AsteriskEqualsToken
                        | SyntaxKind::SlashEqualsToken
                        | SyntaxKind::PercentEqualsToken
                        | SyntaxKind::AsteriskAsteriskEqualsToken
                        | SyntaxKind::LessThanLessThanEqualsToken
                        | SyntaxKind::GreaterThanGreaterThanEqualsToken
                        | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
                        | SyntaxKind::AmpersandEqualsToken
                        | SyntaxKind::CaretEqualsToken
                        | SyntaxKind::BarEqualsToken
                        | SyntaxKind::BarBarEqualsToken
                        | SyntaxKind::AmpersandAmpersandEqualsToken
                        | SyntaxKind::QuestionQuestionEqualsToken
                );

                let right = if is_assignment {
                    let result = self.parse_assignment_expression();
                    if result.is_none() {
                        // Emit TS1109 for incomplete assignment RHS: a = [missing]
                        self.error_expression_expected();
                        // Try to create a placeholder for missing RHS to maintain AST structure
                        let recovered = self.try_recover_binary_rhs();
                        if recovered.is_none() {
                            self.resync_to_next_expression_boundary();
                            // Break out of binary expression loop when parsing fails
                            return left;
                        }
                        recovered
                    } else {
                        result
                    }
                } else {
                    let next_min = if op == SyntaxKind::AsteriskAsteriskToken {
                        precedence // right associative
                    } else {
                        precedence + 1
                    };
                    let result = self.parse_binary_expression(next_min);
                    if result.is_none() {
                        // Emit TS1109 for incomplete binary expression: a + [missing]
                        self.error_expression_expected();
                        // Try to create a placeholder for missing RHS to maintain AST structure
                        let recovered = self.try_recover_binary_rhs();
                        if recovered.is_none() {
                            self.resync_to_next_expression_boundary();
                            // Break out of binary expression loop when parsing fails
                            return left;
                        }
                        recovered
                    } else {
                        result
                    }
                };
                let end_pos = self.token_end();

                let final_right = if right.is_none() { left } else { right };

                left = self.arena.add_binary_expr(
                    syntax_kind_ext::BINARY_EXPRESSION,
                    start_pos,
                    end_pos,
                    BinaryExprData {
                        left,
                        operator_token,
                        right: final_right,
                    },
                );
            }
        }

        self.exit_recursion();
        left
    }

    /// Parse as/satisfies expression: expr as Type, expr satisfies Type
    /// Also handles const assertion: expr as const
    pub(crate) fn parse_as_or_satisfies_expression(
        &mut self,
        expression: NodeIndex,
        start_pos: u32,
    ) -> NodeIndex {
        let is_satisfies = self.is_token(SyntaxKind::SatisfiesKeyword);
        self.next_token(); // consume 'as' or 'satisfies'

        // Handle 'as const' - const assertion
        let type_node = if !is_satisfies && self.is_token(SyntaxKind::ConstKeyword) {
            // Create a token node for 'const' keyword
            let const_start = self.token_pos();
            let const_end = self.token_end();
            self.next_token(); // consume 'const'
            self.arena
                .add_token(SyntaxKind::ConstKeyword as u16, const_start, const_end)
        } else {
            self.parse_type()
        };
        let end_pos = self.token_end();

        let result = self.arena.add_type_assertion(
            if is_satisfies {
                syntax_kind_ext::SATISFIES_EXPRESSION
            } else {
                syntax_kind_ext::AS_EXPRESSION
            },
            start_pos,
            end_pos,
            crate::parser::node::TypeAssertionData {
                expression,
                type_node,
            },
        );

        // Allow chaining: x as T as U
        if self.is_token(SyntaxKind::AsKeyword) || self.is_token(SyntaxKind::SatisfiesKeyword) {
            return self.parse_as_or_satisfies_expression(result, start_pos);
        }

        result
    }

    /// Parse unary expression
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
                // TS1109: ++await and --await are invalid because await expressions
                // are not valid left-hand-side expressions for increment/decrement
                if is_update_operator && self.token() == SyntaxKind::AwaitKeyword {
                    self.error_expression_expected();
                }
                let operand = self.parse_unary_expression();
                if operand.is_none() {
                    // Emit TS1109 for incomplete unary expression: +[missing], ++[missing], etc.
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
            SyntaxKind::AwaitKeyword => {
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
                if self.in_static_block_context()
                    && !self.in_async_context()
                    && has_following_expression
                {
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'await' expression cannot be used inside a class static block.",
                        diagnostic_codes::AWAIT_IN_STATIC_BLOCK,
                    );
                    // Fall through to parse as await expression
                } else if !self.in_async_context()
                    && has_following_expression
                    && !self.in_parameter_default_context()
                {
                    // TS1359: await expression outside async function (general case)
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "An 'await' expression is only allowed within an async function.",
                        diagnostic_codes::AWAIT_EXPRESSION_ONLY_IN_ASYNC_FUNCTION,
                    );
                    // Fall through to parse as await expression
                } else if self.in_parameter_default_context() && has_following_expression {
                    // TS2524: 'await' expressions cannot be used in a parameter initializer
                    // This only applies when there IS a following expression (e.g., `async (a = await foo)`)
                    // When there's no following expression (e.g., `async (a = await)`), we fall through
                    // to emit TS1109 from the await expression parsing at line 859-865
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'await' expressions cannot be used in a parameter initializer.",
                        diagnostic_codes::AWAIT_IN_PARAMETER_DEFAULT,
                    );
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
                                escaped_text: String::from("await"),
                                original_text: None,
                                type_arguments: None,
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

                    if !has_following_expression && !is_computed_property_context {
                        // In static blocks, 'await' used as a bare identifier should emit TS1359
                        // (reserved word cannot be used here) to match TSC behavior
                        if self.in_static_block_context() {
                            use crate::checker::types::diagnostics::diagnostic_codes;
                            self.parse_error_at_current_token(
                                "Identifier expected. 'await' is a reserved word that cannot be used here.",
                                diagnostic_codes::AWAIT_IDENTIFIER_ILLEGAL,
                            );
                        } else {
                            self.error_expression_expected();
                        }
                    }

                    // Fall through to parse as identifier/postfix expression
                    return self.parse_postfix_expression();
                }

                // In async context, parse as await expression
                let start_pos = self.token_pos();
                self.next_token();

                // Check for missing operand (e.g., just "await" with nothing after it)
                if self.can_parse_semicolon()
                    || self.is_token(SyntaxKind::SemicolonToken)
                    || !self.is_expression_start()
                {
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
            SyntaxKind::YieldKeyword => {
                // Check if 'yield' is followed by an expression
                let snapshot = self.scanner.save_state();
                let current_token = self.current_token;
                self.next_token(); // consume 'yield'
                // Check for asterisk (yield*)
                let has_asterisk = self.is_token(SyntaxKind::AsteriskToken);
                if has_asterisk {
                    self.next_token();
                }
                let next_token = self.token();
                self.scanner.restore_state(snapshot);
                self.current_token = current_token;

                let has_following_expression = !matches!(
                    next_token,
                    SyntaxKind::SemicolonToken
                        | SyntaxKind::CloseBracketToken
                        | SyntaxKind::CommaToken
                        | SyntaxKind::ColonToken
                        | SyntaxKind::CloseParenToken
                        | SyntaxKind::CloseBraceToken
                        | SyntaxKind::EndOfFileToken
                );

                // In static block context with a following expression, but NOT in a generator context
                // (i.e., directly in the static block, not in a nested generator function),
                // emit TS1163 and parse as yield expression for correct AST structure
                if self.in_static_block_context()
                    && !self.in_generator_context()
                    && (has_following_expression || has_asterisk)
                {
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "A 'yield' expression is only allowed in a generator body.",
                        diagnostic_codes::YIELD_EXPRESSION_ONLY_IN_GENERATOR,
                    );
                    // Fall through to parse as yield expression
                } else if !self.in_generator_context() {
                    // Outside a generator context, 'yield' is a regular identifier,
                    // not a yield expression. This mirrors how 'await' is handled
                    // outside async contexts.
                    // e.g., function f(yield = yield) {} -- 'yield' is an identifier here
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
                            escaped_text: String::from("yield"),
                            original_text: None,
                            type_arguments: None,
                        },
                    );
                }

                let start_pos = self.token_pos();

                // Check if 'yield' is used in a parameter default context
                // TS2523: 'yield' expressions cannot be used in a parameter initializer
                if self.in_generator_context() && self.in_parameter_default_context() {
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'yield' expressions cannot be used in a parameter initializer.",
                        diagnostic_codes::YIELD_IN_PARAMETER_DEFAULT,
                    );
                    // Fall through to parse as yield expression
                }

                self.next_token();

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
            _ => self.parse_postfix_expression(),
        }
    }

    /// Parse postfix expression
    pub(crate) fn parse_postfix_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut expr = self.parse_left_hand_side_expression();

        // Handle postfix operators
        if !self.scanner.has_preceding_line_break()
            && (self.is_token(SyntaxKind::PlusPlusToken)
                || self.is_token(SyntaxKind::MinusMinusToken))
        {
            let operator = self.token() as u16;
            self.next_token();
            let end_pos = self.token_end();

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

    /// Parse left-hand side expression (member access, call, etc.)
    pub(crate) fn parse_left_hand_side_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut expr = self.parse_primary_expression();

        loop {
            match self.token() {
                SyntaxKind::DotToken => {
                    self.next_token();
                    // Handle both regular identifiers and private identifiers (#name)
                    let name = if self.is_token(SyntaxKind::PrivateIdentifier) {
                        self.parse_private_identifier()
                    } else if self.is_identifier_or_keyword() {
                        self.parse_identifier_name()
                    } else {
                        self.error_identifier_expected();
                        NodeIndex::NONE
                    };
                    let end_pos = self.token_end();

                    expr = self.arena.add_access_expr(
                        syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                        start_pos,
                        end_pos,
                        AccessExprData {
                            expression: expr,
                            name_or_argument: name,
                            question_dot_token: false,
                        },
                    );
                }
                SyntaxKind::OpenBracketToken => {
                    self.next_token();
                    let argument = self.parse_expression();
                    if argument.is_none() {
                        // Emit TS1109 for empty brackets or invalid expression: obj[[missing]]
                        self.error_expression_expected();
                    }
                    let end_pos = self.token_end();
                    self.parse_expected(SyntaxKind::CloseBracketToken);

                    expr = self.arena.add_access_expr(
                        syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION,
                        start_pos,
                        end_pos,
                        AccessExprData {
                            expression: expr,
                            name_or_argument: argument,
                            question_dot_token: false,
                        },
                    );
                }
                SyntaxKind::OpenParenToken => {
                    let callee_expr = expr;
                    self.next_token();
                    let arguments = self.parse_argument_list();
                    let end_pos = self.token_end();
                    self.parse_expected(SyntaxKind::CloseParenToken);

                    let is_optional_chain = self
                        .arena
                        .get(callee_expr)
                        .and_then(|callee_node| self.arena.get_access_expr(callee_node))
                        .is_some_and(|access| access.question_dot_token);
                    let call_expr = self.arena.add_call_expr(
                        syntax_kind_ext::CALL_EXPRESSION,
                        start_pos,
                        end_pos,
                        CallExprData {
                            expression: expr,
                            type_arguments: None,
                            arguments: Some(arguments),
                        },
                    );
                    if is_optional_chain && let Some(call_node) = self.arena.get_mut(call_expr) {
                        call_node.flags |= node_flags::OPTIONAL_CHAIN as u16;
                    }
                    expr = call_expr;
                }
                // Tagged template literals: tag`template` or tag`head${expr}tail`
                SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead => {
                    let template = self.parse_template_literal();
                    let end_pos = self.token_end();

                    expr = self.arena.add_tagged_template(
                        syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION,
                        start_pos,
                        end_pos,
                        TaggedTemplateData {
                            tag: expr,
                            type_arguments: None,
                            template,
                        },
                    );
                }
                // Optional chaining: expr?.prop, expr?.[index], expr?.()
                SyntaxKind::QuestionDotToken => {
                    self.next_token();
                    if self.is_token(SyntaxKind::LessThanToken)
                        && let Some(type_args) = self.try_parse_type_arguments_for_call()
                    {
                        if self.is_token(SyntaxKind::OpenParenToken) {
                            // expr?.<T>()
                            self.next_token();
                            let arguments = self.parse_argument_list();
                            let end_pos = self.token_end();
                            self.parse_expected(SyntaxKind::CloseParenToken);

                            let call_expr = self.arena.add_call_expr(
                                syntax_kind_ext::CALL_EXPRESSION,
                                start_pos,
                                end_pos,
                                CallExprData {
                                    expression: expr,
                                    type_arguments: Some(type_args),
                                    arguments: Some(arguments),
                                },
                            );
                            if let Some(call_node) = self.arena.get_mut(call_expr) {
                                call_node.flags |= node_flags::OPTIONAL_CHAIN as u16;
                            }
                            expr = call_expr;
                            continue;
                        } else if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
                            || self.is_token(SyntaxKind::TemplateHead)
                        {
                            let template = self.parse_template_literal();
                            let end_pos = self.token_end();

                            expr = self.arena.add_tagged_template(
                                syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION,
                                start_pos,
                                end_pos,
                                TaggedTemplateData {
                                    tag: expr,
                                    type_arguments: Some(type_args),
                                    template,
                                },
                            );
                            continue;
                        }
                    }
                    if self.is_token(SyntaxKind::OpenBracketToken) {
                        // expr?.[index]
                        self.next_token();
                        let argument = self.parse_expression();
                        let end_pos = self.token_end();
                        self.parse_expected(SyntaxKind::CloseBracketToken);

                        expr = self.arena.add_access_expr(
                            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION,
                            start_pos,
                            end_pos,
                            AccessExprData {
                                expression: expr,
                                name_or_argument: argument,
                                question_dot_token: true,
                            },
                        );
                    } else if self.is_token(SyntaxKind::OpenParenToken) {
                        // expr?.()
                        self.next_token();
                        let arguments = self.parse_argument_list();
                        let end_pos = self.token_end();
                        self.parse_expected(SyntaxKind::CloseParenToken);

                        let call_expr = self.arena.add_call_expr(
                            syntax_kind_ext::CALL_EXPRESSION,
                            start_pos,
                            end_pos,
                            CallExprData {
                                expression: expr,
                                type_arguments: None,
                                arguments: Some(arguments),
                            },
                        );
                        if let Some(call_node) = self.arena.get_mut(call_expr) {
                            call_node.flags |= node_flags::OPTIONAL_CHAIN as u16;
                        }
                        expr = call_expr;
                    } else {
                        // expr?.prop
                        let name = if self.is_token(SyntaxKind::PrivateIdentifier) {
                            self.parse_private_identifier()
                        } else {
                            self.parse_identifier_name()
                        };
                        let end_pos = self.token_end();

                        expr = self.arena.add_access_expr(
                            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                            start_pos,
                            end_pos,
                            AccessExprData {
                                expression: expr,
                                name_or_argument: name,
                                question_dot_token: true,
                            },
                        );
                    }
                }
                // Non-null assertion: expr!
                SyntaxKind::ExclamationToken => {
                    // Non-null assertion only if no line break before
                    if self.scanner.has_preceding_line_break() {
                        break;
                    }
                    self.next_token();
                    let end_pos = self.token_end();

                    expr = self.arena.add_unary_expr_ex(
                        syntax_kind_ext::NON_NULL_EXPRESSION,
                        start_pos,
                        end_pos,
                        crate::parser::node::UnaryExprDataEx {
                            expression: expr,
                            asterisk_token: false,
                        },
                    );
                }
                // Type arguments followed by call: expr<T>() or expr<T, U>()
                SyntaxKind::LessThanToken => {
                    // Try to parse as type arguments for a call expression
                    // This is tricky because < could be comparison operator
                    if let Some(type_args) = self.try_parse_type_arguments_for_call() {
                        // After type arguments, we expect ( for a call or ` for tagged template
                        if self.is_token(SyntaxKind::OpenParenToken) {
                            self.next_token();
                            let arguments = self.parse_argument_list();
                            let end_pos = self.token_end();
                            self.parse_expected(SyntaxKind::CloseParenToken);

                            expr = self.arena.add_call_expr(
                                syntax_kind_ext::CALL_EXPRESSION,
                                start_pos,
                                end_pos,
                                CallExprData {
                                    expression: expr,
                                    type_arguments: Some(type_args),
                                    arguments: Some(arguments),
                                },
                            );
                        } else if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
                            || self.is_token(SyntaxKind::TemplateHead)
                        {
                            // Tagged template with type arguments: tag<T>`template`
                            let template = self.parse_template_literal();
                            let end_pos = self.token_end();

                            expr = self.arena.add_tagged_template(
                                syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION,
                                start_pos,
                                end_pos,
                                TaggedTemplateData {
                                    tag: expr,
                                    type_arguments: Some(type_args),
                                    template,
                                },
                            );
                        } else {
                            // Not a call - leave type args attached (expression with type args)
                            break;
                        }
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }

        expr
    }

    /// Parse argument list
    pub(crate) fn parse_argument_list(&mut self) -> NodeList {
        let mut args = Vec::new();

        while !self.is_token(SyntaxKind::CloseParenToken) {
            if self.is_token(SyntaxKind::DotDotDotToken) {
                let spread_start = self.token_pos();
                self.next_token();
                let expression = self.parse_assignment_expression();
                if expression.is_none() {
                    // Emit TS1109 for incomplete spread argument: func(...missing)
                    self.error_expression_expected();
                }
                let spread_end = self.token_end();
                let spread = self.arena.add_spread(
                    syntax_kind_ext::SPREAD_ELEMENT,
                    spread_start,
                    spread_end,
                    crate::parser::node::SpreadData { expression },
                );
                args.push(spread);
            } else {
                let arg = self.parse_assignment_expression();
                if arg.is_none() {
                    // Emit TS1109 for missing function argument: func(a, , c)
                    self.error_expression_expected();
                    // Continue parsing for error recovery
                }
                args.push(arg);
            }

            if !self.parse_optional(SyntaxKind::CommaToken) {
                // Missing comma - check if next token looks like another argument
                // If so, emit comma error for better diagnostics
                if self.is_expression_start()
                    && !self.is_token(SyntaxKind::CloseParenToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    self.error_comma_expected();
                    // Continue parsing for error recovery
                } else {
                    break;
                }
            }
        }

        self.make_node_list(args)
    }

    /// Parse primary expression
    pub(crate) fn parse_primary_expression(&mut self) -> NodeIndex {
        match self.token() {
            SyntaxKind::Identifier => self.parse_identifier(),
            SyntaxKind::PrivateIdentifier => self.parse_private_identifier(),
            SyntaxKind::NumericLiteral => self.parse_numeric_literal(),
            SyntaxKind::BigIntLiteral => self.parse_bigint_literal(),
            SyntaxKind::StringLiteral => self.parse_string_literal(),
            SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword => self.parse_boolean_literal(),
            SyntaxKind::NullKeyword => self.parse_null_literal(),
            SyntaxKind::UndefinedKeyword => self.parse_keyword_as_identifier(),
            SyntaxKind::ThisKeyword => self.parse_this_expression(),
            SyntaxKind::SuperKeyword => self.parse_super_expression(),
            SyntaxKind::OpenParenToken => self.parse_parenthesized_expression(),
            SyntaxKind::OpenBracketToken => self.parse_array_literal(),
            SyntaxKind::OpenBraceToken => self.parse_object_literal(),
            SyntaxKind::NewKeyword => self.parse_new_expression(),
            SyntaxKind::FunctionKeyword => self.parse_function_expression(),
            SyntaxKind::ClassKeyword => self.parse_class_expression(),
            SyntaxKind::AtToken => self.parse_decorated_class_expression(),
            SyntaxKind::AsyncKeyword => {
                // async function expression or async arrow function
                if self.look_ahead_is_async_function() {
                    self.parse_async_function_expression()
                } else {
                    // 'async' used as identifier (e.g., variable named async)
                    // Use parse_identifier_name since 'async' is a keyword
                    self.parse_identifier_name()
                }
            }
            SyntaxKind::LessThanToken => self.parse_jsx_element_or_type_assertion(),
            SyntaxKind::NoSubstitutionTemplateLiteral => {
                self.parse_no_substitution_template_literal()
            }
            SyntaxKind::TemplateHead => self.parse_template_expression(),
            // Regex literal - rescan / or /= as regex
            SyntaxKind::SlashToken | SyntaxKind::SlashEqualsToken => self.parse_regex_literal(),
            // Dynamic import or import.meta
            SyntaxKind::ImportKeyword => self.parse_import_expression(),
            // Type keywords and some reserved words can be used as identifiers in expression context
            // e.g., new any[1], new string(), new require() (when require is aliased), etc.
            SyntaxKind::AnyKeyword
            | SyntaxKind::StringKeyword
            | SyntaxKind::NumberKeyword
            | SyntaxKind::BooleanKeyword
            | SyntaxKind::SymbolKeyword
            | SyntaxKind::BigIntKeyword
            | SyntaxKind::ObjectKeyword
            | SyntaxKind::NeverKeyword
            | SyntaxKind::UnknownKeyword
            | SyntaxKind::RequireKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::AwaitKeyword
            | SyntaxKind::YieldKeyword => self.parse_keyword_as_identifier(),
            SyntaxKind::Unknown => {
                // TS1127: Invalid character - emit specific error for invalid characters
                use crate::checker::types::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "Invalid character.",
                    diagnostic_codes::INVALID_CHARACTER,
                );
                let start_pos = self.token_pos();
                let end_pos = self.token_end();
                self.next_token();
                self.arena
                    .add_token(SyntaxKind::Unknown as u16, start_pos, end_pos)
            }
            _ => {
                // Don't consume clause boundaries or expression terminators here.
                // Let callers decide how to recover so constructs like `switch` can resynchronize
                // without losing `case`/`default` tokens.
                if self.is_at_expression_end()
                    || self.is_token(SyntaxKind::CaseKeyword)
                    || self.is_token(SyntaxKind::DefaultKeyword)
                {
                    return NodeIndex::NONE;
                }

                if self.is_identifier_or_keyword() {
                    self.parse_identifier_name()
                } else {
                    // Unknown primary expression - create an error token
                    let start_pos = self.token_pos();
                    let end_pos = self.token_end();

                    self.error_expression_expected();

                    self.next_token();
                    self.arena
                        .add_token(SyntaxKind::Unknown as u16, start_pos, end_pos)
                }
            }
        }
    }

    /// Parse a decorated class expression: `@dec class C { }`
    /// Used when `@` is encountered in expression position.
    fn parse_decorated_class_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let decorators = self.parse_decorators();
        if self.is_token(SyntaxKind::ClassKeyword) || self.is_token(SyntaxKind::AbstractKeyword) {
            self.parse_class_expression_with_decorators(decorators, start_pos)
        } else {
            // Decorators not followed by class - emit error and create error token
            self.error_expression_expected();
            let end_pos = self.token_end();
            self.arena
                .add_token(SyntaxKind::Unknown as u16, start_pos, end_pos)
        }
    }

    /// Parse identifier
    /// Uses zero-copy accessor and only clones when storing
    pub(crate) fn parse_identifier(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();

        // Check for reserved words that cannot be used as identifiers
        // These should emit TS1359 "Identifier expected. '{0}' is a reserved word that cannot be used here."
        if self.is_reserved_word() {
            self.error_reserved_word_identifier();
            // Create a missing identifier placeholder
            return self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                start_pos,
                end_pos,
                IdentifierData {
                    atom: Atom::NONE,
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
                },
            );
        }

        // Check if current token is an identifier or keyword that can be used as identifier
        // This allows contextual keywords (type, interface, package, etc.) to be used as identifiers
        // in appropriate contexts (e.g., type aliases, interface names)
        let (atom, text) = if self.is_identifier_or_keyword() {
            // OPTIMIZATION: Capture atom for O(1) comparison
            let atom = self.scanner.get_token_atom();
            // Use zero-copy accessor and clone only when storing
            let text = self.scanner.get_token_value_ref().to_string();
            self.next_token();
            (atom, text)
        } else {
            self.error_identifier_expected();
            (Atom::NONE, String::new())
        };

        self.arena.add_identifier(
            SyntaxKind::Identifier as u16,
            start_pos,
            end_pos,
            IdentifierData {
                atom,
                escaped_text: text,
                original_text: None,
                type_arguments: None,
            },
        )
    }

    /// Parse identifier name - allows keywords to be used as identifiers
    /// This is used in contexts where keywords are valid identifier names
    /// (e.g., class names, property names, function names)
    pub(crate) fn parse_identifier_name(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        let (atom, text) = if self.is_identifier_or_keyword() {
            // OPTIMIZATION: Capture atom for O(1) comparison
            let atom = self.scanner.get_token_atom();
            let text = self.scanner.get_token_value_ref().to_string();
            self.next_token();
            (atom, text)
        } else {
            self.error_identifier_expected();
            (Atom::NONE, String::new())
        };

        self.arena.add_identifier(
            SyntaxKind::Identifier as u16,
            start_pos,
            end_pos,
            IdentifierData {
                atom,
                escaped_text: text,
                original_text: None,
                type_arguments: None,
            },
        )
    }

    /// Parse private identifier (#name)
    pub(crate) fn parse_private_identifier(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        // OPTIMIZATION: Capture atom for O(1) comparison
        let atom = self.scanner.get_token_atom();
        let text = self.scanner.get_token_value_ref().to_string();
        self.parse_expected(SyntaxKind::PrivateIdentifier);

        self.arena.add_identifier(
            SyntaxKind::PrivateIdentifier as u16,
            start_pos,
            end_pos,
            IdentifierData {
                atom,
                escaped_text: text,
                original_text: None,
                type_arguments: None,
            },
        )
    }

    /// Parse object binding pattern: { x, y: z, ...rest }
    pub(crate) fn parse_object_binding_pattern(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let mut elements = Vec::new();

        while !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let elem_start = self.token_pos();

            // Handle rest element: ...x
            let dot_dot_dot = self.parse_optional(SyntaxKind::DotDotDotToken);

            if dot_dot_dot {
                // Rest element: just name
                let name = self.parse_binding_element_name();
                if name.is_none() {
                    // Emit TS1109 for missing rest binding element: {...missing}
                    self.error_expression_expected();
                }
                let elem_end = self.token_end();
                elements.push(self.arena.add_binding_element(
                    syntax_kind_ext::BINDING_ELEMENT,
                    elem_start,
                    elem_end,
                    crate::parser::node::BindingElementData {
                        dot_dot_dot_token: true,
                        property_name: NodeIndex::NONE,
                        name,
                        initializer: NodeIndex::NONE,
                    },
                ));
            } else {
                // Regular binding element: name or propertyName: name
                let first_name = self.parse_property_name();

                let (property_name, name) = if self.parse_optional(SyntaxKind::ColonToken) {
                    // propertyName: name
                    let name = self.parse_binding_element_name();
                    if name.is_none() {
                        // Emit TS1109 for missing property binding element: {prop: missing}
                        self.error_expression_expected();
                    }
                    (first_name, name)
                } else {
                    // Just name (shorthand)
                    (NodeIndex::NONE, first_name)
                };

                // Optional initializer: = value
                let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
                    let init = self.parse_assignment_expression();
                    if init.is_none() {
                        // Emit TS1109 for missing object binding initializer: {x = missing}
                        self.error_expression_expected();
                    }
                    init
                } else {
                    NodeIndex::NONE
                };

                let elem_end = self.token_end();
                elements.push(self.arena.add_binding_element(
                    syntax_kind_ext::BINDING_ELEMENT,
                    elem_start,
                    elem_end,
                    crate::parser::node::BindingElementData {
                        dot_dot_dot_token: false,
                        property_name,
                        name,
                        initializer,
                    },
                ));
            }

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        let end_pos = if self.parse_expected(SyntaxKind::CloseBraceToken) {
            self.token_end()
        } else {
            // Recover by advancing until we see a closing brace or EOF to avoid infinite loops.
            while !self.is_token(SyntaxKind::CloseBraceToken)
                && !self.is_token(SyntaxKind::EndOfFileToken)
            {
                self.next_token();
            }
            if self.is_token(SyntaxKind::CloseBraceToken) {
                let end = self.token_end();
                self.next_token();
                end
            } else {
                self.token_end()
            }
        };

        self.arena.add_binding_pattern(
            syntax_kind_ext::OBJECT_BINDING_PATTERN,
            start_pos,
            end_pos,
            crate::parser::node::BindingPatternData {
                elements: self.make_node_list(elements),
            },
        )
    }

    /// Parse array binding pattern: [x, y, ...rest]
    pub(crate) fn parse_array_binding_pattern(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBracketToken);

        let mut elements = Vec::new();

        while !self.is_token(SyntaxKind::CloseBracketToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let elem_start = self.token_pos();

            // Handle omitted element: [, , x]
            if self.is_token(SyntaxKind::CommaToken) {
                // Omitted element - push NONE as placeholder
                elements.push(NodeIndex::NONE);
                self.next_token();
                continue;
            }

            // Handle rest element: ...x
            let dot_dot_dot = self.parse_optional(SyntaxKind::DotDotDotToken);

            // Parse name (can be identifier or nested binding pattern)
            let name = self.parse_binding_element_name();
            if name.is_none() {
                // Emit TS1109 for missing binding element: [...missing] or [missing]
                self.error_expression_expected();
            }

            // Optional initializer: = value
            let initializer = if !dot_dot_dot && self.parse_optional(SyntaxKind::EqualsToken) {
                let init = self.parse_assignment_expression();
                if init.is_none() {
                    // Emit TS1109 for missing binding initializer: [x = missing]
                    self.error_expression_expected();
                }
                init
            } else {
                NodeIndex::NONE
            };

            let elem_end = self.token_end();
            elements.push(self.arena.add_binding_element(
                syntax_kind_ext::BINDING_ELEMENT,
                elem_start,
                elem_end,
                crate::parser::node::BindingElementData {
                    dot_dot_dot_token: dot_dot_dot,
                    property_name: NodeIndex::NONE,
                    name,
                    initializer,
                },
            ));

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseBracketToken);

        self.arena.add_binding_pattern(
            syntax_kind_ext::ARRAY_BINDING_PATTERN,
            start_pos,
            end_pos,
            crate::parser::node::BindingPatternData {
                elements: self.make_node_list(elements),
            },
        )
    }

    /// Parse binding element name (can be identifier or nested binding pattern)
    pub(crate) fn parse_binding_element_name(&mut self) -> NodeIndex {
        // Check for illegal binding identifiers (e.g., 'await' in static blocks)
        self.check_illegal_binding_identifier();

        if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_object_binding_pattern()
        } else if self.is_token(SyntaxKind::OpenBracketToken) {
            self.parse_array_binding_pattern()
        } else if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            self.parse_identifier()
        }
    }

    /// Parse numeric literal
    /// Uses zero-copy accessor for parsing, clones only when storing
    pub(crate) fn parse_numeric_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        let text = self.scanner.get_token_value_ref().to_string();

        // Check if this numeric literal has an invalid separator (for TS1351 check)
        let has_invalid_separator =
            (self.scanner.get_token_flags() & TokenFlags::ContainsInvalidSeparator as u32) != 0;

        self.report_invalid_numeric_separator();
        let value = if text.as_bytes().contains(&b'_') {
            let mut sanitized = String::with_capacity(text.len());
            for &byte in text.as_bytes() {
                if byte != b'_' {
                    sanitized.push(byte as char);
                }
            }
            sanitized.parse::<f64>().ok()
        } else {
            text.parse::<f64>().ok()
        };
        self.next_token();

        // TS1351: If a numeric literal has an invalid separator and is immediately
        // followed by an identifier or keyword, report "identifier cannot follow numeric literal"
        // In this case, skip the identifier to avoid "Cannot find name" error (TS2304)
        if has_invalid_separator && self.is_identifier_or_keyword() {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "An identifier or keyword cannot immediately follow a numeric literal.",
                diagnostic_codes::IDENTIFIER_AFTER_NUMERIC_LITERAL,
            );
            // Skip the identifier to prevent cascading TS2304 errors
            self.next_token();
        }

        self.arena.add_literal(
            SyntaxKind::NumericLiteral as u16,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: None,
                value,
            },
        )
    }

    /// Parse bigint literal
    /// Uses zero-copy accessor, stores the raw text (e.g. "123n")
    pub(crate) fn parse_bigint_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let end_pos = self.token_end();
        let text = self.scanner.get_token_value_ref().to_string();
        self.report_invalid_numeric_separator();
        self.next_token();

        self.arena.add_literal(
            SyntaxKind::BigIntLiteral as u16,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: None,
                value: None,
            },
        )
    }

    pub(crate) fn report_invalid_numeric_separator(&mut self) {
        if (self.scanner.get_token_flags() & TokenFlags::ContainsInvalidSeparator as u32) == 0 {
            return;
        }

        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};
        let (message, code) = if self.scanner.invalid_separator_is_consecutive() {
            (
                diagnostic_messages::MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_NOT_PERMITTED,
                diagnostic_codes::MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_NOT_PERMITTED,
            )
        } else {
            (
                diagnostic_messages::NUMERIC_SEPARATORS_NOT_ALLOWED_HERE,
                diagnostic_codes::NUMERIC_SEPARATORS_NOT_ALLOWED_HERE,
            )
        };

        if let Some(pos) = self.scanner.get_invalid_separator_pos() {
            self.parse_error_at(pos as u32, 1, message, code);
        } else {
            self.parse_error_at_current_token(message, code);
        }
    }

    /// Parse boolean literal
    pub(crate) fn parse_boolean_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        let kind = self.token();
        self.next_token();

        self.arena.add_token(kind as u16, start_pos, end_pos)
    }

    /// Parse null literal
    pub(crate) fn parse_null_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        self.next_token();

        self.arena
            .add_token(SyntaxKind::NullKeyword as u16, start_pos, end_pos)
    }

    /// Parse this expression
    pub(crate) fn parse_this_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        self.next_token();

        self.arena
            .add_token(SyntaxKind::ThisKeyword as u16, start_pos, end_pos)
    }

    /// Parse super expression
    pub(crate) fn parse_super_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture end position BEFORE consuming the token
        let end_pos = self.token_end();
        self.next_token();

        self.arena
            .add_token(SyntaxKind::SuperKeyword as u16, start_pos, end_pos)
    }

    /// Parse regex literal: /pattern/flags
    pub(crate) fn parse_regex_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Rescan the / or /= as a regex literal
        self.scanner.re_scan_slash_token();
        self.current_token = self.scanner.get_token();

        // Get the regex text (including slashes and flags)
        let text = self.scanner.get_token_value_ref().to_string();

        // Capture regex flag errors BEFORE calling parse_expected (which clears them via next_token)
        let flag_errors: Vec<_> = self.scanner.get_regex_flag_errors().to_vec();

        self.parse_expected(SyntaxKind::RegularExpressionLiteral);
        let end_pos = self.token_end();

        // Emit errors for all regex flag issues detected by scanner
        for error in flag_errors {
            let (message, code) = match error.kind {
                crate::scanner_impl::RegexFlagErrorKind::Duplicate => {
                    ("Duplicate regular expression flag.", 1500)
                }
                crate::scanner_impl::RegexFlagErrorKind::InvalidFlag => {
                    ("Unknown regular expression flag.", 1499)
                }
                crate::scanner_impl::RegexFlagErrorKind::IncompatibleFlags => (
                    "The Unicode 'u' flag and the Unicode Sets 'v' flag cannot be set simultaneously.",
                    1502,
                ),
            };
            self.parse_error_at(error.pos as u32, 1, message, code);
        }

        self.arena.add_literal(
            SyntaxKind::RegularExpressionLiteral as u16,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: None,
                value: None,
            },
        )
    }

    /// Parse import expression: import(...) or import.meta
    pub(crate) fn parse_import_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ImportKeyword);

        // Check for import.meta
        if self.is_token(SyntaxKind::DotToken) {
            self.next_token(); // consume '.'
            // Create import keyword node first (before borrowing arena again)
            let import_node =
                self.arena
                    .add_token(SyntaxKind::ImportKeyword as u16, start_pos, start_pos + 6);
            // Parse 'meta'
            let name = self.parse_identifier_name();
            let end_pos = self.token_end();

            return self.arena.add_access_expr(
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                start_pos,
                end_pos,
                crate::parser::node::AccessExprData {
                    expression: import_node,
                    question_dot_token: false,
                    name_or_argument: name,
                },
            );
        }

        // Dynamic import: import(...)
        self.parse_expected(SyntaxKind::OpenParenToken);
        let argument = self.parse_assignment_expression();

        // Optional second argument (import attributes in some proposals)
        let options = if self.parse_optional(SyntaxKind::CommaToken) {
            if !self.is_token(SyntaxKind::CloseParenToken) {
                Some(self.parse_assignment_expression())
            } else {
                None // Trailing comma
            }
        } else {
            None
        };

        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Create a call expression with import as the callee
        let import_keyword =
            self.arena
                .add_token(SyntaxKind::ImportKeyword as u16, start_pos, start_pos + 6);
        let mut args = vec![argument];
        if let Some(opt) = options {
            args.push(opt);
        }
        let arguments = self.make_node_list(args);

        self.arena.add_call_expr(
            syntax_kind_ext::CALL_EXPRESSION,
            start_pos,
            end_pos,
            crate::parser::node::CallExprData {
                expression: import_keyword,
                type_arguments: None,
                arguments: Some(arguments),
            },
        )
    }

    /// Parse no-substitution template literal: `hello`
    pub(crate) fn parse_no_substitution_template_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let is_unterminated = self.scanner.is_unterminated();
        let text = self.scanner.get_token_value_ref().to_string();
        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::NoSubstitutionTemplateLiteral);
        if is_unterminated {
            self.error_unterminated_template_literal_at(start_pos, end_pos);
        }

        self.arena.add_literal(
            SyntaxKind::NoSubstitutionTemplateLiteral as u16,
            start_pos,
            end_pos,
            LiteralData {
                text,
                raw_text: None,
                value: None,
            },
        )
    }

    /// Parse template expression: `hello ${name}!`
    pub(crate) fn parse_template_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse template head: `hello ${
        let head_text = self.scanner.get_token_value_ref().to_string();
        let head_start = self.token_pos();
        let head_end = self.token_end();
        self.parse_expected(SyntaxKind::TemplateHead);

        let head = self.arena.add_literal(
            SyntaxKind::TemplateHead as u16,
            head_start,
            head_end,
            LiteralData {
                text: head_text,
                raw_text: None,
                value: None,
            },
        );

        // Parse template spans
        let mut spans = Vec::new();
        let end_pos = loop {
            // Parse expression in ${ }
            let expression = self.parse_expression();

            // Check for missing expression in template literal: `prefix${}tail`
            if expression.is_none() {
                self.error_expression_expected();
                // Continue parsing for error recovery
            }

            if !self.is_token(SyntaxKind::CloseBraceToken) {
                // Unterminated template expression - report and synthesize tail to avoid looping.
                self.error_token_expected("}");
                let literal_start = self.token_pos();
                let literal_end = self.token_end();
                let literal = self.arena.add_literal(
                    SyntaxKind::TemplateTail as u16,
                    literal_start,
                    literal_end,
                    LiteralData {
                        text: String::new(),
                        raw_text: None,
                        value: None,
                    },
                );
                let span_start = self
                    .arena
                    .get(expression)
                    .map(|node| node.pos)
                    .unwrap_or(literal_start);
                let span = self.arena.add_template_span(
                    syntax_kind_ext::TEMPLATE_SPAN,
                    span_start,
                    literal_end,
                    TemplateSpanData {
                        expression,
                        literal,
                    },
                );
                spans.push(span);
                break literal_end;
            }

            // Now we need to rescan the } as a template continuation
            // The scanner needs to be told to rescan as template
            self.scanner.re_scan_template_token(false);
            self.current_token = self.scanner.get_token();

            // Parse template middle or tail
            let literal_start = self.token_pos();
            let is_tail = self.is_token(SyntaxKind::TemplateTail);
            let is_middle = self.is_token(SyntaxKind::TemplateMiddle);
            if !is_tail && !is_middle {
                // Unexpected token after template span - report and finish.
                self.error_token_expected("`");
                let literal_end = self.token_end();
                let literal = self.arena.add_literal(
                    SyntaxKind::TemplateTail as u16,
                    literal_start,
                    literal_end,
                    LiteralData {
                        text: String::new(),
                        raw_text: None,
                        value: None,
                    },
                );
                let span_start = self
                    .arena
                    .get(expression)
                    .map(|node| node.pos)
                    .unwrap_or(literal_start);
                let span = self.arena.add_template_span(
                    syntax_kind_ext::TEMPLATE_SPAN,
                    span_start,
                    literal_end,
                    TemplateSpanData {
                        expression,
                        literal,
                    },
                );
                spans.push(span);
                break literal_end;
            }

            let is_unterminated = self.scanner.is_unterminated();
            let literal_text = self.scanner.get_token_value_ref().to_string();
            let literal_kind = if is_tail {
                SyntaxKind::TemplateTail
            } else {
                SyntaxKind::TemplateMiddle
            };

            let literal_end = self.token_end();
            self.next_token();

            let literal = self.arena.add_literal(
                literal_kind as u16,
                literal_start,
                literal_end,
                LiteralData {
                    text: literal_text,
                    raw_text: None,
                    value: None,
                },
            );
            if is_unterminated {
                self.error_unterminated_template_literal_at(literal_start, literal_end);
            }

            let span_start = if let Some(node) = self.arena.get(expression) {
                node.pos
            } else {
                literal_start
            };
            let span = self.arena.add_template_span(
                syntax_kind_ext::TEMPLATE_SPAN,
                span_start,
                literal_end,
                TemplateSpanData {
                    expression,
                    literal,
                },
            );
            spans.push(span);

            if is_tail {
                break literal_end;
            }
        };

        self.arena.add_template_expr(
            syntax_kind_ext::TEMPLATE_EXPRESSION,
            start_pos,
            end_pos,
            TemplateExprData {
                head,
                template_spans: self.make_node_list(spans),
            },
        )
    }

    /// Parse template literal (either no-substitution or full template expression)
    /// Used for both standalone template literals and as the template part of tagged templates
    pub(crate) fn parse_template_literal(&mut self) -> NodeIndex {
        if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral) {
            self.parse_no_substitution_template_literal()
        } else {
            self.parse_template_expression()
        }
    }

    /// Parse parenthesized expression
    pub(crate) fn parse_parenthesized_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenParenToken);
        let expression = self.parse_expression();
        if expression.is_none() {
            // Emit TS1109 for empty parentheses or invalid expression: ([missing])
            self.error_expression_expected();
        }
        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseParenToken);

        self.arena.add_parenthesized(
            syntax_kind_ext::PARENTHESIZED_EXPRESSION,
            start_pos,
            end_pos,
            ParenthesizedData { expression },
        )
    }

    /// Parse array literal
    pub(crate) fn parse_array_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBracketToken);

        let mut elements = Vec::new();
        while !self.is_token(SyntaxKind::CloseBracketToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            if self.is_token(SyntaxKind::CommaToken) {
                // Elided element
                elements.push(NodeIndex::NONE);
            } else if self.is_token(SyntaxKind::DotDotDotToken) {
                // Spread element: ...expr
                let spread_start = self.token_pos();
                self.next_token();
                let expression = self.parse_assignment_expression();
                if expression.is_none() {
                    // Emit TS1109 for incomplete spread element: [...missing]
                    self.error_expression_expected();
                }
                let spread_end = self.token_end();
                let spread = self.arena.add_spread(
                    syntax_kind_ext::SPREAD_ELEMENT,
                    spread_start,
                    spread_end,
                    crate::parser::node::SpreadData { expression },
                );
                elements.push(spread);
            } else {
                let elem = self.parse_assignment_expression();
                if elem.is_none() {
                    // Emit TS1109 for missing array element: [a, , ] vs [a, b]
                    self.error_expression_expected();
                    // Continue parsing with empty element for error recovery
                }
                elements.push(elem);
            }

            if !self.parse_optional(SyntaxKind::CommaToken) {
                // Missing comma - check if next token looks like another array element
                // If so, emit error and continue parsing (better recovery)
                if self.is_expression_start()
                    && !self.is_token(SyntaxKind::CloseBracketToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    // We have an element-like token but no comma - likely missing comma
                    // Emit the comma error and continue parsing for better recovery
                    // This handles cases like: [1 2 3] instead of [1, 2, 3]
                    self.error_comma_expected();
                } else {
                    // Not followed by an element, so we're really done
                    break;
                }
            }
        }

        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseBracketToken);

        self.arena.add_literal_expr(
            syntax_kind_ext::ARRAY_LITERAL_EXPRESSION,
            start_pos,
            end_pos,
            LiteralExprData {
                elements: self.make_node_list(elements),
                multi_line: false,
            },
        )
    }

    /// Check if current token can start an object property
    /// Used for error recovery in object literals when commas are missing
    pub(crate) fn is_property_start(&self) -> bool {
        match self.token() {
            // Spread operator
            SyntaxKind::DotDotDotToken => true,
            // Get/Set accessors
            SyntaxKind::GetKeyword | SyntaxKind::SetKeyword => true,
            // Async keyword (for async methods)
            SyntaxKind::AsyncKeyword => true,
            // Asterisk (for generator methods)
            SyntaxKind::AsteriskToken => true,
            // String/number literals (computed properties or shorthand)
            SyntaxKind::StringLiteral | SyntaxKind::NumericLiteral | SyntaxKind::BigIntLiteral => {
                true
            }
            // Identifier or keyword (property names)
            SyntaxKind::Identifier => true,
            // Bracket (computed property)
            SyntaxKind::OpenBracketToken => true,
            _ => self.is_identifier_or_keyword(),
        }
    }

    /// Parse object literal
    pub(crate) fn parse_object_literal(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        let mut properties = Vec::new();
        while !self.is_token(SyntaxKind::CloseBraceToken) {
            let prop = self.parse_property_assignment();
            if !prop.is_none() {
                properties.push(prop);
            }

            // Try to parse comma separator
            if !self.parse_optional(SyntaxKind::CommaToken) {
                // Missing comma - check if next token looks like another property
                // If so, emit error and continue parsing (better recovery)
                if self.is_property_start() && !self.is_token(SyntaxKind::CloseBraceToken) {
                    // We have a property-like token but no comma - likely missing comma
                    // Emit the comma error and continue parsing for better recovery
                    // This handles cases like: {a: 1 b: 2} instead of {a: 1, b: 2}
                    self.error_comma_expected();
                } else {
                    // Not followed by a property, so we're really done
                    break;
                }
            }
        }

        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseBraceToken);

        self.arena.add_literal_expr(
            syntax_kind_ext::OBJECT_LITERAL_EXPRESSION,
            start_pos,
            end_pos,
            LiteralExprData {
                elements: self.make_node_list(properties),
                multi_line: false,
            },
        )
    }

    /// Parse property assignment, method, getter, setter, or spread element
    pub(crate) fn parse_property_assignment(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Handle spread element: ...expr
        if self.is_token(SyntaxKind::DotDotDotToken) {
            self.next_token();
            let expression = self.parse_assignment_expression();
            if expression.is_none() {
                // Emit TS1109 for incomplete spread element: {...missing}
                self.error_expression_expected();
            }
            let end_pos = self.token_end();
            return self.arena.add_spread(
                syntax_kind_ext::SPREAD_ASSIGNMENT,
                start_pos,
                end_pos,
                crate::parser::node::SpreadData { expression },
            );
        }

        // NOTE: public/private/protected are contextual keywords in object literals.
        // When followed by get/set/async, they're parsed as modifiers and TS1042 is reported.
        // Otherwise, they're parsed as property names (contextual keywords).
        if matches!(
            self.token(),
            SyntaxKind::PrivateKeyword | SyntaxKind::ProtectedKeyword | SyntaxKind::PublicKeyword
        ) {
            // Look ahead to check if this is followed by an accessor or method
            let snapshot = self.scanner.save_state();
            let current = self.current_token;
            self.next_token(); // skip public/private/protected

            // Check if followed by get/set/async (accessor or method modifier)
            let is_accessor_or_method = matches!(
                self.token(),
                SyntaxKind::GetKeyword | SyntaxKind::SetKeyword | SyntaxKind::AsyncKeyword
            );

            self.scanner.restore_state(snapshot);
            self.current_token = current;

            if is_accessor_or_method {
                use crate::checker::types::diagnostics::diagnostic_codes;
                // Report TS1042 for the specific modifier
                let modifier_name = match self.token() {
                    SyntaxKind::PublicKeyword => "'public'",
                    SyntaxKind::PrivateKeyword => "'private'",
                    SyntaxKind::ProtectedKeyword => "'protected'",
                    _ => "modifier",
                };
                self.parse_error_at_current_token(
                    &format!("{} modifier cannot be used here.", modifier_name),
                    diagnostic_codes::ASYNC_MODIFIER_CANNOT_BE_USED_HERE, // TS1042
                );
                self.next_token(); // consume the modifier
                // Continue parsing - the next token should be get/set/async
            }
            // If not followed by accessor/method, treat as property name (fall through)
        }

        // Handle get accessor: get foo() { }
        if self.is_token(SyntaxKind::GetKeyword) && self.look_ahead_is_object_method() {
            return self.parse_object_get_accessor(start_pos);
        }

        // Handle set accessor: set foo(v) { }
        if self.is_token(SyntaxKind::SetKeyword) && self.look_ahead_is_object_method() {
            return self.parse_object_set_accessor(start_pos);
        }

        // Handle async method: async foo() { }
        if self.is_token(SyntaxKind::AsyncKeyword) && self.look_ahead_is_object_method() {
            return self.parse_object_method(start_pos, true, false);
        }

        // Handle generator method: *foo() { }
        if self.is_token(SyntaxKind::AsteriskToken) {
            self.next_token(); // consume '*'
            return self.parse_object_method(start_pos, false, true);
        }

        if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
            || self.is_token(SyntaxKind::TemplateHead)
        {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Property assignment expected.",
                diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED,
            );
            let name = self.parse_template_literal();
            let initializer = if self.parse_optional(SyntaxKind::ColonToken) {
                self.parse_assignment_expression()
            } else {
                name
            };
            let end_pos = self.token_end();
            return self.arena.add_property_assignment(
                syntax_kind_ext::PROPERTY_ASSIGNMENT,
                start_pos,
                end_pos,
                crate::parser::node::PropertyAssignmentData {
                    modifiers: None,
                    name,
                    initializer,
                },
            );
        }

        // Check if the property name requires `:` syntax (can't be a shorthand property).
        // Shorthand properties only work with identifiers, not:
        // - Reserved words (class, function, etc.)
        // - String literals ("key")
        // - Numeric literals (0, 1, etc.)
        let requires_colon = self.is_reserved_word()
            || self.is_token(SyntaxKind::StringLiteral)
            || self.is_token(SyntaxKind::NumericLiteral);

        let name = self.parse_property_name();

        // Handle method: foo() { } or foo<T>() { }
        if self.is_token(SyntaxKind::OpenParenToken) || self.is_token(SyntaxKind::LessThanToken) {
            return self.parse_object_method_after_name(start_pos, name, false, false);
        }

        // Check for optional property marker '?' - not allowed in object literals
        // TSC emits TS1162: "An object member cannot be declared optional."
        if self.is_token(SyntaxKind::QuestionToken) {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "An object member cannot be declared optional.",
                diagnostic_codes::OBJECT_MEMBER_CANNOT_BE_OPTIONAL,
            );
            self.next_token(); // Skip the '?' for error recovery
        }

        let initializer = if self.parse_optional(SyntaxKind::ColonToken) {
            let expr = self.parse_assignment_expression();
            if expr.is_none() {
                // Emit TS1109 for missing property value: { prop: }
                self.error_expression_expected();
                name // Use property name as fallback for error recovery
            } else {
                expr
            }
        } else {
            // Shorthand property - but certain property names require `:` syntax
            if requires_colon {
                use crate::checker::types::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "':' expected.",
                    diagnostic_codes::TOKEN_EXPECTED,
                );
            }
            name
        };

        let end_pos = self.token_end();
        self.arena.add_property_assignment(
            syntax_kind_ext::PROPERTY_ASSIGNMENT,
            start_pos,
            end_pos,
            crate::parser::node::PropertyAssignmentData {
                modifiers: None,
                name,
                initializer,
            },
        )
    }

    /// Look ahead to check if get/set/async is a method vs property name
    pub(crate) fn look_ahead_is_object_method(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip get/set/async

        // If there's a line break after get/set/async, it's treated as a property name
        // (shorthand property), not as an accessor or async modifier.
        // This matches TypeScript's ASI behavior.
        if self.scanner.has_preceding_line_break() {
            self.scanner.restore_state(snapshot);
            self.current_token = current;
            return false;
        }

        // Check if followed by property name (identifier, keyword, string, number, [)
        // Keywords like 'return', 'throw', 'delete' can be method names
        let is_method = self.is_token(SyntaxKind::Identifier)
            || self.is_token(SyntaxKind::StringLiteral)
            || self.is_token(SyntaxKind::NumericLiteral)
            || self.is_token(SyntaxKind::OpenBracketToken)
            || self.is_token(SyntaxKind::AsteriskToken) // async *foo()
            || self.is_identifier_or_keyword(); // keywords as method names

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_method
    }

    /// Parse get accessor in object literal: get foo() { }
    pub(crate) fn parse_object_get_accessor(&mut self, start_pos: u32) -> NodeIndex {
        self.next_token(); // consume 'get'
        let name = self.parse_property_name();

        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "An accessor cannot have type parameters.",
                diagnostic_codes::ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS,
            );
            Some(self.parse_type_parameters())
        } else {
            None
        };

        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if self.is_token(SyntaxKind::CloseParenToken) {
            self.make_node_list(vec![])
        } else {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "A 'get' accessor cannot have parameters.",
                diagnostic_codes::GETTER_MUST_NOT_HAVE_PARAMETERS,
            );
            self.parse_parameter_list()
        };
        // Save end of ) for error reporting - get it BEFORE consuming the token
        let close_paren_end = self.token_end();
        self.parse_expected(SyntaxKind::CloseParenToken);

        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };
        // If there's a type annotation, use its end; otherwise use close paren end
        let signature_end = if !type_annotation.is_none() {
            self.token_pos()
        } else {
            close_paren_end
        };

        // Parse body if present. Missing body is reported in grammar check, not here.
        // This matches TypeScript's behavior of allowing ASI and checking later.
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            NodeIndex::NONE
        };

        // End position: use token_end for normal case, signature_end for missing body
        let end_pos = if body.is_none() {
            signature_end
        } else {
            self.token_end()
        };
        self.arena.add_accessor(
            syntax_kind_ext::GET_ACCESSOR,
            start_pos,
            end_pos,
            crate::parser::node::AccessorData {
                modifiers: None,
                name,
                type_parameters,
                parameters,
                type_annotation,
                body,
            },
        )
    }

    /// Parse set accessor in object literal: set foo(v) { }
    pub(crate) fn parse_object_set_accessor(&mut self, start_pos: u32) -> NodeIndex {
        self.next_token(); // consume 'set'
        let name = self.parse_property_name();

        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "An accessor cannot have type parameters.",
                diagnostic_codes::ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS,
            );
            Some(self.parse_type_parameters())
        } else {
            None
        };

        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if self.is_token(SyntaxKind::CloseParenToken) {
            self.make_node_list(vec![])
        } else {
            self.parse_parameter_list()
        };
        // Save end of ) for error reporting - get it BEFORE consuming the token
        let close_paren_end = self.token_end();
        self.parse_expected(SyntaxKind::CloseParenToken);

        if parameters.len() != 1 {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "A 'set' accessor must have exactly one parameter.",
                diagnostic_codes::SETTER_MUST_HAVE_EXACTLY_ONE_PARAMETER,
            );
        }

        if self.parse_optional(SyntaxKind::ColonToken) {
            use crate::checker::types::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "A 'set' accessor cannot have a return type annotation.",
                diagnostic_codes::SETTER_CANNOT_HAVE_RETURN_TYPE,
            );
            let _ = self.parse_type();
        }

        // Parse body if present. Missing body is reported in grammar check, not here.
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            NodeIndex::NONE
        };

        // End position: use token_end for normal case, close_paren_end for missing body
        let end_pos = if body.is_none() {
            close_paren_end
        } else {
            self.token_end()
        };
        self.arena.add_accessor(
            syntax_kind_ext::SET_ACCESSOR,
            start_pos,
            end_pos,
            crate::parser::node::AccessorData {
                modifiers: None,
                name,
                type_parameters,
                parameters,
                type_annotation: NodeIndex::NONE,
                body,
            },
        )
    }

    /// Parse method in object literal: foo() { } or async foo() { } or *foo() { }
    pub(crate) fn parse_object_method(
        &mut self,
        start_pos: u32,
        is_async: bool,
        is_generator: bool,
    ) -> NodeIndex {
        // Build modifiers if async
        let modifiers = if is_async {
            self.next_token(); // consume 'async'
            let mod_idx = self
                .arena
                .create_modifier(SyntaxKind::AsyncKeyword, start_pos);
            Some(self.make_node_list(vec![mod_idx]))
        } else {
            None
        };

        // Check for generator after async: async *foo()
        // or standalone generator: *foo()
        let asterisk = if is_generator {
            // Asterisk already consumed by caller for standalone generator
            true
        } else if self.parse_optional(SyntaxKind::AsteriskToken) {
            // async *foo() - consume asterisk here
            true
        } else {
            false
        };

        let name = self.parse_property_name();
        self.parse_object_method_after_name(start_pos, name, asterisk, modifiers.is_some())
    }

    /// Parse method after name has been parsed
    pub(crate) fn parse_object_method_after_name(
        &mut self,
        start_pos: u32,
        name: NodeIndex,
        asterisk: bool,
        is_async: bool,
    ) -> NodeIndex {
        // Optional type parameters
        let type_parameters = if self.is_token(SyntaxKind::LessThanToken) {
            Some(self.parse_type_parameters())
        } else {
            None
        };

        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        // Set context flags for async/generator to properly parse await/yield in method bodies.
        let saved_flags = self.context_flags;
        if is_async {
            self.context_flags |= CONTEXT_FLAG_ASYNC;
        }
        if asterisk {
            self.context_flags |= CONTEXT_FLAG_GENERATOR;
        }

        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            NodeIndex::NONE
        };

        // Restore context flags after parsing body.
        self.context_flags = saved_flags;

        let modifiers = if is_async {
            let mod_idx = self
                .arena
                .create_modifier(SyntaxKind::AsyncKeyword, start_pos);
            Some(self.make_node_list(vec![mod_idx]))
        } else {
            None
        };

        let end_pos = self.token_end();
        self.arena.add_method_decl(
            syntax_kind_ext::METHOD_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::node::MethodDeclData {
                modifiers,
                asterisk_token: asterisk,
                name,
                question_token: false,
                type_parameters,
                parameters,
                type_annotation,
                body,
            },
        )
    }

    /// Parse property name (identifier, string literal, numeric literal, computed)
    pub(crate) fn parse_property_name(&mut self) -> NodeIndex {
        match self.token() {
            SyntaxKind::StringLiteral => {
                // String literal can be property name: { "key": value }
                self.parse_string_literal()
            }
            SyntaxKind::NumericLiteral => {
                // Numeric literal can be property name: { 0: value }
                self.parse_numeric_literal()
            }
            SyntaxKind::OpenBracketToken => {
                // Computed property name: { [expr]: value }
                let start_pos = self.token_pos();
                self.next_token();

                // Note: await in computed property name is NOT a parser error
                // The type checker will emit TS2304 if 'await' is not in scope
                // Example: { [await]: foo } should only emit TS2304, not TS1109

                let expression = self.parse_expression();
                if expression.is_none() {
                    // Emit TS1109 for empty computed property: { [[missing]]: value }
                    self.error_expression_expected();
                }
                self.parse_expected(SyntaxKind::CloseBracketToken);
                let end_pos = self.token_end();

                self.arena.add_computed_property(
                    syntax_kind_ext::COMPUTED_PROPERTY_NAME,
                    start_pos,
                    end_pos,
                    crate::parser::node::ComputedPropertyData { expression },
                )
            }
            SyntaxKind::PrivateIdentifier => {
                // Private identifier: #name
                self.parse_private_identifier()
            }
            _ => {
                // Identifier or keyword used as property name
                // But first check if it's actually a valid identifier/keyword
                let start_pos = self.token_pos();
                let is_identifier_or_keyword = self.is_identifier_or_keyword();

                if !is_identifier_or_keyword {
                    use crate::checker::types::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Property assignment expected.",
                        diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED,
                    );
                }

                // OPTIMIZATION: Capture atom for O(1) comparison
                let atom = self.scanner.get_token_atom();
                // Use zero-copy accessor
                let text = self.scanner.get_token_value_ref().to_string();
                self.next_token(); // Accept any token as property name (error recovery)
                let end_pos = self.token_end();

                self.arena.add_identifier(
                    SyntaxKind::Identifier as u16,
                    start_pos,
                    end_pos,
                    IdentifierData {
                        atom,
                        escaped_text: text,
                        original_text: None,
                        type_arguments: None,
                    },
                )
            }
        }
    }

    /// Parse new expression
    pub(crate) fn parse_new_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::NewKeyword);

        // Handle new.target meta-property
        if self.is_token(SyntaxKind::DotToken) {
            self.next_token(); // consume '.'
            let new_node =
                self.arena
                    .add_token(SyntaxKind::NewKeyword as u16, start_pos, start_pos + 3);
            let name = self.parse_identifier_name();
            let end_pos = self
                .arena
                .get(name)
                .map(|n| n.end)
                .unwrap_or(self.token_end());
            return self.arena.add_access_expr(
                syntax_kind_ext::META_PROPERTY,
                start_pos,
                end_pos,
                crate::parser::node::AccessExprData {
                    expression: new_node,
                    question_dot_token: false,
                    name_or_argument: name,
                },
            );
        }

        // Type assertion syntax (<T>expr) is not valid in new expressions
        // Check if the next token is '<' and report TS1109 if so
        if self.is_token(SyntaxKind::LessThanToken) {
            self.error_expression_expected();
        }

        // Parse the callee expression - member access without call (we handle call ourselves)
        let expression = self.parse_member_expression_base();
        let mut end_pos = self
            .arena
            .get(expression)
            .map(|node| node.end)
            .unwrap_or(self.token_end());

        // Parse type arguments: new Array<string>()
        let type_arguments = if self.is_token(SyntaxKind::LessThanToken) {
            // Try to parse as type arguments
            Some(self.parse_type_arguments())
        } else {
            None
        };
        if let Some(type_args) = type_arguments.as_ref()
            && let Some(last) = type_args.nodes.last()
            && let Some(node) = self.arena.get(*last)
        {
            end_pos = end_pos.max(node.end);
        }

        let arguments = if self.is_token(SyntaxKind::OpenParenToken) {
            self.next_token();
            let args = self.parse_argument_list();
            let call_end = self.token_end();
            self.parse_expected(SyntaxKind::CloseParenToken);
            end_pos = call_end;
            Some(args)
        } else {
            None
        };

        self.arena.add_call_expr(
            syntax_kind_ext::NEW_EXPRESSION,
            start_pos,
            end_pos,
            CallExprData {
                expression,
                type_arguments,
                arguments,
            },
        )
    }

    /// Parse member expression base (identifier with property/element access, but no calls)
    pub(crate) fn parse_member_expression_base(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut expr = self.parse_primary_expression();

        loop {
            match self.token() {
                SyntaxKind::DotToken => {
                    self.next_token();
                    let diag_count_before = self.parse_diagnostics.len();
                    let name = if self.is_token(SyntaxKind::PrivateIdentifier) {
                        self.parse_private_identifier()
                    } else if self.is_identifier_or_keyword() {
                        self.parse_identifier_name()
                    } else {
                        self.error_identifier_expected();
                        NodeIndex::NONE
                    };

                    // If parsing the name produced an error, don't create a property access
                    // expression to avoid spurious semantic errors (e.g., TS2339 for incomplete `this.`)
                    if self.parse_diagnostics.len() > diag_count_before {
                        break;
                    }

                    let end_pos = self.token_end();

                    expr = self.arena.add_access_expr(
                        syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                        start_pos,
                        end_pos,
                        AccessExprData {
                            expression: expr,
                            name_or_argument: name,
                            question_dot_token: false,
                        },
                    );
                }
                SyntaxKind::OpenBracketToken => {
                    self.next_token();
                    let argument = self.parse_expression();
                    if argument.is_none() {
                        // Emit TS1109 for empty brackets or invalid expression: obj[[missing]]
                        self.error_expression_expected();
                    }
                    let end_pos = self.token_end();
                    self.parse_expected(SyntaxKind::CloseBracketToken);

                    expr = self.arena.add_access_expr(
                        syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION,
                        start_pos,
                        end_pos,
                        AccessExprData {
                            expression: expr,
                            name_or_argument: argument,
                            question_dot_token: false,
                        },
                    );
                }
                _ => break,
            }
        }

        expr
    }
}
