impl ParserState {
    fn parse_type_argument_in_type_arguments(&mut self) -> NodeIndex {
        if !self.is_token(SyntaxKind::QuestionToken) {
            // Each type argument is a fresh, complete type-expression scope. Clear the
            // scope-barrier flags so a conditional type is permitted even when the
            // reference appears in an outer `extends` position (e.g.
            // `T extends Foo<A extends B ? C : D> ? 1 : 2`), and a postfix `?` is not
            // mis-reserved for a tuple-level optional when the reference is nested in a
            // tuple element. Matches tsc, where every type argument is parsed by a fresh
            // `parseType`, so the one-level `noConditionalTypes` block does not leak in.
            let saved_flags = self.context_flags;
            self.context_flags &= !(crate::parser::state::CONTEXT_FLAG_DISALLOW_CONDITIONAL_TYPES
                | crate::parser::state::CONTEXT_FLAG_IN_TUPLE_ELEMENT);
            let type_node = self.parse_type();
            self.context_flags = saved_flags;
            return type_node;
        }

        use tsz_common::diagnostics::diagnostic_codes;

        let question_start = self.token_pos();
        let question_end = self.token_end();
        self.next_token(); // consume '?'

        if self.is_greater_than_or_compound() || self.is_token(SyntaxKind::CommaToken) {
            // `foo<?>` should not emit TS1110; consume the `>` path via caller's expected parser.
            self.parse_error_at(
                question_start,
                question_end - question_start,
                "JSDoc types can only be used inside documentation comments.",
                diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS,
            );
            return self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                question_start,
                question_end,
                crate::parser::node::IdentifierData {
                    atom: tsz_common::interner::Atom::NONE,
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
                },
            );
        }

        if !self.can_token_start_type() {
            // `foo<?` with no valid following type should emit TS8020.
            self.parse_error_at(
                question_start,
                question_end - question_start,
                "JSDoc types can only be used inside documentation comments.",
                diagnostic_codes::JSDOC_TYPES_CAN_ONLY_BE_USED_INSIDE_DOCUMENTATION_COMMENTS,
            );
            return self.arena.add_identifier(
                SyntaxKind::Identifier as u16,
                question_start,
                question_end,
                crate::parser::node::IdentifierData {
                    atom: tsz_common::interner::Atom::NONE,
                    escaped_text: String::new(),
                    original_text: None,
                    type_arguments: None,
                },
            );
        }

        // `?T` in type-argument position should emit TS17020 (JSDoc prefix style in types),
        // not TS8020. This preserves the behavior expected by TS conformance.
        let inner_type = self.parse_type();
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
        // For `?T?` (both prefix and postfix `?`) the inner_type span now
        // covers `T?` because postfix-? widens to a `T | null` UNION_TYPE.
        // The suggestion text should still reference just `T`, matching tsc.
        let suggestion = Self::prefix_nullable_type_suggestion(&suggested);
        let msg = format!(
            "'?' at the start of a type is not valid TypeScript syntax. Did you mean to write '{suggestion}'?"
        );
        self.parse_error_at(
            question_start,
            diag_end - question_start,
            &msg,
            diagnostic_codes::AT_THE_START_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE,
        );

        if let Some(node) = self.arena.get_mut(inner_type) {
            node.pos = question_start;
        }

        inner_type
    }

    /// Try to parse type arguments for a call expression: foo<T>()
    /// Returns Some(NodeList) if successful, None if this is not type arguments.
    /// Uses look-ahead to distinguish from comparison operators.
    pub(crate) fn try_parse_type_arguments_for_call(&mut self) -> Option<NodeList> {
        // Full checkpoint — `parse_type_argument_in_type_arguments` can
        // toggle flags/recovery state. See `speculation.rs`.
        let checkpoint = self.speculation_checkpoint();

        // Save the `<` position before consuming it so TS1099 points at `<`, not `>`
        let less_than_start = self.u32_from_usize(self.scanner.get_token_start());
        let less_than_end = self.u32_from_usize(self.scanner.get_token_end());

        // Consume `<` (handles `<<` by splitting into two `<` tokens)
        self.parse_expected_less_than();

        // Check for empty type argument list: <>
        // TypeScript reports TS1099: "Type argument list cannot be empty"
        if self.is_plain_greater_than_for_expression_type_arguments() {
            use tsz_common::diagnostics::diagnostic_codes;
            self.parse_error_at(
                less_than_start,
                less_than_end - less_than_start,
                "Type argument list cannot be empty.",
                diagnostic_codes::TYPE_ARGUMENT_LIST_CANNOT_BE_EMPTY,
            );
            self.parse_expected_greater_than();

            // Check if followed by ( (call) or a token that can follow type
            // arguments in expression context (instantiation expression like `fx<>;`).
            if !self.is_token(SyntaxKind::OpenParenToken)
                && !self.can_follow_type_arguments_in_expression()
            {
                // Not a call or instantiation expression - full rollback
                self.restore_speculation_checkpoint(checkpoint);
                return None;
            }
            return Some(self.make_node_list(Vec::new()));
        }

        let mut args = Vec::new();
        let mut expecting_argument = true;
        let mut closed_type_arguments = false;
        let mut has_trailing_comma = false;

        while !self.is_token(SyntaxKind::EndOfFileToken) {
            if self.is_plain_greater_than_for_expression_type_arguments() {
                closed_type_arguments = true;
                break;
            }

            if self.is_token(SyntaxKind::CommaToken) {
                let comma_can_be_trailing = !expecting_argument;
                if expecting_argument {
                    self.error_type_expected();
                    args.push(self.error_node());
                }

                self.next_token();
                if comma_can_be_trailing
                    && self.is_plain_greater_than_for_expression_type_arguments()
                {
                    has_trailing_comma = true;
                }
                expecting_argument = true;
                continue;
            }

            if !expecting_argument {
                break;
            }

            if self.is_token(SyntaxKind::SemicolonToken)
                || self.is_token(SyntaxKind::CloseBraceToken)
                || self.is_token(SyntaxKind::EndOfFileToken)
            {
                break;
            }

            let type_node = self.parse_type_argument_in_type_arguments();
            args.push(type_node);
            expecting_argument = false;
        }

        if closed_type_arguments {
            // Successfully parsed type arguments, now consume >
            self.parse_expected_greater_than();

            // Check if the following token indicates these were type arguments
            // (call, tagged template, or instantiation expression)
            if self.can_follow_type_arguments_in_expression() {
                let mut list = self.make_node_list(args);
                list.has_trailing_comma = has_trailing_comma;
                return Some(list);
            }
        }

        // Not type arguments - full rollback to undo any context-flag or
        // recovery-flag mutations the speculative type parses may have made.
        self.restore_speculation_checkpoint(checkpoint);
        None
    }

    /// Check if the token following `>` can follow type arguments in an expression.
    /// Implements tsc's `canFollowTypeArgumentsInExpression()`.
    ///
    /// Returns true for:
    /// - `(` — call expression: `f<T>(args)`
    /// - template literal — tagged template: "f<T>\`...\`"
    /// - line break — instantiation expression: `f<T>\n`
    /// - binary operator — instantiation expression: `f<T> || fallback`
    /// - non-expression-starter — instantiation expression: `f<T>; f<T>}`
    ///
    /// Returns false for:
    /// - `<` — ambiguous: `f<T><U>` → treat as relational
    /// - `>` — ambiguous: `f<T>>` → treat as relational
    /// - `+`/`-` — unary: `f < T > +1` → treat as relational chain
    fn can_follow_type_arguments_in_expression(&self) -> bool {
        match self.token() {
            // These always indicate type arguments (call or tagged template)
            SyntaxKind::OpenParenToken
            | SyntaxKind::NoSubstitutionTemplateLiteral
            | SyntaxKind::TemplateHead => true,

            // These never follow type arguments (ambiguous with relational or unary context)
            SyntaxKind::LessThanToken
            | SyntaxKind::GreaterThanToken
            | SyntaxKind::PlusToken
            | SyntaxKind::MinusToken => false,

            // Everything else: favor type arguments when followed by
            // a line break, binary operator, or non-expression-starter.
            // Assignment operators like `=` are not expression starters,
            // so `f<T> = x` correctly returns true here (tsc treats
            // instantiation expression assignment as TS2364).
            _ => {
                self.scanner.has_preceding_line_break()
                    || self.is_binary_operator()
                    || !self.is_expression_start()
            }
        }
    }

    fn is_plain_greater_than_for_expression_type_arguments(&mut self) -> bool {
        // tsc's expression disambiguation calls reScanGreaterToken() and accepts
        // only a plain `>`. Compound tokens like `>=` and `>>` keep the parse in
        // relational-expression space instead of becoming speculative type args.
        self.try_rescan_greater_token() == SyntaxKind::GreaterThanToken
    }

    /// Parse array type suffix (T[]) or indexed access type (T[K])
    pub(crate) fn parse_array_type(
        &mut self,
        start_pos: u32,
        element_type: NodeIndex,
    ) -> NodeIndex {
        let mut current = element_type;

        while self.is_token(SyntaxKind::OpenBracketToken) {
            if self.look_ahead_is_computed_type_member_boundary() {
                break;
            }
            if self.look_ahead_is_index_signature() {
                break;
            }
            self.next_token();

            // Check if this is array type [] or indexed access type [K]
            if self.is_token(SyntaxKind::CloseBracketToken) {
                // Array type: T[]
                self.next_token();
                let end_pos = self.token_full_start();

                current = self.arena.add_array_type(
                    syntax_kind_ext::ARRAY_TYPE,
                    start_pos,
                    end_pos,
                    crate::parser::node::ArrayTypeData {
                        element_type: current,
                    },
                );
            } else {
                // Private identifiers are not currently valid as indexed-access
                // type arguments (e.g. `C[#bar]`). Keep the malformed name in the
                // token stream so declaration-list recovery can parse it as the
                // next invalid declarator, matching tsc's emitted JS shape.
                if self.is_token(SyntaxKind::PrivateIdentifier) {
                    self.parse_expected(SyntaxKind::CloseBracketToken);
                    break;
                }

                // Indexed access type: T[K]
                let index_type = self.parse_type();
                self.parse_expected(SyntaxKind::CloseBracketToken);
                let end_pos = self.token_full_start();

                current = self.arena.add_indexed_access_type(
                    syntax_kind_ext::INDEXED_ACCESS_TYPE,
                    start_pos,
                    end_pos,
                    crate::parser::node::IndexedAccessTypeData {
                        object_type: current,
                        index_type,
                    },
                );
            }
        }

        current
    }

    pub(crate) fn look_ahead_is_computed_type_member_boundary(&mut self) -> bool {
        if !self.is_token(SyntaxKind::OpenBracketToken) {
            return false;
        }

        if !self.scanner.has_preceding_line_break() {
            return false;
        }

        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token(); // skip `[`
        let empty_brackets = self.is_token(SyntaxKind::CloseBracketToken);
        let mut bracket_depth = 1_u32;
        // Track template substitution nesting so `}` inside a template is
        // re-scanned as TemplateMiddle/TemplateTail rather than treated as a
        // plain CloseBraceToken that would confuse the bracket-depth counter.
        let mut template_depth = 0_u32;
        while bracket_depth > 0 && !self.is_token(SyntaxKind::EndOfFileToken) {
            match self.token() {
                SyntaxKind::OpenBracketToken => {
                    bracket_depth += 1;
                    self.next_token();
                }
                SyntaxKind::CloseBracketToken => {
                    bracket_depth -= 1;
                    self.next_token();
                }
                SyntaxKind::TemplateHead => {
                    // Entering a template literal with substitutions.
                    template_depth += 1;
                    self.next_token();
                }
                SyntaxKind::CloseBraceToken if template_depth > 0 => {
                    // Re-scan `}` as a template continuation to consume the
                    // TemplateMiddle/TemplateTail token and stay in sync with
                    // the template literal structure.
                    self.scanner.re_scan_template_token(false);
                    self.current_token = self.scanner.get_token();
                    if matches!(self.current_token, SyntaxKind::TemplateTail) {
                        template_depth -= 1;
                    }
                    self.next_token();
                }
                _ => {
                    self.next_token();
                }
            }
        }

        let is_boundary = if bracket_depth == 0 {
            match self.token() {
                SyntaxKind::ColonToken | SyntaxKind::OpenParenToken | SyntaxKind::LessThanToken => {
                    true
                }
                SyntaxKind::SemicolonToken
                | SyntaxKind::CommaToken
                | SyntaxKind::CloseBraceToken
                    if empty_brackets =>
                {
                    true
                }
                SyntaxKind::QuestionToken => {
                    self.next_token();
                    matches!(
                        self.token(),
                        SyntaxKind::ColonToken
                            | SyntaxKind::OpenParenToken
                            | SyntaxKind::LessThanToken
                    )
                }
                _ => false,
            }
        } else {
            false
        };

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_boundary
    }

    /// Check if current keyword can be used as a property name
    /// (when followed by :, ?, (, <, or at end of type member)
    pub(crate) fn look_ahead_is_property_name_after_keyword(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        // Skip the keyword
        self.next_token();

        // If followed by these, the keyword is being used as a property name
        let is_property_name = self.is_token(SyntaxKind::ColonToken)
            || self.is_token(SyntaxKind::QuestionToken)
            || self.is_token(SyntaxKind::OpenParenToken)
            || self.is_token(SyntaxKind::LessThanToken)
            || self.is_token(SyntaxKind::SemicolonToken)
            || self.is_token(SyntaxKind::CommaToken)
            || self.is_token(SyntaxKind::CloseBraceToken);

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        is_property_name
    }

    /// Check if there is a line break between the current keyword and the next token.
    /// Used to detect ASI in type member contexts where `protected\n p` means two properties.
    pub(crate) fn look_ahead_has_line_break_after_keyword(&mut self) -> bool {
        let snapshot = self.scanner.save_state();
        let current = self.current_token;

        self.next_token();
        let has_line_break = self.scanner.has_preceding_line_break();

        self.scanner.restore_state(snapshot);
        self.current_token = current;
        has_line_break
    }

    // Function types, type assertions, JSX → state_types_jsx.rs
}
