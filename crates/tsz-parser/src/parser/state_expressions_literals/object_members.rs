//! Object-literal member and property-name parsing.

use super::{
    CONTEXT_FLAG_ASYNC, CONTEXT_FLAG_FUNCTION_BODY, CONTEXT_FLAG_GENERATOR,
    CONTEXT_FLAG_STATIC_BLOCK, ParserState,
};
use crate::parser::{
    NodeIndex, node::IdentifierData, state::CONTEXT_FLAG_GENERATOR_MEMBER_NAME, syntax_kind_ext,
};
use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
use tsz_common::interner::IdentText;
use tsz_scanner::SyntaxKind;
use tsz_scanner::scanner_impl::TokenFlags;

impl ParserState {
    /// Check if current token can start an object property
    /// Used for error recovery in object literals when commas are missing
    pub(crate) const fn is_property_start(&self) -> bool {
        match self.token() {
            SyntaxKind::DotDotDotToken
            | SyntaxKind::GetKeyword
            | SyntaxKind::SetKeyword
            | SyntaxKind::AsyncKeyword
            | SyntaxKind::AsteriskToken
            | SyntaxKind::StringLiteral
            | SyntaxKind::NumericLiteral
            | SyntaxKind::BigIntLiteral
            | SyntaxKind::Identifier
            | SyntaxKind::OpenBracketToken => true,
            _ => self.is_identifier_or_keyword(),
        }
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

        // NOTE: Certain keywords can appear as modifiers before object literal members.
        // When used as a modifier (followed by another property name), they are consumed
        // and errors are reported. When used as a property name (followed by `:`, `,`, `}`,
        // etc.), they're treated as identifiers.
        //
        // public/private/protected/abstract → TS1042 "modifier cannot be used here"
        // static/export → silently consumed (tsc parses them via parseModifiers() and
        //   the grammar checker handles them separately; no TS1042 is emitted)
        if matches!(
            self.token(),
            SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::StaticKeyword
                | SyntaxKind::ExportKeyword
        ) && !self.look_ahead_is_property_name_after_keyword()
        {
            let emit_ts1042 = matches!(
                self.token(),
                SyntaxKind::PrivateKeyword
                    | SyntaxKind::ProtectedKeyword
                    | SyntaxKind::PublicKeyword
                    | SyntaxKind::AbstractKeyword
            );
            if emit_ts1042 {
                use tsz_common::diagnostics::diagnostic_codes;
                let modifier_name = match self.token() {
                    SyntaxKind::PublicKeyword => "'public'",
                    SyntaxKind::PrivateKeyword => "'private'",
                    SyntaxKind::ProtectedKeyword => "'protected'",
                    SyntaxKind::AbstractKeyword => "'abstract'",
                    _ => "modifier",
                };
                self.parse_error_at_current_token(
                    &format!("{modifier_name} modifier cannot be used here."),
                    diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE, // TS1042
                );
                // TSC also emits TS1184 — but only when the modifier precedes a
                // shorthand method (`public foo() {}`).  Property assignments
                // (`public foo: v`) and accessor declarations (`public get foo()`)
                // only get TS1042.
                {
                    let snap = self.scanner.save_state();
                    let saved_tok = self.current_token;
                    self.next_token(); // peek past modifier
                    let is_method = if self.is_identifier_or_keyword() {
                        // Check if identifier is followed by `(` or `<` (method call)
                        self.next_token();
                        matches!(
                            self.token(),
                            SyntaxKind::OpenParenToken | SyntaxKind::LessThanToken
                        )
                    } else {
                        false
                    };
                    self.scanner.restore_state(snap);
                    self.current_token = saved_tok;
                    if is_method {
                        self.parse_companion_error_at_current_token(
                            "Modifiers cannot appear here.",
                            diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE, // TS1184
                        );
                    }
                }
            }
            self.next_token(); // consume the modifier
            // Continue parsing the actual property/method
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
            // A template literal cannot be a property name. tsc's
            // `parsePropertyName` does not accept template literals, so
            // `parseObjectLiteralElement` reports TS1136 and aborts the
            // member list *without consuming the template*. The object
            // literal then closes (synthetic `}`), and the surrounding
            // expression/statement parser handles the template as a tagged
            // template tail on the object expression, with anything after it
            // parsed as separate statements. Mirror that recovery here so the
            // recovered AST — and therefore emit — matches tsc, instead of
            // absorbing the template (and a trailing value) as a property.
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token(
                "Property assignment expected.",
                diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED,
            );
            self.abort_object_literal_recovery_once = true;
            // Signal the variable-declaration-list recovery that a `:` after
            // this object-literal initializer is a misplaced separator, not a
            // type annotation, so a trailing `\`tpl\`: value` recovers as a
            // tagged template plus a separate statement (matching tsc).
            self.recovered_template_literal_property_in_object = true;
            return NodeIndex::NONE;
        }

        // Check if the property name requires `:` syntax (can't be a shorthand property).
        // Shorthand properties only work with identifiers, not:
        // - Reserved words (class, function, etc.)
        // - Contextually reserved words (await in async/static contexts)
        // - String literals ("key")
        // - Numeric literals (0, 1, etc.)
        let property_name_start = self.token_pos();
        let property_name_kind = self.token();
        let property_name_had_prior_missing_colon =
            self.parse_diagnostics.last().is_some_and(|diag| {
                diag.start == property_name_start && diag.message == "':' expected."
            });
        let literal_property_name = matches!(
            property_name_kind,
            SyntaxKind::StringLiteral
                | SyntaxKind::NumericLiteral
                | SyntaxKind::BigIntLiteral
                | SyntaxKind::TrueKeyword
                | SyntaxKind::FalseKeyword
                | SyntaxKind::NullKeyword
        );
        let requires_colon = self.is_reserved_word()
            || (self.is_token(SyntaxKind::AwaitKeyword)
                && (self.in_async_context() || self.in_static_block_context()))
            || self.is_token(SyntaxKind::StringLiteral)
            || self.is_token(SyntaxKind::NumericLiteral)
            || self.is_token(SyntaxKind::BigIntLiteral)
            || self.is_token(SyntaxKind::OpenBracketToken);

        // tsc captures whether the property-name token was a *real* identifier
        // (`isIdentifier()` — Identifier or a contextual keyword, but NOT a
        // reserved word, string/numeric/bigint literal, or computed `[`)
        // before consuming it. This drives the shorthand-vs-assignment decision
        // below exactly as tsc does (`parser.ts`: `tokenIsIdentifier`).
        //
        // tsc's `isIdentifier()` also returns false for a contextually reserved
        // keyword: `yield` in a generator and `await` in an async/static-block
        // context. Such a token can never start a shorthand member, so it must
        // fall into the property-assignment branch and report `':' expected`
        // (e.g. `({ await })` inside a `static { }` block).
        let token_is_identifier = self.is_identifier() && !self.is_contextually_reserved_label();

        let name = self.parse_property_name();

        // TS18016: Check for private identifiers in object literals
        // Private identifiers (#foo) are not allowed in object literals
        if let Some(name_node) = self.arena.get(name)
            && name_node.kind == SyntaxKind::PrivateIdentifier as u16
        {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at(
                name_node.pos,
                name_node.end - name_node.pos,
                "Private identifiers are not allowed outside class bodies.",
                diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
            );
        }

        // Handle method: foo() { } or foo<T>() { }
        if self.is_token(SyntaxKind::OpenParenToken) || self.is_token(SyntaxKind::LessThanToken) {
            return self.parse_object_method_after_name(start_pos, name, false, false);
        }

        // Check for optional property marker '?' - not allowed in object literals
        // TSC emits TS1162: "An object member cannot be declared optional."
        let question_pos = if self.is_token(SyntaxKind::QuestionToken) {
            use tsz_common::diagnostics::diagnostic_codes;
            let pos = self.token_pos();
            self.parse_error_at_current_token(
                "An object member cannot be declared optional.",
                diagnostic_codes::AN_OBJECT_MEMBER_CANNOT_BE_DECLARED_OPTIONAL,
            );
            self.next_token(); // Skip the '?' for error recovery

            // After skipping '?', if followed by '(' or '<', continue parsing as method
            // for error recovery (e.g., `{ foo?() { } }` should still parse the method body).
            // Preserve `question_token=true` on the recovered method so downstream
            // type inference marks the inferred property as optional — tsc's .d.ts
            // output for an inferred `{ foo?() {} }` is `{ foo?(): void }`.
            if self.is_token(SyntaxKind::OpenParenToken) || self.is_token(SyntaxKind::LessThanToken)
            {
                return self.parse_object_method_after_name_with_optional(
                    start_pos, name, false, false, true,
                );
            }
            pos
        } else {
            0
        };

        // Check for definite assignment assertion '!' - not allowed in object literals.
        // TSC emits TS1255 as a grammar error (not a parse error), so it does not
        // suppress downstream semantic checks. We skip the '!' here for error recovery
        // and let the checker emit TS1255 based on the exclamation_token_pos field.
        let exclamation_pos = if self.is_token(SyntaxKind::ExclamationToken) {
            let pos = self.token_pos();
            self.next_token(); // Skip the '!' for error recovery
            pos
        } else {
            0
        };

        // After consuming '!', check for method syntax again: `foo!() { }` or `foo!<T>() { }`
        // tsc's parser handles this because it checks for method tokens after consuming '!'.
        if exclamation_pos != 0
            && (self.is_token(SyntaxKind::OpenParenToken)
                || self.is_token(SyntaxKind::LessThanToken))
        {
            return self.parse_object_method_after_name(start_pos, name, false, false);
        }

        // tsc's shorthand-vs-property decision (`parser.ts`):
        //   const isShorthandPropertyAssignment =
        //       tokenIsIdentifier && (token() !== SyntaxKind.ColonToken);
        // A member is shorthand *only* when its name was a real identifier and
        // the next token is not `:`. Reserved words, string/numeric/bigint
        // literals, computed `[`, and any stray punctuation taken as a name are
        // never shorthand — they always become a property assignment whose `:`
        // is parsed with `parseExpected` (reporting `':' expected` if missing,
        // but never consuming the following value token) and whose initializer
        // is the following assignment expression. This is what makes tsc emit
        // `class: C4` for `{ class C4 {} }` instead of a value-less shorthand.
        let is_shorthand = token_is_identifier && !self.is_token(SyntaxKind::ColonToken);
        if !is_shorthand {
            // When the colon is missing but the name was a literal name that
            // already received a `':' expected` diagnostic at this position and
            // the next token is `;`, defer to the object-literal comma-recovery
            // path (matches tsc's malformed-arrow object recovery, e.g.
            // `foo((1)=>{return 0;})`), which reports `',' expected` at the `;`
            // rather than a second missing-colon diagnostic here.
            let defer_to_comma_recovery = literal_property_name
                && property_name_had_prior_missing_colon
                && self.is_token(SyntaxKind::SemicolonToken);
            if defer_to_comma_recovery {
                // Preserve the prior shorthand recovery: emit no `':' expected`
                // here and let the outer object-literal loop recover the `;` as a
                // missing comma, producing a shorthand member (matches tsc's
                // malformed-arrow object recovery, e.g. `foo((1)=>{return 0;})`).
                let end_pos = self.token_end();
                return self.arena.add_shorthand_property(
                    syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT,
                    start_pos,
                    end_pos,
                    crate::parser::node::ShorthandPropertyData {
                        modifiers: None,
                        name,
                        equals_token: false,
                        equals_token_pos: 0,
                        exclamation_token_pos: exclamation_pos,
                        question_token_pos: question_pos,
                        object_assignment_initializer: NodeIndex::NONE,
                    },
                );
            }
            // Replicate tsc's `parseExpected(ColonToken)`: consume the colon when
            // present, otherwise report `':' expected.` at the current token
            // *without consuming it*. tsc emits this through
            // `parseErrorAtCurrentToken` → `parseErrorAtPosition`, which dedups
            // only against an error at the exact same position. tsz's
            // `parse_expected` instead routes the missing-token error through the
            // distance-based `should_report_error()` gate, which wrongly
            // *suppresses* the colon error when a `',' expected.` recovery error
            // was emitted within three columns just before — e.g. `{ a[1], }`,
            // where `a` recovers as shorthand (`',' expected.` at `[`) and the
            // short `[1]` puts the trailing `,` within the suppression window.
            // Use the exact-position-dedup path so this matches tsc for short and
            // long computed/literal names alike (`a[1],` vs `a["ss"],`).
            if !self.parse_optional(SyntaxKind::ColonToken) {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token("':' expected.", diagnostic_codes::EXPECTED);
            }
            let expr = self.parse_assignment_expression();
            let mut end_pos = self.token_end();
            let initializer = if expr.is_none() {
                // Emit TS1109 for missing property value: { prop: }
                self.error_expression_expected();
                if self.scanner.has_preceding_line_break() && self.is_property_start() {
                    self.suppress_object_literal_comma_once = true;
                }
                // Use a *distinct* missing-expression node (not the name node) so
                // this stays a property assignment with an empty value. The
                // emitter detects shorthand via `name == initializer`; reusing
                // `name` here would render `1`/`""` instead of tsc's `1:`/`"":`
                // for a value-less property such as `{ x()?: 1 }` → `1:` or the
                // recovered `{ [s: symbol]: "" }` tail → `"": `.
                self.create_missing_property_value(self.token_pos())
            } else {
                // Recover a stray-annotation computed-indexer tail
                // (`{ [s: symbol]: "" }`): emits the diagnostics, consumes the
                // recovered `]`/`:`, and re-parses the remainder as a fresh
                // member. Because that advances the scanner onto the next
                // member, pin this member's end to its parsed value so the
                // emitter's source-line layout (this member's end vs. the next
                // member's start) keeps the recovered members on one line.
                if self.recover_object_literal_computed_indexer_tail(name) {
                    end_pos = self.arena.get(expr).map_or(end_pos, |node| node.end);
                }
                expr
            };
            // Regular property assignment with explicit value
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
        } else {
            // Shorthand property (`{ name }` / `{ name = expr }`). The
            // `requires_colon` cases are handled in the property-assignment
            // branch above, so no `':' expected` is emitted here.
            let _ = requires_colon;

            // CoverInitializedName: `{ x = expr }` in destructuring patterns
            // ECMAScript: CoverInitializedName[Yield] : IdentifierReference[?Yield] Initializer[In, ?Yield]
            let equals_token_pos = if self.is_token(SyntaxKind::EqualsToken) {
                self.token_pos()
            } else {
                0
            };
            let has_equals = self.parse_optional(SyntaxKind::EqualsToken);
            let initializer = if has_equals {
                self.parse_assignment_expression()
            } else {
                NodeIndex::NONE
            };

            let end_pos = self.token_end();
            // Create SHORTHAND_PROPERTY_ASSIGNMENT node for `{ name }` or `{ name = expr }` syntax
            self.arena.add_shorthand_property(
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT,
                start_pos,
                end_pos,
                crate::parser::node::ShorthandPropertyData {
                    modifiers: None,
                    name,
                    equals_token: has_equals,
                    equals_token_pos,
                    exclamation_token_pos: exclamation_pos,
                    question_token_pos: question_pos,
                    object_assignment_initializer: initializer,
                },
            )
        }
    }

    /// Recover the malformed tail of an object-literal member whose computed
    /// name carried a stray type annotation (`{ [s: symbol]: "" }`).
    ///
    /// Rule: when a member's computed name `[expr]` was just closed by a
    /// recovered `]` (the scanner is *on* a `CloseBracketToken` after a parsed
    /// value) and that `]` is immediately followed by `:`, tsc does not fold the
    /// tail into the current member. It closes the member with the parsed value,
    /// reports `','`/`Property assignment` expected at the `]`/`:`, consumes
    /// both, then re-reads the remainder as a fresh member. This helper performs
    /// that as side effects, suppresses the next comma requirement so the outer
    /// loop re-enters element parsing on the remaining token, and returns `true`
    /// when it fired so the caller keeps the parsed value (and its source end)
    /// as the member's initializer instead of the post-recovery scanner position.
    fn recover_object_literal_computed_indexer_tail(&mut self, name: NodeIndex) -> bool {
        let Some(name_node) = self.arena.get(name) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME
            || !self.is_token(SyntaxKind::CloseBracketToken)
        {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;
        self.next_token();
        let has_colon_after_bracket = self.is_token(SyntaxKind::ColonToken);
        self.scanner.restore_state(snapshot);
        self.current_token = current;
        if !has_colon_after_bracket {
            return false;
        }

        // `']' expected` was already emitted while parsing the computed name; the
        // recovered `]` now closes the current member's value. tsc reports a
        // missing separator at the `]` and a missing property at the following
        // `:`, then re-reads the remaining tokens as a new member.
        self.parse_error_at_current_token("',' expected.", diagnostic_codes::EXPECTED);
        self.next_token(); // consume the `]`
        self.parse_error_at_current_token(
            "Property assignment expected.",
            diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED,
        );
        self.next_token(); // consume the stray `:`

        // Let the outer object-literal loop re-enter element parsing on the
        // remaining token without requiring a comma separator. The trailing
        // `':' expected` (when the new member's name butts against `}`) and any
        // missing-comma diagnostic then fall out of normal member parsing,
        // matching tsc instead of being synthesized here.
        self.suppress_object_literal_comma_once = true;
        true
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

        // Check if followed by property name (identifier, keyword, string, number, bigint, [)
        // Keywords like 'return', 'throw', 'delete' can be method names
        let is_method = self.is_token(SyntaxKind::Identifier)
            || self.is_token(SyntaxKind::StringLiteral)
            || self.is_token(SyntaxKind::NumericLiteral)
            || self.is_token(SyntaxKind::BigIntLiteral)
            || self.is_token(SyntaxKind::OpenBracketToken)
            || self.is_token(SyntaxKind::AsteriskToken) // async *foo()
            || self.is_identifier_or_keyword(); // keywords as method names

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_method
    }

    /// Parse get accessor in object literal: get `foo()` { }
    pub(crate) fn parse_object_get_accessor(&mut self, start_pos: u32) -> NodeIndex {
        self.next_token(); // consume 'get'
        let name = self.parse_property_name();

        // TS18016: Check for private identifiers in object literals
        if let Some(name_node) = self.arena.get(name)
            && name_node.kind == SyntaxKind::PrivateIdentifier as u16
        {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at(
                name_node.pos,
                name_node.end - name_node.pos,
                "Private identifiers are not allowed outside class bodies.",
                diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
            );
        }

        let type_parameters = self.is_token(SyntaxKind::LessThanToken).then(|| {
            self.report_accessor_type_parameters_error(name);
            self.parse_type_parameters()
        });

        let had_open_paren = self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if !had_open_paren {
            // If ( was missing entirely, don't consume following tokens as parameters.
            // They belong to the enclosing context (e.g., object literal list).
            // This prevents `get e,` from consuming `,` as a parameter delimiter
            // and cascading errors into subsequent properties.
            Self::make_node_list(vec![])
        } else if self.is_token(SyntaxKind::CloseParenToken) {
            Self::make_node_list(vec![])
        } else if self.is_token(SyntaxKind::CommaToken) {
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.parse_error_at_current_token(
                diagnostic_messages::PARAMETER_DECLARATION_EXPECTED,
                diagnostic_codes::PARAMETER_DECLARATION_EXPECTED,
            );
            self.next_token();
            Self::make_node_list(vec![])
        } else {
            use tsz_common::diagnostics::diagnostic_codes;
            let parsed = self.parse_parameter_list();
            // A `this` parameter is not a value parameter — see the identical
            // exclusion on the class-member getter arm. tsc rejects a getter's
            // `this` parameter in the checker with TS2784, not here.
            let only_this_parameter = parsed.nodes.len() == 1
                && parsed.nodes.first().is_some_and(|&param_idx| {
                    let name_idx = match self.arena.get_parameter_at(param_idx) {
                        Some(param) => param.name,
                        None => return false,
                    };
                    self.arena
                        .get(name_idx)
                        .is_some_and(|name_node| name_node.kind == SyntaxKind::ThisKeyword as u16)
                });
            if !only_this_parameter {
                // Report error at the accessor name, matching tsc behavior
                if let Some(name_node) = self.arena.get(name) {
                    self.parse_error_at(
                        name_node.pos,
                        name_node.end - name_node.pos,
                        "A 'get' accessor cannot have parameters.",
                        diagnostic_codes::A_GET_ACCESSOR_CANNOT_HAVE_PARAMETERS,
                    );
                } else {
                    self.parse_error_at_current_token(
                        "A 'get' accessor cannot have parameters.",
                        diagnostic_codes::A_GET_ACCESSOR_CANNOT_HAVE_PARAMETERS,
                    );
                }
            }
            parsed
        };
        // Save end of ) for error reporting - get it BEFORE consuming the token
        let close_paren_end = self.token_end();
        // Only expect ) if ( was actually found
        if had_open_paren {
            self.parse_expected(SyntaxKind::CloseParenToken);
        }

        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };
        // If there's a type annotation, use its end; otherwise use close paren end
        let signature_end = if type_annotation.is_none() {
            close_paren_end
        } else {
            self.token_pos()
        };

        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            let saved_body_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_FUNCTION_BODY;
            let block = self.parse_block();
            self.context_flags = saved_body_flags;
            block
        } else {
            if had_open_paren {
                use tsz_common::diagnostics::diagnostic_codes;
                if self.is_token(SyntaxKind::CloseBraceToken) {
                    // Body-less object-literal accessor terminated by the object's
                    // closing `}` (`{ get foo() }`). tsc's `parseFunctionBlockOrSemicolon`
                    // accepts the parse (`canParseSemicolon` is true before `}`) and lets
                    // `checkGrammarAccessor` report TS1005 `'{' expected` via
                    // `grammarErrorAtPos(accessor, accessor.end - 1, 1)` — i.e. at the last
                    // character of the signature (the `)`), not at the following `}`.
                    self.parse_error_at(
                        signature_end.saturating_sub(1),
                        1,
                        "'{' expected.",
                        diagnostic_codes::EXPECTED,
                    );
                } else {
                    self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
                }
            }
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

        // TS18016: Check for private identifiers in object literals
        if let Some(name_node) = self.arena.get(name)
            && name_node.kind == SyntaxKind::PrivateIdentifier as u16
        {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at(
                name_node.pos,
                name_node.end - name_node.pos,
                "Private identifiers are not allowed outside class bodies.",
                diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
            );
        }

        let type_parameters = self.is_token(SyntaxKind::LessThanToken).then(|| {
            self.report_accessor_type_parameters_error(name);
            self.parse_type_parameters()
        });

        let had_open_paren = self.parse_expected(SyntaxKind::OpenParenToken);
        let parameters = if !had_open_paren {
            // If ( was missing entirely, don't consume following tokens as parameters.
            // They belong to the enclosing context (e.g., object literal list).
            Self::make_node_list(vec![])
        } else if self.is_token(SyntaxKind::CloseParenToken) {
            Self::make_node_list(vec![])
        } else {
            self.parse_parameter_list()
        };
        // Save end of ) for error reporting - get it BEFORE consuming the token
        let close_paren_end = self.token_end();
        if had_open_paren {
            self.parse_expected(SyntaxKind::CloseParenToken);
        }

        // TS1049: A 'set' accessor must have exactly one parameter. tsc's
        // `checkGrammarAccessor` reports the count error before the other
        // `set`-specific grammar checks, so a wrong count suppresses them.
        let count_error =
            had_open_paren && self.report_set_accessor_parameter_count(name, &parameters);

        // TS1051: A 'set' accessor cannot have an optional parameter. tsc applies
        // this to an object-literal setter exactly as it does to a class one; this
        // arm was the only one of the three that never checked it.
        self.report_set_accessor_optional_parameter(&parameters, count_error);

        if self.parse_optional(SyntaxKind::ColonToken) {
            // TS1095, suppressed when TS1049 already fired.
            self.report_set_accessor_return_type_annotation(name, count_error);
            let _ = self.parse_return_type();
        }

        let body = if self.is_token(SyntaxKind::OpenBraceToken) {
            let saved_body_flags = self.context_flags;
            self.context_flags |= CONTEXT_FLAG_FUNCTION_BODY;
            let block = self.parse_block();
            self.context_flags = saved_body_flags;
            block
        } else {
            if had_open_paren {
                use tsz_common::diagnostics::diagnostic_codes;
                if self.is_token(SyntaxKind::CloseBraceToken) {
                    // Body-less object-literal accessor terminated by the object's
                    // closing `}` (`{ set foo(a) }`). tsc reports TS1005 `'{' expected`
                    // via `checkGrammarAccessor`'s `grammarErrorAtPos(accessor,
                    // accessor.end - 1, 1)` — at the `)`, not at the following `}`.
                    self.parse_error_at(
                        close_paren_end.saturating_sub(1),
                        1,
                        "'{' expected.",
                        diagnostic_codes::EXPECTED,
                    );
                } else {
                    self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
                }
            }
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

    /// Parse method in object literal: `foo()` { } or async `foo()` { } or *`foo()` { }
    pub(crate) fn parse_object_method(
        &mut self,
        start_pos: u32,
        is_async: bool,
        is_generator: bool,
    ) -> NodeIndex {
        // Build modifiers if async
        let modifiers = is_async.then(|| {
            self.next_token();
            let mod_idx = self
                .arena
                .create_modifier(SyntaxKind::AsyncKeyword, start_pos);
            Self::make_node_list(vec![mod_idx])
        });

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

        // Recovery for malformed generator object members:
        //   *{}        -> synthesize empty parameter list and parse body
        //   *<T>() {}  -> parse type params/signature, omit missing name
        //   *} / *,    -> drop invalid member
        if asterisk
            && (self.is_token(SyntaxKind::LessThanToken)
                || self.is_token(SyntaxKind::OpenParenToken)
                || self.is_token(SyntaxKind::OpenBraceToken)
                || self.is_token(SyntaxKind::CloseBraceToken)
                || self.is_token(SyntaxKind::CommaToken))
        {
            if self.is_token(SyntaxKind::CloseBraceToken) || self.is_token(SyntaxKind::CommaToken) {
                // TS1003: Identifier expected (after `*` with no name before `}` or `,`)
                self.parse_error_at_current_token(
                    tsz_common::diagnostics::diagnostic_messages::IDENTIFIER_EXPECTED,
                    tsz_common::diagnostics::diagnostic_codes::IDENTIFIER_EXPECTED,
                );
                return NodeIndex::NONE;
            }

            // TS1003: Identifier expected (generator method without name)
            self.parse_error_at_current_token(
                tsz_common::diagnostics::diagnostic_messages::IDENTIFIER_EXPECTED,
                tsz_common::diagnostics::diagnostic_codes::IDENTIFIER_EXPECTED,
            );

            let type_parameters = self
                .is_token(SyntaxKind::LessThanToken)
                .then(|| self.parse_type_parameters());

            let parameters = if self.is_token(SyntaxKind::OpenParenToken) {
                self.parse_expected(SyntaxKind::OpenParenToken);
                let params = self.parse_parameter_list();
                self.parse_expected(SyntaxKind::CloseParenToken);
                params
            } else {
                Self::make_node_list(vec![])
            };

            let saved_flags = self.context_flags;
            self.context_flags &=
                !(CONTEXT_FLAG_ASYNC | CONTEXT_FLAG_GENERATOR | CONTEXT_FLAG_STATIC_BLOCK);
            if is_async {
                self.context_flags |= CONTEXT_FLAG_ASYNC;
            }
            self.context_flags |= CONTEXT_FLAG_GENERATOR;
            self.context_flags |= CONTEXT_FLAG_FUNCTION_BODY;
            self.push_label_scope();
            let body = if self.is_token(SyntaxKind::OpenBraceToken) {
                self.parse_block()
            } else {
                NodeIndex::NONE
            };
            self.pop_label_scope();
            self.context_flags = saved_flags;

            let end_pos = self.token_end();
            return self.arena.add_method_decl(
                syntax_kind_ext::METHOD_DECLARATION,
                start_pos,
                end_pos,
                crate::parser::node::MethodDeclData {
                    modifiers,
                    asterisk_token: true,
                    name: NodeIndex::NONE,
                    question_token: false,
                    type_parameters,
                    parameters,
                    type_annotation: NodeIndex::NONE,
                    body,
                },
            );
        }

        let name = self.parse_property_name();

        // TS18016: Check for private identifiers in object literals
        if let Some(name_node) = self.arena.get(name)
            && name_node.kind == SyntaxKind::PrivateIdentifier as u16
        {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at(
                name_node.pos,
                name_node.end - name_node.pos,
                "Private identifiers are not allowed outside class bodies.",
                diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_OUTSIDE_CLASS_BODIES,
            );
        }

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
        self.parse_object_method_after_name_with_optional(
            start_pos, name, asterisk, is_async, false,
        )
    }

    /// Parse method after name with explicit optional (`?`) marker.
    ///
    /// `{ foo?() {} }` is a grammar error (TS1162) but tsc still types
    /// the resulting property as optional, so the emitter can render
    /// `foo?(): void` in the inferred `.d.ts`. The caller emits TS1162
    /// when recovering from the `?`; this path just records that the
    /// method carried the optional marker.
    pub(crate) fn parse_object_method_after_name_with_optional(
        &mut self,
        start_pos: u32,
        name: NodeIndex,
        asterisk: bool,
        is_async: bool,
        question_token: bool,
    ) -> NodeIndex {
        // Optional type parameters
        let type_parameters = self
            .is_token(SyntaxKind::LessThanToken)
            .then(|| self.parse_type_parameters());

        let saved_flags = self.context_flags;
        self.context_flags &=
            !(CONTEXT_FLAG_ASYNC | CONTEXT_FLAG_GENERATOR | CONTEXT_FLAG_STATIC_BLOCK);
        if is_async {
            self.context_flags |= CONTEXT_FLAG_ASYNC;
        }
        if asterisk {
            self.context_flags |= CONTEXT_FLAG_GENERATOR;
        }
        self.context_flags |= CONTEXT_FLAG_FUNCTION_BODY;

        let has_open_paren = self.parse_optional(SyntaxKind::OpenParenToken);
        let dot_tail_recovery = self.recovered_object_literal_dot_tail_once;
        let dot_tail_diag_len = self.parse_diagnostics.len();
        let mut body_already_consumed_by_recovery = false;
        let parameters = if has_open_paren {
            let parameters = self.parse_parameter_list();
            self.parse_expected(SyntaxKind::CloseParenToken);
            parameters
        } else {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at_current_token("'(' expected.", diagnostic_codes::EXPECTED);
            body_already_consumed_by_recovery = self.recover_from_missing_method_open_paren();
            Self::make_node_list(vec![])
        };

        let type_annotation = if self.parse_optional(SyntaxKind::ColonToken) {
            self.parse_return_type()
        } else {
            NodeIndex::NONE
        };

        // Push a new label scope for the method body
        self.push_label_scope();
        let body = if body_already_consumed_by_recovery {
            // recover_from_missing_method_open_paren already consumed the body
            // block while recovering past the missing `(`. Skipping the body
            // lookup here avoids a redundant TS1005 `'{' expected` at the
            // outer object-literal closing brace (or EOF).
            NodeIndex::NONE
        } else if self.is_token(SyntaxKind::OpenBraceToken) {
            self.parse_block()
        } else if self.is_token(SyntaxKind::EqualsGreaterThanToken) {
            // An object-literal method written with `=>` where its body block `{`
            // is expected. tsc reports `'{' expected.` at the `=>`, then recovers.
            // It *additionally* reports TS1434 ("Unexpected keyword or
            // identifier.") on the token after the `=>` only for the
            // mistyped-return-annotation shape `m(a) => T {` (the user meant
            // `m(a): T {}`): that is, when the token after `=>` is a lone
            // identifier/keyword *immediately followed by a `{` block*. When `=>`
            // instead introduces a concise arrow body — `m(a) => x`, `=> a + 1`,
            // `=> f()`, `=> x.y` — tsc emits no TS1434 and lets the trailing
            // tokens recover as a statement (TS1128). tsz previously reported
            // TS1434 on *every* identifier body, over-diagnosing the concise-body
            // case; gate it on the following `{` to match tsc's recovery exactly.
            use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
            self.next_token(); // consume =>
            if self.is_identifier_or_keyword()
                && self.speculate(|parser| {
                    parser.next_token();
                    parser.is_token(SyntaxKind::OpenBraceToken)
                })
            {
                self.parse_error_at_current_token(
                    diagnostic_messages::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                    diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
                );
            }
            self.abort_object_literal_recovery_once = true;
            self.pop_label_scope();
            self.context_flags = saved_flags;
            return NodeIndex::NONE;
        } else if dot_tail_recovery && self.is_token(SyntaxKind::SemicolonToken) {
            // Dot-tail recovery (`{ a.b }`-style) already produced the relevant
            // diagnostics; drop the speculative ones and suppress the missing-body
            // `'{' expected` for the trailing `;` so recovery matches tsc.
            self.parse_diagnostics.truncate(dot_tail_diag_len);
            self.last_error_pos = self
                .parse_diagnostics
                .last()
                .map_or(0, |diagnostic| diagnostic.start);
            NodeIndex::NONE
        } else {
            // An object-literal method whose `{` body is missing is reported by
            // tsc as `'{' expected` at the current token. For a trailing `;`,
            // tsc only does so when the method is the *final* member (the `;` is
            // followed by `}` / EOF); when another member follows, the `;`
            // recovers as a delimiter and the missing comma is reported at that
            // member instead. tsz previously suppressed `'{' expected` for *any*
            // `;`, so the last-member case (`var v = { foo(); }`) wrongly emitted
            // a spurious `',' expected` from the outer object-literal loop.
            //
            // Mirror tsc: emit `'{' expected` here unless a `;` is followed by a
            // further member. The outer loop's same-position comma error is then
            // deduped, leaving exactly tsc's single `'{' expected`.
            let separating_semicolon = self.is_token(SyntaxKind::SemicolonToken)
                && self.speculate(|parser| {
                    parser.next_token();
                    !parser.is_token(SyntaxKind::CloseBraceToken)
                        && !parser.is_token(SyntaxKind::EndOfFileToken)
                });
            if !separating_semicolon {
                use tsz_common::diagnostics::diagnostic_codes;
                self.parse_error_at_current_token("'{' expected.", diagnostic_codes::EXPECTED);
            }
            NodeIndex::NONE
        };
        if dot_tail_recovery {
            self.recovered_object_literal_dot_tail_once = false;
        }
        self.pop_label_scope();

        // Restore context flags after parsing body.
        self.context_flags = saved_flags;

        let modifiers = is_async.then(|| {
            let mod_idx = self
                .arena
                .create_modifier(SyntaxKind::AsyncKeyword, start_pos);
            Self::make_node_list(vec![mod_idx])
        });

        let end_pos = self.token_end();
        self.arena.add_method_decl(
            syntax_kind_ext::METHOD_DECLARATION,
            start_pos,
            end_pos,
            crate::parser::node::MethodDeclData {
                modifiers,
                asterisk_token: asterisk,
                name,
                question_token,
                type_parameters,
                parameters,
                type_annotation,
                body,
            },
        )
    }

    /// Parse property name (identifier, string literal, numeric literal, bigint literal, computed)
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
            SyntaxKind::BigIntLiteral => {
                // BigInt literal can be a property name for parser recovery/parity.
                self.parse_bigint_literal()
            }
            SyntaxKind::OpenBracketToken => {
                // Computed property name: { [expr]: value }
                let start_pos = self.token_pos();
                self.next_token();
                let bare_static_block_await_name =
                    self.in_static_block_context() && self.is_token(SyntaxKind::AwaitKeyword) && {
                        let snapshot = self.scanner.save_state();
                        let current = self.current_token;
                        self.next_token();
                        let is_bare_await = self.is_token(SyntaxKind::CloseBracketToken);
                        self.scanner.restore_state(snapshot);
                        self.current_token = current;
                        is_bare_await
                    };

                // In class member computed property names, keywords such as `public`
                // and `yield` should emit TS1213.
                // Skip the check for generator method names (`* [yield]()`) — tsc does
                // not emit TS1213 for `yield` in computed property names of generators.
                // Skip it too when `await` is followed by something that can start an
                // expression (`[await x]`) — tsc parses that as a genuine AwaitExpression,
                // grammar-checked later by the checker (TS1308), not as an illegal binding
                // identifier. A bare `[await]` (nothing that could be its operand) still
                // falls through to the identifier check below.
                if self.in_class_member_name()
                    && !self.in_generator_context()
                    && !self.is_computed_class_member_yield_expression()
                    && !self.is_computed_class_member_await_expression()
                    && (self.context_flags & CONTEXT_FLAG_GENERATOR_MEMBER_NAME) == 0
                {
                    self.check_illegal_binding_identifier();
                }

                // Note: await in computed property name is NOT a parser error
                // The type checker will emit TS2304 if 'await' is not in scope
                // Example: { [await]: foo } should only emit TS2304, not TS1109

                let expression = self.parse_expression();
                if expression.is_none() {
                    // Emit TS1109 for empty computed property: { [[missing]]: value }
                    self.error_expression_expected();
                } else if self.computed_name_is_comma_expression(expression) {
                    let Some(expr_node) = self.arena.get(expression) else {
                        return self.arena.add_computed_property(
                            syntax_kind_ext::COMPUTED_PROPERTY_NAME,
                            start_pos,
                            self.token_end(),
                            crate::parser::node::ComputedPropertyData { expression },
                        );
                    };
                    self.parse_error_at(
                        expr_node.pos,
                        expr_node.end.saturating_sub(expr_node.pos),
                        diagnostic_messages::A_COMMA_EXPRESSION_IS_NOT_ALLOWED_IN_A_COMPUTED_PROPERTY_NAME,
                        diagnostic_codes::A_COMMA_EXPRESSION_IS_NOT_ALLOWED_IN_A_COMPUTED_PROPERTY_NAME,
                    );
                }
                if bare_static_block_await_name && self.is_token(SyntaxKind::CloseBracketToken) {
                    self.error_expression_expected();
                }
                // Capture the `]` token's own end before `parse_expected` advances past it —
                // `token_end()` after the call would report the end of the *next* token instead.
                let end_pos = self.token_end();
                self.parse_expected(SyntaxKind::CloseBracketToken);

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
                    use tsz_common::diagnostics::diagnostic_codes;
                    self.parse_error_at_current_token(
                        "Property assignment expected.",
                        diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED,
                    );
                    // For object-literal terminators/separators (`,`, `}`, `;`, EOF), do NOT
                    // consume the token. Consuming a `,` here causes us to synthesize a
                    // SHORTHAND_PROPERTY_ASSIGNMENT with an empty name, which then prints
                    // the source-text comma in the emitted output (e.g. `{ x: 0,, }` →
                    // `{ x: 0,\n    ,, }`). Returning an empty Identifier without consuming
                    // lets the outer object-literal loop see the separator and recover.
                    if matches!(
                        self.token(),
                        SyntaxKind::CommaToken
                            | SyntaxKind::CloseBraceToken
                            | SyntaxKind::SemicolonToken
                            | SyntaxKind::EndOfFileToken
                    ) {
                        return self.arena.add_identifier(
                            SyntaxKind::Identifier as u16,
                            start_pos,
                            start_pos,
                            IdentifierData {
                                atom: self.scanner.interner_mut().intern(""),
                                escaped_text: IdentText::empty(),
                                original_text: None,
                            },
                        );
                    }
                }

                // OPTIMIZATION: Capture atom for O(1) comparison
                let atom = self.scanner.get_token_atom();
                // Share the interner's allocation for the cooked text
                let text = self.scanner.token_ident_text();
                // Preserve unicode escape sequences for emission parity with tsc
                let original_text =
                    if (self.scanner.get_token_flags() & TokenFlags::UnicodeEscape as u32) != 0 {
                        let src = self.scanner.source_text();
                        let start = self.scanner.get_token_start();
                        let end = self.scanner.get_token_end();
                        if start < end && end <= src.len() {
                            let slice = &src[start..end];
                            if slice != text.as_str() {
                                Some(IdentText::from(slice))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                self.next_token(); // Accept any token as property name (error recovery)
                let end_pos = self.token_end();

                self.arena.add_identifier(
                    SyntaxKind::Identifier as u16,
                    start_pos,
                    end_pos,
                    IdentifierData {
                        atom,
                        escaped_text: text,
                        original_text,
                    },
                )
            }
        }
    }

    pub(crate) fn is_computed_class_member_yield_expression(&mut self) -> bool {
        if !self.in_class_member_name() || !self.is_token(SyntaxKind::YieldKeyword) {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current_token = self.current_token;
        self.next_token();
        let next_token = self.token();
        let has_line_break = self.scanner.has_preceding_line_break();
        self.scanner.restore_state(snapshot);
        self.current_token = current_token;

        if has_line_break {
            return false;
        }

        !matches!(
            next_token,
            SyntaxKind::CloseBracketToken
                | SyntaxKind::CloseParenToken
                | SyntaxKind::CloseBraceToken
                | SyntaxKind::ColonToken
                | SyntaxKind::CommaToken
                | SyntaxKind::EqualsGreaterThanToken
                | SyntaxKind::SemicolonToken
                | SyntaxKind::EndOfFileToken
        )
    }

    /// Whether the current token is `await` at the start of a class member's
    /// computed name (`[await ...]`).
    ///
    /// Unlike the modifier-like keywords (`public`, `static`, ...) and unlike
    /// `yield`, `tsc` never treats `await` here as an illegal binding
    /// identifier — not even a *bare* `[await]` with nothing that could be
    /// its operand. Oracle-verified (`tsc@7.0.2`): `class K { [await]() {} }`
    /// reports only TS2304 ("Cannot find name 'await'"), the same as any
    /// other unresolved identifier reference; `class K { [await key]() {}
    /// }` parses `await key` as a genuine `AwaitExpression`, grammar-checked
    /// later by the checker (TS1308). So this is unconditional, unlike
    /// [`Self::is_computed_class_member_yield_expression`]'s next-token
    /// lookahead.
    pub(crate) fn is_computed_class_member_await_expression(&self) -> bool {
        self.in_class_member_name() && self.is_token(SyntaxKind::AwaitKeyword)
    }

    /// Check whether an expression node is a computed property name that uses a top-level
    /// comma expression (e.g., `[0, 1]`).
    fn computed_name_is_comma_expression(&self, expression: NodeIndex) -> bool {
        if let Some(node) = self.arena.get(expression)
            && let Some(binary_expr) = self.arena.get_binary_expr(node)
        {
            return binary_expr.operator_token == SyntaxKind::CommaToken as u16;
        }
        false
    }
}
