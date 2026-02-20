use tsz_common::diagnostics::diagnostic_codes;

/// Parser state - expression parsing methods
use super::state::{
    CONTEXT_FLAG_ARROW_PARAMETERS, CONTEXT_FLAG_ASYNC, CONTEXT_FLAG_GENERATOR,
    CONTEXT_FLAG_IN_CONDITIONAL_TRUE, ParserState,
};
use crate::parser::{
    NodeIndex, NodeList,
    node::{
        AccessExprData, BinaryExprData, CallExprData, ConditionalExprData, FunctionData,
        IdentifierData, TaggedTemplateData, UnaryExprData, UnaryExprDataEx,
    },
    node_flags, syntax_kind_ext,
};
use tsz_common::interner::Atom;
use tsz_scanner::SyntaxKind;

impl ParserState {
    // =========================================================================
    // Parse Methods - Expressions
    // =========================================================================

    // Parse an expression (including comma operator)
    pub fn parse_expression(&mut self) -> NodeIndex {
        // Clear the decorator context when parsing Expression, as it should be
        // unambiguous when parsing a decorator's parenthesized sub-expression.
        // This matches tsc's parseExpression() behavior.
        let saved_flags = self.context_flags;
        self.context_flags &= !crate::parser::state::CONTEXT_FLAG_IN_DECORATOR;

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

        self.context_flags = saved_flags;
        left
    }

    // Parse assignment expression
    pub(crate) fn parse_assignment_expression(&mut self) -> NodeIndex {
        // Check for arrow function first (including async arrow)
        let lookahead_token = self.current_token;
        let lookahead_state = self.scanner.save_state();
        let is_arrow_start = self.is_start_of_arrow_function();
        self.scanner.restore_state(lookahead_state);
        self.current_token = lookahead_token;
        if is_arrow_start {
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

    // Parse async arrow function: async (x) => ... or async x => ...
    pub(crate) fn parse_async_arrow_function_expression(&mut self) -> NodeIndex {
        self.parse_expected(SyntaxKind::AsyncKeyword);
        self.parse_arrow_function_expression_with_async(true)
    }

    // Check if we're at the start of an arrow function
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
            _ => {
                // In generator context, 'yield' is always a yield expression, never an arrow parameter
                // Example: function * foo(a = yield => yield) {} - first 'yield' is expression, not param
                if self.in_generator_context() && self.is_token(SyntaxKind::YieldKeyword) {
                    return false;
                }
                // In async context (including parameter defaults), 'await' cannot start an arrow function
                // Example: async (a = await => x) => {} - 'await' triggers TS1109, not treated as arrow param
                if self.in_async_context() && self.is_token(SyntaxKind::AwaitKeyword) {
                    return false;
                }
                self.is_identifier_or_keyword() && self.look_ahead_is_simple_arrow_function()
            }
        }
    }

    // Look ahead to see if < starts a generic arrow function: <T>(x) => or <T, U>() =>
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

    // Look ahead after async to see if it's an arrow function: async (x) => or async x => or async <T>(x) =>
    //
    // ASI Rule: If there's a line break after 'async', it's NOT an async arrow function.
    // The line break prevents 'async' from being treated as a modifier.
    // Example: `async\nx => x` parses as `async; (x => x);` not as an async arrow function.
    pub(crate) fn look_ahead_is_arrow_function_after_async(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip 'async'
        self.next_token();

        // Check for line break AFTER 'async' — if the next token has a preceding
        // line break, 'async' is not a modifier (ASI applies)
        if self.scanner.has_preceding_line_break() {
            self.scanner.restore_state(snapshot);
            self.current_token = current;
            return false;
        }

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

    // Look ahead to see if ( starts an arrow function: () => or (x) => or (x, y) =>
    //
    // ASI Rule: If there's a line break between ) and =>, it's NOT an arrow function.
    // Example: `(x)\n=> y` should NOT be parsed as an arrow function.
    pub(crate) fn look_ahead_is_arrow_function(&mut self) -> bool {
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
                // Line break before => — still parse as arrow function but TS1200 will
                // be emitted during actual parsing. Empty parens `()` can't be a valid
                // expression, so this must be arrow function params.
                self.is_token(SyntaxKind::EqualsGreaterThanToken)
                    || self.is_token(SyntaxKind::OpenBraceToken)
            } else if self.is_token(SyntaxKind::ColonToken) {
                // (): is definitely an arrow function with a return type annotation.
                // Empty parens () are never a valid expression, so ():
                // can only appear as arrow function parameters + return type.
                // Don't try to parse the return type here - the type parser
                // can greedily consume past the arrow's => (e.g. for function
                // types like `(): (() => T) => body`).
                true
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

        // Check for optional return type annotation.
        // Important: check for `:` (return type) BEFORE checking has_line_break.
        // `(params): type =>` is unambiguously an arrow function regardless of
        // line breaks — TS1200 handles line terminator errors separately.
        let is_arrow = if self.is_token(SyntaxKind::ColonToken) {
            // When we see `:` after `)`, it could be either:
            // 1. A return type annotation for an arrow function: (x): T => body
            // 2. The else separator of a conditional: a ? (x) : y
            // Disambiguate by checking for `=>` after a return type.
            self.next_token();
            let saved_arena_len = self.arena.nodes.len();
            let saved_diagnostics_len = self.parse_diagnostics.len();
            let _ = self.parse_return_type();
            // After parsing the return type, check for `=>` or `{`. Line breaks
            // between the return type and `=>` are allowed here — TS1200 will be
            // emitted during actual parsing. The `(params): type` prefix is
            // unambiguous, so we don't need the line break check.
            let mut result = self.is_token(SyntaxKind::EqualsGreaterThanToken)
                || self.is_token(SyntaxKind::OpenBraceToken);

            // In the true branch of a conditional expression, only accept
            // `(x): T => ...` as an arrow function when the simulated body
            // leaves a `:` token. This matches TypeScript's disambiguation.
            if (self.context_flags & CONTEXT_FLAG_IN_CONDITIONAL_TRUE) != 0 {
                if result && self.is_token(SyntaxKind::EqualsGreaterThanToken) {
                    let body_snapshot = self.scanner.save_state();
                    let body_token = self.current_token;
                    let body_arena_len = self.arena.nodes.len();
                    let body_diagnostics_len = self.parse_diagnostics.len();

                    self.next_token();
                    let _ = self.parse_assignment_expression();
                    result = self.is_token(SyntaxKind::ColonToken)
                        && !self.scanner.has_preceding_line_break();

                    self.arena.nodes.truncate(body_arena_len);
                    self.parse_diagnostics.truncate(body_diagnostics_len);
                    self.scanner.restore_state(body_snapshot);
                    self.current_token = body_token;
                } else {
                    result = false;
                }
            }

            self.arena.nodes.truncate(saved_arena_len);
            self.parse_diagnostics.truncate(saved_diagnostics_len);

            result
        } else if has_line_break {
            // Line break before => — still parse as arrow function but TS1200 will
            // be emitted during actual parsing. Parenthesized params `(x, y)` followed
            // by `=>` are unambiguously arrow function params even with line breaks.
            self.is_token(SyntaxKind::EqualsGreaterThanToken)
                || self.is_token(SyntaxKind::OpenBraceToken)
        } else {
            // Check for => or { (error recovery: user forgot =>)
            self.is_token(SyntaxKind::EqualsGreaterThanToken)
                || self.is_token(SyntaxKind::OpenBraceToken)
        };
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_arrow
    }

    // Look ahead to see if identifier is followed by => (simple arrow function)
    //
    // If there's a line break between the identifier and =>, this is still
    // recognized as an arrow function but TS1200 will be emitted during parsing.
    // `=>` cannot start a statement, so there is no ASI ambiguity.
    pub(crate) fn look_ahead_is_simple_arrow_function(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip identifier
        self.next_token();

        // Check if => follows the identifier. Line breaks before => are
        // allowed here — TS1200 will be emitted during actual parsing.
        let is_arrow = if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
            true
        } else if self.is_token(SyntaxKind::ColonToken)
            && (self.context_flags & CONTEXT_FLAG_IN_CONDITIONAL_TRUE) == 0
        {
            // Support single-parameter typed arrows in lookahead: `x: T => expr`.
            let saved_arena_len = self.arena.nodes.len();
            let saved_diagnostics_len = self.parse_diagnostics.len();

            self.next_token();
            let _ = self.parse_type();
            let result = !self.scanner.has_preceding_line_break()
                && self.is_token(SyntaxKind::EqualsGreaterThanToken);

            self.arena.nodes.truncate(saved_arena_len);
            self.parse_diagnostics.truncate(saved_diagnostics_len);
            result
        } else {
            false
        };

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_arrow
    }

    // Parse arrow function expression: (params) => body or x => body or <T>(x) => body
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

        // Arrow functions cannot be generators (there's no `*=>` syntax)
        // Clear generator context to allow 'yield' as an identifier
        // Example: function * foo(a = yield => yield) {} - both 'yield' are identifiers
        self.context_flags &= !CONTEXT_FLAG_GENERATOR;

        if is_async {
            self.context_flags |= CONTEXT_FLAG_ASYNC;
        }

        // Parse optional type parameters: <T, U extends Foo>
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse parameters
        let parameters = if self.is_token(SyntaxKind::OpenParenToken) {
            // Parenthesized parameter list: (a, b) =>
            self.parse_expected(SyntaxKind::OpenParenToken);
            self.context_flags |= CONTEXT_FLAG_ARROW_PARAMETERS;
            let params = self.parse_parameter_list();
            self.context_flags &= !CONTEXT_FLAG_ARROW_PARAMETERS;
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

        // Check for line terminator before arrow (TS1200)
        // The spec forbids a line break between `)` and `=>` in arrow functions,
        // but we still parse it as an arrow function to match TSC behavior.
        if self.scanner.has_preceding_line_break()
            && self.is_token(SyntaxKind::EqualsGreaterThanToken)
        {
            self.parse_error_at_current_token(
                "Line terminator not permitted before arrow.",
                diagnostic_codes::LINE_TERMINATOR_NOT_PERMITTED_BEFORE_ARROW,
            );
        }

        // Recovery: Handle missing fat arrow - common typo: (a, b) { return a; }
        // If we see { immediately after parameters/return type, the user forgot =>
        if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_error_at_current_token("'=>' expected.", diagnostic_codes::EXPECTED);
            // Don't consume the {, just continue to body parsing
            // The arrow is logically present but missing
        } else {
            // TypeScript rule: Line terminator is not permitted before arrow (TS1200)
            // Example: `f(() \n => {})` should emit TS1200 at the =>
            if self.scanner.has_preceding_line_break()
                && self.is_token(SyntaxKind::EqualsGreaterThanToken)
            {
                self.parse_error_at_current_token(
                    "Line terminator not permitted before arrow.",
                    diagnostic_codes::LINE_TERMINATOR_NOT_PERMITTED_BEFORE_ARROW,
                );
                // Still consume the => token to continue parsing
                self.next_token();
            } else {
                // Normal case: expect =>
                self.parse_expected(SyntaxKind::EqualsGreaterThanToken);
            }
        }

        // Async context was already set at the start of this function for parameter parsing
        // and remains set for body parsing

        // Parse body (block or expression)
        // Push a new label scope for arrow function bodies
        self.push_label_scope();
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else {
            // Check if next token starts a statement but not an expression
            // This catches cases like `() => var x` where `{` was expected
            // But NOT semicolons — `() => ;` should emit TS1109 "Expression expected"
            if self.is_statement_start()
                && !self.is_expression_start()
                && !self.is_token(SyntaxKind::SemicolonToken)
            {
                self.error_token_expected("{");
            }
            let expr = self.parse_assignment_expression();
            // If no expression was parsed (e.g. `() => ;`), emit TS1109
            if expr.is_none() {
                self.error_expression_expected();
            }
            expr
        };
        self.pop_label_scope();

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

    // Parse type parameters: <T, U extends Foo, V = `DefaultType`>
    pub(crate) fn parse_type_parameters(&mut self) -> NodeList {
        let mut params = Vec::new();

        self.parse_expected(SyntaxKind::LessThanToken);

        // Check for empty type parameter list: <>
        // TypeScript reports TS1098: "Type parameter list cannot be empty"
        if self.is_token(SyntaxKind::GreaterThanToken) {
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

    // Parse a single type parameter: T or T extends U or T = Default or T extends U = Default
    // Also supports modifiers: `const T`, `in T`, `out T`, `in out T`, `const in T`, etc.
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

    // Parse type parameter modifiers: `const`, `in`, `out`
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

    // Parse binary expression with precedence climbing
    pub(crate) fn parse_binary_expression(&mut self, min_precedence: u8) -> NodeIndex {
        let start_pos = self.token_pos();
        if !self.enter_recursion() {
            return NodeIndex::NONE;
        }

        let left = self.parse_binary_expression_chain(min_precedence, start_pos);
        self.exit_recursion();
        left
    }

    fn parse_binary_expression_chain(&mut self, min_precedence: u8, start_pos: u32) -> NodeIndex {
        let mut left = self.parse_unary_expression();

        loop {
            let op = if self.is_token(SyntaxKind::GreaterThanToken) {
                self.try_rescan_greater_token()
            } else {
                self.token()
            };

            if !self.in_parenthesized_expression_context()
                && op == SyntaxKind::BarBarToken
                && self.is_assignment_target_with_block_bodied_arrow(left)
            {
                break;
            }

            let precedence = self.get_operator_precedence(op);
            if precedence == 0 || precedence < min_precedence {
                break;
            }

            if op == SyntaxKind::AsKeyword || op == SyntaxKind::SatisfiesKeyword {
                // `as` and `satisfies` do not bind across line terminators.
                // `x\nas Type` is two statements via ASI, not a type assertion.
                if self.scanner.has_preceding_line_break() {
                    break;
                }
                left = self.parse_as_or_satisfies_expression(left, start_pos);
                continue;
            }

            left = self.parse_binary_expression_remainder(left, start_pos, op, precedence);
        }

        left
    }

    fn is_assignment_target_with_block_bodied_arrow(&self, node: NodeIndex) -> bool {
        let mut current = node;
        loop {
            let Some(node_data) = self.arena.get(current) else {
                return false;
            };
            if node_data.kind != syntax_kind_ext::BINARY_EXPRESSION {
                return false;
            }

            let Some(binary) = self.arena.get_binary_expr(node_data) else {
                return false;
            };
            let operator =
                SyntaxKind::try_from_u16(binary.operator_token).unwrap_or(SyntaxKind::Unknown);
            if !self.is_assignment_operator(operator) {
                return false;
            }
            if self.is_block_bodied_arrow_function(binary.right) {
                return true;
            }
            current = binary.right;
        }
    }

    fn is_block_bodied_arrow_function(&self, node: NodeIndex) -> bool {
        let Some(node_data) = self.arena.get(node) else {
            return false;
        };
        if node_data.kind != syntax_kind_ext::ARROW_FUNCTION {
            return false;
        }
        let Some(function_data) = self.arena.get_function(node_data) else {
            return false;
        };
        let Some(body_node) = self.arena.get(function_data.body) else {
            return false;
        };

        body_node.kind == syntax_kind_ext::BLOCK
    }

    const fn is_assignment_operator(&self, operator: SyntaxKind) -> bool {
        matches!(
            operator,
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
        )
    }

    fn parse_binary_expression_remainder(
        &mut self,
        left: NodeIndex,
        start_pos: u32,
        op: SyntaxKind,
        precedence: u8,
    ) -> NodeIndex {
        let operator_token = op as u16;
        self.next_token();

        if op == SyntaxKind::QuestionToken {
            return self.parse_conditional_expression(left, start_pos);
        }

        let right = self.parse_binary_expression_rhs(left, op, precedence);
        let end_pos = self.token_end();
        let final_right = if right.is_none() { left } else { right };

        self.arena.add_binary_expr(
            syntax_kind_ext::BINARY_EXPRESSION,
            start_pos,
            end_pos,
            BinaryExprData {
                left,
                operator_token,
                right: final_right,
            },
        )
    }

    fn parse_conditional_expression(&mut self, condition: NodeIndex, start_pos: u32) -> NodeIndex {
        let saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_CONDITIONAL_TRUE;

        let mut when_true = self.parse_assignment_expression();
        self.context_flags = saved_flags;

        if when_true.is_none() {
            self.error_expression_expected();
            when_true = self.create_missing_expression();
        }

        self.parse_expected(SyntaxKind::ColonToken);
        let mut when_false = self.parse_assignment_expression();
        self.context_flags = saved_flags;
        if when_false.is_none() {
            self.error_expression_expected();
            when_false = self.create_missing_expression();
        }
        let end_pos = self.token_end();

        self.arena.add_conditional_expr(
            syntax_kind_ext::CONDITIONAL_EXPRESSION,
            start_pos,
            end_pos,
            ConditionalExprData {
                condition,
                when_true,
                when_false,
            },
        )
    }

    fn parse_binary_expression_rhs(
        &mut self,
        left: NodeIndex,
        op: SyntaxKind,
        precedence: u8,
    ) -> NodeIndex {
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
        let next_min = if op == SyntaxKind::AsteriskAsteriskToken {
            precedence
        } else {
            precedence + 1
        };
        let right = if is_assignment {
            self.parse_assignment_expression()
        } else {
            self.parse_binary_expression(next_min)
        };

        if right.is_none() {
            self.error_expression_expected();
            let recovered = self.try_recover_binary_rhs();
            if recovered.is_none() {
                self.resync_to_next_expression_boundary();
                return left;
            }
            return recovered;
        }

        right
    }

    // Parse as/satisfies expression: expr as Type, expr satisfies Type
    // Also handles const assertion: expr as const
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

    // Parse unary expression
    pub(crate) fn parse_unary_expression(&mut self) -> NodeIndex {
        match self.token() {
            SyntaxKind::PlusToken
            | SyntaxKind::MinusToken
            | SyntaxKind::AsteriskToken
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
                    if is_update_operator {
                        // For `++`/`--` with no operand (e.g., `a++ ++;`), emit TS1109
                        // unconditionally. Bypass should_report_error() because `++;`
                        // is a distinct syntactic unit — the TS1109 must not be
                        // suppressed by a prior TS1005 for `';' expected` at `++`.

                        self.parse_error_at_current_token(
                            "Expression expected.",
                            diagnostic_codes::EXPRESSION_EXPECTED,
                        );
                    } else {
                        self.error_expression_expected();
                    }
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
                    self.parse_error_at_current_token(
                        "'await' expression cannot be used inside a class static block.",
                        diagnostic_codes::AWAIT_EXPRESSION_CANNOT_BE_USED_INSIDE_A_CLASS_STATIC_BLOCK,
                    );
                    // Fall through to parse as await expression
                } else if !self.in_async_context()
                    && has_following_expression
                    && !self.in_parameter_default_context()
                {
                    // Parse as await expression - the checker will emit TS1308
                    // (not TS1359 from the parser) to match TSC behavior
                } else if self.in_async_context() && self.in_parameter_default_context() {
                    // TS2524: 'await' expressions cannot be used in a parameter initializer
                    // Emit TS2524 in async functions with parameter defaults, regardless of whether
                    // there's a following expression.
                    // Examples:
                    //   - `async (a = await foo)` → emit TS2524
                    //   - `async (a = await)` → emit TS2524 + TS1109 (expression expected)

                    self.parse_error_at_current_token(
                        "'await' expressions cannot be used in a parameter initializer.",
                        diagnostic_codes::AWAIT_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
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
                            self.parse_error_at_current_token(
                                "Identifier expected. 'await' is a reserved word that cannot be used here.",
                                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
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
                self.consume_keyword(); // TS1260 check for await keyword with escapes

                // In parameter-default context, `await =>` should report missing operand.
                // Consume `=>` and the following token to prevent cascading errors
                // (e.g., TS1359 for `await` used as arrow body in async context).
                if self.in_parameter_default_context()
                    && self.is_token(SyntaxKind::EqualsGreaterThanToken)
                {
                    self.error_expression_expected();
                    self.next_token(); // consume `=>`
                    // Skip the arrow body token (e.g., second `await`) for recovery
                    if !self.is_token(SyntaxKind::CloseParenToken)
                        && !self.is_token(SyntaxKind::EndOfFileToken)
                    {
                        self.next_token();
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
            SyntaxKind::YieldKeyword => {
                if self.in_class_member_name()
                    && !self.in_generator_context()
                    && !self.is_computed_class_member_yield_expression()
                {
                    return self.parse_identifier_name();
                }

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

                // Outside a generator context with a following expression or asterisk,
                // emit TS1163 and parse as yield expression for correct AST structure.
                // This handles both static block contexts and top-level/function contexts
                // where `yield foo;` should report "yield is only allowed in a generator body"
                // rather than falling through to TS1434 "Unexpected keyword or identifier".
                if !self.in_generator_context() && (has_following_expression || has_asterisk) {
                    self.parse_error_at_current_token(
                        "A 'yield' expression is only allowed in a generator body.",
                        diagnostic_codes::A_YIELD_EXPRESSION_IS_ONLY_ALLOWED_IN_A_GENERATOR_BODY,
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
                    self.parse_error_at_current_token(
                        "'yield' expressions cannot be used in a parameter initializer.",
                        diagnostic_codes::YIELD_EXPRESSIONS_CANNOT_BE_USED_IN_A_PARAMETER_INITIALIZER,
                    );
                    // Fall through to parse as yield expression
                }

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
            _ => self.parse_postfix_expression(),
        }
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

    // Parse left-hand side expression (member access, call, etc.)
    pub(crate) fn parse_left_hand_side_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut expr = self.parse_primary_expression();

        loop {
            match self.token() {
                SyntaxKind::DotToken => {
                    if let Some(node) = self.arena.get(expr)
                        && node.kind
                            == crate::parser::syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
                        && let Some(eta) = self.arena.get_expr_type_args(node)
                    {
                        // TSC emits TS1477 at the `<…>` type-argument span (from `<` to
                        // past `>`), not at the whole expression start. This avoids
                        // setting THIS_NODE_HAS_ERROR on the expression identifier itself,
                        // which would suppress TS2304 for unresolved names like `List`.
                        //
                        // `<` starts at expression_node.end; `>` ends at node.end.
                        // NodeList.pos/end are always 0, so we can't use type_args span.
                        let err_pos = self
                            .arena
                            .get(eta.expression)
                            .map_or(node.pos, |expr_node| expr_node.end);
                        let err_len = node.end.saturating_sub(err_pos);
                        self.parse_error_at(
                            err_pos,
                            err_len,
                            tsz_common::diagnostics::diagnostic_messages::AN_INSTANTIATION_EXPRESSION_CANNOT_BE_FOLLOWED_BY_A_PROPERTY_ACCESS,
                            tsz_common::diagnostics::diagnostic_codes::AN_INSTANTIATION_EXPRESSION_CANNOT_BE_FOLLOWED_BY_A_PROPERTY_ACCESS,
                        );
                    }
                    self.next_token();
                    // Handle both regular identifiers and private identifiers (#name)
                    let name = if self.is_token(SyntaxKind::PrivateIdentifier) {
                        self.parse_private_identifier()
                    } else if self.is_identifier_or_keyword() {
                        // When there's a line break after the dot and the current token
                        // starts a declaration (e.g. `foo.\nvar y = 1;`), don't consume
                        // the token as a property name. Instead, emit TS1003 and create
                        // a missing identifier. This matches tsc's parseRightSideOfDot.
                        if self.scanner.has_preceding_line_break()
                            && self.look_ahead_next_is_identifier_or_keyword_on_same_line()
                        {
                            self.error_identifier_expected();
                            NodeIndex::NONE
                        } else {
                            self.parse_identifier_name()
                        }
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
                    // In decorator context, `[` starts a computed property name, not element access
                    if (self.context_flags & crate::parser::state::CONTEXT_FLAG_IN_DECORATOR) != 0 {
                        break;
                    }
                    self.next_token();
                    let argument = self.parse_expression();
                    if argument.is_none() {
                        // TS1011: An element access expression should take an argument
                        self.parse_error_at_current_token(
                            tsz_common::diagnostics::diagnostic_messages::AN_ELEMENT_ACCESS_EXPRESSION_SHOULD_TAKE_AN_ARGUMENT,
                            tsz_common::diagnostics::diagnostic_codes::AN_ELEMENT_ACCESS_EXPRESSION_SHOULD_TAKE_AN_ARGUMENT,
                        );
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
                    let optional_chain_flag = self.u16_from_node_flags(node_flags::OPTIONAL_CHAIN);
                    if is_optional_chain && let Some(call_node) = self.arena.get_mut(call_expr) {
                        call_node.flags |= optional_chain_flag;
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
                    if self.is_less_than_or_compound()
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
                            let optional_chain_flag =
                                self.u16_from_node_flags(node_flags::OPTIONAL_CHAIN);
                            if let Some(call_node) = self.arena.get_mut(call_expr) {
                                call_node.flags |= optional_chain_flag;
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
                        let optional_chain_flag =
                            self.u16_from_node_flags(node_flags::OPTIONAL_CHAIN);
                        if let Some(call_node) = self.arena.get_mut(call_expr) {
                            call_node.flags |= optional_chain_flag;
                        }
                        expr = call_expr;
                    } else {
                        // expr?.prop
                        let is_private_identifier = self.is_token(SyntaxKind::PrivateIdentifier);
                        let name = if is_private_identifier {
                            self.parse_private_identifier()
                        } else {
                            self.parse_identifier_name()
                        };

                        // TS18030: Optional chain cannot contain private identifiers
                        if is_private_identifier && let Some(name_node) = self.arena.get(name) {
                            self.parse_error_at(
                                    name_node.pos,
                                    name_node.end - name_node.pos,
                                    "An optional chain cannot contain private identifiers.",
                                    diagnostic_codes::AN_OPTIONAL_CHAIN_CANNOT_CONTAIN_PRIVATE_IDENTIFIERS,
                                );
                        }

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
                // Also handles `<<` for nested generics: foo<<T>(x: T) => number>(fn)
                SyntaxKind::LessThanToken | SyntaxKind::LessThanLessThanToken => {
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
                            // Not a call or tagged template - this is an instantiation expression
                            // (e.g., f<string>, new Foo<number>, a<b>?.())
                            let end_pos = self.token_end();
                            expr = self.arena.add_expr_with_type_args(
                                crate::parser::syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS,
                                start_pos,
                                end_pos,
                                crate::parser::node::ExprWithTypeArgsData {
                                    expression: expr,
                                    type_arguments: Some(type_args),
                                },
                            );
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

    // Parse argument list
    pub(crate) fn parse_argument_list(&mut self) -> NodeList {
        let mut args = Vec::new();

        while !self.is_token(SyntaxKind::CloseParenToken) {
            if self.is_argument_list_recovery_boundary() {
                self.error_argument_expression_expected();
                break;
            }

            if self.is_token(SyntaxKind::DotDotDotToken) {
                let spread_start = self.token_pos();
                self.next_token();
                let expression = self.parse_assignment_expression();
                if expression.is_none() {
                    // Emit TS1135 for incomplete spread argument: func(...missing)
                    self.error_argument_expression_expected();
                }
                let spread_end = self.token_end();
                let spread = self.arena.add_spread(
                    syntax_kind_ext::SPREAD_ELEMENT,
                    spread_start,
                    spread_end,
                    crate::parser::node::SpreadData { expression },
                );
                args.push(spread);
            } else if self.is_token(SyntaxKind::CommaToken) {
                // TS1135: missing argument before comma: func(a, , c)
                self.error_argument_expression_expected();
                args.push(NodeIndex::NONE);
            } else if self.is_token(SyntaxKind::SemicolonToken) {
                // Semicolon terminates argument list — don't emit TS1135 here.
                // Let parse_expected(CloseParenToken) emit TS1005 instead,
                // matching tsc which treats `;` as a clear boundary.
                break;
            } else {
                let arg = self.parse_assignment_expression();
                if arg.is_none() {
                    // TS1135 for missing function argument
                    self.error_argument_expression_expected();
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

    // Returns true for statement-only keywords that should stop argument parsing
    // during recovery to avoid cascading diagnostics.
    const fn is_argument_list_recovery_boundary(&self) -> bool {
        matches!(
            self.token(),
            SyntaxKind::ReturnKeyword
                | SyntaxKind::BreakKeyword
                | SyntaxKind::ContinueKeyword
                | SyntaxKind::ThrowKeyword
                | SyntaxKind::TryKeyword
                | SyntaxKind::CatchKeyword
                | SyntaxKind::FinallyKeyword
                | SyntaxKind::IfKeyword
                | SyntaxKind::ForKeyword
                | SyntaxKind::WhileKeyword
                | SyntaxKind::DoKeyword
                | SyntaxKind::SwitchKeyword
                | SyntaxKind::VarKeyword
                | SyntaxKind::LetKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::WithKeyword
                | SyntaxKind::DebuggerKeyword
                | SyntaxKind::CaseKeyword
                | SyntaxKind::DefaultKeyword
                | SyntaxKind::ElseKeyword
                | SyntaxKind::EndOfFileToken
        )
    }

    // Parse primary expression
    pub(crate) fn parse_primary_expression(&mut self) -> NodeIndex {
        match self.token() {
            SyntaxKind::Identifier => self.parse_identifier(),
            SyntaxKind::PrivateIdentifier => self.parse_private_identifier(),
            SyntaxKind::NumericLiteral => self.parse_numeric_literal(),
            SyntaxKind::BigIntLiteral => self.parse_bigint_literal(),
            SyntaxKind::StringLiteral => self.parse_string_literal(),
            SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword => self.parse_boolean_literal(),
            SyntaxKind::NullKeyword => self.parse_null_literal(),
            SyntaxKind::UndefinedKeyword
            | SyntaxKind::AnyKeyword
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
            // `<<` at expression start is invalid as a primary expression.
            // It is usually an ambiguous generic assertion case that should fall
            // through as a malformed left side and then recover with
            // TS1109: Expression expected.
            SyntaxKind::LessThanLessThanToken => {
                self.error_expression_expected();
                NodeIndex::NONE
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
            // `as` and `satisfies` are binary operators but also valid identifiers.
            // When they appear at expression start, they must be identifiers
            // (e.g., `var x = as as string` — first `as` is the variable).
            SyntaxKind::AsKeyword | SyntaxKind::SatisfiesKeyword => self.parse_identifier_name(),
            SyntaxKind::Unknown => {
                // TS1127: Invalid character - emit specific error for invalid characters

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
                // ColonToken is a structural delimiter (case clauses, labels, type annotations)
                // and must not be consumed as an error token.
                if self.is_binary_operator() {
                    // Binary operator at expression start means missing LHS.
                    // Emit TS1109 matching tsc's parsePrimaryExpression behavior.
                    self.error_expression_expected();
                    return NodeIndex::NONE;
                }
                if self.is_at_expression_end()
                    || self.is_token(SyntaxKind::CaseKeyword)
                    || self.is_token(SyntaxKind::DefaultKeyword)
                    || self.is_token(SyntaxKind::ColonToken)
                {
                    return NodeIndex::NONE;
                }

                // Statement-only keywords cannot start expressions.
                // Return NONE so callers emit TS1109 (Expression expected).
                if matches!(
                    self.token(),
                    SyntaxKind::ReturnKeyword
                        | SyntaxKind::BreakKeyword
                        | SyntaxKind::ContinueKeyword
                        | SyntaxKind::ThrowKeyword
                        | SyntaxKind::TryKeyword
                        | SyntaxKind::CatchKeyword
                        | SyntaxKind::FinallyKeyword
                        | SyntaxKind::DoKeyword
                        | SyntaxKind::WhileKeyword
                        | SyntaxKind::ForKeyword
                        | SyntaxKind::SwitchKeyword
                        | SyntaxKind::WithKeyword
                        | SyntaxKind::DebuggerKeyword
                        | SyntaxKind::IfKeyword
                        | SyntaxKind::ElseKeyword
                ) {
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

    // Parse a decorated class expression: `@dec class C { }`
    // Used when `@` is encountered in expression position.
    fn parse_decorated_class_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let decorators = self.parse_decorators();
        if self.is_token(SyntaxKind::ClassKeyword) || self.is_token(SyntaxKind::AbstractKeyword) {
            self.parse_class_expression_with_decorators(decorators, start_pos)
        } else {
            // Decorators not followed by class - emit error and create error token
            self.error_expression_expected();

            self.parse_error_at(
                self.token_pos(),
                1,
                "';' expected.",
                diagnostic_codes::EXPECTED,
            );
            let end_pos = self.token_end();
            self.arena
                .add_token(SyntaxKind::Unknown as u16, start_pos, end_pos)
        }
    }

    // Parse identifier
    // Uses zero-copy accessor and only clones when storing
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

    // Parse identifier name - allows keywords to be used as identifiers
    // This is used in contexts where keywords are valid identifier names
    // (e.g., class names, property names, function names)
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

    // Parse private identifier (#name)
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

    // Binding patterns, literals, array/object literals, property names,
    // new/member expressions → state_expressions_literals.rs
}
