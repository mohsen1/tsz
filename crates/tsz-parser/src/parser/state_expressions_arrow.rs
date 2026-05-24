use super::state::*;
use crate::parser::node::*;
use crate::parser::{NodeIndex, syntax_kind_ext};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_scanner::SyntaxKind;

impl ParserState {
    pub(crate) fn look_ahead_can_commit_async_arrow_function(&mut self) -> bool {
        self.speculate(|p| {
            p.saw_arrow_parameter_recovery = false;
            let _ = p.parse_async_arrow_function_expression();
            !p.saw_arrow_parameter_recovery
        })
    }

    // Parse async arrow function: async (x) => ... or async x => ...
    pub(crate) fn parse_async_arrow_function_expression(&mut self) -> NodeIndex {
        // Capture position of 'async' keyword before consuming it, so the arrow
        // function node span starts at 'async' (matching tsc's node.pos).
        let async_pos = self.token_pos();
        self.parse_expected(SyntaxKind::AsyncKeyword);
        self.parse_arrow_function_expression_with_async_at(true, async_pos)
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
                if (self.in_async_context() || self.in_static_block_context())
                    && self.is_token(SyntaxKind::AwaitKeyword)
                {
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

        let mut at_type_parameter_start = true;
        let mut type_parameter_count = 0u32;
        let mut saw_top_level_type_parameter_delimiter = false;
        let mut saw_top_level_constraint_or_default = false;
        let mut paren_depth = 0u32;
        let mut brace_depth = 0u32;
        let mut bracket_depth = 0u32;

        // Skip type parameters until we find >
        let mut depth = 1;
        while depth > 0 && !self.is_token(SyntaxKind::EndOfFileToken) {
            let token = self.token();
            let at_type_parameter_top_level =
                depth == 1 && paren_depth == 0 && brace_depth == 0 && bracket_depth == 0;

            if matches!(
                token,
                SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead
            ) {
                self.skip_template_literal_in_arrow_lookahead();
                continue;
            }

            // In TSX/JSX ambiguity resolution, malformed `extends` clauses such as
            // `<T extends>() => {}` and `<T extends={...}>() => {}` should NOT commit
            // to generic-arrow parsing. tsc treats these as JSX and surfaces JSX
            // diagnostics (for example TS1382), not type-parameter TS1110/TS1109.
            if at_type_parameter_top_level {
                if token == SyntaxKind::CommaToken {
                    at_type_parameter_start = true;
                    saw_top_level_type_parameter_delimiter = true;
                } else if at_type_parameter_start {
                    if !matches!(
                        token,
                        SyntaxKind::ConstKeyword | SyntaxKind::InKeyword | SyntaxKind::OutKeyword
                    ) && (self.is_identifier_or_keyword() || self.is_reserved_word())
                    {
                        at_type_parameter_start = false;
                        type_parameter_count += 1;
                    }
                } else if token == SyntaxKind::ExtendsKeyword {
                    saw_top_level_constraint_or_default = true;
                    self.next_token();
                    if matches!(
                        self.token(),
                        SyntaxKind::GreaterThanToken
                            | SyntaxKind::EqualsToken
                            | SyntaxKind::CommaToken
                            | SyntaxKind::CloseParenToken
                    ) {
                        self.scanner.restore_state(snapshot);
                        self.current_token = current;
                        return false;
                    }
                    continue;
                } else if token == SyntaxKind::EqualsToken {
                    saw_top_level_constraint_or_default = true;
                }
            }

            match token {
                SyntaxKind::LessThanToken => {
                    depth += 1;
                }
                SyntaxKind::GreaterThanToken => {
                    depth -= 1;
                }
                SyntaxKind::OpenParenToken => {
                    paren_depth += 1;
                }
                SyntaxKind::CloseParenToken => {
                    paren_depth = paren_depth.saturating_sub(1);
                }
                SyntaxKind::OpenBraceToken => {
                    brace_depth += 1;
                }
                SyntaxKind::CloseBraceToken => {
                    brace_depth = brace_depth.saturating_sub(1);
                }
                SyntaxKind::OpenBracketToken => {
                    bracket_depth += 1;
                }
                SyntaxKind::CloseBracketToken => {
                    bracket_depth = bracket_depth.saturating_sub(1);
                }
                _ => {}
            }
            self.next_token();
        }

        // In JSX language variants, `<T>() => ...` remains JSX (not a generic arrow)
        // unless the type parameter list is disambiguated by:
        // - multiple/trailing parameters (`<T,>()`, `<T, U>()`)
        // - a constraint/default (`<T extends X>()`, `<T = X>()`)
        if self.is_jsx_file()
            && type_parameter_count == 1
            && !saw_top_level_type_parameter_delimiter
            && !saw_top_level_constraint_or_default
        {
            self.scanner.restore_state(snapshot);
            self.current_token = current;
            return false;
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
        // Track whether we're at the start of a parameter slot (right after `(` or `,`
        // at depth 1). If we see `(` at the start of a parameter slot, this cannot be
        // a valid arrow function parameter list — e.g., `(a, (b, c)) =>` or `((a)) =>`.
        // This matches tsc's behavior of rejecting nested-paren parameter patterns.
        let mut depth = 1;
        let mut brace_depth: u32 = 0;
        let mut bracket_depth: u32 = 0;
        let mut angle_bracket_depth: u32 = 0;
        let mut saw_parameter_syntax = false;
        let mut slot_in_type_context = false;
        let mut slot_in_initializer_context = false;
        let mut at_param_start = true; // true at the first position in a parameter slot
        let mut previous_top_level_can_end_parameter_name = false;
        let mut previous_top_level_was_optional_parameter = false;
        let mut saw_top_level_conditional_operator = false;
        let mut previous_top_level_token = SyntaxKind::Unknown;
        while depth > 0 && !self.is_token(SyntaxKind::EndOfFileToken) {
            let token = self.token();
            let at_top_level =
                depth == 1 && brace_depth == 0 && bracket_depth == 0 && angle_bracket_depth == 0;

            if self.in_static_block_context()
                && at_top_level
                && at_param_start
                && token == SyntaxKind::AwaitKeyword
            {
                self.scanner.restore_state(snapshot);
                self.current_token = current;
                return false;
            }

            // `(x = y ==== z) { ... }` should not be treated as a missing-arrow
            // head. Once the initializer enters an equality chain, a second `=`
            // indicates a malformed expression tail that tsc recovers as a normal
            // parenthesized expression (later reporting `';' expected` at `{`).
            if at_top_level
                && slot_in_initializer_context
                && token == SyntaxKind::EqualsToken
                && matches!(
                    previous_top_level_token,
                    SyntaxKind::EqualsEqualsToken
                        | SyntaxKind::EqualsEqualsEqualsToken
                        | SyntaxKind::ExclamationEqualsToken
                        | SyntaxKind::ExclamationEqualsEqualsToken
                )
            {
                self.scanner.restore_state(snapshot);
                self.current_token = current;
                return false;
            }

            if at_top_level
                && at_param_start
                && !saw_parameter_syntax
                && !matches!(
                    token,
                    SyntaxKind::CloseParenToken
                        | SyntaxKind::AtToken
                        | SyntaxKind::DotDotDotToken
                        | SyntaxKind::OpenBracketToken
                        | SyntaxKind::OpenBraceToken
                )
                && !self.is_identifier_or_keyword()
            {
                self.scanner.restore_state(snapshot);
                self.current_token = current;
                return false;
            }
            let saw_optional_parameter_marker = at_top_level
                && token == SyntaxKind::QuestionToken
                && self.look_ahead_question_is_optional_parameter_marker(
                    previous_top_level_can_end_parameter_name,
                );
            let can_continue_top_level_parameter = at_top_level
                && (previous_top_level_can_end_parameter_name
                    || previous_top_level_was_optional_parameter);
            let token_can_follow_top_level_parameter = matches!(
                token,
                SyntaxKind::CloseParenToken
                    | SyntaxKind::CommaToken
                    | SyntaxKind::ColonToken
                    | SyntaxKind::EqualsToken
            ) || saw_optional_parameter_marker;

            if can_continue_top_level_parameter
                && !slot_in_type_context
                && !slot_in_initializer_context
                && !token_can_follow_top_level_parameter
            {
                self.scanner.restore_state(snapshot);
                self.current_token = current;
                return false;
            }

            if at_top_level && token == SyntaxKind::QuestionToken && !saw_optional_parameter_marker
            {
                saw_top_level_conditional_operator = true;
            }

            if at_top_level
                && (saw_optional_parameter_marker
                    || (token == SyntaxKind::ColonToken
                        && !saw_top_level_conditional_operator
                        && (previous_top_level_can_end_parameter_name
                            || previous_top_level_was_optional_parameter)))
            {
                saw_parameter_syntax = true;
                slot_in_type_context = token == SyntaxKind::ColonToken;
                if saw_optional_parameter_marker {
                    slot_in_type_context = false;
                }
            } else if at_top_level
                && token == SyntaxKind::EqualsToken
                && (previous_top_level_can_end_parameter_name
                    || previous_top_level_was_optional_parameter)
            {
                slot_in_type_context = false;
                slot_in_initializer_context = true;
            }

            if matches!(
                token,
                SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead
            ) {
                self.skip_template_literal_in_arrow_lookahead();
                at_param_start = false;
                previous_top_level_can_end_parameter_name = false;
                previous_top_level_was_optional_parameter = false;
                if at_top_level {
                    previous_top_level_token = token;
                }
                continue;
            }

            if token == SyntaxKind::OpenParenToken {
                // `(` at the start of a top-level parameter slot is not a valid
                // parameter pattern. Reject early so the expression is NOT parsed
                // as an arrow function (avoiding false TS1003 in parse_parameter).
                if depth == 1 && at_param_start {
                    self.scanner.restore_state(snapshot);
                    self.current_token = current;
                    return false;
                }
                depth += 1;
                at_param_start = false;
            } else if token == SyntaxKind::OpenBraceToken {
                brace_depth += 1;
                at_param_start = false;
            } else if token == SyntaxKind::CloseBraceToken {
                brace_depth = brace_depth.saturating_sub(1);
                at_param_start = false;
            } else if token == SyntaxKind::OpenBracketToken {
                bracket_depth += 1;
                at_param_start = false;
            } else if token == SyntaxKind::CloseBracketToken {
                bracket_depth = bracket_depth.saturating_sub(1);
                at_param_start = false;
            } else if token == SyntaxKind::LessThanToken {
                angle_bracket_depth += 1;
                at_param_start = false;
            } else if token == SyntaxKind::GreaterThanToken && angle_bracket_depth > 0 {
                angle_bracket_depth -= 1;
                at_param_start = false;
            } else if token == SyntaxKind::CloseParenToken {
                depth -= 1;
                at_param_start = false;
            } else if token == SyntaxKind::CommaToken
                && depth == 1
                && brace_depth == 0
                && bracket_depth == 0
                && angle_bracket_depth == 0
            {
                // A comma at the top level separates parameters; the next token
                // starts a new parameter slot.
                slot_in_type_context = false;
                slot_in_initializer_context = false;
                saw_top_level_conditional_operator = false;
                at_param_start = true;
            } else {
                // Any other token (identifier, keyword, `[`, `{`, `...`, `=`, etc.)
                // means we've moved past the start of this parameter slot.
                at_param_start = false;
            }

            previous_top_level_can_end_parameter_name = depth == 1
                && brace_depth == 0
                && bracket_depth == 0
                && ((self.is_identifier_or_keyword()
                    && token != SyntaxKind::QuestionToken
                    && !self.is_parameter_modifier())
                    || token == SyntaxKind::CloseBraceToken
                    || token == SyntaxKind::CloseBracketToken);
            previous_top_level_was_optional_parameter = depth == 1
                && brace_depth == 0
                && bracket_depth == 0
                && saw_optional_parameter_marker;
            if at_top_level {
                previous_top_level_token = token;
            }
            self.next_token();
        }

        // Check for line break before =>
        let has_line_break = self.scanner.has_preceding_line_break();

        // Check for optional return type annotation.
        // Important: check for `:` (return type) BEFORE checking has_line_break.
        // `(params): type =>` is unambiguously an arrow function regardless of
        // line breaks — TS1200 handles line terminator errors separately.
        let token_after_parameters = self.token();
        let is_arrow = if self.is_token(SyntaxKind::ColonToken) {
            // When we see `:` after `)`, it could be either:
            // 1. A return type annotation for an arrow function: (x): T => body
            // 2. The else separator of a conditional: a ? (x) : y
            // Disambiguate by checking for `=>` after a return type.
            self.next_token();
            if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
                self.scanner.restore_state(snapshot);
                self.current_token = current;
                return false;
            }
            let saved_arena_len = self.arena.nodes.len();
            let saved_diagnostics_len = self.parse_diagnostics.len();
            let type_start = self.token_pos();
            let type_node = self.parse_return_type();
            let parsed_return_type = self.token_pos() != type_start
                || self
                    .arena
                    .get(type_node)
                    .is_some_and(|node| node.end > node.pos);
            // After parsing the return type, check for `=>` or `{`. Line breaks
            // between the return type and `=>` are allowed here — TS1200 will be
            // emitted during actual parsing. The `(params): type` prefix is
            // unambiguous, so we don't need the line break check.
            let mut result = parsed_return_type
                && (self.is_token(SyntaxKind::EqualsGreaterThanToken)
                    || self.is_token(SyntaxKind::OpenBraceToken)
                    || matches!(
                        self.token(),
                        SyntaxKind::SemicolonToken
                            | SyntaxKind::CommaToken
                            | SyntaxKind::CloseBraceToken
                            | SyntaxKind::EndOfFileToken
                    ));

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
                // Typed parameter syntax inside the parens is strong evidence of
                // an arrow head; when no `=>` follows, tsc keeps the arrow
                // interpretation and reports a missing arrow at the next token
                // (property access, call, operators, statement terminators, etc.).
                || (saw_parameter_syntax
                    && !matches!(token_after_parameters, SyntaxKind::ColonToken))
        };
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_arrow
    }

    pub(crate) fn skip_template_literal_in_arrow_lookahead(&mut self) {
        match self.token() {
            SyntaxKind::NoSubstitutionTemplateLiteral => {
                self.next_token();
                return;
            }
            SyntaxKind::TemplateHead => {}
            _ => return,
        }

        self.next_token();

        loop {
            let mut brace_depth = 0u32;
            let mut paren_depth = 0u32;
            let mut bracket_depth = 0u32;
            let mut angle_depth = 0u32;

            while !self.is_token(SyntaxKind::EndOfFileToken) {
                match self.token() {
                    SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead => {
                        self.skip_template_literal_in_arrow_lookahead();
                        continue;
                    }
                    SyntaxKind::OpenBraceToken => {
                        brace_depth += 1;
                    }
                    SyntaxKind::CloseBraceToken if brace_depth == 0 => {
                        break;
                    }
                    SyntaxKind::CloseBraceToken => {
                        brace_depth -= 1;
                    }
                    SyntaxKind::OpenParenToken => {
                        paren_depth += 1;
                    }
                    SyntaxKind::CloseParenToken if paren_depth > 0 => {
                        paren_depth -= 1;
                    }
                    SyntaxKind::OpenBracketToken => {
                        bracket_depth += 1;
                    }
                    SyntaxKind::CloseBracketToken if bracket_depth > 0 => {
                        bracket_depth -= 1;
                    }
                    SyntaxKind::LessThanToken => {
                        angle_depth += 1;
                    }
                    SyntaxKind::GreaterThanToken if angle_depth > 0 => {
                        angle_depth -= 1;
                    }
                    _ => {}
                }
                self.next_token();
            }

            if !self.is_token(SyntaxKind::CloseBraceToken) {
                return;
            }

            self.scanner.re_scan_template_token(false);
            self.current_token = self.scanner.get_token();

            match self.token() {
                SyntaxKind::TemplateTail => {
                    self.next_token();
                    return;
                }
                SyntaxKind::TemplateMiddle => {
                    self.next_token();
                }
                _ => return,
            }
        }
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
        let is_arrow = self.is_token(SyntaxKind::EqualsGreaterThanToken);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_arrow
    }

    // Parse arrow function expression: (params) => body or x => body or <T>(x) => body
    pub(crate) fn parse_arrow_function_expression_with_async(
        &mut self,
        is_async: bool,
    ) -> NodeIndex {
        self.parse_arrow_function_expression_with_async_at(is_async, self.token_pos())
    }

    pub(crate) fn parse_arrow_function_expression_with_async_at(
        &mut self,
        is_async: bool,
        start_pos: u32,
    ) -> NodeIndex {
        // Set async context BEFORE parsing parameters
        // This is important for correctly handling 'await' in parameter defaults:
        // - `async (a = await) => {}` should emit TS1109 (Expression expected)
        // - TSC sets async context for the entire async function scope including parameters
        let saved_flags = self.context_flags;

        // Arrow functions cannot be generators (there's no `*=>` syntax)
        // Clear generator context to allow 'yield' as an identifier
        // Example: function * foo(a = yield => yield) {} - both 'yield' are identifiers
        // Keep STATIC_BLOCK set — inside a static block, 'await' is reserved even
        // in arrow function parameters, matching tsc behavior.
        self.context_flags &=
            !(CONTEXT_FLAG_GENERATOR | CONTEXT_FLAG_ASYNC | CONTEXT_FLAG_CLASS_FIELD_INITIALIZER);

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
            // Single identifier parameter: x => or await => (where await is used as identifier)
            let param_start = self.token_pos();

            // In static blocks and async contexts, 'await' is reserved and cannot be
            // used as a parameter name. Emit TS1109 "Expression expected." at the
            // 'await' token position, matching tsc behavior.
            if self.is_token(SyntaxKind::AwaitKeyword)
                && (self.in_static_block_context() || self.in_async_context())
            {
                self.error_expression_expected();
            }

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

        // Clear STATIC_BLOCK before body — the body is a new scope where
        // 'await' is a valid identifier (unless the function is async).
        self.context_flags &= !CONTEXT_FLAG_STATIC_BLOCK;
        self.context_flags |= CONTEXT_FLAG_FUNCTION_BODY;

        // Parse body (block or expression)
        // Push a new label scope for arrow function bodies
        self.push_label_scope();
        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else if self.is_statement_start()
            && !self.is_expression_start()
            && !self.is_token(SyntaxKind::SemicolonToken)
        {
            // Statement keyword (var, return, etc.) after `=>` — missing `{`.
            // Emit TS1005 and recover by parsing statements as a block body.
            self.error_token_expected("{");
            let block_start = self.token_pos();
            let stmts = self.parse_statements();
            let block_end = if self.is_token(SyntaxKind::CloseBraceToken) {
                let end = self.token_end();
                self.next_token();
                end
            } else {
                self.token_end()
            };
            self.arena.add_block(
                syntax_kind_ext::BLOCK,
                block_start,
                block_end,
                crate::parser::node::BlockData {
                    statements: stmts,
                    multi_line: true,
                },
            )
        } else {
            let expr = self.parse_assignment_expression();
            // If no expression was parsed (e.g. `() => ;`), emit TS1109
            if expr.is_none() {
                self.error_expression_expected();
                if self.is_token(SyntaxKind::CloseBraceToken) {
                    let deferred_close_braces =
                        self.count_following_close_braces().saturating_sub(1);
                    self.deferred_module_close_braces =
                        self.deferred_module_close_braces.max(deferred_close_braces);
                }
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
}
