//! Parser state - function type, type assertion, and JSX parsing.

use super::state::{ParseDiagnostic, ParserState};
use crate::parser::{NodeArena, NodeIndex, NodeList, node, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

impl ParserState {
    pub(crate) const fn is_jsx_attribute_list_abort_token(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::NumericLiteral | SyntaxKind::MinusToken | SyntaxKind::PlusToken
        )
    }

    pub(crate) const fn jsx_attribute_abort_consumes_following_identifier(
        kind: SyntaxKind,
    ) -> bool {
        matches!(kind, SyntaxKind::MinusToken | SyntaxKind::PlusToken)
    }

    pub(crate) fn look_ahead_next_is_identifier_or_keyword_or_greater_than(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let result = self.is_identifier_or_keyword() || self.is_token(SyntaxKind::GreaterThanToken);
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        result
    }

    /// Look ahead to see if ( starts a function type: () => T or (x: T) => U
    /// Determines if `(` starts a function type like `(x: T) => U`.
    /// Matches tsc's `isUnambiguouslyStartOfFunctionType` — only returns true
    /// when the first token(s) after `(` clearly indicate parameter syntax.
    /// This avoids treating parenthesized types like `(() => T)` as function types
    /// when followed by `=>` from an enclosing arrow function.
    pub(crate) fn look_ahead_is_function_type(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip (
        self.next_token();

        // Empty params: () =>
        if self.is_token(SyntaxKind::CloseParenToken) {
            self.next_token();
            let is_arrow = self.is_token(SyntaxKind::EqualsGreaterThanToken);
            self.scanner.restore_state(snapshot);
            self.current_token = current;
            return is_arrow;
        }

        // Rest parameter: (...) =>
        if self.is_token(SyntaxKind::DotDotDotToken) {
            self.scanner.restore_state(snapshot);
            self.current_token = current;
            return true;
        }

        // Skip parameter modifier keywords (public, private, protected, readonly)
        // before checking for parameter name. E.g., (public x: T) => U
        // BUT only consume them as modifiers when followed by something that can
        // act as a parameter name. `(private)` and `(private, public) => void`
        // have those keywords AS the parameter names — they must not be eaten
        // here, otherwise the lookahead concludes "not a function type" and
        // the outer `parse_parenthesized_type_or_function_type` falls back to
        // parenthesized-type parsing, where the keywords are not valid type
        // identifiers.
        if matches!(
            self.token(),
            SyntaxKind::PublicKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::ReadonlyKeyword
        ) {
            let saved_state = self.scanner.save_state();
            let saved_token = self.current_token;
            self.next_token();
            let next_can_be_param_name =
                self.is_identifier_or_keyword() || self.is_token(SyntaxKind::ThisKeyword);
            if !next_can_be_param_name {
                // Modifier keyword is being used as the parameter name itself.
                // Roll back so the unified parameter-start branch below sees it.
                self.scanner.restore_state(saved_state);
                self.current_token = saved_token;
            }
        }

        // Try to skip a parameter start (identifier/keyword or `this`)
        // and check if it's followed by parameter-like tokens (: , ?)
        if self.is_identifier_or_keyword() || self.is_token(SyntaxKind::ThisKeyword) {
            self.next_token();
            if matches!(
                self.token(),
                SyntaxKind::ColonToken
                    | SyntaxKind::CommaToken
                    | SyntaxKind::QuestionToken
                    | SyntaxKind::EqualsToken
            ) {
                self.scanner.restore_state(snapshot);
                self.current_token = current;
                return true;
            }
            // Single param followed by ) then => : (x) =>
            if self.is_token(SyntaxKind::CloseParenToken) {
                self.next_token();
                if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
                    self.scanner.restore_state(snapshot);
                    self.current_token = current;
                    return true;
                }
            }
        }

        // Array/object destructuring patterns as parameters: ([a, b]) => or ({a}) =>
        if self.is_token(SyntaxKind::OpenBracketToken) || self.is_token(SyntaxKind::OpenBraceToken)
        {
            // Skip to matching bracket
            let open = self.token();
            let close = if open == SyntaxKind::OpenBracketToken {
                SyntaxKind::CloseBracketToken
            } else {
                SyntaxKind::CloseBraceToken
            };
            let mut depth = 1;
            self.next_token();
            while depth > 0 && !self.is_token(SyntaxKind::EndOfFileToken) {
                if self.token() == open {
                    depth += 1;
                } else if self.token() == close {
                    depth -= 1;
                }
                if depth > 0 {
                    self.next_token();
                }
            }
            if depth == 0 {
                self.next_token(); // skip closing bracket
                // After destructuring, check for : , ?
                if matches!(
                    self.token(),
                    SyntaxKind::ColonToken | SyntaxKind::CommaToken | SyntaxKind::QuestionToken
                ) {
                    self.scanner.restore_state(snapshot);
                    self.current_token = current;
                    return true;
                }
                // Single destructured param: ([a]) =>
                if self.is_token(SyntaxKind::CloseParenToken) {
                    self.next_token();
                    if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
                        self.scanner.restore_state(snapshot);
                        self.current_token = current;
                        return true;
                    }
                }
            }
        }

        // Not unambiguously a function type — treat as parenthesized type
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        false
    }

    /// Parse function type: (x: T, y: U) => V
    pub(crate) fn parse_function_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse parameters
        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_type_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Parse =>
        self.parse_expected(SyntaxKind::EqualsGreaterThanToken);

        // Parse return type (supports type predicates: param is T)
        let type_annotation = self.parse_return_type();

        let end_pos = self.token_full_start();

        self.arena.add_function_type(
            syntax_kind_ext::FUNCTION_TYPE,
            start_pos,
            end_pos,
            crate::parser::node::FunctionTypeData {
                type_parameters: None,
                parameters,
                type_annotation,
                is_abstract: false,
            },
        )
    }

    /// Parse generic function type: <T>() => T or <T, U extends V>(x: T) => U
    pub(crate) fn parse_generic_function_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse type parameters: <T, U extends V>
        let type_parameters = self.parse_type_parameters();

        // Parse parameters: (x: T, y: U)
        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_type_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Parse =>
        self.parse_expected(SyntaxKind::EqualsGreaterThanToken);

        // Parse return type (supports type predicates: param is T)
        let type_annotation = self.parse_return_type();

        let end_pos = self.token_full_start();

        self.arena.add_function_type(
            syntax_kind_ext::FUNCTION_TYPE,
            start_pos,
            end_pos,
            crate::parser::node::FunctionTypeData {
                type_parameters: Some(type_parameters),
                parameters,
                type_annotation,
                is_abstract: false,
            },
        )
    }

    /// Parse constructor type: new () => T or new <T>() => T
    /// Also handles abstract constructor types: abstract new () => T
    pub(crate) fn parse_constructor_type(&mut self, is_abstract: bool) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::NewKeyword);

        // Parse optional type parameters: new <T>() => T
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        // Parse parameters: new (x: T, y: U) => ...
        self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = self.parse_type_parameter_list();
        self.parse_expected(SyntaxKind::CloseParenToken);

        // Parse => and return type
        self.parse_expected(SyntaxKind::EqualsGreaterThanToken);
        let type_annotation = self.parse_return_type();

        let end_pos = self.token_full_start();

        // Use ConstructorType kind - reuse FunctionTypeData since structure is the same
        self.arena.add_function_type(
            syntax_kind_ext::CONSTRUCTOR_TYPE,
            start_pos,
            end_pos,
            crate::parser::node::FunctionTypeData {
                type_parameters,
                parameters,
                type_annotation,
                is_abstract,
            },
        )
    }

    /// Parse type parameter list for function types: (x: T, y: U)
    /// Also handles invalid modifiers like (public x) which TypeScript parses but errors on semantically
    pub(crate) fn parse_type_parameter_list(&mut self) -> NodeList {
        let mut params = Vec::new();

        while !self.is_token(SyntaxKind::CloseParenToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let param_start = self.token_pos();

            // Parse optional modifiers (public/private/protected/readonly)
            // These are syntactically valid but semantically invalid in function types
            let modifiers = self.parse_parameter_modifiers(false);

            // Parse optional ...rest
            let dot_dot_dot = self.parse_optional(SyntaxKind::DotDotDotToken);

            // Parse parameter name - can be identifier, keyword, or binding pattern
            let name = if self.is_token(SyntaxKind::OpenBraceToken) {
                self.parse_object_binding_pattern()
            } else if self.is_token(SyntaxKind::OpenBracketToken) {
                self.parse_array_binding_pattern()
            } else if self.is_identifier_or_keyword() {
                self.parse_identifier_name()
            } else {
                self.parse_identifier()
            };

            // Parse optional ?
            let question_pos = self.token_pos();
            let question = self.parse_optional(SyntaxKind::QuestionToken);

            // TS1047: A rest parameter cannot be optional
            if dot_dot_dot && question {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at(
                    question_pos,
                    1,
                    "A rest parameter cannot be optional.",
                    diagnostic_codes::A_REST_PARAMETER_CANNOT_BE_OPTIONAL,
                );
            }

            // Parse type annotation
            let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
                self.parse_type()
            } else {
                NodeIndex::NONE
            };

            // Parse optional initializer (= expr).
            // Syntactically valid here; the checker reports TS2371 if invalid.
            let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
                self.parse_assignment_expression()
            } else {
                NodeIndex::NONE
            };

            let param_end = self.token_end();

            let param = self.arena.add_parameter(
                syntax_kind_ext::PARAMETER,
                param_start,
                param_end,
                crate::parser::node::ParameterData {
                    modifiers,
                    dot_dot_dot_token: dot_dot_dot,
                    name,
                    question_token: question,
                    type_annotation,
                    initializer,
                },
            );
            params.push(param);

            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        self.make_node_list(params)
    }

    /// Parse a keyword as an identifier (for type keywords like string, number, etc.)
    pub(crate) fn parse_keyword_as_identifier_with_check(
        &mut self,
        check_yield_reserved: bool,
    ) -> NodeIndex {
        // `yield` is reserved in generator contexts and class bodies.
        if self.is_token(SyntaxKind::YieldKeyword) {
            let start_pos = self.token_pos();
            let end_pos = self.token_end();
            if check_yield_reserved && self.in_generator_context() {
                self.report_yield_reserved_word_error();
            }

            // Advance manually because we are returning early in this branch.
            let atom = self.scanner.get_token_atom();
            let text = self.scanner.get_token_value_ref().to_string();
            self.next_token();
            return self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                start_pos,
                end_pos,
                node::IdentifierData {
                    atom,
                    escaped_text: text,
                    original_text: None,
                    type_arguments: None,
                },
            );
        }

        let start_pos = self.token_pos();
        let end_pos = self.token_end();
        // OPTIMIZATION: Capture atom for O(1) comparison
        let atom = self.scanner.get_token_atom();
        let text = self.scanner.get_token_value_ref().to_string();
        self.next_token();

        self.arena.add_identifier(
            SyntaxKind::Identifier as u16,
            start_pos,
            end_pos,
            node::IdentifierData {
                atom,
                escaped_text: text,
                original_text: None,
                type_arguments: None,
            },
        )
    }

    /// Parse a keyword as an identifier (expression context - checks yield reserved).
    pub(crate) fn parse_keyword_as_identifier(&mut self) -> NodeIndex {
        self.parse_keyword_as_identifier_with_check(true)
    }

    /// Parse qualified name rest: given a left name, parse `.Right.Rest` parts
    /// Handles: foo.Bar, A.B.C, etc.
    ///
    /// Returns `(qualified_name, jsdoc_type_arguments)`. When the dot is followed by
    /// `<…>` (JSDoc-legacy `Foo.<T>` syntax), TS8020 is emitted and the type arguments
    /// are bubbled up to the caller so the whole reference can be treated as `Foo<T>`
    /// rather than a qualified-name namespace access (which would cascade into TS2702).
    pub(crate) fn parse_qualified_name_rest(
        &mut self,
        left: NodeIndex,
    ) -> (NodeIndex, Option<NodeList>) {
        let mut current = left;
        let mut jsdoc_type_arguments: Option<NodeList> = None;

        while self.is_token(SyntaxKind::DotToken) {
            let start_pos = if let Some(node) = self.arena.get(current) {
                node.pos
            } else {
                self.token_pos()
            };

            // Capture the span of the dot itself so JSDoc-legacy `Foo.<T>` diagnostics
            // can anchor at the `.` (matching tsc) instead of the following `<`.
            let dot_start = self.token_pos();
            let dot_end = self.token_end();
            self.next_token(); // consume .

            // `Foo.<T>` is JSDoc-legacy syntax for `Foo<T>`.  Emit TS8020 at the `.`
            // (matching tsc's anchor), consume the type arguments, and bubble them up
            // to the caller instead of creating a `QualifiedName` — otherwise the
            // checker sees `Foo.<synthetic>` and emits a cascading TS2702
            // ("only refers to a type, but is being used as a namespace").
            if self.is_token(SyntaxKind::LessThanToken) {
                self.parse_error_at(
                    dot_start,
                    dot_end - dot_start,
                    "JSDoc types can only be used inside documentation comments.",
                    tsz_common::diagnostics::diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS,
                );
                jsdoc_type_arguments = Some(self.parse_type_arguments());
                break;
            }

            // Line break recovery: if there's a line break before the next token and it
            // looks like a new declaration (keyword followed by identifier on same line),
            // emit TS1003 and create a missing identifier instead of consuming the keyword.
            // This matches tsc's parseRightSideOfDot heuristic.
            let right =
                if self.scanner.has_preceding_line_break() && self.is_identifier_or_keyword() {
                    let snapshot = self.scanner.save_state();
                    let saved_token = self.current_token;
                    self.next_token();
                    let next_is_ident_on_same_line =
                        !self.scanner.has_preceding_line_break() && self.is_identifier_or_keyword();
                    self.scanner.restore_state(snapshot);
                    self.current_token = saved_token;
                    if next_is_ident_on_same_line {
                        // Looks like a new declaration — emit TS1003 at the position
                        // right after the dot (matching tsc's reportAtCurrentPosition)
                        self.parse_error_at(
                            dot_end,
                            0,
                            "Identifier expected.",
                            tsz_common::diagnostics::diagnostic_codes::IDENTIFIER_EXPECTED,
                        );
                        self.arena.add_identifier(
                            SyntaxKind::Identifier as u16,
                            dot_end,
                            dot_end,
                            crate::parser::node::IdentifierData {
                                atom: tsz_common::interner::Atom::NONE,
                                escaped_text: String::new(),
                                original_text: None,
                                type_arguments: None,
                            },
                        )
                    } else {
                        self.parse_identifier_name()
                    }
                } else {
                    self.parse_identifier_name()
                };
            let end_pos = self.token_full_start();

            current = self.arena.add_qualified_name(
                syntax_kind_ext::QUALIFIED_NAME,
                start_pos,
                end_pos,
                crate::parser::node::QualifiedNameData {
                    left: current,
                    right,
                },
            );
        }

        (current, jsdoc_type_arguments)
    }

    // =========================================================================
    // Accessors
    // =========================================================================

    /// Get parse diagnostics
    #[must_use]
    pub fn get_diagnostics(&self) -> &[ParseDiagnostic] {
        &self.parse_diagnostics
    }

    /// Get the arena
    #[must_use]
    pub const fn get_arena(&self) -> &NodeArena {
        &self.arena
    }

    /// Consume the parser and return the arena.
    /// This is used for lib files where we need to store the arena in an Arc.
    #[must_use]
    pub fn into_arena(mut self) -> NodeArena {
        // Transfer the interner from the scanner to the arena so atoms can be resolved
        self.arena.set_interner(self.scanner.take_interner());
        self.arena
    }

    /// Get node count
    #[must_use]
    pub const fn get_node_count(&self) -> usize {
        self.arena.len()
    }

    /// Get the source text.
    /// Delegates to the scanner which owns the source text.
    #[must_use]
    pub fn get_source_text(&self) -> &str {
        self.scanner.source_text()
    }

    /// Get the file name
    #[must_use]
    pub fn get_file_name(&self) -> &str {
        &self.file_name
    }
}
