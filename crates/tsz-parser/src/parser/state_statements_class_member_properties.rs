//! Parser state - class member property parsing and recovery helpers.

use super::state::{
    CONTEXT_FLAG_ASYNC, CONTEXT_FLAG_GENERATOR, CONTEXT_FLAG_STATIC_BLOCK, ParserState,
};
use super::state_statements_class_members::ClassMemberModifierSet;
use crate::parser::{NodeIndex, syntax_kind_ext};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_scanner::SyntaxKind;

impl ParserState {
    /// Construct the body of a property class member: parse type annotation,
    /// optional/definite tokens, and initializer expression.
    pub(crate) fn construct_class_member_property(
        &mut self,
        start_pos: u32,
        mods: ClassMemberModifierSet,
        name: NodeIndex,
        question_token: bool,
        exclamation_token: bool,
        late_property_name_decorator: bool,
        method_saved_flags: u32,
    ) -> NodeIndex {
        use tsz_common::diagnostics::diagnostic_codes;

        // Property - parse optional type and initializer
        self.context_flags = method_saved_flags;
        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        let init_saved_flags = self.context_flags;
        self.context_flags &=
            !(CONTEXT_FLAG_ASYNC | CONTEXT_FLAG_GENERATOR | CONTEXT_FLAG_STATIC_BLOCK);
        self.context_flags |= crate::parser::state::CONTEXT_FLAG_CLASS_FIELD_INITIALIZER;

        let has_equals_initializer = self.parse_optional(SyntaxKind::EqualsToken);
        let initializer = if has_equals_initializer {
            self.parse_assignment_expression()
        } else if type_annotation != NodeIndex::NONE
            && !self.is_token(SyntaxKind::SemicolonToken)
            && !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
            && (((self.is_token(SyntaxKind::StringLiteral)
                || self.is_token(SyntaxKind::NumericLiteral)
                || self.is_token(SyntaxKind::BigIntLiteral))
                // A literal that looks like the next member's property name (followed by
                // `:` or `?`) starts the next member, not an initializer for this one.
                && !self.look_ahead_is_next_class_member_property_name())
                || self.is_token(SyntaxKind::DotToken))
        {
            self.parse_error_at_current_token(
                "Expected '=' for property initializer.",
                diagnostic_codes::EXPECTED_FOR_PROPERTY_INITIALIZER,
            );
            self.parse_assignment_expression()
        } else {
            NodeIndex::NONE
        };

        self.context_flags = init_saved_flags;

        if has_equals_initializer
            && self.is_token(SyntaxKind::CommaToken)
            && !self.scanner.has_preceding_line_break()
        {
            self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
        }

        // When a property with an initializer is followed by a line break and
        // a continuation token (`[`, `(`, `.`), report a missing semicolon.
        // Exception: if the property has a computed name, no type annotation,
        // and the next line starts with `[`, treat `[` as a new computed
        // property (ASI), not element access on the initializer.
        // tsc only treats `[` as a continuation when there IS a type
        // annotation (e.g., `[e]: number = 0\n[e2]` → TS1005), but not
        // when there's only an initializer (e.g., `[e] = "A"\n[e2] = "B"`).
        let is_computed_name = self
            .arena
            .get(name)
            .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME);
        if initializer != NodeIndex::NONE
            && !self.is_token(SyntaxKind::SemicolonToken)
            && self.scanner.has_preceding_line_break()
            && self.class_member_initializer_continues_on_next_line()
            && !(is_computed_name
                && type_annotation == NodeIndex::NONE
                && self.is_token(SyntaxKind::OpenBracketToken)
                && !self.look_ahead_is_invalid_class_member_method_like_continuation())
        {
            self.report_missing_semicolon_after_class_field_initializer();
            self.recover_invalid_class_member_initializer_continuation();
        }

        // TS1442: when a property has a type annotation but no initializer
        // and the next token cannot end the declaration (not `;`, `}`, EOF,
        // and no preceding line break), emit "Expected '=' for property
        // initializer." — matching tsc's parseSemicolonAfterPropertyName.
        if type_annotation != NodeIndex::NONE
            && initializer == NodeIndex::NONE
            && !late_property_name_decorator
            && !self.can_parse_semicolon()
        {
            let (message, code) = if self.is_token(SyntaxKind::OpenParenToken) {
                (
                    "Cannot start a function call in a type annotation.",
                    diagnostic_codes::CANNOT_START_A_FUNCTION_CALL_IN_A_TYPE_ANNOTATION,
                )
            } else {
                (
                    "Expected '=' for property initializer.",
                    diagnostic_codes::EXPECTED_FOR_PROPERTY_INITIALIZER,
                )
            };
            self.parse_error_at_current_token(message, code);
        }

        // Match tsc's parseSemicolonAfterPropertyName: when a property has
        // no type annotation and no initializer and no semicolon follows,
        // use keyword-aware semicolon error (TS1434/TS1435) instead of
        // the generic "';' expected". This produces "Unexpected keyword or
        // identifier" for bare identifiers like `NoMove` in class bodies.
        if !mods.has_var_let
            && type_annotation == NodeIndex::NONE
            && initializer == NodeIndex::NONE
            && !late_property_name_decorator
            && !self.is_token(SyntaxKind::SemicolonToken)
            && !self.can_parse_semicolon()
        {
            let name_is_identifier = self
                .arena
                .get(name)
                .is_some_and(|node| node.kind == SyntaxKind::Identifier as u16);
            if !name_is_identifier
                && matches!(
                    self.token(),
                    SyntaxKind::CommaToken
                        | SyntaxKind::CloseBracketToken
                        | SyntaxKind::CloseParenToken
                )
            {
                self.parse_error_at_current_token("';' expected.", diagnostic_codes::EXPECTED);
            } else {
                self.parse_error_for_missing_semicolon_after(name);
            }
        }

        let end_pos = self.token_end();
        self.arena.add_property_decl(
            syntax_kind_ext::PROPERTY_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::node::PropertyDeclData {
                modifiers: mods.modifiers,
                name,
                question_token,
                exclamation_token,
                type_annotation,
                initializer,
            },
        )
    }

    pub(crate) fn look_ahead_is_class_body_variable_statement(&mut self) -> bool {
        if !matches!(
            self.token(),
            SyntaxKind::VarKeyword | SyntaxKind::LetKeyword
        ) {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token();
        let is_match = if self.scanner.has_preceding_line_break() {
            false
        } else if matches!(
            self.token(),
            SyntaxKind::OpenBraceToken | SyntaxKind::OpenBracketToken
        ) {
            true
        } else if self.is_identifier_or_keyword() || self.is_token(SyntaxKind::PrivateIdentifier) {
            self.next_token();
            !self.scanner.has_preceding_line_break() && !self.is_token(SyntaxKind::OpenParenToken)
        } else {
            false
        };

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_match
    }

    pub(crate) fn look_ahead_is_class_body_function_statement(&mut self) -> bool {
        if !self.is_token(SyntaxKind::FunctionKeyword) {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let is_match = self.is_identifier_or_keyword() && !self.scanner.has_preceding_line_break();
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_match
    }

    pub(crate) fn look_ahead_is_property_name_same_line(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token();
        let is_match = self.is_property_name() && !self.scanner.has_preceding_line_break();

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_match
    }

    pub(crate) fn recover_invalid_class_body_variable_statement(&mut self) {
        self.parse_error_at_current_token(
            "Unexpected token. A constructor, method, accessor, or property was expected.",
            diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
        );

        while !self.is_token(SyntaxKind::SemicolonToken)
            && !self.is_token(SyntaxKind::CloseBraceToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            self.next_token();
        }

        if self.is_token(SyntaxKind::SemicolonToken) {
            self.next_token();
        }

        if self.is_token(SyntaxKind::CloseBraceToken) {
            self.parse_error_at_current_token(
                "Declaration or statement expected.",
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            );
        }
    }

    pub(crate) fn recover_invalid_class_body_function_statement(&mut self) {
        self.parse_error_at_current_token(
            "Unexpected token. A constructor, method, accessor, or property was expected.",
            diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
        );

        // Skip the invalid `function ...` statement body on the same line.
        // Unlike module-like recovery, don't emit an intermediate TS1005
        // ("';' expected.") here — tsc reports only TS1068 + trailing TS1128.
        self.next_token();
        while !self.is_token(SyntaxKind::EndOfFileToken) && !self.scanner.has_preceding_line_break()
        {
            self.next_token();
        }

        // If we're at the class closing brace, report the follow-up statement-level
        // recovery diagnostic.
        if self.is_token(SyntaxKind::CloseBraceToken) {
            self.parse_error_at_current_token(
                "Declaration or statement expected.",
                diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED,
            );
        }
    }

    pub(crate) fn class_member_is_recovered_invalid_if_method(&self, member: NodeIndex) -> bool {
        let text = self.get_source_text();
        let Some(member_node) = self.arena.get(member) else {
            return false;
        };
        let Some(method) = self.arena.get_method_decl(member_node) else {
            return false;
        };
        let Some(name_node) = self.arena.get(method.name) else {
            return false;
        };
        let is_if_name = name_node.kind == SyntaxKind::IfKeyword as u16
            || self
                .arena
                .get_identifier(name_node)
                .is_some_and(|ident| ident.escaped_text == "if");
        if !is_if_name {
            return false;
        }
        if let Some(body_node) = self.arena.get(method.body) {
            if body_node.pos == body_node.end && self.current_token_is_comparison_tail() {
                return true;
            }
            let start = (name_node.end as usize).min(text.len());
            let end = (body_node.pos as usize).min(text.len());
            return start < end && text[start..end].contains("!=");
        }

        self.current_token_is_comparison_tail()
    }

    const fn class_member_initializer_continues_on_next_line(&self) -> bool {
        matches!(
            self.token(),
            SyntaxKind::OpenParenToken
                | SyntaxKind::OpenBracketToken
                | SyntaxKind::DotToken
                | SyntaxKind::QuestionDotToken
                | SyntaxKind::NoSubstitutionTemplateLiteral
                | SyntaxKind::TemplateHead
        )
    }

    fn report_missing_semicolon_after_class_field_initializer(&mut self) {
        if let Some((pos, len)) = self.class_field_initializer_continuation_anchor() {
            self.parse_error_at(pos, len, "';' expected.", diagnostic_codes::EXPECTED);
        } else {
            self.error_token_expected(";");
        }
    }

    fn class_field_initializer_continuation_anchor(&mut self) -> Option<(u32, u32)> {
        let (open, close) = match self.token() {
            SyntaxKind::OpenBracketToken => {
                (SyntaxKind::OpenBracketToken, SyntaxKind::CloseBracketToken)
            }
            SyntaxKind::OpenParenToken => (SyntaxKind::OpenParenToken, SyntaxKind::CloseParenToken),
            _ => return None,
        };

        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        let mut depth = 0u32;
        let mut anchor = None;

        while !self.is_token(SyntaxKind::EndOfFileToken) {
            if self.is_token(open) {
                depth += 1;
            } else if self.is_token(close) {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let pos = self.token_end();
                    anchor = Some((pos, 1));

                    if open == SyntaxKind::OpenBracketToken {
                        self.next_token();
                        if self.is_token(SyntaxKind::OpenParenToken) {
                            let mut paren_depth = 0u32;
                            while !self.is_token(SyntaxKind::EndOfFileToken) {
                                if self.is_token(SyntaxKind::OpenParenToken) {
                                    paren_depth += 1;
                                } else if self.is_token(SyntaxKind::CloseParenToken) {
                                    paren_depth = paren_depth.saturating_sub(1);
                                    if paren_depth == 0 {
                                        self.next_token();
                                        if self.is_token(SyntaxKind::OpenBraceToken) {
                                            anchor = Some((self.token_pos(), 1));
                                        }
                                        break;
                                    }
                                }
                                self.next_token();
                            }
                        }
                    }
                    break;
                }
            }
            self.next_token();
        }

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        anchor
    }

    fn recover_invalid_class_member_initializer_continuation(&mut self) {
        if !self.look_ahead_is_invalid_class_member_method_like_continuation() {
            return;
        }

        if self.is_token(SyntaxKind::OpenBracketToken) {
            let mut bracket_depth = 0u32;
            while !self.is_token(SyntaxKind::EndOfFileToken) {
                if self.is_token(SyntaxKind::OpenBracketToken) {
                    bracket_depth += 1;
                } else if self.is_token(SyntaxKind::CloseBracketToken) {
                    bracket_depth = bracket_depth.saturating_sub(1);
                    if bracket_depth == 0 {
                        self.next_token();
                        break;
                    }
                }
                self.next_token();
            }
        }

        if !self.is_token(SyntaxKind::OpenParenToken) {
            return;
        }
        self.suppress_next_missing_class_close_brace_error_once = true;

        let mut paren_depth = 0u32;
        while !self.is_token(SyntaxKind::EndOfFileToken) {
            if self.is_token(SyntaxKind::OpenParenToken) {
                paren_depth += 1;
            } else if self.is_token(SyntaxKind::CloseParenToken) {
                paren_depth = paren_depth.saturating_sub(1);
                if paren_depth == 0 {
                    self.next_token();
                    break;
                }
            }
            self.next_token();
        }
    }

    fn look_ahead_is_invalid_class_member_method_like_continuation(&mut self) -> bool {
        if !self.is_token(SyntaxKind::OpenBracketToken)
            && !self.is_token(SyntaxKind::OpenParenToken)
        {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        let mut is_match = false;

        if self.is_token(SyntaxKind::OpenBracketToken) {
            let mut bracket_depth = 0u32;
            while !self.is_token(SyntaxKind::EndOfFileToken) {
                if self.is_token(SyntaxKind::OpenBracketToken) {
                    bracket_depth += 1;
                } else if self.is_token(SyntaxKind::CloseBracketToken) {
                    bracket_depth = bracket_depth.saturating_sub(1);
                    if bracket_depth == 0 {
                        self.next_token();
                        break;
                    }
                }
                self.next_token();
            }
        }

        if self.is_token(SyntaxKind::OpenParenToken) {
            let mut paren_depth = 0u32;
            while !self.is_token(SyntaxKind::EndOfFileToken) {
                if self.is_token(SyntaxKind::OpenParenToken) {
                    paren_depth += 1;
                } else if self.is_token(SyntaxKind::CloseParenToken) {
                    paren_depth = paren_depth.saturating_sub(1);
                    if paren_depth == 0 {
                        self.next_token();
                        is_match = self.is_token(SyntaxKind::OpenBraceToken);
                        break;
                    }
                }
                self.next_token();
            }
        }

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_match
    }

    /// Look ahead to check if the current string/numeric literal token is actually
    /// the property name of the next class member (followed by `:` or `?`).
    /// This prevents false TS1442 when two string-literal-named properties appear
    /// in sequence, e.g., `"d": string; "e": number;`.
    fn look_ahead_is_next_class_member_property_name(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip the string/numeric literal

        let is_property_name = matches!(
            self.token(),
            SyntaxKind::ColonToken       // "d": type
            | SyntaxKind::QuestionToken // "d"?: type
        );

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_property_name
    }

    /// Look ahead to see if we have an accessor (get/set followed by property name and ()
    pub(crate) fn look_ahead_is_accessor(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip 'get' or 'set'
        self.next_token();

        // Note: line breaks after get/set do NOT prevent accessor parsing.
        // The ECMAScript grammar has no [no LineTerminator here] restriction
        // for get/set in class method definitions.

        // Check the token AFTER 'get' or 'set' to determine what we have:
        // - `:`, `=`, `;`, `}`, `?` → property named 'get'/'set' (e.g., `get: number`)
        // - `(` → method named 'get'/'set' (e.g., `get() {}`)
        // - identifier/string/etc → accessor (e.g., `get foo() {}`)
        let next_token = self.token();
        let is_accessor = !matches!(
            next_token,
            SyntaxKind::ColonToken          // `get: number` - property
                | SyntaxKind::EqualsToken     // `get = 1` - property
                | SyntaxKind::SemicolonToken  // `get;` - property
                | SyntaxKind::CloseBraceToken // `get }` - property
                | SyntaxKind::OpenParenToken  // `get()` - method
                | SyntaxKind::QuestionToken // `get?` - property
        ) && self.is_property_name(); // Also ensure there's a valid property name

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_accessor
    }

    /// Look ahead to see if we have a static block: static { ... }
    pub(crate) fn look_ahead_is_static_block(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip 'static'
        self.next_token();
        // Check for '{'
        let is_block = self.is_token(SyntaxKind::OpenBraceToken);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_block
    }

    /// Parse static block: static { ... }
    pub(crate) fn parse_static_block(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Consume 'static'
        self.parse_expected(SyntaxKind::StaticKeyword);

        // Parse the block body with static block context (where 'await' is reserved)
        // IMPORTANT: Static blocks create a fresh execution context - they do NOT inherit
        // async/generator context from enclosing functions. Clear those flags.
        self.parse_expected(SyntaxKind::OpenBraceToken);
        let saved_flags = self.context_flags;
        // Clear async/generator flags and set static block flag
        self.context_flags &= !(CONTEXT_FLAG_ASYNC | CONTEXT_FLAG_GENERATOR);
        self.context_flags |= CONTEXT_FLAG_STATIC_BLOCK;
        let statements = self.parse_statements();
        self.context_flags = saved_flags;
        // Capture the `}` token's own end before `parse_expected` advances past it —
        // `token_end()` after the call would report the end of the *next* token instead.
        let end_pos = self.token_end();
        self.parse_expected(SyntaxKind::CloseBraceToken);

        self.arena.add_block(
            syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::node::BlockData {
                statements,
                multi_line: true,
            },
        )
    }

    /// Look ahead to see if this is an index signature: [key: Type]: `ValueType`
    /// vs a computed property: [expr]: Type or [computed]()
    ///
    /// Matches tsc's `isUnambiguouslyIndexSignature`. Recognizes:
    ///   [id:    [id,    [id?:    [id?,    [id?]    [...    [modifier id
    pub(crate) fn look_ahead_is_index_signature(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip '['
        self.next_token();

        let is_index_sig = if self.is_token(SyntaxKind::DotDotDotToken) {
            true
        } else if self.is_parameter_modifier() {
            self.next_token();
            self.is_identifier_or_keyword()
        } else if !self.is_identifier_or_keyword() {
            false
        } else {
            self.next_token();
            if self.is_token(SyntaxKind::ColonToken) || self.is_token(SyntaxKind::CommaToken) {
                // `[id:` or `[id,`
                true
            } else if self.is_token(SyntaxKind::QuestionToken) {
                // `[id?` — check what follows: `:`, `,`, or `]` means index signature
                self.next_token();
                self.is_token(SyntaxKind::ColonToken)
                    || self.is_token(SyntaxKind::CommaToken)
                    || self.is_token(SyntaxKind::CloseBracketToken)
            } else {
                false
            }
        };

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_index_sig
    }

    /// Check if this is `[]` — an empty index signature (malformed, no parameters).
    /// Used in type member contexts where `[]` should be an empty index signature,
    /// NOT in type suffix contexts where `[]` is an array type.
    pub(crate) fn look_ahead_is_empty_index_signature(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip `[`
        let is_empty = self.is_token(SyntaxKind::CloseBracketToken);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_empty
    }
}
