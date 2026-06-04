impl ParserState {
    /// Parse member expression base (identifier with property/element access, but no calls)
    pub(crate) fn parse_member_expression_base(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut expr = self.parse_primary_expression();

        loop {
            match self.token() {
                SyntaxKind::DotToken => {
                    self.next_token();
                    let diag_count_before = self.parse_diagnostics.len();
                    let name = if let Some(name) = self.parse_private_identifier_or_bare_hash() {
                        name
                    } else if self.is_identifier_or_keyword() {
                        self.parse_identifier_name()
                    } else {
                        self.error_identifier_expected();
                        NodeIndex::NONE
                    };

                    // If parsing the name produced an error, don't create a property access
                    // expression to avoid spurious semantic errors (e.g., TS2339 for incomplete `this.`)
                    if self.parse_diagnostics.len() > diag_count_before {
                        break;
                    }

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
                    let missing_argument_start = self.u32_from_usize(self.scanner.get_token_end());
                    self.next_token();
                    let argument = self.parse_expression();
                    if argument.is_none() {
                        // TS1011: An element access expression should take an argument
                        let current_start = self.u32_from_usize(self.scanner.get_token_start());
                        self.parse_error_at(
                            missing_argument_start,
                            (current_start.saturating_sub(missing_argument_start)).max(1),
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
                // Optional chaining: `new A?.b()` — parse `?.prop` or `?.[idx]`
                // as part of the member expression so the NewExpression wraps
                // the whole chain.  The checker later emits TS1209 for this.
                SyntaxKind::QuestionDotToken => {
                    self.next_token();
                    if self.is_token(SyntaxKind::OpenBracketToken) {
                        // `new A?.[idx]()`
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
                    } else {
                        // `new A?.b()` — property access
                        let name = if self.is_identifier_or_keyword() {
                            self.parse_identifier_name()
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
                                question_dot_token: true,
                            },
                        );
                    }
                }
                // Tagged template literals: tag`template` — needed so that
                // `new f\`abc\`.member(...)` parses the tagged template as
                // part of the member expression, not as `(new f)\`abc\`...`.
                SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead => {
                    self.in_tagged_template = true;
                    let template = self.parse_template_literal();
                    self.in_tagged_template = false;
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
                _ => break,
            }
        }

        expr
    }
}
