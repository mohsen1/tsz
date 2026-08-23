use super::super::{
    AuthoredLiteralFact, AuthoredLiteralKind, ClassDeclaration, Expression, ExpressionKind,
    Literal, ObjectProperty, Parameter, ParserRecoveryFact, ParserRecoveryKind, PropertyNameKind,
    Statement, StringLiteral, Token, TokenKind, class_contains_no_substitution_template,
    comments_form_no_substitution_template_expression_file,
    expression_contains_no_substitution_template,
    statements_form_no_substitution_template_safe_file,
};
use super::Parser;
use crate::diagnostics::Diagnostic;
use crate::source::Span;

impl Parser<'_> {
    pub(super) fn authored_literal_facts(
        &self,
        statements: &[Statement],
        parser_recovery_facts: &[ParserRecoveryFact],
    ) -> Vec<AuthoredLiteralFact> {
        let recovery_spans = self
            .numeric_literals
            .iter()
            .map(|literal| (literal.span.start, literal.span.end))
            .collect::<std::collections::BTreeSet<_>>();
        let separator_spans = self
            .numeric_separator_spans
            .iter()
            .map(|span| (span.start, span.end))
            .collect::<std::collections::BTreeSet<_>>();
        let mut facts = Vec::new();
        let mut template_starts = Vec::new();
        for (token_index, token) in self.tokens.iter().enumerate() {
            match token.kind {
                TokenKind::NoSubstitutionTemplateLiteral => facts.push(AuthoredLiteralFact {
                    span: token.span,
                    recovery_extent: token.span,
                    kind: AuthoredLiteralKind::Template,
                    owner: self.authored_literal_owner(
                        statements,
                        parser_recovery_facts,
                        token.span,
                    ),
                }),
                TokenKind::TemplateHead => template_starts.push(token.span),
                TokenKind::TemplateTail => {
                    if let Some(start) = template_starts.pop() {
                        facts.push(AuthoredLiteralFact {
                            span: start.merge(token.span),
                            recovery_extent: start.merge(token.span),
                            kind: AuthoredLiteralKind::Template,
                            owner: self.authored_literal_owner(
                                statements,
                                parser_recovery_facts,
                                start,
                            ),
                        });
                    }
                }
                _ => {}
            }
            if recovery_spans.contains(&(token.span.start, token.span.end)) {
                facts.push(AuthoredLiteralFact {
                    span: token.span,
                    recovery_extent: self.attached_numeric_recovery_extent(token_index),
                    kind: AuthoredLiteralKind::NumericRecovery,
                    owner: self.authored_literal_owner(
                        statements,
                        parser_recovery_facts,
                        token.span,
                    ),
                });
            }
            if separator_spans.contains(&(token.span.start, token.span.end)) {
                facts.push(AuthoredLiteralFact {
                    span: token.span,
                    recovery_extent: self.attached_numeric_recovery_extent(token_index),
                    kind: AuthoredLiteralKind::NumericSeparator,
                    owner: self.authored_literal_owner(
                        statements,
                        parser_recovery_facts,
                        token.span,
                    ),
                });
            }
        }
        let end = self.source.text.len();
        facts.extend(
            template_starts
                .into_iter()
                .map(|start| AuthoredLiteralFact {
                    span: start.merge(Span::new(self.source.id, end, end)),
                    recovery_extent: start.merge(Span::new(self.source.id, end, end)),
                    kind: AuthoredLiteralKind::Template,
                    owner: self.authored_literal_owner(statements, parser_recovery_facts, start),
                }),
        );
        facts.sort_unstable_by_key(|fact| {
            (
                fact.kind,
                fact.span.start,
                fact.span.end,
                fact.recovery_extent.start,
                fact.recovery_extent.end,
            )
        });
        facts.dedup_by_key(|fact| {
            (
                fact.kind,
                fact.span.start,
                fact.span.end,
                fact.recovery_extent.start,
                fact.recovery_extent.end,
            )
        });
        facts
    }

    fn authored_literal_owner(
        &self,
        statements: &[Statement],
        parser_recovery_facts: &[ParserRecoveryFact],
        span: Span,
    ) -> super::super::ParserRecoveryOwner {
        parser_recovery_facts
            .iter()
            .find(|fact| fact.authored_span.start == span.start)
            .map(|fact| fact.owner)
            .or_else(|| super::recovery::recovery_owner(statements, span))
            .expect("a scanner-authored literal token must have a represented statement owner")
    }

    /// Extend a recovered numeric token through the parser's same-line
    /// recovery segment. Explicit statement terminators, a closed block, and
    /// line boundaries end the syntax-owned extent.
    fn attached_numeric_recovery_extent(&self, token_index: usize) -> Span {
        let span = self.tokens[token_index].span;
        let mut end = span.end;
        for (index, token) in self.tokens.iter().enumerate().skip(token_index + 1) {
            if matches!(token.kind, TokenKind::Semicolon | TokenKind::EndOfFile)
                || !self.tokens_are_on_same_line(index - 1, index)
            {
                break;
            }
            end = token.span.end;
            if token.kind == TokenKind::RightBrace {
                break;
            }
        }
        Span {
            file: span.file,
            start: span.start,
            end,
        }
    }

    pub(super) fn parse_primary_expression(&mut self) -> Expression {
        let token = *self.current();
        match token.kind {
            _ if matches!(
                token.kind,
                TokenKind::Import | TokenKind::This | TokenKind::Super
            ) || token.kind.is_identifier() =>
            {
                self.bump();
                let name = self.text(token.span).to_string();
                if self.eat(TokenKind::FatArrow) {
                    let parameter = Parameter {
                        name,
                        name_span: token.span,
                        annotation: None,
                        initializer: None,
                        optional: false,
                        optional_span: None,
                        rest: false,
                        rest_span: None,
                        modifiers: Vec::new(),
                        overload_completion_supported: token.kind == TokenKind::Identifier,
                        function_implementation_completion_supported: token.kind.is_identifier(),
                        span: token.span,
                    };
                    let body = self.parse_arrow_body();
                    let end = self.previous().span;
                    return Expression {
                        id: self.alloc_node(),
                        span: token.span.merge(end),
                        kind: ExpressionKind::Arrow {
                            parameters: vec![parameter],
                            return_type: None,
                            body,
                        },
                    };
                }
                Expression {
                    id: self.alloc_node(),
                    span: token.span,
                    kind: if token.kind == TokenKind::This {
                        ExpressionKind::This
                    } else {
                        ExpressionKind::Identifier {
                            name,
                            name_span: token.span,
                            entity_name: token.kind.is_identifier(),
                        }
                    },
                }
            }
            TokenKind::New => self.parse_new_expression(),
            TokenKind::True
            | TokenKind::False
            | TokenKind::Null
            | TokenKind::NumericLiteral
            | TokenKind::BigIntLiteral
            | TokenKind::StringLiteral => {
                self.bump();
                Expression {
                    id: self.alloc_node(),
                    span: token.span,
                    kind: ExpressionKind::Literal(self.literal_from(token)),
                }
            }
            TokenKind::NoSubstitutionTemplateLiteral => {
                self.parse_no_substitution_template_literal()
            }
            TokenKind::RegularExpressionLiteral => self.parse_regular_expression_literal(),
            TokenKind::LeftBrace => self.parse_object_literal(),
            TokenKind::LeftBracket => self.parse_array_literal(),
            TokenKind::LeftParen if self.paren_expression_is_arrow() => {
                let left = self.bump().span;
                let mut parameters = Vec::new();
                while !self.at_any(&[TokenKind::RightParen, TokenKind::EndOfFile]) {
                    parameters.push(self.parse_parameter());
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::RightParen, "')' expected.", 1005);
                let return_type = self.eat(TokenKind::Colon).then(|| self.parse_type());
                self.expect(TokenKind::FatArrow, "'=>' expected.", 1005);
                let body = self.parse_arrow_body();
                let end = self.previous().span;
                Expression {
                    id: self.alloc_node(),
                    span: left.merge(end),
                    kind: ExpressionKind::Arrow {
                        parameters,
                        return_type,
                        body,
                    },
                }
            }
            TokenKind::LeftParen => {
                let left = self.bump().span;
                let inner = self.parse_expression();
                let right = self.current().span;
                self.expect(TokenKind::RightParen, "')' expected.", 1005);
                Expression {
                    id: self.alloc_node(),
                    span: left.merge(right),
                    kind: ExpressionKind::Parenthesized(Box::new(inner)),
                }
            }
            _ => {
                self.observe_unmodeled_regular_expression_if_current();
                self.observe_unmodeled_template_if_current();
                let recovery_extent = self.recovery_extent_from_current(token.span);
                self.retain_parser_recovery(
                    ParserRecoveryKind::Expression,
                    token.span,
                    recovery_extent,
                );
                self.error_current("Expression expected.", 1109);
                self.bump();
                Expression {
                    id: self.alloc_node(),
                    span: token.span,
                    kind: ExpressionKind::Missing,
                }
            }
        }
    }

    fn parse_object_literal(&mut self) -> Expression {
        let left = self.bump().span;
        let mut properties = Vec::new();
        while !self.at_any(&[TokenKind::RightBrace, TokenKind::EndOfFile]) {
            let start = self.current().span;
            let (name, name_span, name_kind) = self.parse_property_name();
            let has_colon = self.eat(TokenKind::Colon);
            let shorthand = !has_colon && name_kind == PropertyNameKind::Identifier;
            let value = if has_colon {
                self.parse_expression()
            } else {
                Expression {
                    id: self.alloc_node(),
                    span: name_span,
                    kind: ExpressionKind::Identifier {
                        name: name.clone(),
                        name_span,
                        entity_name: true,
                    },
                }
            };
            let (value, shorthand_equals_span) = if shorthand && self.at(TokenKind::Equals) {
                let equals_span = self.bump().span;
                let right = self.parse_assignment_expression();
                (
                    Expression {
                        id: self.alloc_node(),
                        span: value.span.merge(right.span),
                        kind: ExpressionKind::Assignment {
                            left: Box::new(value),
                            right: Box::new(right),
                        },
                    },
                    Some(equals_span),
                )
            } else {
                (value, None)
            };
            let span = start.merge(value.span);
            properties.push(ObjectProperty {
                name,
                name_span,
                shorthand,
                shorthand_equals_span,
                value,
                span,
            });
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        let right = self.current().span;
        self.expect(TokenKind::RightBrace, "'}' expected.", 1005);
        Expression {
            id: self.alloc_node(),
            span: left.merge(right),
            kind: ExpressionKind::Object(properties),
        }
    }

    pub(super) fn finish_no_substitution_template_source(
        &mut self,
        statements: &[Statement],
    ) -> bool {
        let has_authored_template = !self.template_literals.is_empty();
        let valid_template_count = self
            .template_literals
            .iter()
            .filter(|literal| literal.syntax_literal().is_some())
            .count();
        if has_authored_template
            && (!self.diagnostics.is_empty()
                || self.has_unmodeled_trivia
                || self.has_unmodeled_top_level_syntax
                || !comments_form_no_substitution_template_expression_file(
                    self.source,
                    statements,
                    &self.comments,
                )
                || !statements_form_no_substitution_template_safe_file(
                    self.source,
                    statements,
                    valid_template_count,
                ))
        {
            self.product_capabilities.observe_unmodeled_template();
        }
        has_authored_template
    }

    pub(super) fn observe_template_expression_semantics(&mut self, expression: &Expression) {
        if expression_contains_no_substitution_template(expression) {
            self.product_capabilities.observe_unmodeled_template();
        }
    }

    pub(super) fn observe_class_template_semantics(&mut self, declaration: &ClassDeclaration) {
        if class_contains_no_substitution_template(declaration) {
            self.product_capabilities.observe_unmodeled_template();
        }
    }

    pub(super) fn observe_unmodeled_template_tail(&mut self, expression: &Expression) {
        if !expression_contains_no_substitution_template(expression)
            || self.at_any(&[
                TokenKind::Semicolon,
                TokenKind::RightBrace,
                TokenKind::RightParen,
                TokenKind::RightBracket,
                TokenKind::EndOfFile,
            ])
        {
            return;
        }
        let is_postfix_continuation = matches!(
            self.kind(),
            TokenKind::LeftBracket
                | TokenKind::LeftParen
                | TokenKind::Dot
                | TokenKind::QuestionDot
                | TokenKind::Satisfies
        );
        if !is_postfix_continuation
            && !self.tokens_are_on_same_line(self.index.saturating_sub(1), self.index)
        {
            return;
        }
        self.product_capabilities.observe_unmodeled_template();
        let recovery_extent = self.recovery_extent_from_current(expression.span);
        self.retain_parser_recovery(
            ParserRecoveryKind::Template,
            expression.span,
            recovery_extent,
        );
    }

    pub(super) fn parse_new_expression(&mut self) -> Expression {
        let left = self.bump().span;
        let mut callee = self.parse_primary_expression();
        while self.at_any(&[TokenKind::Dot, TokenKind::LeftBracket]) {
            callee = self.parse_member_access(callee);
        }
        let has_type_arguments = self.at(TokenKind::LessThan);
        let type_arguments = if has_type_arguments {
            self.parse_type_arguments()
        } else {
            Vec::new()
        };
        let mut arguments = Vec::new();
        let mut end = callee.span;
        if self.eat(TokenKind::LeftParen) {
            while !self.at_any(&[TokenKind::RightParen, TokenKind::EndOfFile]) {
                arguments.push(self.parse_expression());
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            end = self.current().span;
            self.expect(TokenKind::RightParen, "')' expected.", 1005);
        }
        if has_type_arguments {
            self.product_capabilities
                .observe_unmodeled_expression_products();
        }
        let expression = Expression {
            id: self.alloc_node(),
            span: left.merge(end),
            kind: ExpressionKind::New {
                callee: Box::new(callee),
                type_arguments,
                arguments,
            },
        };
        self.observe_template_expression_semantics(&expression);
        expression
    }

    pub(super) fn parse_unsupported_await_template(&mut self) -> Option<Expression> {
        if !self.at(TokenKind::Await)
            || !matches!(
                self.peek_kind(1),
                TokenKind::NoSubstitutionTemplateLiteral | TokenKind::TemplateHead
            )
        {
            return None;
        }
        let await_token = self.bump();
        self.product_capabilities.observe_unmodeled_template();
        let template = *self.current();
        let authored_span = await_token.span.merge(template.span);
        let recovery_extent = self.recovery_extent_from_current(authored_span);
        self.retain_parser_recovery(ParserRecoveryKind::Template, authored_span, recovery_extent);
        self.bump();
        Some(Expression {
            id: self.alloc_node(),
            span: await_token.span.merge(template.span),
            kind: ExpressionKind::Missing,
        })
    }

    pub(super) fn consume_non_null_postfix(&mut self) -> bool {
        if !self.source.kind().supports_expression_type_arguments()
            || !self.at(TokenKind::Bang)
            || !self.tokens_are_on_same_line(self.index.saturating_sub(1), self.index)
        {
            return false;
        }
        let tagged = matches!(
            self.peek_kind(1),
            TokenKind::NoSubstitutionTemplateLiteral | TokenKind::TemplateHead
        );
        let bang = self.bump().span;
        if !tagged {
            self.retain_parser_recovery(ParserRecoveryKind::Expression, bang, bang);
        }
        true
    }

    pub(super) fn observe_unmodeled_non_null_template_adjacency(&mut self) {
        if self.at(TokenKind::Bang)
            && matches!(
                self.peek_kind(1),
                TokenKind::NoSubstitutionTemplateLiteral | TokenKind::TemplateHead
            )
        {
            // JavaScript has no non-null assertion. Fail closed instead of
            // later graduating the template as an unrelated statement.
            self.product_capabilities.observe_unmodeled_template();
        }
    }

    pub(super) fn parse_no_substitution_template_literal(&mut self) -> Expression {
        let token = *self.current();
        let metadata = self
            .template_literals
            .binary_search_by_key(&token.span.start, |literal| literal.span.start)
            .ok()
            .map(|index| &self.template_literals[index]);
        let literal = metadata.and_then(|metadata| metadata.syntax_literal());
        let escape_diagnostic = metadata.and_then(|metadata| metadata.escape_diagnostic());
        let Some(literal) = literal else {
            self.product_capabilities.observe_unmodeled_template();
            let recovery_extent = self.recovery_extent_from_current(token.span);
            self.retain_parser_recovery(ParserRecoveryKind::Template, token.span, recovery_extent);
            if let Some(diagnostic) = escape_diagnostic {
                let start = token.span.start + diagnostic.relative_start;
                self.diagnostics.push(Diagnostic::at(
                    self.source,
                    Span {
                        file: token.span.file,
                        start,
                        end: start + diagnostic.length,
                    },
                    diagnostic.message,
                    diagnostic.code,
                ));
            }
            self.bump();
            return Expression {
                id: self.alloc_node(),
                span: token.span,
                kind: ExpressionKind::Missing,
            };
        };
        self.bump();
        Expression {
            id: self.alloc_node(),
            span: token.span,
            kind: ExpressionKind::Literal(Literal::NoSubstitutionTemplate(literal)),
        }
    }

    pub(super) fn reject_tagged_template(&mut self, tag_span: Span) -> bool {
        if !matches!(
            self.kind(),
            TokenKind::NoSubstitutionTemplateLiteral | TokenKind::TemplateHead
        ) {
            return false;
        }
        self.product_capabilities.observe_unmodeled_template();
        let recovery_extent = self.recovery_extent_from_current(tag_span);
        self.retain_parser_recovery(ParserRecoveryKind::Template, tag_span, recovery_extent);
        self.bump();
        true
    }

    pub(super) fn observe_unmodeled_template_if_current(&mut self) {
        if matches!(
            self.kind(),
            TokenKind::NoSubstitutionTemplateLiteral
                | TokenKind::TemplateHead
                | TokenKind::TemplateMiddle
                | TokenKind::TemplateTail
        ) {
            self.product_capabilities.observe_unmodeled_template();
            let authored_span = self.current().span;
            let recovery_extent = self.recovery_extent_from_current(authored_span);
            self.retain_parser_recovery(
                ParserRecoveryKind::Template,
                authored_span,
                recovery_extent,
            );
        }
    }

    pub(super) fn literal_from(&self, token: Token) -> Literal {
        match token.kind {
            TokenKind::True => Literal::Boolean(true),
            TokenKind::False => Literal::Boolean(false),
            TokenKind::Null => Literal::Null,
            TokenKind::StringLiteral => {
                Literal::String(self.extended_unicode_string_literal(token).map_or_else(
                    || StringLiteral::Plain(self.ordinary_string_literal_value(token)),
                    StringLiteral::Extended,
                ))
            }
            TokenKind::BigIntLiteral => Literal::BigInt(self.text(token.span).to_string()),
            _ => Literal::Number(self.number_literal(token)),
        }
    }

    pub(super) fn parse_module_specifier(&mut self) -> (String, Span) {
        let token = *self.current();
        if token.kind == TokenKind::StringLiteral {
            self.bump();
            (self.ordinary_string_literal_value(token), token.span)
        } else {
            self.error_current("String literal expected.", 1141);
            self.bump();
            (String::new(), token.span)
        }
    }
}

pub(super) fn unquote(text: &str) -> String {
    if text.len() >= 2 {
        let first = text.as_bytes()[0];
        let last = text.as_bytes()[text.len() - 1];
        if (first == b'\'' || first == b'"') && first == last {
            return text[1..text.len() - 1].to_string();
        }
    }
    text.to_string()
}
