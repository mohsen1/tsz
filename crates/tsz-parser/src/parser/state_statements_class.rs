//! Parser state - class expression and class declaration parsing.

use super::state::{
    CONTEXT_FLAG_AMBIENT, CONTEXT_FLAG_ARROW_PARAMETERS, CONTEXT_FLAG_CONSTRUCTOR_PARAMETERS,
    CONTEXT_FLAG_IN_CLASS, CONTEXT_FLAG_PARAMETER_DEFAULT, ParserState,
};
use crate::parser::{NodeIndex, NodeList, node::ClassData, syntax_kind_ext};
use tsz_common::diagnostics::diagnostic_codes;
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
            // Check BEFORE parsing the next parameter (but only emit once)
            if seen_rest_parameter && !emitted_rest_error {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
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
                // Recovery: in malformed parameter initializers like
                // `function* f(a = yield => yield) {}` or
                // `async function f(a = await => await) {}`
                // treat `=>` as a missing comma boundary to continue parsing.
                if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
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
                    self.error_comma_expected();
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

    /// Parse parameter modifiers (public, private, protected, readonly, override,
    /// and invalid ones like static/export/declare/async which get TS1090).
    pub(crate) fn parse_parameter_modifiers(&mut self) -> Option<NodeList> {
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
            if !self.is_valid_parameter_modifier() {
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

    /// Parse a single parameter
    pub(crate) fn parse_parameter(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse parameter decorators and parameter modifiers (public/private/readonly).
        // We store decorators in the same `modifiers` list used elsewhere in the Thin AST.
        let decorators = self.parse_decorators();
        let param_modifiers = self.parse_parameter_modifiers();
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

        // Check for illegal binding identifiers (e.g., 'await' in async contexts, 'yield' in generator contexts)
        // This must be called BEFORE parsing the parameter name to catch reserved words
        self.check_illegal_binding_identifier();
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
        // Error points at the decorator/modifier position (start_pos), not the 'this' keyword.
        if self.is_token(SyntaxKind::ThisKeyword) && modifiers.is_some() {
            let this_end = self.token_end();
            self.parse_error_at(
                start_pos,
                this_end - start_pos,
                "Neither decorators nor modifiers may be applied to 'this' parameters.",
                diagnostic_codes::NEITHER_DECORATORS_NOR_MODIFIERS_MAY_BE_APPLIED_TO_THIS_PARAMETERS,
            );
        }

        // Parse parameter name - can be an identifier, keyword, or binding pattern
        let name = if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_object_binding_pattern()
        } else if self.is_token(SyntaxKind::OpenBracketToken) {
            self.parse_array_binding_pattern()
        } else if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            self.parse_identifier()
        };

        // Parse optional question mark
        let question_pos = self.token_pos();
        let question_token = self.parse_optional(SyntaxKind::QuestionToken);

        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            // Allow type predicates in parameter type annotations (matching tsc).
            // Type predicates in non-return positions are syntactically valid;
            // the checker emits TS1228 "A type predicate is only allowed in
            // return type position" when appropriate.
            self.parse_type_with_predicates(true)
        } else {
            NodeIndex::NONE
        };

        let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
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

    /// Parse class declaration
    pub(crate) fn parse_class_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Check for illegal binding identifiers (e.g., 'await' in static blocks)
        self.check_illegal_binding_identifier();

        // Parse class name - keywords like 'any', 'string' can be used as class names
        // EXCEPT extends/implements which start heritage clauses
        // AND reserved words like 'void', 'null' which cannot be identifiers
        //
        // Special case: `class implements {` and `class extends {` — if the next
        // token after extends/implements is `{`, the keyword is the class name
        // (the class body follows immediately), not a heritage clause start.
        // tsc uses the same disambiguation via isImplementsClause() lookahead.
        let is_heritage_keyword = (self.is_token(SyntaxKind::ExtendsKeyword)
            || self.is_token(SyntaxKind::ImplementsKeyword))
            && !self.look_ahead_next_is_open_brace_on_same_line();
        let name = if self.is_identifier_or_keyword() && !is_heritage_keyword {
            // TS1005: Reserved words cannot be used as class names
            if self.is_reserved_word() {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
                // Consume the invalid token to avoid cascading errors
                self.next_token();
                NodeIndex::NONE
            } else {
                self.parse_identifier_name()
            }
        } else {
            NodeIndex::NONE
        };

        // Parse type parameters: class Foo<T, U> {}
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse heritage clauses (extends, implements)
        let heritage_clauses = self.parse_heritage_clauses();

        // Parse class body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let class_saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_CLASS;
        let members = self.parse_class_members();
        self.context_flags = class_saved_flags;
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers: None,
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse class declaration with explicit modifiers.
    pub(crate) fn parse_class_declaration_with_modifiers(
        &mut self,
        start_pos: u32,
        modifiers: Option<NodeList>,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Parse class name - keywords like 'any', 'string' can be used as class names
        // EXCEPT extends/implements which start heritage clauses
        // AND reserved words like 'void', 'null' which cannot be identifiers
        //
        // Special case: `class implements {` and `class extends {` — if the next
        // token after extends/implements is `{`, the keyword is the class name
        // (the class body follows immediately), not a heritage clause start.
        let is_heritage_keyword = (self.is_token(SyntaxKind::ExtendsKeyword)
            || self.is_token(SyntaxKind::ImplementsKeyword))
            && !self.look_ahead_next_is_open_brace_on_same_line();
        let name = if self.is_identifier_or_keyword() && !is_heritage_keyword {
            // TS1005: Reserved words cannot be used as class names
            if self.is_reserved_word() {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
                // Consume the invalid token to avoid cascading errors
                self.next_token();
                NodeIndex::NONE
            } else {
                self.parse_identifier_name()
            }
        } else {
            NodeIndex::NONE
        };

        // Parse type parameters: class Foo<T, U> {}
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse heritage clauses (extends, implements)
        let heritage_clauses = self.parse_heritage_clauses();

        // Parse class body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let class_saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_CLASS;
        let members = self.parse_class_members();
        self.context_flags = class_saved_flags;
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers,
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse abstract class declaration: abstract class Foo {}
    pub(crate) fn parse_abstract_class_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Create abstract modifier node
        let abstract_start = self.token_pos();
        self.parse_expected(SyntaxKind::AbstractKeyword);
        let abstract_end = self.token_end();
        let abstract_modifier = self.arena.add_token(
            SyntaxKind::AbstractKeyword as u16,
            abstract_start,
            abstract_end,
        );

        // Now parse the class
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Parse class name - keywords like 'any', 'string' can be used as class names
        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        // Parse type parameters: abstract class Foo<T, U> {}
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse heritage clauses (extends, implements)
        let heritage_clauses = self.parse_heritage_clauses();

        // Parse class body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let class_saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_CLASS;
        let members = self.parse_class_members();
        self.context_flags = class_saved_flags;
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers: Some(self.make_node_list(vec![abstract_modifier])),
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse declare class: declare class Foo {}
    pub(crate) fn parse_declare_class(
        &mut self,
        start_pos: u32,
        declare_modifier: NodeIndex,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::ClassKeyword);

        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        let heritage_clauses = self.parse_heritage_clauses();

        self.parse_expected(SyntaxKind::OpenBraceToken);

        // Set ambient context for class members
        let saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_AMBIENT | CONTEXT_FLAG_IN_CLASS;

        let members = self.parse_class_members();

        // Restore context flags
        self.context_flags = saved_flags;

        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers: Some(self.make_node_list(vec![declare_modifier])),
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse declare abstract class: declare abstract class Foo {}
    pub(crate) fn parse_declare_abstract_class(
        &mut self,
        start_pos: u32,
        declare_modifier: NodeIndex,
    ) -> NodeIndex {
        // Create abstract modifier node
        let abstract_start = self.token_pos();
        self.parse_expected(SyntaxKind::AbstractKeyword);
        let abstract_end = self.token_end();
        let abstract_modifier = self.arena.add_token(
            SyntaxKind::AbstractKeyword as u16,
            abstract_start,
            abstract_end,
        );

        self.parse_expected(SyntaxKind::ClassKeyword);

        let name = if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            NodeIndex::NONE
        };

        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        let heritage_clauses = self.parse_heritage_clauses();

        self.parse_expected(SyntaxKind::OpenBraceToken);

        // Set ambient context for class members
        let saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_AMBIENT | CONTEXT_FLAG_IN_CLASS;

        let members = self.parse_class_members();

        // Restore context flags
        self.context_flags = saved_flags;

        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers: Some(self.make_node_list(vec![declare_modifier, abstract_modifier])),
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse a decorated declaration: @decorator class/function
    pub(crate) fn parse_decorated_declaration(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse decorators
        let decorators = self.parse_decorators();

        // After decorators, expect class or abstract class
        // Decorators on other declarations are invalid (TS1206)
        match self.token() {
            SyntaxKind::ClassKeyword => {
                self.parse_class_declaration_with_decorators(decorators, start_pos)
            }
            SyntaxKind::AbstractKeyword => {
                // abstract class with decorators
                self.parse_abstract_class_declaration_with_decorators(decorators, start_pos)
            }
            SyntaxKind::FunctionKeyword => {
                // TS1206: Decorators are not valid on function declarations
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_function_declaration()
            }
            SyntaxKind::EnumKeyword => {
                // TS1206: Decorators are not valid on enum declarations
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_enum_declaration_with_modifiers(start_pos, decorators)
            }
            SyntaxKind::InterfaceKeyword => {
                // TS1206: Decorators are not valid on interface declarations
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_interface_declaration_with_modifiers(start_pos, decorators)
            }
            SyntaxKind::TypeKeyword => {
                // TS1206: Decorators are not valid on type alias declarations
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_type_alias_declaration_with_modifiers(start_pos, decorators)
            }
            SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword => {
                // TS1206: Decorators are not valid on namespace/module declarations
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_module_declaration_with_modifiers(start_pos, decorators)
            }
            SyntaxKind::VarKeyword | SyntaxKind::LetKeyword | SyntaxKind::ConstKeyword => {
                // TS1206: Decorators are not valid on variable statements
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                self.parse_variable_statement_with_modifiers(Some(start_pos), decorators)
            }
            SyntaxKind::UsingKeyword => {
                // tsc does NOT emit TS1206 for `@dec using ...`; it relies on
                // the variable-declaration parser to emit TS1134 when the syntax
                // after `using` is invalid (e.g., `using 1`).  When the syntax IS
                // valid (`using x`), no error is reported at all.
                self.parse_variable_statement_with_modifiers(Some(start_pos), decorators)
            }
            SyntaxKind::ImportKeyword => {
                // TS1206: Decorators are not valid on import statements
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    start_pos,
                    0,
                    "Decorators are not valid here.",
                    diagnostic_codes::DECORATORS_ARE_NOT_VALID_HERE,
                );
                // Check if this is import equals (import X = ...) or regular import
                if self.look_ahead_is_import_equals() {
                    self.parse_import_equals_declaration()
                } else {
                    self.parse_import_declaration()
                }
            }
            SyntaxKind::ExportKeyword => {
                // Export with decorators: @decorator export class Foo {}
                self.parse_export_declaration_with_decorators(start_pos, decorators)
            }
            SyntaxKind::DefaultKeyword => {
                // TS1029: `export` must precede `default`.
                use tsz_common::diagnostics::diagnostic_codes;
                let default_start = self.token_pos();
                let default_end = self.token_end();
                self.parse_error_at(
                    default_start,
                    default_end - default_start,
                    "'export' modifier must precede 'default' modifier.",
                    diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
                );

                // Consume `default` so declaration parsing can continue.
                self.next_token();
                let default_modifier = self.arena.add_token(
                    SyntaxKind::DefaultKeyword as u16,
                    default_start,
                    default_end,
                );
                let mut nodes = decorators.map(|list| list.nodes).unwrap_or_default();
                nodes.push(default_modifier);
                let modifiers = Some(self.make_node_list(nodes));

                match self.token() {
                    SyntaxKind::ClassKeyword => {
                        self.parse_class_declaration_with_modifiers(start_pos, modifiers)
                    }
                    SyntaxKind::AbstractKeyword => self.parse_abstract_class_declaration(),
                    SyntaxKind::InterfaceKeyword => {
                        self.parse_interface_declaration_with_modifiers(start_pos, modifiers)
                    }
                    _ => self.parse_expression_statement(),
                }
            }
            _ => {
                // TS1146: When decorators are followed by a non-declaration token,
                // tsc emits "Declaration expected" rather than "Decorators are not valid here"
                // because the decorator implies the user intended to write a declaration.
                // Use token_full_start (including leading trivia) to match tsc's error position.
                use tsz_common::diagnostics::diagnostic_codes;
                let err_pos = self.token_full_start();
                self.parse_error_at(
                    err_pos,
                    0,
                    "Declaration expected.",
                    diagnostic_codes::DECLARATION_EXPECTED,
                );
                self.parse_expression_statement()
            }
        }
    }

    /// Parse decorators: @decorator1 @decorator2(arg) ...
    pub(crate) fn parse_decorators(&mut self) -> Option<NodeList> {
        if !self.is_token(SyntaxKind::AtToken) {
            return None;
        }

        let mut decorators = Vec::new();

        while self.is_token(SyntaxKind::AtToken) {
            if let Some(decorator) = self.try_parse_decorator() {
                decorators.push(decorator);
            } else {
                break;
            }
        }

        if decorators.is_empty() {
            None
        } else {
            Some(self.make_node_list(decorators))
        }
    }

    /// Try to parse a single decorator
    pub(crate) fn try_parse_decorator(&mut self) -> Option<NodeIndex> {
        if !self.is_token(SyntaxKind::AtToken) {
            return None;
        }

        let start_pos = self.token_pos();
        let snapshot = self.scanner.save_state();
        let at_token = self.current_token;
        self.next_token(); // consume @
        if self.is_token(SyntaxKind::Unknown) {
            self.scanner.restore_state(snapshot);
            self.current_token = at_token;
            return None;
        }

        // Parse the decorator expression (identifier, member access, or call)
        // Set CONTEXT_FLAG_IN_DECORATOR so that '[' is NOT treated as element access
        // (it starts a computed property name on the decorated member instead)
        let saved_flags = self.context_flags;
        self.context_flags |= crate::parser::state::CONTEXT_FLAG_IN_DECORATOR;
        let expression = self.parse_left_hand_side_expression();
        self.context_flags = saved_flags;

        let end_pos = self.token_end();
        Some(self.arena.add_decorator(
            syntax_kind_ext::DECORATOR,
            start_pos,
            end_pos,
            crate::parser::node::DecoratorData { expression },
        ))
    }

    /// Parse class declaration with pre-parsed decorators
    pub(crate) fn parse_class_declaration_with_decorators(
        &mut self,
        decorators: Option<NodeList>,
        start_pos: u32,
    ) -> NodeIndex {
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Parse class name
        // Check for illegal binding identifiers (e.g., 'await' in static blocks)
        self.check_illegal_binding_identifier();

        let name = if self.is_token(SyntaxKind::Identifier) {
            self.parse_identifier()
        } else {
            NodeIndex::NONE
        };

        // Parse type parameters
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse heritage clauses (extends, implements)
        let heritage_clauses = self.parse_heritage_clauses();

        // Parse class body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let class_saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_CLASS;
        let members = self.parse_class_members();
        self.context_flags = class_saved_flags;
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();

        // Create a modifiers list from decorators
        // In TypeScript, decorators are part of the modifiers
        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers: decorators,
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse abstract class declaration with pre-parsed decorators
    pub(crate) fn parse_abstract_class_declaration_with_decorators(
        &mut self,
        decorators: Option<NodeList>,
        start_pos: u32,
    ) -> NodeIndex {
        // Create abstract modifier node
        let abstract_start = self.token_pos();
        self.parse_expected(SyntaxKind::AbstractKeyword);
        let abstract_end = self.token_end();
        let abstract_modifier = self.arena.add_token(
            SyntaxKind::AbstractKeyword as u16,
            abstract_start,
            abstract_end,
        );

        // Now parse the class
        self.parse_expected(SyntaxKind::ClassKeyword);

        // Parse class name
        let name = if self.is_token(SyntaxKind::Identifier) {
            self.parse_identifier()
        } else {
            NodeIndex::NONE
        };

        // Parse type parameters
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse heritage clauses (extends, implements)
        let heritage_clauses = self.parse_heritage_clauses();

        // Parse class body
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let class_saved_flags = self.context_flags;
        self.context_flags |= CONTEXT_FLAG_IN_CLASS;
        let members = self.parse_class_members();
        self.context_flags = class_saved_flags;
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();

        // Combine decorators with abstract modifier
        let modifiers = if let Some(dec_list) = decorators {
            // Add abstract modifier to decorator list
            let mut nodes: Vec<NodeIndex> = dec_list.nodes;
            nodes.push(abstract_modifier);
            Some(self.make_node_list(nodes))
        } else {
            Some(self.make_node_list(vec![abstract_modifier]))
        };

        self.arena.add_class(
            syntax_kind_ext::CLASS_DECLARATION,
            start_pos,
            end_pos,
            ClassData {
                modifiers,
                name,
                type_parameters,
                heritage_clauses,
                members,
            },
        )
    }

    /// Parse heritage clauses (extends, implements)
    pub(crate) fn parse_heritage_clauses(&mut self) -> Option<NodeList> {
        let mut clauses = Vec::new();
        let mut seen_extends = false;
        let mut seen_implements = false;

        loop {
            if self.is_token(SyntaxKind::ExtendsKeyword) {
                if let Some(clause) =
                    self.parse_heritage_clause_extends(&mut seen_extends, seen_implements)
                {
                    clauses.push(clause);
                }
                continue;
            }

            if self.is_token(SyntaxKind::ImplementsKeyword) {
                if let Some(clause) = self.parse_heritage_clause_implements(&mut seen_implements) {
                    clauses.push(clause);
                }
                continue;
            }

            break;
        }

        if clauses.is_empty() {
            None
        } else {
            Some(self.make_node_list(clauses))
        }
    }

    fn parse_heritage_clause_extends(
        &mut self,
        seen_extends: &mut bool,
        seen_implements: bool,
    ) -> Option<NodeIndex> {
        use tsz_common::diagnostics::diagnostic_codes;

        let start_pos = self.token_pos();
        let is_duplicate = *seen_extends;

        if is_duplicate {
            self.parse_error_at_current_token(
                "'extends' clause already seen.",
                diagnostic_codes::EXTENDS_CLAUSE_ALREADY_SEEN,
            );
        } else if seen_implements {
            self.parse_error_at_current_token(
                "'extends' clause must precede 'implements' clause.",
                diagnostic_codes::EXTENDS_CLAUSE_MUST_PRECEDE_IMPLEMENTS_CLAUSE,
            );
        }

        *seen_extends = true;
        self.next_token();

        if self.is_token(SyntaxKind::ImplementsKeyword)
            || (self.is_token(SyntaxKind::OpenBraceToken)
                && !self.look_ahead_is_object_literal_heritage_expression())
        {
            // Use full start position (including leading trivia) to match TSC's
            // grammarErrorAtPos(node, types.pos, 0, ...) which uses getTokenFullStart().
            self.parse_error_at(
                self.token_full_start(),
                0,
                "'extends' list cannot be empty.",
                diagnostic_codes::LIST_CANNOT_BE_EMPTY,
            );
            // Still create the heritage clause with empty types list for error recovery.
            // tsc preserves the `extends` keyword in the output even when the list is empty.
            let end_pos = self.token_full_start();
            return Some(self.arena.add_heritage(
                syntax_kind_ext::HERITAGE_CLAUSE,
                start_pos,
                end_pos,
                crate::parser::node::HeritageData {
                    token: SyntaxKind::ExtendsKeyword as u16,
                    types: self.make_node_list(Vec::new()),
                },
            ));
        }

        if is_duplicate {
            self.skip_heritage_type_references_for_recovery();
            return None;
        }

        let type_ref = self.parse_heritage_type_reference();
        let mut type_refs = vec![type_ref];

        while self.is_token(SyntaxKind::CommaToken) {
            let comma_pos = self.token_pos();
            let comma_end = self.token_end();
            self.next_token();
            if self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::ImplementsKeyword)
            {
                self.parse_error_at(
                    comma_pos,
                    comma_end - comma_pos,
                    tsz_common::diagnostics::diagnostic_messages::TRAILING_COMMA_NOT_ALLOWED,
                    diagnostic_codes::TRAILING_COMMA_NOT_ALLOWED,
                );
                break;
            }
            self.parse_error_at(
                self.token_pos(),
                0,
                "Classes can only extend a single class.",
                diagnostic_codes::CLASSES_CAN_ONLY_EXTEND_A_SINGLE_CLASS,
            );
            let extra_ref = self.parse_heritage_type_reference();
            type_refs.push(extra_ref);
        }

        let end_pos = self.token_end();
        Some(self.arena.add_heritage(
            syntax_kind_ext::HERITAGE_CLAUSE,
            start_pos,
            end_pos,
            crate::parser::node::HeritageData {
                token: SyntaxKind::ExtendsKeyword as u16,
                types: self.make_node_list(type_refs),
            },
        ))
    }

    fn parse_heritage_clause_implements(
        &mut self,
        seen_implements: &mut bool,
    ) -> Option<NodeIndex> {
        use tsz_common::diagnostics::diagnostic_codes;

        let start_pos = self.token_pos();
        if *seen_implements {
            self.parse_error_at_current_token(
                "'implements' clause already seen.",
                diagnostic_codes::IMPLEMENTS_CLAUSE_ALREADY_SEEN,
            );
        }

        let is_duplicate = *seen_implements;
        *seen_implements = true;
        self.next_token();

        // TS1097: 'implements' list cannot be empty.
        if self.is_token(SyntaxKind::OpenBraceToken) || self.is_token(SyntaxKind::ExtendsKeyword) {
            // Use full start position (including leading trivia) to match TSC's
            // grammarErrorAtPos(node, types.pos, 0, ...) which uses getTokenFullStart().
            self.parse_error_at(
                self.token_full_start(),
                0,
                "'implements' list cannot be empty.",
                diagnostic_codes::LIST_CANNOT_BE_EMPTY,
            );
            return None;
        }

        if is_duplicate {
            self.skip_heritage_type_references_for_recovery();
            return None;
        }

        let mut types = Vec::new();
        loop {
            let type_ref = self.parse_heritage_type_reference();
            types.push(type_ref);
            if self.is_token(SyntaxKind::CommaToken) {
                let comma_pos = self.token_pos();
                let comma_end = self.token_end();
                self.next_token();
                // Trailing comma before { — emit TS1009 like the extends clause does
                if self.is_token(SyntaxKind::OpenBraceToken)
                    || self.is_token(SyntaxKind::ExtendsKeyword)
                {
                    self.parse_error_at(
                        comma_pos,
                        comma_end - comma_pos,
                        tsz_common::diagnostics::diagnostic_messages::TRAILING_COMMA_NOT_ALLOWED,
                        diagnostic_codes::TRAILING_COMMA_NOT_ALLOWED,
                    );
                    break;
                }
            } else {
                break;
            }
        }

        let end_pos = self.token_end();
        Some(self.arena.add_heritage(
            syntax_kind_ext::HERITAGE_CLAUSE,
            start_pos,
            end_pos,
            crate::parser::node::HeritageData {
                token: SyntaxKind::ImplementsKeyword as u16,
                types: self.make_node_list(types),
            },
        ))
    }

    fn skip_heritage_type_references_for_recovery(&mut self) {
        while !self.is_token(SyntaxKind::OpenBraceToken)
            && !self.is_token(SyntaxKind::ExtendsKeyword)
            && !self.is_token(SyntaxKind::ImplementsKeyword)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let _ = self.parse_heritage_type_reference();
            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }
    }

    /// Parse a heritage type reference: Foo or Foo<T> or Foo.Bar<T> or base<T>()
    /// This is used in extends/implements clauses
    pub(crate) fn parse_heritage_type_reference(&mut self) -> NodeIndex {
        // parse_heritage_left_hand_expression now handles:
        // - Simple identifiers: Foo
        // - Property access: Foo.Bar.Baz
        // - Type arguments: Foo<T>
        // - Call expressions: Mixin(Parent) or base<T>()
        self.parse_heritage_left_hand_expression()
    }

    fn look_ahead_is_object_literal_heritage_expression(&mut self) -> bool {
        if !self.is_token(SyntaxKind::OpenBraceToken) {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let saved_token = self.current_token;
        let mut brace_depth = 0u32;
        let mut result = false;

        while !self.is_token(SyntaxKind::EndOfFileToken) {
            match self.token() {
                SyntaxKind::OpenBraceToken => {
                    brace_depth += 1;
                    self.next_token();
                }
                SyntaxKind::CloseBraceToken => {
                    if brace_depth == 0 {
                        break;
                    }
                    brace_depth -= 1;
                    self.next_token();
                    if brace_depth == 0 {
                        result = self.is_token(SyntaxKind::OpenBraceToken);
                        break;
                    }
                }
                _ => {
                    self.next_token();
                }
            }
        }

        self.scanner.restore_state(snapshot);
        self.current_token = saved_token;
        result
    }

    /// Parse heritage type reference for interfaces (extends clause).
    /// Interfaces must reference types; literals or arbitrary expressions should produce diagnostics.
    pub(crate) fn parse_interface_heritage_type_reference(&mut self) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        if self.is_token(SyntaxKind::OpenParenToken) || self.is_token(SyntaxKind::OpenBracketToken)
        {
            let start_pos = self.token_pos();
            let invalid_ref = self.parse_heritage_type_reference();
            let end_pos = self.token_end();
            self.parse_error_at(
                start_pos,
                end_pos - start_pos,
                "An interface can only extend an identifier/qualified-name with optional type arguments.",
                diagnostic_codes::AN_INTERFACE_CAN_ONLY_EXTEND_AN_IDENTIFIER_QUALIFIED_NAME_WITH_OPTIONAL_TYPE_ARG,
            );
            return invalid_ref;
        }

        if matches!(
            self.token(),
            SyntaxKind::NullKeyword
                | SyntaxKind::TrueKeyword
                | SyntaxKind::FalseKeyword
                | SyntaxKind::UndefinedKeyword
                | SyntaxKind::VoidKeyword
                | SyntaxKind::NumericLiteral
                | SyntaxKind::StringLiteral
                | SyntaxKind::BigIntLiteral
                | SyntaxKind::NoSubstitutionTemplateLiteral
                | SyntaxKind::TemplateHead
                | SyntaxKind::ClassKeyword
                | SyntaxKind::NewKeyword
                | SyntaxKind::OpenParenToken
        ) {
            let start = self.token_pos();
            let end = self.token_end();
            self.parse_error_at_current_token(
                "Type name expected in interface extends clause.",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
            self.next_token();
            return self.arena.add_token(SyntaxKind::Unknown as u16, start, end);
        }

        self.parse_heritage_type_reference()
    }

    /// Parse left-hand expression for heritage clauses: Foo, Foo.Bar, or Mixin(Parent)
    /// This is a subset of member expression that allows identifiers, dots, and call expressions
    pub(crate) fn parse_heritage_left_hand_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut expr = self.parse_heritage_left_hand_expression_base();

        while let Some(next_expr) = self.parse_heritage_left_hand_expression_chain(start_pos, expr)
        {
            expr = next_expr;
        }

        expr
    }

    fn parse_heritage_left_hand_expression_base(&mut self) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        if self.is_token(SyntaxKind::ClassKeyword) {
            self.parse_class_expression()
        } else if self.is_token(SyntaxKind::ThisKeyword) {
            self.parse_this_expression()
        } else if self.is_token(SyntaxKind::OpenParenToken) || self.is_token(SyntaxKind::NewKeyword)
        {
            self.parse_left_hand_side_expression()
        } else if self.is_token(SyntaxKind::VoidKeyword) {
            let start_pos = self.token_pos();
            let end_pos = self.token_end();
            self.error_expression_expected();
            self.next_token();
            self.arena
                .add_token(SyntaxKind::Unknown as u16, start_pos, end_pos)
        } else if matches!(
            self.token(),
            SyntaxKind::NullKeyword
                | SyntaxKind::TrueKeyword
                | SyntaxKind::FalseKeyword
                | SyntaxKind::UndefinedKeyword
                | SyntaxKind::NumericLiteral
                | SyntaxKind::StringLiteral
                | SyntaxKind::BigIntLiteral
                | SyntaxKind::NoSubstitutionTemplateLiteral
                | SyntaxKind::TemplateHead
                | SyntaxKind::OpenBraceToken
                | SyntaxKind::OpenBracketToken
        ) {
            self.parse_primary_expression()
        } else if self.is_identifier_or_keyword() {
            self.parse_identifier_name()
        } else {
            self.parse_error_at_current_token(
                "Class name or type expression expected",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
            let start_pos = self.token_pos();
            let end_pos = self.token_end();
            self.next_token();
            self.arena
                .add_token(SyntaxKind::Unknown as u16, start_pos, end_pos)
        }
    }

    fn parse_heritage_left_hand_expression_chain(
        &mut self,
        start_pos: u32,
        expr: NodeIndex,
    ) -> Option<NodeIndex> {
        match self.token() {
            SyntaxKind::DotToken => {
                self.next_token();
                let name = if self.is_identifier_or_keyword() {
                    self.parse_identifier_name()
                } else {
                    self.parse_identifier()
                };
                let end_pos = self.token_end();
                Some(self.arena.add_access_expr(
                    syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                    start_pos,
                    end_pos,
                    crate::parser::node::AccessExprData {
                        expression: expr,
                        name_or_argument: name,
                        question_dot_token: false,
                    },
                ))
            }
            SyntaxKind::QuestionDotToken => {
                self.next_token();
                let name = if self.is_identifier_or_keyword() {
                    self.parse_identifier_name()
                } else {
                    self.parse_identifier()
                };
                let end_pos = self.token_end();
                Some(self.arena.add_access_expr(
                    syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                    start_pos,
                    end_pos,
                    crate::parser::node::AccessExprData {
                        expression: expr,
                        name_or_argument: name,
                        question_dot_token: true,
                    },
                ))
            }
            SyntaxKind::LessThanToken => {
                self.next_token();
                let mut type_args = Vec::new();
                while !self.is_token(SyntaxKind::GreaterThanToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    type_args.push(self.parse_type());
                    if !self.parse_optional(SyntaxKind::CommaToken) {
                        break;
                    }
                }
                self.parse_expected(SyntaxKind::GreaterThanToken);
                if self.is_token(SyntaxKind::OpenParenToken) {
                    self.next_token();
                    let (end_pos, args) = self.parse_heritage_call_arguments();
                    Some(self.arena.add_call_expr(
                        syntax_kind_ext::CALL_EXPRESSION,
                        start_pos,
                        end_pos,
                        crate::parser::node::CallExprData {
                            expression: expr,
                            type_arguments: Some(self.make_node_list(type_args)),
                            arguments: Some(args),
                        },
                    ))
                } else {
                    Some(self.arena.add_expr_with_type_args(
                        syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS,
                        start_pos,
                        self.token_end(),
                        crate::parser::node::ExprWithTypeArgsData {
                            expression: expr,
                            type_arguments: Some(self.make_node_list(type_args)),
                        },
                    ))
                }
            }
            SyntaxKind::OpenParenToken => {
                self.next_token();
                let (end_pos, args) = self.parse_heritage_call_arguments();
                Some(self.arena.add_call_expr(
                    syntax_kind_ext::CALL_EXPRESSION,
                    start_pos,
                    end_pos,
                    crate::parser::node::CallExprData {
                        expression: expr,
                        type_arguments: None,
                        arguments: Some(args),
                    },
                ))
            }
            _ => None,
        }
    }

    fn parse_heritage_call_arguments(&mut self) -> (u32, NodeList) {
        let mut args = Vec::new();
        while !self.is_token(SyntaxKind::CloseParenToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            args.push(self.parse_assignment_expression());
            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }
        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseParenToken);
        (end_pos, self.make_node_list(args))
    }
}
