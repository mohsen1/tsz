//! Parser state - class expression and class declaration parsing.

use super::state::{
    CONTEXT_FLAG_ARROW_PARAMETERS, CONTEXT_FLAG_CONSTRUCTOR_PARAMETERS, CONTEXT_FLAG_IN_CLASS,
    CONTEXT_FLAG_PARAMETER_BINDING_PATTERN, CONTEXT_FLAG_PARAMETER_DEFAULT, ParserState,
};
use crate::parser::{
    NodeIndex, NodeList,
    node::{ClassData, IdentifierData},
    syntax_kind_ext,
};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::interner::Atom;
use tsz_scanner::SyntaxKind;

impl ParserState {
    fn report_missing_close_paren_after_body_recovery(&mut self) {
        let snapshot = self.scanner.save_state();
        let saved_token = self.current_token;
        let mut brace_depth = 0u32;
        let mut missing_pos = None;

        while !self.is_token(SyntaxKind::EndOfFileToken) {
            match self.token() {
                SyntaxKind::OpenBraceToken => {
                    brace_depth += 1;
                }
                SyntaxKind::CloseBraceToken => {
                    if brace_depth == 0 {
                        missing_pos = Some(self.token_end());
                        break;
                    }
                    brace_depth -= 1;
                    if brace_depth == 0 {
                        missing_pos = Some(self.token_end());
                        break;
                    }
                }
                _ => {}
            }
            self.next_token();
        }

        self.scanner.restore_state(snapshot);
        self.current_token = saved_token;

        if let Some(pos) = missing_pos {
            self.parse_error_at(pos, 0, "')' expected.", diagnostic_codes::EXPECTED);
            self.suppress_next_missing_close_paren_error_once = true;
        }
    }

    fn report_definite_assignment_parameter_tail_recovery(&mut self) {
        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};

        if !self.is_token(SyntaxKind::CloseParenToken) {
            return;
        }

        let snapshot = self.scanner.save_state();
        let saved_token = self.current_token;
        let saved_scanner_diagnostics_high_water_mark = self.scanner_diagnostics_high_water_mark;
        let close_start = self.token_pos();
        let close_length = self.token_end().saturating_sub(close_start);

        self.next_token();
        if !self.is_token(SyntaxKind::ColonToken) {
            self.scanner.restore_state(snapshot);
            self.current_token = saved_token;
            self.scanner_diagnostics_high_water_mark = saved_scanner_diagnostics_high_water_mark;
            return;
        }

        self.parse_error_at(
            close_start,
            close_length,
            "Expression expected.",
            diagnostic_codes::EXPRESSION_EXPECTED,
        );
        self.parse_error_at_current_token(
            "Declaration or statement expected.",
            diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
        );
        self.next_token();

        let mut reported_empty_element_access = false;
        let mut saw_for_keyword = false;
        let mut saw_for_await = false;
        let mut pending_for_await_header = false;
        let mut for_header_paren_depth = 0u32;
        let mut reported_for_await_expression = false;
        let mut reported_for_body_property = false;
        let mut pending_const_binding_name_colon = false;
        let mut pending_const_binding_semicolon_comma = false;
        let mut pending_for_binding_name_colon = false;
        let mut pending_for_of_comma = false;
        let mut pending_for_expression_comma = false;
        let mut reported_for_expression_start_comma = false;
        let mut pending_member_tail_comma = false;
        let mut pending_statement_semicolon_comma = false;
        let mut last_close_brace_pos = None;

        while !self.is_token(SyntaxKind::EndOfFileToken) {
            if saw_for_keyword {
                if self.is_token(SyntaxKind::AwaitKeyword) {
                    self.parse_error_at_current_token("':' expected.", diagnostic_codes::EXPECTED);
                    saw_for_keyword = false;
                    saw_for_await = true;
                    self.next_token();
                    continue;
                }
                saw_for_keyword = false;
            }

            if saw_for_await {
                if self.is_token(SyntaxKind::OpenParenToken) {
                    if !reported_for_await_expression {
                        self.parse_error_at(
                            self.token_end(),
                            0,
                            "Expression expected.",
                            diagnostic_codes::EXPRESSION_EXPECTED,
                        );
                        reported_for_await_expression = true;
                    }
                    saw_for_await = false;
                    pending_for_await_header = true;
                    for_header_paren_depth = 1;
                    self.next_token();
                    continue;
                }
                saw_for_await = false;
            }

            if pending_for_await_header {
                match self.token() {
                    SyntaxKind::OpenParenToken => {
                        for_header_paren_depth += 1;
                    }
                    SyntaxKind::CloseParenToken => {
                        if for_header_paren_depth == 1 && pending_for_expression_comma {
                            self.parse_error_at_current_token(
                                "',' expected.",
                                diagnostic_codes::EXPECTED,
                            );
                            pending_for_expression_comma = false;
                        }
                        for_header_paren_depth = for_header_paren_depth.saturating_sub(1);
                    }
                    SyntaxKind::OpenBraceToken
                        if for_header_paren_depth == 0 && !reported_for_body_property =>
                    {
                        self.parse_error_at_current_token(
                            "Property assignment expected.",
                            diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED,
                        );
                        reported_for_body_property = true;
                        pending_for_await_header = false;
                    }
                    token
                        if token == SyntaxKind::ConstKeyword
                            || (token == SyntaxKind::Identifier
                                && self.scanner.get_token_value_ref() == "const") =>
                    {
                        pending_for_binding_name_colon = true;
                    }
                    token
                        if pending_for_binding_name_colon
                            && (token == SyntaxKind::Identifier
                                || token == SyntaxKind::InKeyword
                                || token == SyntaxKind::OutKeyword) =>
                    {
                        self.parse_error_at_current_token(
                            "':' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        pending_for_binding_name_colon = false;
                        pending_for_of_comma = true;
                    }
                    SyntaxKind::OfKeyword if pending_for_of_comma => {
                        self.parse_error_at_current_token(
                            "',' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        pending_for_of_comma = false;
                        pending_for_expression_comma = true;
                    }
                    SyntaxKind::Identifier
                        if pending_for_expression_comma && !reported_for_expression_start_comma =>
                    {
                        self.parse_error_at_current_token(
                            "',' expected.",
                            diagnostic_codes::EXPECTED,
                        );
                        reported_for_expression_start_comma = true;
                    }
                    _ => {}
                }
            }

            match self.token() {
                SyntaxKind::CloseBracketToken if !reported_empty_element_access => {
                    self.parse_error_at_current_token(
                        diagnostic_messages::AN_ELEMENT_ACCESS_EXPRESSION_SHOULD_TAKE_AN_ARGUMENT,
                        diagnostic_codes::AN_ELEMENT_ACCESS_EXPRESSION_SHOULD_TAKE_AN_ARGUMENT,
                    );
                    reported_empty_element_access = true;
                }
                SyntaxKind::ForKeyword => {
                    saw_for_keyword = true;
                }
                token
                    if !pending_for_await_header
                        && (token == SyntaxKind::ConstKeyword
                            || (token == SyntaxKind::Identifier
                                && self.scanner.get_token_value_ref() == "const")) =>
                {
                    pending_const_binding_name_colon = true;
                }
                token
                    if pending_const_binding_name_colon
                        && (token == SyntaxKind::Identifier
                            || token == SyntaxKind::InKeyword
                            || token == SyntaxKind::OutKeyword) =>
                {
                    self.parse_error_at_current_token("':' expected.", diagnostic_codes::EXPECTED);
                    pending_const_binding_name_colon = false;
                    pending_const_binding_semicolon_comma = true;
                }
                _ if pending_const_binding_name_colon => {
                    pending_const_binding_name_colon = false;
                }
                SyntaxKind::SemicolonToken if pending_const_binding_semicolon_comma => {
                    self.parse_error_at_current_token("',' expected.", diagnostic_codes::EXPECTED);
                    pending_const_binding_semicolon_comma = false;
                }
                SyntaxKind::DotToken => {
                    self.parse_error_at_current_token("',' expected.", diagnostic_codes::EXPECTED);
                    pending_member_tail_comma = true;
                }
                SyntaxKind::OpenParenToken if pending_member_tail_comma => {
                    pending_statement_semicolon_comma = true;
                    pending_member_tail_comma = false;
                }
                SyntaxKind::SemicolonToken if pending_statement_semicolon_comma => {
                    self.parse_error_at_current_token("',' expected.", diagnostic_codes::EXPECTED);
                    pending_statement_semicolon_comma = false;
                }
                SyntaxKind::CloseBraceToken => {
                    last_close_brace_pos = Some(self.token_pos());
                }
                _ => {}
            }

            self.next_token();
        }

        let expression_recovery_pos = last_close_brace_pos.unwrap_or_else(|| self.token_pos());
        self.parse_error_at(
            expression_recovery_pos,
            0,
            diagnostic_messages::EXPRESSION_EXPECTED,
            diagnostic_codes::EXPRESSION_EXPECTED,
        );

        self.scanner.restore_state(snapshot);
        self.current_token = saved_token;
        self.scanner_diagnostics_high_water_mark = self.scanner.get_scanner_diagnostics().len();
    }

    /// Parse class expression: class {} or class Name {}
    ///
    /// Unlike class declarations, class expressions can be anonymous.
    pub(crate) fn parse_class_expression(&mut self) -> NodeIndex {
        self.parse_class_expression_with_decorators(None, self.token_pos())
    }

    pub(crate) fn parse_class_expression_with_decorators(
        &mut self,
        decorators: Option<NodeList>,
        start_pos: u32,
    ) -> NodeIndex {
        // ES decorators (TC39 Stage 3) are valid on class expressions.
        // With --experimentalDecorators, the checker emits TS1206 if needed.

        self.parse_expected(SyntaxKind::ClassKeyword);

        // Check for illegal binding identifiers (e.g., 'await' in static blocks)
        self.check_illegal_binding_identifier();

        // Parse optional name (class expressions can be anonymous)
        // Like class declarations, keywords can be used as class names
        // EXCEPT extends/implements which start heritage clauses.
        // Special case: if extends/implements is followed by `{`, it's the
        // class name, not a heritage clause start.
        let is_heritage_keyword = (self.is_token(SyntaxKind::ExtendsKeyword)
            || self.is_token(SyntaxKind::ImplementsKeyword))
            && !self.next_token_is_open_brace();
        let name = if self.is_identifier_or_keyword() && !is_heritage_keyword {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        // Parse optional type parameters
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse heritage (extends/implements)
        let heritage = self.parse_heritage_clauses();

        // Parse body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let class_saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_CLASS;
        let members = self.parse_class_members();
        self.context_flags = class_saved_flags;
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();

        self.arena.add_class(
            syntax_kind_ext::CLASS_EXPRESSION,
            start_pos,
            end_pos,
            ClassData {
                modifiers: decorators,
                name,
                type_parameters,
                heritage_clauses: heritage,
                members,
            },
        )
    }

    /// Parse parameter list
    pub(crate) fn parse_parameter_list(&mut self) -> NodeList {
        let mut params = Vec::new();
        let mut seen_rest_parameter = false;
        let mut emitted_rest_error = false;
        let mut rest_param_start: u32 = 0;
        let mut rest_param_length: u32 = 0;
        let mut recover_tail_from_stray_colon = false;
        let mut recover_tail_from_definite_assignment_colon = false;

        while !self.is_token(SyntaxKind::CloseParenToken) {
            // If we see `=>` before any parameters were parsed, this is likely a
            // degenerate case like `function =>` with no parens. Don't consume `=>`
            // here — let the caller handle it, avoiding a spurious `)` expected error.
            if self.is_token(SyntaxKind::EqualsGreaterThanToken) && params.is_empty() {
                break;
            }

            if self.is_token(SyntaxKind::ColonToken) {
                use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.parse_error_at_current_token(
                    diagnostic_messages::PARAMETER_DECLARATION_EXPECTED,
                    diagnostic_codes::PARAMETER_DECLARATION_EXPECTED,
                );
                self.next_token();
                if !matches!(
                    self.token(),
                    SyntaxKind::CommaToken
                        | SyntaxKind::CloseParenToken
                        | SyntaxKind::OpenBraceToken
                        | SyntaxKind::EndOfFileToken
                ) {
                    let recover_start = self.token_pos();
                    let _ = self.parse_type();
                    if self.token_pos() == recover_start
                        && !matches!(
                            self.token(),
                            SyntaxKind::CommaToken
                                | SyntaxKind::CloseParenToken
                                | SyntaxKind::OpenBraceToken
                                | SyntaxKind::EndOfFileToken
                        )
                    {
                        self.next_token();
                    }
                }
                break;
            }

            // TS1014: A rest parameter must be last in a parameter list
            // Emit at the rest parameter's location (matching tsc), not the next param.
            if seen_rest_parameter && !emitted_rest_error {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    rest_param_start,
                    rest_param_length,
                    "A rest parameter must be last in a parameter list.",
                    diagnostic_codes::A_REST_PARAMETER_MUST_BE_LAST_IN_A_PARAMETER_LIST,
                );
                emitted_rest_error = true;
            }

            let param = self.parse_parameter();

            // Check if this is a rest parameter (...)
            let is_rest_param = if let Some(node) = self.arena.get(param) {
                if let Some(param_data) = self.arena.get_parameter(node) {
                    param_data.dot_dot_dot_token
                } else {
                    false
                }
            } else {
                false
            };

            if is_rest_param
                && !seen_rest_parameter
                && let Some(node) = self.arena.get(param)
            {
                rest_param_start = node.pos;
                rest_param_length = node.end.saturating_sub(node.pos);
            }
            seen_rest_parameter = seen_rest_parameter || is_rest_param;
            params.push(param);

            let has_comma = self.parse_optional(SyntaxKind::CommaToken);

            if is_rest_param
                && has_comma
                && (self.is_token(SyntaxKind::CloseParenToken)
                    || self.is_token(SyntaxKind::EndOfFileToken))
            {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                        self.token_pos() - 1, // approximate comma position
                        1,
                        "A rest parameter or binding pattern may not have a trailing comma.",
                        diagnostic_codes::A_REST_PARAMETER_OR_BINDING_PATTERN_MAY_NOT_HAVE_A_TRAILING_COMMA,
                    );
            }

            if !has_comma {
                if recover_tail_from_stray_colon && self.is_token(SyntaxKind::EndOfFileToken) {
                    if let Some(node) = self.arena.get(param) {
                        self.parse_error_at(
                            node.end,
                            0,
                            "')' expected.",
                            tsz_common::diagnostics::diagnostic_codes::EXPECTED,
                        );
                        self.suppress_next_missing_close_paren_error_once = true;
                    }
                    break;
                }

                // Recovery: in malformed parameter initializers like
                // `function* f(a = yield => yield) {}` or
                // `async function f(a = await => await) {}`
                // treat `=>` as a missing comma boundary to continue parsing.
                if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
                    if (self.context_flags & CONTEXT_FLAG_ARROW_PARAMETERS) != 0 {
                        self.saw_arrow_parameter_recovery = true;
                    }
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token("',' expected.", diagnostic_codes::EXPECTED);
                    self.next_token(); // consume =>
                    if self.is_parameter_start() {
                        continue;
                    }
                    break;
                }
                // Trailing commas are allowed in parameter lists
                // Emit appropriate error based on context
                if !self.is_token(SyntaxKind::CloseParenToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    if recover_tail_from_definite_assignment_colon
                        && matches!(
                            self.token(),
                            SyntaxKind::LessThanToken | SyntaxKind::GreaterThanToken
                        )
                    {
                        self.parse_companion_error_at_current_token(
                            "',' expected.",
                            tsz_common::diagnostics::diagnostic_codes::EXPECTED,
                        );
                    } else {
                        self.error_comma_expected();
                    }
                    // Definite-assignment marker (`!`) is invalid on a
                    // parameter. tsc anchors TS1005 at the `!` (emitted just
                    // above) and TS1138 at the following `:`, then keeps
                    // recovering the tail as parameter-list elements. That is
                    // observable for generic tails like `x!: A<T>`, where the
                    // `<T>` and return type produce further recovery
                    // diagnostics instead of being consumed as a clean type
                    // annotation.
                    if self.is_token(SyntaxKind::ExclamationToken) {
                        let snapshot = self.scanner.save_state();
                        let saved_token = self.current_token;
                        self.next_token();
                        if self.is_token(SyntaxKind::ColonToken) {
                            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
                            self.parse_error_at_current_token(
                                diagnostic_messages::PARAMETER_DECLARATION_EXPECTED,
                                diagnostic_codes::PARAMETER_DECLARATION_EXPECTED,
                            );
                            self.next_token();
                            recover_tail_from_definite_assignment_colon = true;
                            continue;
                        }
                        self.scanner.restore_state(snapshot);
                        self.current_token = saved_token;
                    }
                    if recover_tail_from_definite_assignment_colon
                        && self.is_token(SyntaxKind::LessThanToken)
                    {
                        self.next_token();
                        continue;
                    }
                    if self.is_token(SyntaxKind::ColonToken) {
                        self.next_token();
                        if self.can_token_start_type() {
                            self.parse_type();
                        } else if !matches!(
                            self.token(),
                            SyntaxKind::CommaToken
                                | SyntaxKind::CloseParenToken
                                | SyntaxKind::OpenBraceToken
                                | SyntaxKind::EndOfFileToken
                        ) {
                            self.next_token();
                        }
                        if self.is_parameter_start() {
                            if self.is_token(SyntaxKind::OpenBraceToken) {
                                self.parse_error_at_current_token(
                                    "',' expected.",
                                    tsz_common::diagnostics::diagnostic_codes::EXPECTED,
                                );
                                recover_tail_from_stray_colon = true;
                            }
                            continue;
                        }
                    }
                    if self.is_token(SyntaxKind::IsKeyword) && self.is_parameter_start() {
                        // `function f(a: b is A)` is not a legal parameter type
                        // predicate. TSC recovers by treating both `is` and the
                        // following type name as parameter-list elements with
                        // missing commas, so leave `is` for the next iteration
                        // instead of consuming it as type syntax.
                        let snapshot = self.scanner.save_state();
                        let saved_token = self.current_token;
                        self.next_token();
                        if self.is_parameter_start() {
                            self.parse_error_at_current_token(
                                "',' expected.",
                                tsz_common::diagnostics::diagnostic_codes::EXPECTED,
                            );
                        }
                        self.scanner.restore_state(snapshot);
                        self.current_token = saved_token;
                        continue;
                    }
                    if is_rest_param && self.is_parameter_start() {
                        // `...public rest: T` is invalid, but tsc recovers as if
                        // a comma separated the malformed rest parameter from
                        // `rest`. Keep parsing so JS emit preserves both names.
                        continue;
                    }
                    if self.is_parameter_start() {
                        // General missing-comma recovery. For example,
                        // `constructor(public @dec p: number)` is invalid, but
                        // tsc preserves the recovered `public, p` parameter list
                        // and the parameter decorator on `p`.
                        continue;
                    }
                    if self.is_token(SyntaxKind::OpenBraceToken)
                        && (self.context_flags & CONTEXT_FLAG_ARROW_PARAMETERS) == 0
                    {
                        self.report_missing_close_paren_after_body_recovery();
                    }
                    // Recovery: skip tokens until we find `)` or `{` so that the
                    // caller's parse_expected(CloseParenToken) succeeds and the
                    // class body parses normally.  Without this, stray tokens
                    // from malformed parameters (e.g., `...public rest: string[]`)
                    // leave the parser stranded, causing a cascading TS1128 at EOF.
                    let mut paren_depth = 0i32;
                    while !self.is_token(SyntaxKind::EndOfFileToken) {
                        if self.is_token(SyntaxKind::OpenParenToken) {
                            paren_depth += 1;
                            self.next_token();
                        } else if self.is_token(SyntaxKind::CloseParenToken) {
                            if paren_depth == 0 {
                                break;
                            }
                            paren_depth -= 1;
                            self.next_token();
                        } else if self.is_token(SyntaxKind::OpenBraceToken) && paren_depth == 0 {
                            // Hit function body — stop before `{` so it parses normally
                            break;
                        } else {
                            self.next_token();
                        }
                    }
                }
                if recover_tail_from_definite_assignment_colon {
                    self.report_definite_assignment_parameter_tail_recovery();
                }
                break;
            }
        }

        self.make_node_list(params)
    }

    /// Check if current token is a valid parameter modifier
    pub(crate) const fn is_valid_parameter_modifier(&self) -> bool {
        matches!(
            self.current_token,
            SyntaxKind::PublicKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::ReadonlyKeyword
                | SyntaxKind::OverrideKeyword
        )
    }

    /// Check if current token is a modifier keyword used as a parameter modifier.
    /// This includes invalid modifiers like static/export that tsc accepts during
    /// parsing but reports TS1090 for in the checker.
    /// Uses look-ahead to distinguish `(static x: number)` (modifier) from
    /// `(async: boolean)` (parameter name).
    pub(crate) fn is_parameter_modifier(&mut self) -> bool {
        if !self.is_valid_parameter_modifier()
            && !matches!(
                self.current_token,
                SyntaxKind::StaticKeyword
                    | SyntaxKind::ExportKeyword
                    | SyntaxKind::DeclareKeyword
                    | SyntaxKind::AsyncKeyword
                    | SyntaxKind::AbstractKeyword
                    | SyntaxKind::AccessorKeyword
                    | SyntaxKind::ConstKeyword
                    | SyntaxKind::DefaultKeyword
                    | SyntaxKind::InKeyword
                    | SyntaxKind::OutKeyword
            )
        {
            return false;
        }
        // Look ahead: if the next token can follow a modifier (identifier/keyword,
        // string/number literal, [, {, *, ...), then this keyword is being used as
        // a modifier. Otherwise it's a parameter name (e.g., `(readonly)` or
        // `(async: boolean)`). This applies to ALL modifier keywords including
        // valid ones like `readonly` — when `readonly` is followed by `)` it's
        // a parameter name, not a modifier.
        // This mirrors tsc's canFollowModifier() + isLiteralPropertyName() check.
        let snapshot = self.scanner.save_state();
        let saved_token = self.current_token;
        self.next_token();
        let can_follow = !self.scanner.has_preceding_line_break()
            && (matches!(
                self.current_token,
                SyntaxKind::OpenBracketToken
                    | SyntaxKind::OpenBraceToken
                    | SyntaxKind::AsteriskToken
                    | SyntaxKind::DotDotDotToken
                    | SyntaxKind::StringLiteral
                    | SyntaxKind::NumericLiteral
                    | SyntaxKind::BigIntLiteral
            ) || self.is_identifier_or_keyword());
        self.scanner.restore_state(snapshot);
        self.current_token = saved_token;
        can_follow
    }

    /// Speculate past any modifier-like tokens at the current position to test
    /// whether the parameter name slot is `this`. Used to suppress TS1090 on
    /// invalid modifiers when tsc instead emits only TS1433 at the parameter
    /// (e.g. `function f(async this: C)` produces only TS1433 in tsc, not
    /// TS1090 on `async` followed by TS1433 — but our `parse_error_at`
    /// position-dedup would swallow the TS1433 if both fire at the same start).
    pub(crate) fn lookahead_param_name_is_this(&mut self) -> bool {
        let saved_state = self.scanner.save_state();
        let saved_token = self.current_token;
        while self.is_parameter_modifier() {
            self.next_token();
        }
        let result = self.is_token(SyntaxKind::ThisKeyword);
        self.scanner.restore_state(saved_state);
        self.current_token = saved_token;
        result
    }

    /// Parse parameter modifiers (public, private, protected, readonly, override,
    /// and invalid ones like static/export/declare/async which get TS1090).
    ///
    /// `suppress_invalid_modifier_diagnostics` skips TS1090 emission. Caller sets
    /// it when the parameter name is `this`, since tsc routes that case through
    /// TS1433 only — and TS1090 would otherwise dedup the TS1433.
    pub(crate) fn parse_parameter_modifiers(
        &mut self,
        suppress_invalid_modifier_diagnostics: bool,
    ) -> Option<NodeList> {
        let mut modifiers = Vec::new();
        let mut seen_readonly = false;
        let mut seen_accessibility = false;
        let mut seen_override = false;
        let mut reported_accessibility_duplicate = false;

        while self.is_parameter_modifier() {
            let mod_start = self.token_pos();
            let mod_kind = self.current_token;

            // Emit TS1090 for modifiers that cannot appear on parameters.
            // tsc does this in the checker via checkGrammarModifiers, but we
            // emit it here during parsing so we don't need checker support yet.
            if !self.is_valid_parameter_modifier() && !suppress_invalid_modifier_diagnostics {
                use tsz_common::diagnostics::diagnostic_codes;
                let modifier_name = match mod_kind {
                    SyntaxKind::StaticKeyword => "static",
                    SyntaxKind::ExportKeyword => "export",
                    SyntaxKind::DeclareKeyword => "declare",
                    SyntaxKind::AsyncKeyword => "async",
                    SyntaxKind::AbstractKeyword => "abstract",
                    SyntaxKind::AccessorKeyword => "accessor",
                    SyntaxKind::ConstKeyword => "const",
                    SyntaxKind::DefaultKeyword => "default",
                    SyntaxKind::InKeyword => "in",
                    SyntaxKind::OutKeyword => "out",
                    _ => "modifier",
                };
                self.parse_error_at_current_token(
                    &format!("'{modifier_name}' modifier cannot appear on a parameter."),
                    diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_PARAMETER,
                );
            }

            // Check for modifier ordering violations
            // Parameter modifiers must be in order: accessibility, override, readonly
            if matches!(
                mod_kind,
                SyntaxKind::PublicKeyword
                    | SyntaxKind::PrivateKeyword
                    | SyntaxKind::ProtectedKeyword
            ) {
                if seen_accessibility && !reported_accessibility_duplicate {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Accessibility modifier already seen.",
                        diagnostic_codes::ACCESSIBILITY_MODIFIER_ALREADY_SEEN,
                    );
                    reported_accessibility_duplicate = true;
                }
                // TS1029: Accessibility modifier must precede override and readonly
                if seen_override || seen_readonly {
                    use tsz_common::diagnostics::diagnostic_codes;
                    let modifier_name = match mod_kind {
                        SyntaxKind::PrivateKeyword => "private",
                        SyntaxKind::ProtectedKeyword => "protected",
                        _ => "public",
                    };
                    let other = if seen_override {
                        "override"
                    } else {
                        "readonly"
                    };
                    self.parse_error_at_current_token(
                        &format!("'{modifier_name}' modifier must precede '{other}' modifier."),
                        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                    );
                }
                seen_accessibility = true;
            } else if mod_kind == SyntaxKind::OverrideKeyword {
                seen_override = true;
            } else if mod_kind == SyntaxKind::ReadonlyKeyword {
                if seen_readonly {
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "'readonly' modifier already seen.",
                        diagnostic_codes::MODIFIER_ALREADY_SEEN,
                    );
                }
                seen_readonly = true;
            }

            self.next_token();
            let mod_end = self.token_end();
            modifiers.push(self.arena.add_token(mod_kind as u16, mod_start, mod_end));
        }

        if modifiers.is_empty() {
            None
        } else {
            Some(self.make_node_list(modifiers))
        }
    }

    fn report_this_parameter_initializer_recovery(&mut self) {
        use tsz_common::diagnostics::diagnostic_codes;

        let saved_state = self.scanner.save_state();
        let saved_token = self.current_token;

        self.next_token();
        if self.is_token(SyntaxKind::NewKeyword) {
            let new_start = self.token_pos();
            self.parse_error_at(
                new_start,
                self.token_end().saturating_sub(new_start),
                "Identifier expected. 'new' is a reserved word that cannot be used here.",
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_THAT_CANNOT_BE_USED_HERE,
            );
            self.next_token();

            if self.is_identifier_or_keyword() {
                self.next_token();
            }

            if self.is_token(SyntaxKind::OpenParenToken) {
                let open_start = self.token_pos();
                self.parse_error_at(
                    open_start,
                    self.token_end().saturating_sub(open_start),
                    "',' expected.",
                    diagnostic_codes::EXPECTED,
                );
                self.next_token();
            }

            if self.is_token(SyntaxKind::CloseParenToken) {
                let close_start = self.token_pos();
                self.parse_error_at(
                    close_start,
                    self.token_end().saturating_sub(close_start),
                    "Expression expected.",
                    diagnostic_codes::EXPRESSION_EXPECTED,
                );
                self.next_token();
            }

            if self.is_token(SyntaxKind::CloseParenToken) {
                let close_start = self.token_pos();
                self.parse_error_at(
                    close_start,
                    self.token_end().saturating_sub(close_start),
                    "';' expected.",
                    diagnostic_codes::EXPECTED,
                );
                self.next_token();
            }

            if self.is_token(SyntaxKind::ColonToken) {
                let colon_start = self.token_pos();
                self.parse_error_at(
                    colon_start,
                    self.token_end().saturating_sub(colon_start),
                    "Declaration or statement expected.",
                    diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
                );
                self.next_token();
            }

            if self.is_identifier_or_keyword() {
                let type_start = self.token_pos();
                self.parse_error_at(
                    type_start,
                    self.token_end().saturating_sub(type_start),
                    "Unexpected keyword or identifier.",
                    diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                );
            }
        }

        self.scanner.restore_state(saved_state);
        self.current_token = saved_token;
    }

    /// Parse a single parameter
    pub(crate) fn parse_parameter(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // Capture full start (including leading trivia) for diagnostics like TS1433,
        // matching tsc's use of node.pos for error ranges.
        let full_start_pos = self.token_full_start();

        // Parse parameter decorators and parameter modifiers (public/private/readonly).
        // We store decorators in the same `modifiers` list used elsewhere in the Thin AST.
        let decorators = self.parse_decorators();
        // Look ahead once: if the parameter name is `this`, suppress TS1090 inside
        // modifier parsing so the TS1433 emitted below is not eaten by
        // `parse_error_at`'s same-start-position dedup. This matches tsc, which
        // routes `async this:` / `static this:` etc. through TS1433 only.
        let param_name_is_this = self.lookahead_param_name_is_this();
        let param_modifiers = self.parse_parameter_modifiers(param_name_is_this);
        let modifiers = match (decorators, param_modifiers) {
            (None, None) => None,
            (Some(list), None) | (None, Some(list)) => Some(list),
            (Some(decorators), Some(param_modifiers)) => {
                let mut nodes = Vec::with_capacity(
                    decorators
                        .nodes
                        .len()
                        .saturating_add(param_modifiers.nodes.len()),
                );
                nodes.extend(decorators.nodes);
                nodes.extend(param_modifiers.nodes);
                Some(self.make_node_list(nodes))
            }
        };

        // Parse rest parameter (...)
        let dot_dot_dot_token = self.parse_optional(SyntaxKind::DotDotDotToken);

        // NOTE: tsc's parser does NOT check for `await`/`yield` as reserved words
        // in parameter names. Any such errors are deferred to the checker/binder.
        // `async function * f(await) {}` produces no parser error in tsc.
        // Do NOT call check_illegal_binding_identifier() here.
        if (self.context_flags & CONTEXT_FLAG_CONSTRUCTOR_PARAMETERS) != 0
            && self.is_token(SyntaxKind::StaticKeyword)
        {
            self.parse_error_at_current_token(
                "Identifier expected. 'static' is a reserved word in strict mode. Class definitions are automatically in strict mode.",
                diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
            );
        }

        // TS18009: Check for private identifiers used as parameters (check before parsing)
        if self.is_token(SyntaxKind::PrivateIdentifier) {
            let start = self.token_pos();
            let length = self.token_end() - start;
            self.parse_error_at(
                start,
                length,
                "Private identifiers cannot be used as parameters.",
                diagnostic_codes::PRIVATE_IDENTIFIERS_CANNOT_BE_USED_AS_PARAMETERS,
            );
        }

        // TS1433: Neither decorators nor modifiers may be applied to 'this' parameters.
        // Error uses full_start_pos (with leading trivia) to match tsc's node.pos range.
        if self.is_token(SyntaxKind::ThisKeyword) && modifiers.is_some() {
            let this_end = self.token_end();
            self.parse_error_at(
                full_start_pos,
                this_end - full_start_pos,
                "Neither decorators nor modifiers may be applied to 'this' parameters.",
                diagnostic_codes::NEITHER_DECORATORS_NOR_MODIFIERS_MAY_BE_APPLIED_TO_THIS_PARAMETERS,
            );
        }

        let parameter_name_is_reserved_word =
            self.is_reserved_word() && !self.is_token(SyntaxKind::DefaultKeyword);
        let is_invalid_rest_this_parameter =
            dot_dot_dot_token && self.is_token(SyntaxKind::ThisKeyword);
        let is_this_parameter =
            self.is_token(SyntaxKind::ThisKeyword) && !is_invalid_rest_this_parameter;
        // Literal reserved words (`null`, `true`, `false`) cannot form parameter
        // names at all. tsc still parses the type annotation but anchors a
        // TS1138 `Parameter declaration expected.` diagnostic at the following
        // colon (see `reservedWords2.ts`). Track this so we can replicate the
        // extra diagnostic below without emitting a cascading TS1005.
        let parameter_name_is_literal_reserved_word = matches!(
            self.token(),
            SyntaxKind::NullKeyword | SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword
        );

        // Parse parameter name - can be an identifier, keyword, or binding pattern
        let name = if self.is_token(SyntaxKind::OpenBraceToken) {
            let saved_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_PARAMETER_BINDING_PATTERN;
            let pattern = self.parse_object_binding_pattern();
            self.context_flags = saved_flags;
            pattern
        } else if self.is_token(SyntaxKind::OpenBracketToken) {
            let saved_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_PARAMETER_BINDING_PATTERN;
            let pattern = self.parse_array_binding_pattern();
            self.context_flags = saved_flags;
            pattern
        } else if is_invalid_rest_this_parameter {
            let reserved_start = self.token_pos();
            let reserved_end = self.token_end();
            self.error_reserved_word_identifier();
            self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                reserved_start,
                reserved_end,
                IdentifierData {
                    atom: Atom::NONE,
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
                },
            )
        } else if self.is_token(SyntaxKind::ThisKeyword) {
            let start_pos = self.token_pos();
            let end_pos = self.token_end();
            self.next_token();
            self.arena
                .add_token(SyntaxKind::ThisKeyword as u16, start_pos, end_pos)
        } else if matches!(
            self.token(),
            SyntaxKind::EnumKeyword
                | SyntaxKind::ClassKeyword
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::WhileKeyword
                | SyntaxKind::ForKeyword
        ) {
            let keyword = self.token();
            let reserved_start = self.token_pos();
            let reserved_end = self.token_end();
            if dot_dot_dot_token {
                self.error_reserved_word_identifier();
                if matches!(keyword, SyntaxKind::WhileKeyword | SyntaxKind::ForKeyword) {
                    self.parse_error_at_current_token("'(' expected.", diagnostic_codes::EXPECTED);
                }
            } else {
                self.error_reserved_word_in_parameter_name();
            }
            self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                reserved_start,
                reserved_end,
                IdentifierData {
                    atom: Atom::NONE,
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
                },
            )
        } else if self.is_identifier_or_keyword() {
            if parameter_name_is_literal_reserved_word {
                // Literal reserved words (`null`, `true`, `false`) cannot be
                // parameter names. tsc emits TS1359 ("Identifier expected.
                // 'X' is a reserved word that cannot be used here.") at the
                // keyword position in addition to the TS1138 emitted later
                // at the colon. Without this, the keyword would silently fall
                // into the strict-mode-reserved-word branch below and lose
                // the TS1359 diagnostic.
                let reserved_start = self.token_pos();
                let reserved_end = self.token_end();
                self.error_reserved_word_identifier();
                if self.token_pos() == reserved_start {
                    self.next_token();
                }
                self.arena.add_identifier(
                    SyntaxKind::Identifier as u16,
                    reserved_start,
                    reserved_end,
                    IdentifierData {
                        atom: Atom::NONE,
                        escaped_text: String::new(),
                        original_text: None,
                        type_arguments: None,
                    },
                )
            } else if parameter_name_is_reserved_word {
                // Strict-mode reserved parameter names are grammar-checked later.
                // Preserve the spelling so checker diagnostics and implicit-any
                // messages use `static`/`let` instead of an empty recovery name.
                self.parse_identifier_name()
            } else if self.is_reserved_word() {
                let reserved_start = self.token_pos();
                let reserved_end = self.token_end();
                self.error_reserved_word_identifier();
                if self.token_pos() == reserved_start {
                    self.next_token();
                }
                self.arena.add_identifier(
                    SyntaxKind::Identifier as u16,
                    reserved_start,
                    reserved_end,
                    IdentifierData {
                        atom: Atom::NONE,
                        escaped_text: String::new(),
                        original_text: None,
                        type_arguments: None,
                    },
                )
            } else {
                self.parse_identifier_name()
            }
        } else {
            self.parse_identifier()
        };

        // Parse optional question mark
        let question_pos = self.token_pos();
        let question_token = self.parse_optional(SyntaxKind::QuestionToken);
        if is_this_parameter && question_token {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at(question_pos, 1, "',' expected.", diagnostic_codes::EXPECTED);
        }

        let type_annotation = if parameter_name_is_literal_reserved_word
            && self.is_token(SyntaxKind::ColonToken)
        {
            // Emit TS1138 at the colon to match tsc's reserved-literal recovery,
            // then still consume `: <type>` so we don't cascade into TS1005.
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.parse_error_at_current_token(
                diagnostic_messages::PARAMETER_DECLARATION_EXPECTED,
                diagnostic_codes::PARAMETER_DECLARATION_EXPECTED,
            );
            self.next_token();
            self.parse_type()
        } else if is_invalid_rest_this_parameter && self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else if parameter_name_is_reserved_word && !is_this_parameter {
            NodeIndex::NONE
        } else if self.parse_optional(SyntaxKind::ColonToken) {
            // Parameter type annotations do NOT allow type predicates (matching tsc).
            // In tsc, parseParameterType() calls parseType(), not parseTypeOrTypePredicate().
            // Type predicates are only valid in return type positions.
            // `this is T` is still parsed as a type predicate here because
            // parse_type() always allows `this is T` (tsc: parseThisTypeOrThisTypePredicate).
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        let initializer = if is_this_parameter && self.is_token(SyntaxKind::EqualsToken) {
            self.report_this_parameter_initializer_recovery();
            NodeIndex::NONE
        } else if self.parse_optional(SyntaxKind::EqualsToken) {
            // NOTE: TS1015 (Parameter cannot have question mark and initializer)
            // is a grammar check emitted by the checker, not the parser.
            // See CheckerState::check_parameter_ordering.

            // Default parameter values are evaluated in the parent scope, not in the function body.
            // Set parameter default context flag to detect 'await' usage.
            // IMPORTANT: Keep async context set - TSC emits TS1109 "Expression expected" when
            // 'await' appears in a parameter default without an operand (e.g., `async (a = await)`)
            let saved_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_PARAMETER_DEFAULT;
            let initializer = self.parse_assignment_expression();
            if initializer.is_none() {
                // Emit TS1109 for missing parameter default value: param = [missing]
                self.error_expression_expected();
            }
            self.context_flags = saved_flags;
            initializer
        } else {
            NodeIndex::NONE
        };

        // TS1047: A rest parameter cannot be optional
        if dot_dot_dot_token && question_token {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at(
                question_pos,
                1,
                "A rest parameter cannot be optional.",
                diagnostic_codes::A_REST_PARAMETER_CANNOT_BE_OPTIONAL,
            );
        }

        if dot_dot_dot_token
            && question_token
            && type_annotation.is_none()
            && initializer.is_none()
            && let Some(node) = self.arena.get_mut(name)
        {
            node.pos = start_pos;
        }

        // TS1048: A rest parameter cannot have an initializer
        if dot_dot_dot_token && initializer != NodeIndex::NONE {
            use tsz_common::diagnostics::diagnostic_codes;
            if let Some(node) = self.arena.get(name) {
                self.parse_error_at(
                    node.pos,
                    node.end - node.pos,
                    "A rest parameter cannot have an initializer.",
                    diagnostic_codes::A_REST_PARAMETER_CANNOT_HAVE_AN_INITIALIZER,
                );
            }
        }

        let mut parameter_start_pos = start_pos;
        if question_token && self.is_js_file() && modifiers.is_none() && !dot_dot_dot_token {
            parameter_start_pos = question_pos;
        }

        let end_pos = self.token_end();
        self.arena.add_parameter(
            syntax_kind_ext::PARAMETER,
            parameter_start_pos,
            end_pos,
            crate::parser::node::ParameterData {
                modifiers,
                dot_dot_dot_token,
                name,
                question_token,
                type_annotation,
                initializer,
            },
        )
    }

    // Class declarations, decorators, and heritage clauses -> state_statements_class_declarations.rs
}
