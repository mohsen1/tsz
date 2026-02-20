//! Parser state - function type, type assertion, and JSX parsing.

use super::state::{ParseDiagnostic, ParserState};
use crate::parser::{NodeArena, NodeIndex, NodeList, node, syntax_kind_ext};
use tsz_common::Atom;
use tsz_scanner::SyntaxKind;

impl ParserState {
    /// Check if current token is a keyword that can be used as a property name
    pub(crate) const fn is_property_name_keyword(&self) -> bool {
        matches!(
            self.token(),
            SyntaxKind::TypeKeyword
                | SyntaxKind::GetKeyword
                | SyntaxKind::SetKeyword
                | SyntaxKind::ReadonlyKeyword
                | SyntaxKind::AsyncKeyword
                | SyntaxKind::AwaitKeyword
                | SyntaxKind::NewKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::StaticKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::OverrideKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::ExportKeyword
                | SyntaxKind::DefaultKeyword
                | SyntaxKind::LetKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::VarKeyword
                | SyntaxKind::IfKeyword
                | SyntaxKind::ElseKeyword
                | SyntaxKind::ForKeyword
                | SyntaxKind::WhileKeyword
                | SyntaxKind::DoKeyword
                | SyntaxKind::SwitchKeyword
                | SyntaxKind::CaseKeyword
                | SyntaxKind::BreakKeyword
                | SyntaxKind::ContinueKeyword
                | SyntaxKind::ReturnKeyword
                | SyntaxKind::ThrowKeyword
                | SyntaxKind::TryKeyword
                | SyntaxKind::CatchKeyword
                | SyntaxKind::FinallyKeyword
                | SyntaxKind::ClassKeyword
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::ImportKeyword
                | SyntaxKind::FromKeyword
                | SyntaxKind::AsKeyword
                | SyntaxKind::InKeyword
                | SyntaxKind::OfKeyword
                | SyntaxKind::InstanceOfKeyword
                | SyntaxKind::ThisKeyword
                | SyntaxKind::SuperKeyword
                | SyntaxKind::DeleteKeyword
                | SyntaxKind::VoidKeyword
                | SyntaxKind::TypeOfKeyword
                | SyntaxKind::YieldKeyword
                | SyntaxKind::ConstructorKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::EnumKeyword
                | SyntaxKind::ImplementsKeyword
                | SyntaxKind::ExtendsKeyword
                | SyntaxKind::ModuleKeyword
                | SyntaxKind::NamespaceKeyword
                | SyntaxKind::RequireKeyword
                | SyntaxKind::GlobalKeyword
                | SyntaxKind::TrueKeyword
                | SyntaxKind::FalseKeyword
                | SyntaxKind::NullKeyword
                | SyntaxKind::UndefinedKeyword
                | SyntaxKind::OutKeyword
                | SyntaxKind::SatisfiesKeyword
                | SyntaxKind::AssertKeyword
                | SyntaxKind::AssertsKeyword
                | SyntaxKind::KeyOfKeyword
                | SyntaxKind::UniqueKeyword
                | SyntaxKind::InferKeyword
                | SyntaxKind::IsKeyword
                | SyntaxKind::AnyKeyword
                | SyntaxKind::NeverKeyword
                | SyntaxKind::UnknownKeyword
                | SyntaxKind::BigIntKeyword
                | SyntaxKind::ObjectKeyword
                | SyntaxKind::StringKeyword
                | SyntaxKind::NumberKeyword
                | SyntaxKind::SymbolKeyword
                | SyntaxKind::UsingKeyword
                | SyntaxKind::AccessorKeyword
                | SyntaxKind::DeferKeyword
        )
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
        if matches!(
            self.token(),
            SyntaxKind::PublicKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::ReadonlyKeyword
        ) {
            self.next_token();
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

        let end_pos = self.token_end();

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

        let end_pos = self.token_end();

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

        let end_pos = self.token_end();

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
            let modifiers = self.parse_parameter_modifiers();

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
            let question = self.parse_optional(SyntaxKind::QuestionToken);

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
    pub(crate) fn parse_keyword_as_identifier(&mut self) -> NodeIndex {
        // `yield` is reserved in generator contexts and class bodies.
        if self.is_token(SyntaxKind::YieldKeyword) {
            let start_pos = self.token_pos();
            if self.in_generator_context() {
                use tsz_common::diagnostics::diagnostic_codes;
                let is_class_context = self.in_class_body() || self.in_class_member_name();
                if is_class_context {
                    self.parse_error_at_current_token(
                        "Identifier expected. 'yield' is a reserved word in strict mode. Class definitions are automatically in strict mode.",
                        diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE_CLASS_DEFINITIONS_ARE_AUTO,
                    );
                } else {
                    self.parse_error_at_current_token(
                        "Identifier expected. 'yield' is a reserved word in strict mode.",
                        diagnostic_codes::IDENTIFIER_EXPECTED_IS_A_RESERVED_WORD_IN_STRICT_MODE,
                    );
                }
            }

            // Advance manually because we are returning early in this branch.
            let atom = self.scanner.get_token_atom();
            let text = self.scanner.get_token_value_ref().to_string();
            self.next_token();
            let end_pos = self.token_end();
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
        // OPTIMIZATION: Capture atom for O(1) comparison
        let atom = self.scanner.get_token_atom();
        let text = self.scanner.get_token_value_ref().to_string();
        self.next_token();
        let end_pos = self.token_end();

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

    /// Parse qualified name rest: given a left name, parse `.Right.Rest` parts
    /// Handles: foo.Bar, A.B.C, etc.
    pub(crate) fn parse_qualified_name_rest(&mut self, left: NodeIndex) -> NodeIndex {
        let mut current = left;

        while self.is_token(SyntaxKind::DotToken) {
            let start_pos = if let Some(node) = self.arena.get(current) {
                node.pos
            } else {
                self.token_pos()
            };

            self.next_token(); // consume .
            let right = self.parse_identifier_name();
            let end_pos = self.token_end();

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

        current
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

    // =========================================================================
    // JSX Parsing
    // =========================================================================

    /// Determine if we should parse a type assertion or JSX element.
    /// Type assertions use <Type>expr syntax, JSX uses <Element>.
    pub(crate) fn parse_jsx_element_or_type_assertion(&mut self) -> NodeIndex {
        // In .tsx/.jsx files, all <...> syntax is JSX (use "as Type" for type assertions)
        // In .ts files, we need to distinguish type assertions from JSX
        if self.is_jsx_file() {
            return self.parse_jsx_element_or_self_closing_or_fragment(true);
        }

        // In .ts files (non-JSX), always try to parse as type assertion first.
        // This will produce appropriate errors (e.g., TS1005 " '>' expected") for invalid JSX-like syntax.
        if self.is_ambiguous_generic_type_assertion() {
            self.error_expression_expected();
        }
        self.parse_type_assertion()
    }

    fn is_ambiguous_generic_type_assertion(&mut self) -> bool {
        if !self.is_token(SyntaxKind::LessThanToken) {
            return false;
        }

        let first_end = self.token_end();
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // `<<T>(x) => T` is ambiguous in parser grammar.
        // Treat this as the shift-like form when there is no whitespace between `<<`.
        self.next_token();
        let is_ambiguous = self.is_token(SyntaxKind::LessThanToken)
            && self.token_pos() == first_end
            && self.look_ahead_is_generic_arrow_function();

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_ambiguous
    }

    /// Parse a type assertion: <Type>expression
    pub(crate) fn parse_type_assertion(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::LessThanToken);
        let type_node = self.parse_type();
        self.parse_expected(SyntaxKind::GreaterThanToken);

        // TypeScript doesn't allow bare 'yield' after type assertion
        // Unlike 'await', 'yield' is not allowed as a simple unary expression in this context
        // Example: <number> yield 0 → TS1109 "Expression expected"
        // But:     <number> (yield 0) → valid (parens make it a primary expression)
        if self.is_token(SyntaxKind::YieldKeyword) {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Expression expected.",
                diagnostic_codes::EXPRESSION_EXPECTED,
            );
        }

        let expression = self.parse_unary_expression();
        if expression.is_none() {
            self.error_expression_expected();
        }
        let end_pos = self.token_end();

        self.arena.add_type_assertion(
            syntax_kind_ext::TYPE_ASSERTION,
            start_pos,
            end_pos,
            node::TypeAssertionData {
                expression,
                type_node,
            },
        )
    }

    /// Parse a JSX element, self-closing element, or fragment.
    /// Called when we see `<` in an expression context.
    pub(crate) fn parse_jsx_element_or_self_closing_or_fragment(
        &mut self,
        in_expression_context: bool,
    ) -> NodeIndex {
        self.parse_jsx_element_or_self_closing_or_fragment_inner(in_expression_context, None)
    }

    /// Internal JSX parse with parent tag context for mismatch detection.
    /// `currently_opened_tag` is the parent element's opening tag name (if any),
    /// used to distinguish TS17008 (closer matches parent) from TS17002 (wrong closer).
    fn parse_jsx_element_or_self_closing_or_fragment_inner(
        &mut self,
        in_expression_context: bool,
        currently_opened_tag: Option<NodeIndex>,
    ) -> NodeIndex {
        let start_pos = self.token_pos();
        let opening = self.parse_jsx_opening_or_self_closing_or_fragment(in_expression_context);

        // Check what type of opening element we got
        let kind = self.arena.get(opening).map_or(0, |n| n.kind);

        if kind == syntax_kind_ext::JSX_OPENING_ELEMENT {
            // Get the tag name from the opening element for error reporting
            let opening_tag_name = self
                .arena
                .get(opening)
                .and_then(|n| self.arena.get_jsx_opening(n))
                .map(|data| data.tag_name);

            // Parse children, passing our opening tag name for parent-match detection
            let children = self.parse_jsx_children(opening_tag_name);

            // Check if last child is a JsxElement whose closing tag "stole" our closer
            let last_child_stole_closer =
                self.check_last_child_stole_closer(&children, opening_tag_name);

            let closing =
                if let Some((child_opening_tag, child_closing_idx)) = last_child_stole_closer {
                    // TS17008: The child element was never properly closed
                    // (dedup at same position handles double emission from inner + outer)
                    self.emit_jsx_unclosed_tag_error(child_opening_tag);
                    // Reuse the child's closing element as our own
                    child_closing_idx
                } else {
                    // Parse our own closing element
                    let closing = self.parse_jsx_closing_element();
                    // Check for tag name mismatch
                    if let Some(open_tag) = opening_tag_name
                        && let Some(close_node) = self.arena.get(closing)
                        && let Some(close_data) = self.arena.get_jsx_closing(close_node)
                    {
                        let close_tag = close_data.tag_name;
                        let open_text = self.get_jsx_tag_name_text(open_tag);
                        let close_text = self.get_jsx_tag_name_text(close_tag);
                        if open_text != close_text {
                            // Check if closing matches parent's tag (tsc pattern)
                            let matches_parent = currently_opened_tag
                                .is_some_and(|pt| self.get_jsx_tag_name_text(pt) == close_text);
                            if matches_parent {
                                // TS17008: Our tag is unclosed (closer belongs to parent)
                                self.emit_jsx_unclosed_tag_error(open_tag);
                            } else {
                                // TS17002: Wrong closing tag
                                self.emit_jsx_mismatched_closing_tag_error(open_tag, close_tag);
                            }
                        }
                    }
                    closing
                };

            let end_pos = self.token_end();

            self.arena.add_jsx_element(
                syntax_kind_ext::JSX_ELEMENT,
                start_pos,
                end_pos,
                crate::parser::node::JsxElementData {
                    opening_element: opening,
                    children,
                    closing_element: closing,
                },
            )
        } else if kind == syntax_kind_ext::JSX_OPENING_FRAGMENT {
            // Parse children and closing fragment
            let children = self.parse_jsx_children(None);
            let closing = self.parse_jsx_closing_fragment();
            let end_pos = self.token_end();

            self.arena.add_jsx_fragment(
                syntax_kind_ext::JSX_FRAGMENT,
                start_pos,
                end_pos,
                crate::parser::node::JsxFragmentData {
                    opening_fragment: opening,
                    children,
                    closing_fragment: closing,
                },
            )
        } else {
            // Self-closing element, already complete
            opening
        }
    }

    /// Parse JSX opening element, self-closing element, or opening fragment.
    pub(crate) fn parse_jsx_opening_or_self_closing_or_fragment(
        &mut self,
        _in_expression_context: bool,
    ) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::LessThanToken);

        // Check for fragment: <>
        if self.is_token(SyntaxKind::GreaterThanToken) {
            let end_pos = self.token_end();
            self.next_token(); // consume >
            return self
                .arena
                .add_token(syntax_kind_ext::JSX_OPENING_FRAGMENT, start_pos, end_pos);
        }

        // Parse tag name
        let tag_name = self.parse_jsx_element_name();

        // Parse optional type arguments
        let type_arguments = self
            .is_less_than_or_compound()
            .then(|| self.parse_type_arguments());

        // Parse attributes
        let attributes = self.parse_jsx_attributes();

        // Check for self-closing: />
        if self.is_token(SyntaxKind::SlashToken) {
            self.next_token(); // consume /
            let end_pos = self.token_end();
            self.parse_expected(SyntaxKind::GreaterThanToken);
            return self.arena.add_jsx_opening(
                syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT,
                start_pos,
                end_pos,
                crate::parser::node::JsxOpeningData {
                    tag_name,
                    type_arguments,
                    attributes,
                },
            );
        }

        // Opening element: consume > and continue parsing children
        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::GreaterThanToken);
        self.arena.add_jsx_opening(
            syntax_kind_ext::JSX_OPENING_ELEMENT,
            start_pos,
            end_pos,
            crate::parser::node::JsxOpeningData {
                tag_name,
                type_arguments,
                attributes,
            },
        )
    }

    /// Parse JSX element name (identifier, this, namespaced, or property access).
    pub(crate) fn parse_jsx_element_name(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Error recovery: if the current token can't start a JSX element name,
        // return a missing identifier to avoid crashes
        if !self.is_token(SyntaxKind::Identifier)
            && !self.is_token(SyntaxKind::ThisKeyword)
            && !self.is_identifier_or_keyword()
        {
            self.error_identifier_expected();
            // Create a missing identifier node
            let end_pos = self.token_end();
            return self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                start_pos,
                end_pos,
                node::IdentifierData {
                    atom: Atom::NONE,
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
                },
            );
        }

        // Parse the initial name (identifier or this)
        let mut expr = if self.is_token(SyntaxKind::ThisKeyword) {
            let pos = self.token_pos();
            self.next_token();
            let end_pos = self.token_end();
            self.arena
                .add_token(SyntaxKind::ThisKeyword as u16, pos, end_pos)
        } else {
            // scan_jsx_identifier handles both identifiers and keywords,
            // extending the token to include hyphens (e.g., public-foo)
            self.scanner.scan_jsx_identifier();
            let name = self.parse_identifier_name();

            // Check for namespaced name (a:b)
            if self.is_token(SyntaxKind::ColonToken) {
                self.next_token(); // consume :
                let local_name = self.parse_identifier_name();
                let end_pos = self.token_end();
                return self.arena.add_jsx_namespaced_name(
                    syntax_kind_ext::JSX_NAMESPACED_NAME,
                    start_pos,
                    end_pos,
                    crate::parser::node::JsxNamespacedNameData {
                        namespace: name,
                        name: local_name,
                    },
                );
            }

            name
        };

        // Parse property access chain (Foo.Bar.Baz)
        while self.is_token(SyntaxKind::DotToken) {
            self.next_token(); // consume .
            let name = self.parse_identifier();
            let end_pos = self.token_end();
            expr = self.arena.add_access_expr(
                syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION,
                start_pos,
                end_pos,
                crate::parser::node::AccessExprData {
                    expression: expr,
                    name_or_argument: name,
                    question_dot_token: false,
                },
            );
        }

        expr
    }

    /// Parse JSX attributes list.
    pub(crate) fn parse_jsx_attributes(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut properties = Vec::new();

        while !self.is_token(SyntaxKind::GreaterThanToken)
            && !self.is_token(SyntaxKind::SlashToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            if self.is_token(SyntaxKind::OpenBraceToken) {
                // Spread attribute: {...props}
                properties.push(self.parse_jsx_spread_attribute());
            } else {
                // Regular attribute: name="value" or name={expr} or just name
                properties.push(self.parse_jsx_attribute());
            }
        }

        let end_pos = self.token_end();
        self.arena.add_jsx_attributes(
            syntax_kind_ext::JSX_ATTRIBUTES,
            start_pos,
            end_pos,
            crate::parser::node::JsxAttributesData {
                properties: self.make_node_list(properties),
            },
        )
    }

    /// Parse a single JSX attribute.
    pub(crate) fn parse_jsx_attribute(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Error recovery: if the current token can't start an attribute name,
        // report error and skip to next attribute or end of attributes
        if !self.is_token(SyntaxKind::Identifier) && !self.is_identifier_or_keyword() {
            self.error_identifier_expected();
            // Skip the invalid token to prevent infinite loops
            self.next_token();
            // Return a dummy attribute with missing name
            let end_pos = self.token_end();
            return self.arena.add_jsx_attribute(
                syntax_kind_ext::JSX_ATTRIBUTE,
                start_pos,
                end_pos,
                crate::parser::node::JsxAttributeData {
                    name: NodeIndex::NONE,
                    initializer: NodeIndex::NONE,
                },
            );
        }

        let name = self.parse_jsx_attribute_name();

        // Check for value: = followed by string, expression, or nested JSX
        let initializer = if self.parse_optional(SyntaxKind::EqualsToken) {
            // Rescan the next token using the JSX attribute value scanner.
            // JSX attribute strings allow literal newlines (unlike regular JS strings),
            // so we must rescan in JSX mode to handle multiline attribute values.
            self.current_token = self.scanner.re_scan_jsx_attribute_value();
            if self.is_token(SyntaxKind::StringLiteral) {
                self.parse_string_literal()
            } else if self.is_token(SyntaxKind::OpenBraceToken) {
                self.parse_jsx_expression()
            } else if self.is_token(SyntaxKind::LessThanToken) {
                self.parse_jsx_element_or_self_closing_or_fragment(true)
            } else {
                // TS1145: '{' or JSX element expected.
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token(
                    "'{' or JSX element expected.",
                    diagnostic_codes::OR_JSX_ELEMENT_EXPECTED,
                );
                NodeIndex::NONE
            }
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_end();
        self.arena.add_jsx_attribute(
            syntax_kind_ext::JSX_ATTRIBUTE,
            start_pos,
            end_pos,
            crate::parser::node::JsxAttributeData { name, initializer },
        )
    }

    /// Parse JSX attribute name (possibly namespaced).
    /// JSX attribute names can be keywords like "extends", "class", etc.
    pub(crate) fn parse_jsx_attribute_name(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // scan_jsx_identifier handles both identifiers and keywords,
        // extending the token to include hyphens (e.g., class-id, data-testid)
        self.scanner.scan_jsx_identifier();
        // Use parse_identifier_name to allow keywords as attribute names
        let name = self.parse_identifier_name();

        // Check for namespaced name (a:b)
        if self.is_token(SyntaxKind::ColonToken) {
            self.next_token(); // consume :
            // Also allow keywords for the local part of namespaced names
            let local_name = self.parse_identifier_name();
            let end_pos = self.token_end();
            return self.arena.add_jsx_namespaced_name(
                syntax_kind_ext::JSX_NAMESPACED_NAME,
                start_pos,
                end_pos,
                crate::parser::node::JsxNamespacedNameData {
                    namespace: name,
                    name: local_name,
                },
            );
        }

        name
    }

    /// Parse a JSX spread attribute: {...props}
    pub(crate) fn parse_jsx_spread_attribute(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);
        self.parse_expected(SyntaxKind::DotDotDotToken);
        let expression = self.parse_expression();
        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_jsx_spread_attribute(
            syntax_kind_ext::JSX_SPREAD_ATTRIBUTE,
            start_pos,
            end_pos,
            crate::parser::node::JsxSpreadAttributeData { expression },
        )
    }

    /// Parse a JSX expression: {expr} or {...expr}
    pub(crate) fn parse_jsx_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.parse_expected(SyntaxKind::OpenBraceToken);

        // Check for spread: {...}
        let dot_dot_dot_token = self.parse_optional(SyntaxKind::DotDotDotToken);

        // Check for empty expression: {}
        let expression = if self.is_token(SyntaxKind::CloseBraceToken) {
            NodeIndex::NONE
        } else {
            self.parse_expression()
        };

        self.parse_expected(SyntaxKind::CloseBraceToken);

        let end_pos = self.token_end();
        self.arena.add_jsx_expression(
            syntax_kind_ext::JSX_EXPRESSION,
            start_pos,
            end_pos,
            crate::parser::node::JsxExpressionData {
                dot_dot_dot_token,
                expression,
            },
        )
    }

    /// Parse JSX children (elements, text, expressions).
    /// `opening_tag_name` is the `NodeIndex` of the opening element's tag name,
    /// used to emit TS17008 if we hit EOF without a corresponding closing tag.
    pub(crate) fn parse_jsx_children(&mut self, opening_tag_name: Option<NodeIndex>) -> NodeList {
        let mut children = Vec::new();

        loop {
            // Rescan in JSX context to get proper JsxText tokens and LessThanSlashToken
            // This is necessary because after parsing expressions or nested elements,
            // the scanner may not be in JSX mode.
            self.current_token = self.scanner.re_scan_jsx_token(true);

            match self.current_token {
                SyntaxKind::LessThanSlashToken => {
                    // Closing tag/fragment - stop parsing children
                    break;
                }
                SyntaxKind::LessThanToken => {
                    // Nested JSX element — pass our opening tag as parent context
                    let child = self.parse_jsx_element_or_self_closing_or_fragment_inner(
                        false,
                        opening_tag_name,
                    );
                    children.push(child);
                    // Check if this child stole our closing tag (tsc pattern):
                    // If the child is a JsxElement with mismatched tags and its
                    // closing tag matches our opening tag, break early.
                    if let Some(parent_tag) = opening_tag_name
                        && self.jsx_child_stole_closer(child, parent_tag)
                    {
                        break;
                    }
                }
                SyntaxKind::OpenBraceToken => {
                    // JSX expression: {expr}
                    children.push(self.parse_jsx_expression());
                }
                SyntaxKind::JsxText => {
                    // Text node
                    children.push(self.parse_jsx_text());
                }
                SyntaxKind::EndOfFileToken => {
                    // TS17008: JSX element has no corresponding closing tag
                    if let Some(tag_name_idx) = opening_tag_name {
                        self.emit_jsx_unclosed_tag_error(tag_name_idx);
                    }
                    break;
                }
                _ => {
                    // Unknown token in JSX children - stop
                    break;
                }
            }
        }

        self.make_node_list(children)
    }

    /// Parse JSX text content.
    pub(crate) fn parse_jsx_text(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let text = self.scanner.get_token_value_ref().to_string();
        let end_pos = self.token_end();
        self.next_token();

        self.arena.add_jsx_text(
            SyntaxKind::JsxText as u16,
            start_pos,
            end_pos,
            crate::parser::node::JsxTextData {
                text,
                contains_only_trivia_white_spaces: false,
            },
        )
    }

    /// Get the text of a JSX tag name node from source text.
    /// Works for identifiers, property access (Foo.Bar), and namespaced names (a:b).
    /// For property access nodes, finds the tight end by using the last name child.
    fn get_jsx_tag_name_text(&self, tag_name: NodeIndex) -> String {
        if let Some(node) = self.arena.get(tag_name) {
            let source = self.scanner.source_text();
            let start = node.pos as usize;
            // For property access expressions, the node.end may be too broad
            // (includes trailing token position). Find the tight end by looking
            // at the name child of the property access chain.
            let end = self.get_jsx_tag_name_end(tag_name) as usize;
            if start < end && end <= source.len() {
                return source[start..end].to_string();
            }
            // Fallback to node boundaries
            let end = node.end as usize;
            if start < end && end <= source.len() {
                return source[start..end].to_string();
            }
        }
        String::new()
    }

    /// Get the tight end position of a JSX tag name, following property access chains.
    fn get_jsx_tag_name_end(&self, tag_name: NodeIndex) -> u32 {
        if let Some(node) = self.arena.get(tag_name) {
            // For property access expressions (Foo.Bar), use the name child's end
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.arena.get_access_expr(node)
            {
                return self.get_jsx_tag_name_end(access.name_or_argument);
            }
            node.end
        } else {
            0
        }
    }

    /// Emit TS17008: JSX element '{0}' has no corresponding closing tag.
    /// Points at the opening tag name span (tight end for property access chains).
    fn emit_jsx_unclosed_tag_error(&mut self, tag_name: NodeIndex) {
        use tsz_common::diagnostics::diagnostic_codes;
        let tag_text = self.get_jsx_tag_name_text(tag_name);
        if let Some(node) = self.arena.get(tag_name) {
            let start = node.pos;
            let end = self.get_jsx_tag_name_end(tag_name);
            self.parse_error_at(
                start,
                end - start,
                &format!("JSX element '{tag_text}' has no corresponding closing tag."),
                diagnostic_codes::JSX_ELEMENT_HAS_NO_CORRESPONDING_CLOSING_TAG,
            );
        }
    }

    /// Emit TS17002: Expected corresponding JSX closing tag for '{0}'.
    /// Points at the closing tag name span (where the mismatch is).
    fn emit_jsx_mismatched_closing_tag_error(
        &mut self,
        open_tag_name: NodeIndex,
        close_tag_name: NodeIndex,
    ) {
        use tsz_common::diagnostics::diagnostic_codes;
        let open_text = self.get_jsx_tag_name_text(open_tag_name);
        if let Some(close_node) = self.arena.get(close_tag_name) {
            let start = close_node.pos;
            let length = close_node.end - close_node.pos;
            self.parse_error_at(
                start,
                length,
                &format!("Expected corresponding JSX closing tag for '{open_text}'."),
                diagnostic_codes::EXPECTED_CORRESPONDING_JSX_CLOSING_TAG_FOR,
            );
        }
    }

    /// Check if a child `JsxElement` has mismatched tags where its closing tag
    /// matches the given parent opening tag name. This implements the tsc pattern
    /// where a child element "steals" the parent's closing tag.
    fn jsx_child_stole_closer(&self, child: NodeIndex, parent_tag_name: NodeIndex) -> bool {
        let child_node = match self.arena.get(child) {
            Some(n) if n.kind == syntax_kind_ext::JSX_ELEMENT => n,
            _ => return false,
        };
        let elem_data = match self.arena.get_jsx_element(child_node) {
            Some(d) => d.clone(),
            None => return false,
        };
        // Get the child's opening and closing tag names
        let child_open_tag = self
            .arena
            .get(elem_data.opening_element)
            .and_then(|n| self.arena.get_jsx_opening(n))
            .map(|d| d.tag_name);
        let child_close_tag = self
            .arena
            .get(elem_data.closing_element)
            .and_then(|n| self.arena.get_jsx_closing(n))
            .map(|d| d.tag_name);
        match (child_open_tag, child_close_tag) {
            (Some(open), Some(close)) => {
                let open_text = self.get_jsx_tag_name_text(open);
                let close_text = self.get_jsx_tag_name_text(close);
                let parent_text = self.get_jsx_tag_name_text(parent_tag_name);
                // Child has mismatched tags AND its closing matches our opening
                open_text != close_text && close_text == parent_text
            }
            _ => false,
        }
    }

    /// Check if the last child in a `NodeList` stole the parent's closing tag.
    /// Returns (`child_opening_tag_name`, `child_closing_element`) if so.
    fn check_last_child_stole_closer(
        &self,
        children: &NodeList,
        parent_tag_name: Option<NodeIndex>,
    ) -> Option<(NodeIndex, NodeIndex)> {
        let parent_tag = parent_tag_name?;
        let last_child = *children.nodes.last()?;
        let child_node = self.arena.get(last_child)?;
        if child_node.kind != syntax_kind_ext::JSX_ELEMENT {
            return None;
        }
        let elem_data = self.arena.get_jsx_element(child_node)?.clone();
        let child_open_tag = self
            .arena
            .get(elem_data.opening_element)
            .and_then(|n| self.arena.get_jsx_opening(n))
            .map(|d| d.tag_name)?;
        let child_close_tag = self
            .arena
            .get(elem_data.closing_element)
            .and_then(|n| self.arena.get_jsx_closing(n))
            .map(|d| d.tag_name)?;
        let open_text = self.get_jsx_tag_name_text(child_open_tag);
        let close_text = self.get_jsx_tag_name_text(child_close_tag);
        let parent_text = self.get_jsx_tag_name_text(parent_tag);
        if open_text != close_text && close_text == parent_text {
            Some((child_open_tag, elem_data.closing_element))
        } else {
            None
        }
    }

    /// Parse a JSX closing element: </Foo>
    pub(crate) fn parse_jsx_closing_element(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // In JSX mode, </ is scanned as a single LessThanSlashToken
        self.parse_expected(SyntaxKind::LessThanSlashToken);
        let tag_name = self.parse_jsx_element_name();
        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::GreaterThanToken);
        self.arena.add_jsx_closing(
            syntax_kind_ext::JSX_CLOSING_ELEMENT,
            start_pos,
            end_pos,
            crate::parser::node::JsxClosingData { tag_name },
        )
    }

    /// Parse a JSX closing fragment: </>
    pub(crate) fn parse_jsx_closing_fragment(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        // In JSX mode, </ is scanned as a single LessThanSlashToken
        self.parse_expected(SyntaxKind::LessThanSlashToken);
        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::GreaterThanToken);
        self.arena
            .add_token(syntax_kind_ext::JSX_CLOSING_FRAGMENT, start_pos, end_pos)
    }

    /// Consume the parser and return its parts.
    /// This is useful for taking ownership of the arena after parsing.
    #[must_use]
    pub fn into_parts(mut self) -> (NodeArena, Vec<ParseDiagnostic>) {
        // Transfer the interner from the scanner to the arena so atoms can be resolved
        self.arena.set_interner(self.scanner.take_interner());
        (self.arena, self.parse_diagnostics)
    }
}
