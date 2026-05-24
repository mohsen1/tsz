//! Parser state - type parsing, JSX, accessors, and `into_parts` methods

use super::state::ParserState;
use crate::parser::{NodeIndex, NodeList, syntax_kind_ext};
use tsz_common::interner::Atom;
use tsz_scanner::SyntaxKind;

impl ParserState {
    pub(crate) fn prefix_nullable_type_suggestion(suggested: &str) -> String {
        let suggested = suggested.trim_end_matches('?');
        match suggested {
            "any" | "unknown" | "never" | "void" => suggested.to_string(),
            "null" | "undefined" => "null | undefined".to_string(),
            _ => format!("{suggested} | null | undefined"),
        }
    }

    pub(crate) fn finish_type_member_container_close_brace(&mut self) -> u32 {
        if self.deferred_type_member_close_braces > 0 {
            self.deferred_type_member_close_braces -= 1;
            self.token_pos()
        } else {
            self.parse_expected(SyntaxKind::CloseBraceToken);
            self.token_end()
        }
    }

    pub(crate) fn recover_invalid_type_member(&mut self) -> bool {
        let started_with_expression_like_member = matches!(
            self.token(),
            SyntaxKind::LessThanToken
                | SyntaxKind::PlusToken
                | SyntaxKind::MinusToken
                | SyntaxKind::ExclamationToken
                | SyntaxKind::TildeToken
                | SyntaxKind::PlusPlusToken
                | SyntaxKind::MinusMinusToken
                | SyntaxKind::OpenParenToken
                | SyntaxKind::OpenBracketToken
                | SyntaxKind::OpenBraceToken
        );

        self.parse_error_at_current_token(
            tsz_common::diagnostics::diagnostic_messages::PROPERTY_OR_SIGNATURE_EXPECTED,
            tsz_common::diagnostics::diagnostic_codes::PROPERTY_OR_SIGNATURE_EXPECTED,
        );

        if self.is_statement_start() {
            self.deferred_type_member_close_braces = self
                .deferred_type_member_close_braces
                .max(self.type_member_container_depth);
            // For malformed type members that start like expressions (for example `<-`),
            // tsc reports TS1109 at the synchronizing `}` instead of surfacing that
            // `}` as a top-level TS1128 stray-brace diagnostic.
            if started_with_expression_like_member {
                while !self.is_token(SyntaxKind::CloseBraceToken)
                    && !self.is_token(SyntaxKind::EndOfFileToken)
                {
                    self.next_token();
                }
                if self.is_token(SyntaxKind::CloseBraceToken) {
                    self.parse_error_at_current_token(
                        "Expression expected.",
                        tsz_common::diagnostics::diagnostic_codes::EXPRESSION_EXPECTED,
                    );
                }
            }
            true
        } else {
            // Narrow recovery: `<-` inside a type member should surface TS1109 at
            // the synchronizing close brace, not a top-level TS1128 stray brace.
            if self.is_token(SyntaxKind::CloseBraceToken) {
                let source = self.get_source_text().as_bytes();
                let mut cursor = self.token_pos() as usize;
                while cursor > 0 && source[cursor - 1].is_ascii_whitespace() {
                    cursor -= 1;
                }
                if cursor > 0 && source[cursor - 1] == b'-' {
                    let mut before_minus = cursor - 1;
                    while before_minus > 0 && source[before_minus - 1].is_ascii_whitespace() {
                        before_minus -= 1;
                    }
                    if before_minus > 0 && source[before_minus - 1] == b'<' {
                        self.parse_error_at_current_token(
                            "Expression expected.",
                            tsz_common::diagnostics::diagnostic_codes::EXPRESSION_EXPECTED,
                        );
                        return true;
                    }
                }
            }

            self.next_token();
            false
        }
    }

    // =========================================================================
    // Parse Methods - Types (minimal implementation)
    // =========================================================================

    pub(crate) fn is_asserts_keyword(&self) -> bool {
        self.is_token(SyntaxKind::AssertsKeyword)
            || (self.is_token(SyntaxKind::Identifier)
                && self.scanner.get_token_value_ref() == "asserts")
    }

    pub(crate) fn is_asserts_type_predicate_start(&mut self) -> bool {
        if !self.is_asserts_keyword() {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        // Matches tsc's `nextTokenIsIdentifierOrKeywordOnSameLine`: a line break
        // before the next token means ASI applies — `asserts` is a type reference,
        // not the start of a predicate.
        let is_param = (self.is_identifier_or_keyword() || self.is_token(SyntaxKind::ThisKeyword))
            && !self.scanner.has_preceding_line_break();
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_param
    }

    pub(crate) fn consume_asserts_keyword(&mut self) {
        if self.is_asserts_keyword() {
            self.next_token();
        } else {
            self.parse_expected(SyntaxKind::AssertsKeyword);
        }
    }

    /// Parse a type (handles keywords, type references, unions, intersections, conditionals).
    ///
    /// Identifier-based type predicates (`x is T`) are NOT allowed here — they are
    /// only valid in return type position. However, `this is T` predicates ARE parsed
    /// here (matching tsc's `parseThisTypeOrThisTypePredicate` in `parseType`).
    /// Use `parse_return_type()` for return types where both forms are valid.
    pub(crate) fn parse_type(&mut self) -> NodeIndex {
        self.parse_type_with_predicates(false)
    }

    /// Parse a type in a context that explicitly disallows type predicates, such as
    /// `expr as T` and `<T>expr`.
    pub(crate) fn parse_non_predicate_type(&mut self) -> NodeIndex {
        self.parse_type_with_predicates(false)
    }

    pub(crate) fn parse_type_with_predicates(&mut self, allow_type_predicates: bool) -> NodeIndex {
        // `asserts` type predicates can appear in any type position (matches tsc's
        // `parseNonArrayType` which always recognises `AssertsKeyword + ident-on-same-line`).
        // When the resulting predicate is in an invalid context the checker emits
        // TS1228 from `get_type_from_type_node`. The lookahead here is what
        // distinguishes a predicate from a plain `asserts` type reference.
        if self.is_asserts_type_predicate_start() {
            return self.parse_asserts_type_predicate();
        }

        if self.is_identifier_or_keyword() || self.is_token(SyntaxKind::ThisKeyword) {
            let is_this = self.is_token(SyntaxKind::ThisKeyword);
            let snapshot = self.scanner.save_state();
            let current = self.current_token;

            self.next_token();
            // A line break before `is` means ASI applies — the identifier is a type,
            // not a type predicate parameter. Matches tsc's `!scanner.hasPrecedingLineBreak()`.
            let is_predicate =
                self.is_token(SyntaxKind::IsKeyword) && !self.scanner.has_preceding_line_break();
            self.scanner.restore_state(snapshot);
            self.current_token = current;

            // `this is T` is always parsed as a type predicate (tsc: parseThisTypeOrThisTypePredicate).
            // `x is T` is only parsed as a type predicate in return type position.
            if is_predicate && (allow_type_predicates || is_this) {
                let name = self.parse_type_predicate_parameter_name();
                let start_pos = if let Some(node) = self.arena.get(name) {
                    node.pos
                } else {
                    self.token_pos()
                };

                self.next_token(); // consume 'is'
                let type_node = self.parse_type();
                // The inner type already captured its own end before the
                // scanner advanced past it; calling `token_end()` here
                // would reflect the *next* token (e.g. `=>` in
                // `(x): x is string => …`), so the TYPE_PREDICATE's
                // source span would overshoot into the surrounding
                // syntax and leak `=>` into emit-side source-slice
                // helpers (`call_expression_declared_return_type_text`
                // observed `x is string =>`, then re-emitted that into
                // d.ts as `… => x is string =>;`).  Anchor on the
                // inner type instead.
                let end_pos = self
                    .arena
                    .get(type_node)
                    .map(|n| n.end)
                    .unwrap_or_else(|| self.token_end());

                return self.arena.add_type_predicate(
                    syntax_kind_ext::TYPE_PREDICATE,
                    start_pos,
                    end_pos,
                    crate::parser::node::TypePredicateData {
                        asserts_modifier: false,
                        parameter_name: name,
                        type_node,
                    },
                );
            }
        }

        // Error recovery: if the token cannot start a type and we're at a boundary
        // (statement start, EOF, or type terminator like `)` `,` `=>`), emit TS1110.
        // Note: We must check can_token_start_type() because identifiers are both
        // statement starters AND valid type names (e.g., "let x: MyType = ...")
        if !self.can_token_start_type()
            && (self.is_statement_start()
                || self.is_token(SyntaxKind::EndOfFileToken)
                || self.is_type_terminator_token())
        {
            self.error_type_expected();
            return self.error_node();
        }

        self.parse_conditional_type()
    }

    /// Create an error node for recovery when type parsing fails
    pub(crate) fn error_node(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let end_pos = start_pos;
        self.arena
            .add_token(SyntaxKind::Identifier as u16, start_pos, end_pos)
    }

    /// Parse return type, which may be a type predicate (x is T) or a regular type
    pub(crate) fn parse_return_type(&mut self) -> NodeIndex {
        // Re-enable conditional types for return type parsing.
        // Return types are nested type contexts where conditional types should be allowed
        // even if disabled by an outer `infer T extends X` or conditional extends.
        let saved_flags = self.context_flags;
        self.context_flags &= !crate::parser::state::CONTEXT_FLAG_DISALLOW_CONDITIONAL_TYPES;

        let result = self.parse_return_type_inner();

        self.context_flags = saved_flags;
        result
    }

    fn parse_return_type_inner(&mut self) -> NodeIndex {
        self.parse_type_with_predicates(true)
    }

    pub(crate) fn parse_type_predicate_parameter_name(&mut self) -> NodeIndex {
        if self.is_token(SyntaxKind::ThisKeyword) {
            let start_pos = self.token_pos();
            let end_pos = self.token_end();
            self.next_token();
            return self
                .arena
                .add_token(SyntaxKind::ThisKeyword as u16, start_pos, end_pos);
        }

        self.parse_identifier_name()
    }

    /// Parse 'asserts' type predicate: asserts x or asserts x is T
    pub(crate) fn parse_asserts_type_predicate(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        self.consume_asserts_keyword();

        let parameter_name = self.parse_type_predicate_parameter_name();

        let type_node = if self.is_token(SyntaxKind::IsKeyword) {
            self.next_token();
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_full_start();

        self.arena.add_type_predicate(
            syntax_kind_ext::TYPE_PREDICATE,
            start_pos,
            end_pos,
            crate::parser::node::TypePredicateData {
                asserts_modifier: true,
                parameter_name,
                type_node,
            },
        )
    }

    /// Parse conditional type: T extends U ? X : Y
    pub(crate) fn parse_conditional_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Parse the check type (left side of extends)
        let check_type = self.parse_union_type();

        // Check for extends keyword to form conditional type.
        // A line break before `extends` prevents conditional type parsing (ASI).
        // This matches tsc's behavior: `!scanner.hasPrecedingLineBreak()`.
        // Also, when DISALLOW_CONDITIONAL_TYPES is set (inside `infer T extends X` parsing),
        // don't parse as conditional type.
        if !self.is_token(SyntaxKind::ExtendsKeyword)
            || self.scanner.has_preceding_line_break()
            || (self.context_flags & crate::parser::state::CONTEXT_FLAG_DISALLOW_CONDITIONAL_TYPES)
                != 0
        {
            return check_type;
        }

        self.next_token(); // consume extends

        // Parse the extends type (right side of extends) with conditional types disabled.
        // This matches tsc's `disallowConditionalTypesAnd(parseType)` — nested conditional types
        // are not allowed in the extends position. This is critical for `infer T extends U`
        // disambiguation: `T extends infer U extends number ? 1 : 0` should parse the
        // infer constraint as `extends number` and the `?` belongs to the outer conditional.
        let saved_flags = self.context_flags;
        self.context_flags |= crate::parser::state::CONTEXT_FLAG_DISALLOW_CONDITIONAL_TYPES;
        let extends_type = self.parse_type();
        self.context_flags = saved_flags;

        // Expect ?
        self.parse_expected(SyntaxKind::QuestionToken);

        // Parse true type
        let true_type = self.parse_type();

        // Expect :
        self.parse_expected(SyntaxKind::ColonToken);

        // Parse false type
        let false_type = self.parse_type();

        let end_pos = self.token_full_start();

        self.arena.add_conditional_type(
            syntax_kind_ext::CONDITIONAL_TYPE,
            start_pos,
            end_pos,
            crate::parser::node::ConditionalTypeData {
                check_type,
                extends_type,
                true_type,
                false_type,
            },
        )
    }

    /// Parse union type: A | B | C
    pub(crate) fn parse_union_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Handle optional leading | (e.g., type T = | A | B)
        let has_leading_bar = self.parse_optional(SyntaxKind::BarToken);

        // Parse first constituent
        let first = self.parse_intersection_type();

        // Check for | to form union
        if !has_leading_bar && !self.is_token(SyntaxKind::BarToken) {
            return first;
        }

        let mut types = vec![first];

        while self.parse_optional(SyntaxKind::BarToken) {
            types.push(self.parse_intersection_type());
        }

        // Use token_full_start() (start of next un-consumed token's trivia) rather than
        // token_end() (end of that token). After the loop exits, the scanner sits on the
        // first token that is NOT part of this union type (e.g. `;`). token_end() would
        // overshoot to include that token's text, causing node_text() to return "A | B;".
        let end_pos = self.token_full_start();
        self.arena.add_composite_type(
            syntax_kind_ext::UNION_TYPE,
            start_pos,
            end_pos,
            crate::parser::node::CompositeTypeData {
                types: self.make_node_list(types),
            },
        )
    }

    /// Parse intersection type: A & B & C
    pub(crate) fn parse_intersection_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Handle optional leading & (e.g., type T = & A & B)
        let has_leading_amp = self.parse_optional(SyntaxKind::AmpersandToken);

        // Parse first constituent
        let first = self.parse_primary_type();

        let mut fallback_next_import_type_options = false;
        if self.abort_intersection_continuation {
            self.abort_intersection_continuation = false;
            if !self.is_token(SyntaxKind::AmpersandToken) {
                return first;
            }
            fallback_next_import_type_options = true;
        }

        // Check for & to form intersection
        if !has_leading_amp && !self.is_token(SyntaxKind::AmpersandToken) {
            return first;
        }

        let mut types = vec![first];

        while self.parse_optional(SyntaxKind::AmpersandToken) {
            self.fallback_import_type_options_once = fallback_next_import_type_options;
            types.push(self.parse_primary_type());
            self.fallback_import_type_options_once = false;
            fallback_next_import_type_options = false;
        }

        let end_pos = self.token_full_start();
        self.arena.add_composite_type(
            syntax_kind_ext::INTERSECTION_TYPE,
            start_pos,
            end_pos,
            crate::parser::node::CompositeTypeData {
                types: self.make_node_list(types),
            },
        )
    }

    /// Parse primary type (keywords, references, parenthesized, tuples, arrays, function types)
    pub(crate) fn parse_primary_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();

        // Handle JSDoc-style leading `?` before a type (e.g., `?string`).
        // TSC emits TS17020: "'?' at the start of a type is not valid TypeScript syntax."
        // We consume the `?`, emit the error, then parse the type normally so downstream
        // checks (e.g., TS2322) can still run without parser cascade noise.
        if self.is_token(SyntaxKind::QuestionToken) {
            let q_start = self.token_pos();
            let q_end = self.token_end();
            self.next_token(); // consume '?'

            // Bare `?` is legacy JSDoc wildcard syntax. In TS source it should
            // surface TS8020 and stop there rather than cascading into TS17020/TS1110.
            if !self.can_token_start_type() {
                self.parse_error_at(
                    q_start,
                    q_end.saturating_sub(q_start).max(1),
                    tsz_common::diagnostics::diagnostic_messages::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS,
                    tsz_common::diagnostics::diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS,
                );
                return self.arena.add_identifier(
                    SyntaxKind::Identifier as u16,
                    q_start,
                    q_end,
                    crate::parser::node::IdentifierData {
                        atom: Atom::NONE,
                        escaped_text: String::new(),
                        original_text: None,
                        type_arguments: None,
                    },
                );
            }

            let inner_type = self.parse_primary_type();
            let (diag_end, suggested) = if let Some(node) = self.arena.get(inner_type) {
                (
                    node.end,
                    self.scanner
                        .source_slice(node.pos as usize, node.end as usize)
                        .to_string(),
                )
            } else {
                (self.token_pos(), String::from("T"))
            };
            let suggestion = Self::prefix_nullable_type_suggestion(&suggested);
            let msg = format!(
                "'?' at the start of a type is not valid TypeScript syntax. Did you mean to write '{suggestion}'?"
            );
            self.parse_error_at(
                q_start,
                diag_end - q_start,
                &msg,
                tsz_common::diagnostics::diagnostic_codes::AT_THE_START_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE,
            );
            if let Some(node) = self.arena.get_mut(inner_type) {
                node.pos = q_start;
            }
            return inner_type;
        }
        if self.is_token(SyntaxKind::ExclamationToken) {
            let bang_start = self.token_pos();
            self.next_token(); // consume '!'

            if !self.can_token_start_type() {
                let bang_end = self.token_pos();
                self.parse_error_at(
                    bang_start,
                    bang_end - bang_start,
                    "JSDoc types can only be used inside documentation comments.",
                    tsz_common::diagnostics::diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS,
                );
                return self.arena.add_identifier(
                    SyntaxKind::Identifier as u16,
                    bang_start,
                    bang_end,
                    crate::parser::node::IdentifierData {
                        atom: tsz_common::interner::Atom::NONE,
                        escaped_text: String::new(),
                        original_text: None,
                        type_arguments: None,
                    },
                );
            }

            let inner_type = self.parse_primary_type();
            let (diag_end, suggested) = if let Some(node) = self.arena.get(inner_type) {
                (
                    node.end,
                    self.scanner
                        .source_slice(node.pos as usize, node.end as usize)
                        .to_string(),
                )
            } else {
                (self.token_pos(), String::from("T"))
            };
            let msg = format!(
                "'!' at the start of a type is not valid TypeScript syntax. Did you mean to write '{suggested}'?"
            );
            self.parse_error_at(
                bang_start,
                diag_end - bang_start,
                &msg,
                tsz_common::diagnostics::diagnostic_codes::AT_THE_START_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE,
            );
            if let Some(node) = self.arena.get_mut(inner_type) {
                node.pos = bang_start;
            }
            return inner_type;
        }

        if self.is_token(SyntaxKind::FunctionKeyword) {
            // `function(...)` is JSDoc legacy syntax in type positions.
            // Parse it as a function type for recovery so checker diagnostics continue.
            return self.parse_jsdoc_legacy_function_type();
        }

        if self.is_token(SyntaxKind::AsteriskToken) {
            let start = self.token_pos();
            let end = self.token_end();
            self.parse_error_at(
                start,
                end - start,
                "JSDoc types can only be used inside documentation comments.",
                tsz_common::diagnostics::diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS,
            );
            self.next_token();
            return self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                start,
                end,
                crate::parser::node::IdentifierData {
                    atom: tsz_common::interner::Atom::NONE,
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
                },
            );
        }

        // If we encounter a token that can't start a type, emit TS1110 (Type expected).
        // However, suppress the error for delimiter/terminator tokens that indicate a
        // *missing* type rather than an *incorrect* token used as a type. TSC silently
        // creates a missing node for these cases (e.g., `(a: ) =>`, `x: ;`).
        if !self.can_token_start_type() {
            if !self.is_type_terminator_token() {
                self.error_type_expected();
            }
            // Return a synthetic identifier node to allow parsing to continue
            return self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                start_pos,
                self.token_pos(),
                crate::parser::node::IdentifierData {
                    atom: Atom::NONE,
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
                },
            );
        }

        let base_type = if self.should_parse_abstract_constructor_type() {
            self.next_token();
            self.parse_constructor_type(true)
        } else if self.is_token(SyntaxKind::NewKeyword) {
            self.parse_constructor_type(false)
        } else if self.is_token(SyntaxKind::LessThanToken) {
            self.parse_generic_function_type()
        } else {
            self.parse_primary_type_base(start_pos)
        };

        self.parse_primary_type_array_suffix(start_pos, base_type)
    }

    fn parse_jsdoc_legacy_function_type(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let token_end = self.token_end();

        self.parse_error_at(
            start_pos,
            token_end - start_pos,
            "JSDoc types can only be used inside documentation comments.",
            tsz_common::diagnostics::diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS,
        );

        self.next_token(); // consume `function`

        let mut is_constructor = false;
        let mut constructor_return_type = NodeIndex::NONE;
        let mut starting_param_index: u32 = 0;
        let mut parameters = self.make_node_list(Vec::new());

        if self.is_token(SyntaxKind::OpenParenToken) {
            self.parse_expected(SyntaxKind::OpenParenToken);

            // JSDoc-legacy `function(new: R, …)` denotes a constructor type whose
            // return type is R.  Detect the leading `new:` before normal param
            // parsing so we can feed it back into the type as the construct
            // signature's return type rather than treating `new` as a parameter
            // name (which would inflate the arity and cascade into TS2554).
            if self.is_token(SyntaxKind::NewKeyword) && self.next_token_is_colon_lookahead() {
                is_constructor = true;
                self.next_token(); // consume `new`
                self.next_token(); // consume `:`
                constructor_return_type = self.parse_type();
                // Consume the trailing `,` if there are more params, otherwise
                // fall through and let the `)` handling below close the list.
                let _ = self.parse_optional(SyntaxKind::CommaToken);
                starting_param_index = 1;
            }

            if !self.is_token(SyntaxKind::CloseParenToken) {
                parameters = self.parse_jsdoc_legacy_function_parameters(starting_param_index);
            }
        }

        if self.is_token(SyntaxKind::CloseParenToken) {
            self.parse_expected(SyntaxKind::CloseParenToken);
        }

        // When `new:` consumed the return type already, ignore any further
        // `: T` suffix (it would be a stray annotation), but still consume it
        // so it cannot cascade into the outer parser.
        let type_annotation = if is_constructor {
            if self.parse_optional(SyntaxKind::ColonToken) {
                let _ = self.parse_type();
            }
            constructor_return_type
        } else if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_type()
        } else {
            NodeIndex::NONE
        };

        let end_pos = self.token_full_start();

        let kind = if is_constructor {
            syntax_kind_ext::CONSTRUCTOR_TYPE
        } else {
            syntax_kind_ext::FUNCTION_TYPE
        };

        self.arena.add_function_type(
            kind,
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

    /// Lookahead that returns true when the *next* token (after the current one)
    /// is `:`.  Used to detect the `new:` and `this:` JSDoc function-type markers.
    fn next_token_is_colon_lookahead(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let saved_token = self.current_token;
        self.next_token();
        let is_colon = self.is_token(SyntaxKind::ColonToken);
        self.scanner.restore_state(snapshot);
        self.current_token = saved_token;
        is_colon
    }

    /// Parse a JSDoc-legacy function type parameter list such as
    /// `(this: number, string, number)`.  Unlike a normal TS parameter list, entries
    /// may be bare types (no `name:`) — tsc treats `function(T1, T2)` as
    /// `(arg0: T1, arg1: T2) => …`.  Without synthesizing names for bare types the
    /// checker would emit cascading TS7051 ("parameter has a name but no type") and
    /// TS2300 ("duplicate identifier") on top of the TS8020 that was already
    /// reported for this JSDoc syntax.  `starting_index` lets callers skip the
    /// indices taken up by a preceding `new:` slot so bare names line up with
    /// tsc's `argN` convention.
    fn parse_jsdoc_legacy_function_parameters(&mut self, starting_index: u32) -> NodeList {
        let mut params = Vec::new();
        let mut index: u32 = starting_index;

        while !self.is_token(SyntaxKind::CloseParenToken)
            && !self.is_token(SyntaxKind::EndOfFileToken)
        {
            let param = self.parse_jsdoc_legacy_function_parameter(index);
            params.push(param);
            index += 1;
            if !self.parse_optional(SyntaxKind::CommaToken) {
                break;
            }
        }

        self.make_node_list(params)
    }

    fn parse_jsdoc_legacy_function_parameter(&mut self, index: u32) -> NodeIndex {
        let param_start = self.token_pos();
        let dot_dot_dot_token = self.parse_optional(SyntaxKind::DotDotDotToken);

        if self.looks_like_jsdoc_named_parameter() {
            // Conventional `name [?]: type` entry (also covers `this: T` and `new: T`).
            let name = if self.is_identifier_or_keyword() {
                self.parse_identifier_name()
            } else {
                self.parse_identifier()
            };
            let question_token = self.parse_optional(SyntaxKind::QuestionToken);
            let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
                self.parse_type()
            } else {
                NodeIndex::NONE
            };
            let param_end = self.token_full_start();
            self.arena.add_parameter(
                syntax_kind_ext::PARAMETER,
                param_start,
                param_end,
                crate::parser::node::ParameterData {
                    modifiers: None,
                    dot_dot_dot_token,
                    name,
                    question_token,
                    type_annotation,
                    initializer: NodeIndex::NONE,
                },
            )
        } else {
            // Bare type — synthesize an `argN` identifier so the checker sees a
            // well-typed parameter rather than a nameless type that would cascade
            // into TS7051 / TS2300 after the TS8020 we already emitted.
            let name_pos = self.token_pos();
            let type_annotation = self.parse_type();
            let name = self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                name_pos,
                name_pos,
                crate::parser::node::IdentifierData {
                    atom: Atom::NONE,
                    escaped_text: format!("arg{index}"),
                    original_text: None,
                    type_arguments: None,
                },
            );
            let param_end = self.token_full_start();
            self.arena.add_parameter(
                syntax_kind_ext::PARAMETER,
                param_start,
                param_end,
                crate::parser::node::ParameterData {
                    modifiers: None,
                    dot_dot_dot_token,
                    name,
                    question_token: false,
                    type_annotation,
                    initializer: NodeIndex::NONE,
                },
            )
        }
    }

    /// Lookahead: does the current token stream start with `name [?]:` — i.e. a
    /// conventional parameter declaration — as opposed to a bare type?  Used to
    /// decide how to parse entries in a JSDoc-legacy function type parameter list.
    fn looks_like_jsdoc_named_parameter(&mut self) -> bool {
        if !self.is_identifier_or_keyword() {
            return false;
        }
        let snapshot = self.scanner.save_state();
        let saved_token = self.current_token;
        self.next_token();
        if self.is_token(SyntaxKind::QuestionToken) {
            self.next_token();
        }
        let is_colon = self.is_token(SyntaxKind::ColonToken);
        self.scanner.restore_state(snapshot);
        self.current_token = saved_token;
        is_colon
    }

    fn should_parse_abstract_constructor_type(&mut self) -> bool {
        if !self.is_token(SyntaxKind::AbstractKeyword) {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let is_abstract_new = self.is_token(SyntaxKind::NewKeyword);
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_abstract_new
    }

    fn parse_primary_type_base(&mut self, start_pos: u32) -> NodeIndex {
        if self.is_token(SyntaxKind::OpenParenToken) {
            return self.parse_parenthesized_type_or_function_type();
        }

        if self.is_token(SyntaxKind::OpenBracketToken) {
            return self.parse_tuple_type();
        }

        if self.is_token(SyntaxKind::OpenBraceToken) {
            return self.parse_object_or_mapped_type();
        }

        if self.is_token(SyntaxKind::TypeOfKeyword) {
            return self.parse_typeof_type();
        }

        if self.is_token(SyntaxKind::KeyOfKeyword) {
            return self.parse_keyof_type();
        }

        if self.is_token(SyntaxKind::UniqueKeyword) {
            return self.parse_unique_type();
        }

        if self.is_token(SyntaxKind::ReadonlyKeyword) {
            return self.parse_readonly_type();
        }

        if self.is_token(SyntaxKind::InferKeyword) {
            return self.parse_infer_type();
        }

        if self.is_token(SyntaxKind::ThisKeyword) {
            let this_start = self.token_pos();
            let this_end = self.token_end();
            self.next_token();
            return self
                .arena
                .add_token(syntax_kind_ext::THIS_TYPE, this_start, this_end);
        }

        if self.is_token(SyntaxKind::StringLiteral)
            || self.is_token(SyntaxKind::NumericLiteral)
            || self.is_token(SyntaxKind::BigIntLiteral)
            || self.is_token(SyntaxKind::TrueKeyword)
            || self.is_token(SyntaxKind::FalseKeyword)
        {
            return self.parse_literal_type();
        }

        if self.is_token(SyntaxKind::MinusToken) {
            return self.parse_prefix_unary_literal_type();
        }

        if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
            || self.is_token(SyntaxKind::TemplateHead)
        {
            return self.parse_template_literal_type();
        }

        if self.is_token(SyntaxKind::ImportKeyword) {
            return self.parse_import_type();
        }

        if self.is_intrinsic_type_keyword() {
            let kind = self.token();
            let end_pos = self.token_end();
            self.next_token();
            return self.arena.add_token(kind as u16, start_pos, end_pos);
        }

        let first_name = self.parse_type_identifier_or_keyword();
        let (type_name, jsdoc_type_arguments) = self.parse_qualified_name_rest(first_name);
        // Only parse type arguments if `<` is on the same line (no preceding line break).
        // A line break before `<` means it's a new construct (e.g., a call signature
        // in a type literal), not type arguments for this type reference.
        // This matches tsc's `!scanner.hasPrecedingLineBreak()` check.
        // `jsdoc_type_arguments` is Some when we already consumed `Foo.<T>` JSDoc-legacy
        // type arguments while walking the qualified-name rest — prefer those so the
        // caller sees a clean `Foo<T>` rather than a namespace access.
        let type_arguments = jsdoc_type_arguments.or_else(|| {
            (self.is_less_than_or_compound() && !self.scanner.has_preceding_line_break())
                .then(|| self.parse_type_arguments())
        });

        self.arena.add_type_ref(
            syntax_kind_ext::TYPE_REFERENCE,
            start_pos,
            self.token_full_start(),
            crate::parser::node::TypeRefData {
                type_name,
                type_arguments,
            },
        )
    }

    const fn is_intrinsic_type_keyword(&self) -> bool {
        matches!(self.token(), SyntaxKind::VoidKeyword)
    }

    fn parse_primary_type_array_suffix(
        &mut self,
        start_pos: u32,
        base_type: NodeIndex,
    ) -> NodeIndex {
        if self.is_token(SyntaxKind::OpenBracketToken) {
            if self.look_ahead_is_computed_type_member_boundary() {
                return base_type;
            }
            return self.parse_array_type(start_pos, base_type);
        }

        // Handle JSDoc-style postfix `?` after a type (e.g., `string?`).
        // TSC emits TS17019: "'?' at the end of a type is not valid TypeScript syntax."
        // We consume the `?` and emit the error so the parser resumes cleanly and
        // downstream semantic checks can still run.
        //
        // Suppress when:
        // - Inside a tuple element (where `?` is the optional marker for `[T?]`)
        // - The token after `?` can start a type (then `?` is a conditional type operator,
        //   e.g., `T extends U ? X : Y`). The conditional type `?` is always followed by
        //   a type (the true branch). A nullable `?` is followed by a delimiter
        //   (`;`, `)`, `,`, `}`, `]`, `=`, EOF, or a line break).
        if self.is_token(SyntaxKind::QuestionToken)
            && !self.scanner.has_preceding_line_break()
            && (self.context_flags & crate::parser::state::CONTEXT_FLAG_IN_TUPLE_ELEMENT) == 0
            && !self.node_is_bare_infer_type(base_type)
        {
            // Lookahead: if the token after `?` can start a type, this is a conditional
            // type's `?`, not a nullable suffix.
            let snapshot = self.scanner.save_state();
            let saved_token = self.current_token;
            self.next_token(); // look past '?'
            let next_can_start_type = self.can_token_start_type()
                || self.is_token(SyntaxKind::BarToken)
                || self.is_token(SyntaxKind::AmpersandToken);
            self.scanner.restore_state(snapshot);
            self.current_token = saved_token;

            if !next_can_start_type {
                let q_end = self.token_end();
                let (diag_start, suggested) = if let Some(node) = self.arena.get(base_type) {
                    (
                        node.pos,
                        self.scanner
                            .source_slice(node.pos as usize, node.end as usize)
                            .to_string(),
                    )
                } else {
                    (start_pos, String::from("T"))
                };
                self.next_token(); // consume '?'
                // Simplify the suggestion for types that absorb undefined.
                // TSC suggests just the type name when adding | undefined is redundant.
                let suggestion = match suggested.as_str() {
                    "any" | "unknown" | "never" | "void" | "undefined" => suggested.clone(),
                    _ => format!("{suggested} | undefined"),
                };
                let msg = format!(
                    "'?' at the end of a type is not valid TypeScript syntax. Did you mean to write '{suggestion}'?"
                );
                self.parse_error_at(
                    diag_start,
                    q_end - diag_start,
                    &msg,
                    tsz_common::diagnostics::diagnostic_codes::AT_THE_END_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE,
                );
                // JSDoc postfix `?` means "nullable" — `T?` ≡ `T | null` for type
                // semantics. tsc keeps emitting TS17019 (the syntax is invalid in
                // TS) but resolves the annotation as `T | null`, so an assignment
                // like `var x: number? = undefined` reports against
                // `number | null` (not `number`). Synthesize the union here so
                // downstream type resolution sees the correct shape.
                let null_token =
                    self.arena
                        .add_token(SyntaxKind::NullKeyword as u16, q_end - 1, q_end);
                let union = self.arena.add_composite_type(
                    syntax_kind_ext::UNION_TYPE,
                    start_pos,
                    q_end,
                    crate::parser::node::CompositeTypeData {
                        types: self.make_node_list(vec![base_type, null_token]),
                    },
                );
                // Recurse to handle `T?[]` (postfix ? followed by array suffix)
                return self.parse_primary_type_array_suffix(start_pos, union);
            }
        }

        if self.is_token(SyntaxKind::ExclamationToken)
            && !self.scanner.has_preceding_line_break()
            && !self.node_is_bare_infer_type(base_type)
        {
            let bang_end = self.token_end();
            let (diag_start, suggested) = if let Some(node) = self.arena.get(base_type) {
                (
                    node.pos,
                    self.scanner
                        .source_slice(node.pos as usize, node.end as usize)
                        .to_string(),
                )
            } else {
                (start_pos, String::from("T"))
            };
            self.next_token(); // consume '!'
            let msg = format!(
                "'!' at the end of a type is not valid TypeScript syntax. Did you mean to write '{suggested}'?"
            );
            self.parse_error_at(
                diag_start,
                bang_end - diag_start,
                &msg,
                tsz_common::diagnostics::diagnostic_codes::AT_THE_END_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE,
            );
            return self.parse_primary_type_array_suffix(start_pos, base_type);
        }

        base_type
    }

    // Parenthesized, tuple, literal, import, mapped, and type-argument parsing -> state_types_advanced.rs
}
