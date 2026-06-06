/// Parser state - left-hand side, call, member access, and optional chaining expression parsing
use super::state::ParserState;
use crate::parser::{
    NodeIndex,
    node::{AccessExprData, CallExprData, TaggedTemplateData},
    node_flags, syntax_kind_ext,
};
use tsz_scanner::SyntaxKind;

impl ParserState {
    // Parse left-hand side expression (member access, call, etc.)
    pub(crate) fn parse_left_hand_side_expression(&mut self) -> NodeIndex {
        let start_pos = self.token_pos();
        let mut expr = self.parse_primary_expression();

        loop {
            match self.token() {
                SyntaxKind::DotToken => {
                    let missing_name_pos = self.token_end();
                    if let Some(node) = self.arena.get(expr)
                        && node.kind
                            == crate::parser::syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
                        && let Some(eta) = self.arena.get_expr_type_args(node)
                    {
                        // TSC emits TS1477 at the `<…>` type-argument span (from `<` to
                        // past `>`), not at the whole expression start. This avoids
                        // setting THIS_NODE_HAS_ERROR on the expression identifier itself,
                        // which would suppress TS2304 for unresolved names like `List`.
                        //
                        // tsc's formula: `pos = typeArguments.pos - 1` (the `<`),
                        // `end = skipTrivia(typeArguments.end) + 1` (past `>`). Prefer
                        // the first type argument's start - 1 so the column points to
                        // the `<` itself even when whitespace separates `b` from `<`.
                        // Fall back to the expression's end when no args are available.
                        let first_arg_pos = eta
                            .type_arguments
                            .as_ref()
                            .and_then(|list| list.nodes.first())
                            .and_then(|&idx| self.arena.get(idx))
                            .map(|n| n.pos);
                        let err_pos =
                            first_arg_pos
                                .map(|p| p.saturating_sub(1))
                                .unwrap_or_else(|| {
                                    self.arena
                                        .get(eta.expression)
                                        .map_or(node.pos, |expr_node| expr_node.end)
                                });
                        let err_len = node.end.saturating_sub(err_pos);
                        self.parse_error_at(
                            err_pos,
                            err_len,
                            tsz_common::diagnostics::diagnostic_messages::AN_INSTANTIATION_EXPRESSION_CANNOT_BE_FOLLOWED_BY_A_PROPERTY_ACCESS,
                            tsz_common::diagnostics::diagnostic_codes::AN_INSTANTIATION_EXPRESSION_CANNOT_BE_FOLLOWED_BY_A_PROPERTY_ACCESS,
                        );
                    }
                    self.next_token();
                    // Handle both regular identifiers and private identifiers (#name)
                    // Also try rescanning HashToken as PrivateIdentifier.
                    if self.is_token(SyntaxKind::HashToken) {
                        let rescanned = self.scanner.re_scan_hash_token();
                        self.current_token = rescanned;
                    }
                    if self.is_token(SyntaxKind::Unknown) {
                        let rescanned = self.scanner.re_scan_unknown_token_as_identifier_name();
                        self.current_token = rescanned;
                    }
                    let is_private_identifier = self.is_token(SyntaxKind::PrivateIdentifier);
                    let is_optional_chain_continuation =
                        is_private_identifier && self.is_optional_chain_expression(expr);
                    let name = if is_private_identifier {
                        self.parse_private_identifier()
                    } else if self.is_token(SyntaxKind::HashToken) {
                        // Bare `#` after `.` recovers as a private-name-shaped node
                        // so downlevel private-field emit can preserve tsc's tree.
                        self.parse_recovered_bare_hash_private_identifier()
                    } else if self.is_identifier_or_keyword() {
                        // When there's a line break after the dot and the current token
                        // starts a declaration (e.g. `foo.\nvar y = 1;`), don't consume
                        // the token as a property name. Instead, emit TS1003 and create
                        // a missing identifier. This matches tsc's parseRightSideOfDot.
                        if self.scanner.has_preceding_line_break()
                            && self.look_ahead_next_is_identifier_or_keyword_on_same_line()
                        {
                            self.parse_error_at(
                                missing_name_pos,
                                0,
                                "Identifier expected.",
                                tsz_common::diagnostics::diagnostic_codes::IDENTIFIER_EXPECTED,
                            );
                            NodeIndex::NONE
                        } else {
                            self.parse_identifier_name()
                        }
                    } else {
                        // Emit at the current token position (reportAtCurrentPosition: true),
                        // matching tsc's parseRightSideOfDot/createMissingNode behavior.
                        // This ensures the TS1003 error is at the same position as where
                        // parseExpected(CloseParenToken) would emit TS1005, allowing the
                        // duplicate-position suppression to prevent cascading errors.
                        let missing_pos = if self.is_token(SyntaxKind::EndOfFileToken) {
                            missing_name_pos
                        } else {
                            self.token_pos()
                        };
                        if self.is_token(SyntaxKind::Unknown) {
                            self.parse_error_at_current_token(
                                tsz_common::diagnostics::diagnostic_messages::INVALID_CHARACTER,
                                tsz_common::diagnostics::diagnostic_codes::INVALID_CHARACTER,
                            );
                        } else {
                            self.parse_error_at(
                                missing_pos,
                                0,
                                "Identifier expected.",
                                tsz_common::diagnostics::diagnostic_codes::IDENTIFIER_EXPECTED,
                            );
                        }
                        NodeIndex::NONE
                    };
                    if is_optional_chain_continuation && let Some(name_node) = self.arena.get(name)
                    {
                        self.parse_error_at(
                            name_node.pos,
                            name_node.end - name_node.pos,
                            tsz_common::diagnostics::diagnostic_messages::AN_OPTIONAL_CHAIN_CANNOT_CONTAIN_PRIVATE_IDENTIFIERS,
                            tsz_common::diagnostics::diagnostic_codes::AN_OPTIONAL_CHAIN_CANNOT_CONTAIN_PRIVATE_IDENTIFIERS,
                        );
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
                    // In decorator context, `[` starts a computed property name, not element access
                    if (self.context_flags & crate::parser::state::CONTEXT_FLAG_IN_DECORATOR) != 0 {
                        break;
                    }
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
                SyntaxKind::OpenParenToken => {
                    let callee_expr = expr;
                    self.next_token();
                    let arguments = self.parse_argument_list();
                    let end_pos = self.token_end();
                    self.parse_expected(SyntaxKind::CloseParenToken);

                    let is_optional_chain = self
                        .arena
                        .get(callee_expr)
                        .and_then(|callee_node| self.arena.get_access_expr(callee_node))
                        .is_some_and(|access| access.question_dot_token);
                    let call_expr = self.arena.add_call_expr(
                        syntax_kind_ext::CALL_EXPRESSION,
                        start_pos,
                        end_pos,
                        CallExprData {
                            expression: expr,
                            type_arguments: None,
                            arguments: Some(arguments),
                        },
                    );
                    let optional_chain_flag = self.u16_from_node_flags(node_flags::OPTIONAL_CHAIN);
                    if is_optional_chain && let Some(call_node) = self.arena.get_mut(call_expr) {
                        call_node.flags |= optional_chain_flag;
                    }
                    expr = call_expr;
                }
                // Tagged template literals: tag`template` or tag`head${expr}tail`
                SyntaxKind::NoSubstitutionTemplateLiteral | SyntaxKind::TemplateHead => {
                    let template = self.parse_tagged_template_literal();
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
                // Optional chaining: expr?.prop, expr?.[index], expr?.()
                SyntaxKind::QuestionDotToken => {
                    self.next_token();
                    if !self.is_js_file()
                        && self.is_less_than_or_compound()
                        && let Some(type_args) = self.try_parse_type_arguments_for_call()
                    {
                        if self.is_token(SyntaxKind::OpenParenToken) {
                            // expr?.<T>()
                            self.next_token();
                            let arguments = self.parse_argument_list();
                            let end_pos = self.token_end();
                            self.parse_expected(SyntaxKind::CloseParenToken);

                            let call_expr = self.arena.add_call_expr(
                                syntax_kind_ext::CALL_EXPRESSION,
                                start_pos,
                                end_pos,
                                CallExprData {
                                    expression: expr,
                                    type_arguments: Some(type_args),
                                    arguments: Some(arguments),
                                },
                            );
                            let optional_chain_flag =
                                self.u16_from_node_flags(node_flags::OPTIONAL_CHAIN);
                            if let Some(call_node) = self.arena.get_mut(call_expr) {
                                call_node.flags |= optional_chain_flag;
                            }
                            expr = call_expr;
                            continue;
                        } else if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
                            || self.is_token(SyntaxKind::TemplateHead)
                        {
                            let template = self.parse_tagged_template_literal();
                            let end_pos = self.token_end();

                            expr = self.arena.add_tagged_template(
                                syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION,
                                start_pos,
                                end_pos,
                                TaggedTemplateData {
                                    tag: expr,
                                    type_arguments: Some(type_args),
                                    template,
                                },
                            );
                            continue;
                        }
                        // expr?.<T> not followed by `(` or a template literal.
                        // tsc emits TS1005 ('(' expected) here. Do NOT fall
                        // through to the property-access path, which would call
                        // parse_identifier_name() and emit the spurious TS1003.
                        self.parse_expected(SyntaxKind::OpenParenToken);
                        let call_expr = self.arena.add_call_expr(
                            syntax_kind_ext::CALL_EXPRESSION,
                            start_pos,
                            self.token_pos(),
                            CallExprData {
                                expression: expr,
                                type_arguments: Some(type_args),
                                arguments: Some(self.make_node_list(Vec::new())),
                            },
                        );
                        let optional_chain_flag =
                            self.u16_from_node_flags(node_flags::OPTIONAL_CHAIN);
                        if let Some(call_node) = self.arena.get_mut(call_expr) {
                            call_node.flags |= optional_chain_flag;
                        }
                        expr = call_expr;
                        continue;
                    }
                    if self.is_token(SyntaxKind::OpenBracketToken) {
                        // expr?.[index]
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
                    } else if self.is_token(SyntaxKind::OpenParenToken) {
                        // expr?.()
                        self.next_token();
                        let arguments = self.parse_argument_list();
                        let end_pos = self.token_end();
                        self.parse_expected(SyntaxKind::CloseParenToken);

                        let call_expr = self.arena.add_call_expr(
                            syntax_kind_ext::CALL_EXPRESSION,
                            start_pos,
                            end_pos,
                            CallExprData {
                                expression: expr,
                                type_arguments: None,
                                arguments: Some(arguments),
                            },
                        );
                        let optional_chain_flag =
                            self.u16_from_node_flags(node_flags::OPTIONAL_CHAIN);
                        if let Some(call_node) = self.arena.get_mut(call_expr) {
                            call_node.flags |= optional_chain_flag;
                        }
                        expr = call_expr;
                    } else if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
                        || self.is_token(SyntaxKind::TemplateHead)
                    {
                        // expr?.`template` — tagged template in optional chain is not allowed.
                        // tsc emits TS1358 and still parses the tagged template expression.
                        use tsz_common::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.parse_error_at_current_token(
                            diagnostic_messages::TAGGED_TEMPLATE_EXPRESSIONS_ARE_NOT_PERMITTED_IN_AN_OPTIONAL_CHAIN,
                            diagnostic_codes::TAGGED_TEMPLATE_EXPRESSIONS_ARE_NOT_PERMITTED_IN_AN_OPTIONAL_CHAIN,
                        );
                        let template = self.parse_tagged_template_literal();
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
                        continue;
                    } else {
                        // expr?.prop
                        let is_private_identifier = self.is_token(SyntaxKind::PrivateIdentifier);
                        let name = if is_private_identifier {
                            self.parse_private_identifier()
                        } else {
                            self.parse_identifier_name()
                        };

                        // TS18030: Optional chain cannot contain private identifiers
                        if is_private_identifier && let Some(name_node) = self.arena.get(name) {
                            self.parse_error_at(
                                    name_node.pos,
                                    name_node.end - name_node.pos,
                                    "An optional chain cannot contain private identifiers.",
                                    tsz_common::diagnostics::diagnostic_codes::AN_OPTIONAL_CHAIN_CANNOT_CONTAIN_PRIVATE_IDENTIFIERS,
                                );
                        }

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
                // Non-null assertion: expr!
                SyntaxKind::ExclamationToken => {
                    // Non-null assertion only if no line break before
                    if self.scanner.has_preceding_line_break() {
                        break;
                    }
                    self.next_token();
                    let end_pos = self.token_end();

                    expr = self.arena.add_unary_expr_ex(
                        syntax_kind_ext::NON_NULL_EXPRESSION,
                        start_pos,
                        end_pos,
                        crate::parser::node::UnaryExprDataEx {
                            expression: expr,
                            asterisk_token: false,
                        },
                    );
                }
                // Type arguments followed by call: expr<T>() or expr<T, U>()
                // Also handles `<<` for nested generics: foo<<T>(x: T) => number>(fn)
                SyntaxKind::LessThanToken | SyntaxKind::LessThanLessThanToken => {
                    if self.is_js_file() {
                        break;
                    }
                    if self
                        .arena
                        .get(expr)
                        .is_some_and(|node| node.kind == SyntaxKind::SuperKeyword as u16)
                    {
                        let type_arg_start = self.token_pos();
                        let type_args = self.parse_type_arguments();
                        let type_arg_end = self.token_full_start();
                        self.parse_error_at(
                            type_arg_start,
                            (type_arg_end.saturating_sub(type_arg_start)).max(1),
                            tsz_common::diagnostics::diagnostic_messages::SUPER_MAY_NOT_USE_TYPE_ARGUMENTS,
                            tsz_common::diagnostics::diagnostic_codes::SUPER_MAY_NOT_USE_TYPE_ARGUMENTS,
                        );
                        if self.is_token(SyntaxKind::OpenParenToken) {
                            self.next_token();
                            let arguments = self.parse_argument_list();
                            let end_pos = self.token_end();
                            self.parse_expected(SyntaxKind::CloseParenToken);
                            expr = self.arena.add_call_expr(
                                syntax_kind_ext::CALL_EXPRESSION,
                                start_pos,
                                end_pos,
                                CallExprData {
                                    expression: expr,
                                    type_arguments: Some(type_args),
                                    arguments: Some(arguments),
                                },
                            );
                        } else if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
                            || self.is_token(SyntaxKind::TemplateHead)
                        {
                            self.parse_error_at_current_token(
                                tsz_common::diagnostics::diagnostic_messages::SUPER_MUST_BE_FOLLOWED_BY_AN_ARGUMENT_LIST_OR_MEMBER_ACCESS,
                                tsz_common::diagnostics::diagnostic_codes::SUPER_MUST_BE_FOLLOWED_BY_AN_ARGUMENT_LIST_OR_MEMBER_ACCESS,
                            );
                            let template = self.parse_tagged_template_literal();
                            let end_pos = self.token_end();

                            expr = self.arena.add_tagged_template(
                                syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION,
                                start_pos,
                                end_pos,
                                TaggedTemplateData {
                                    tag: expr,
                                    type_arguments: Some(type_args),
                                    template,
                                },
                            );
                        } else if !self.is_token(SyntaxKind::DotToken)
                            && !self.is_token(SyntaxKind::OpenBracketToken)
                        {
                            // TS1034: super<T> followed by something other than
                            // call/member access (e.g., tagged template literal)
                            self.parse_error_at_current_token(
                                tsz_common::diagnostics::diagnostic_messages::SUPER_MUST_BE_FOLLOWED_BY_AN_ARGUMENT_LIST_OR_MEMBER_ACCESS,
                                tsz_common::diagnostics::diagnostic_codes::SUPER_MUST_BE_FOLLOWED_BY_AN_ARGUMENT_LIST_OR_MEMBER_ACCESS,
                            );
                        }
                        continue;
                    }

                    // Try to parse as type arguments for a call expression
                    // This is tricky because < could be comparison operator
                    if let Some(type_args) = self.try_parse_type_arguments_for_call() {
                        // After type arguments, we expect ( for a call or ` for tagged template
                        if self.is_token(SyntaxKind::OpenParenToken) {
                            self.next_token();
                            let arguments = self.parse_argument_list();
                            let end_pos = self.token_end();
                            self.parse_expected(SyntaxKind::CloseParenToken);

                            expr = self.arena.add_call_expr(
                                syntax_kind_ext::CALL_EXPRESSION,
                                start_pos,
                                end_pos,
                                CallExprData {
                                    expression: expr,
                                    type_arguments: Some(type_args),
                                    arguments: Some(arguments),
                                },
                            );
                        } else if self.is_token(SyntaxKind::NoSubstitutionTemplateLiteral)
                            || self.is_token(SyntaxKind::TemplateHead)
                        {
                            // Tagged template with type arguments: tag<T>`template`
                            let template = self.parse_tagged_template_literal();
                            let end_pos = self.token_end();

                            expr = self.arena.add_tagged_template(
                                syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION,
                                start_pos,
                                end_pos,
                                TaggedTemplateData {
                                    tag: expr,
                                    type_arguments: Some(type_args),
                                    template,
                                },
                            );
                        } else {
                            // Not a call or tagged template - this is an instantiation expression
                            // (e.g., f<string>, new Foo<number>, a<b>?.())
                            let end_pos = self.token_end();
                            expr = self.arena.add_expr_with_type_args(
                                crate::parser::syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS,
                                start_pos,
                                end_pos,
                                crate::parser::node::ExprWithTypeArgsData {
                                    expression: expr,
                                    type_arguments: Some(type_args),
                                },
                            );
                        }
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }

        expr
    }
}
