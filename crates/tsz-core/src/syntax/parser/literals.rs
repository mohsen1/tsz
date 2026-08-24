use super::super::{
    AuthoredLiteralFact, AuthoredLiteralKind, ClassDeclaration, Expression, ExpressionKind,
    FunctionLikeExpression, FunctionLikeSyntax, Literal, ObjectProperty, Parameter,
    ParameterNameKind, ParserRecoveryFact, ParserRecoveryKind, PropertyNameKind, SourceSyntaxFact,
    Statement, StringLiteral, Token, TokenKind, class_contains_no_substitution_template,
    comments_form_no_substitution_template_expression_file,
    expression_contains_no_substitution_template,
    statements_form_no_substitution_template_safe_file,
};
use super::{ParenthesizedArrowToken, Parser, parameters::parameter_modifier};
use crate::diagnostics::Diagnostic;
use crate::source::{SourceKind, Span};

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
        let async_arrow = token.kind == TokenKind::Async
            && self.peek_kind(1) == TokenKind::LeftParen
            && self.tokens_are_on_same_line(self.index, self.index + 1)
            && self.async_parenthesized_arrow_is_arrow();
        let async_function = token.kind == TokenKind::Async
            && self.peek_kind(1) == TokenKind::Function
            && self.tokens_are_on_same_line(self.index, self.index + 1);
        if async_function
            || token.kind == TokenKind::Function && self.peek_kind(1) == TokenKind::Star
        {
            self.source_syntax_facts
                .insert(SourceSyntaxFact::AuthoredFunctionExpressionModifier);
        }
        if async_function {
            let recovery_extent = self.recovery_extent_from_current(token.span);
            self.retain_parser_recovery(
                ParserRecoveryKind::Expression,
                token.span,
                recovery_extent,
            );
        }
        if token.kind == TokenKind::Yield && self.current_is_inside_recovered_generator() {
            self.retain_parser_recovery(
                ParserRecoveryKind::Expression,
                token.span,
                self.recovery_extent_from_current(token.span),
            );
        }
        match token.kind {
            TokenKind::LessThan if self.generic_arrow_is_parenthesized_arrow() => {
                self.parse_parenthesized_arrow(true)
            }
            TokenKind::LessThan if self.current_starts_rejected_generic_arrow_prefix() => {
                self.parse_rejected_generic_arrow_type_assertion()
            }
            TokenKind::LessThan if self.source.kind() == SourceKind::TypeScript => {
                self.parse_type_assertion()
            }
            TokenKind::Async if async_function || async_arrow => {
                let modifier = self.bump().span;
                let mut expression = if async_function {
                    self.parse_function_expression()
                } else {
                    self.parse_parenthesized_arrow(false)
                };
                expression.span = modifier.merge(expression.span);
                self.retain_parser_recovery(
                    ParserRecoveryKind::Expression,
                    modifier,
                    expression.span,
                );
                expression
            }
            TokenKind::Function => self.parse_function_expression(),
            TokenKind::Class => self.parse_unsupported_class_expression(),
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
                        recovered_binding_names: Vec::new(),
                        name_kind: ParameterNameKind::Binding,
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
                        kind: ExpressionKind::FunctionLike(Box::new(FunctionLikeExpression {
                            type_parameters: Vec::new(),
                            parameters: vec![parameter],
                            return_type: None,
                            syntax: FunctionLikeSyntax::Arrow(body),
                        })),
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
            TokenKind::TemplateHead => self.parse_unsupported_template_expression(),
            TokenKind::RegularExpressionLiteral => self.parse_regular_expression_literal(),
            TokenKind::LeftBrace => self.parse_object_literal(),
            TokenKind::LeftBracket => self.parse_array_literal(),
            TokenKind::LeftParen if self.primary_parenthesized_arrow_index().is_some() => {
                self.parse_parenthesized_arrow(false)
            }
            TokenKind::LeftParen => {
                let left = self.bump().span;
                let inner = if self.at(TokenKind::RightParen) {
                    self.error_current("Expression expected.", 1109);
                    Expression {
                        id: self.alloc_node(),
                        span: self.current().span,
                        kind: ExpressionKind::Missing,
                    }
                } else {
                    self.parse_expression()
                };
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

    fn async_parenthesized_arrow_is_arrow(&mut self) -> bool {
        let saved = self.index;
        self.index += 1;
        let arrow = self.primary_parenthesized_arrow_index().is_some();
        self.index = saved;
        arrow
    }

    fn parse_rejected_generic_arrow_type_assertion(&mut self) -> Expression {
        let left = self.bump().span;
        let ty = self.parse_type();
        self.expect_type_close();
        let missing = Expression {
            id: self.alloc_node(),
            span: self.current().span,
            kind: ExpressionKind::Missing,
        };
        Expression {
            id: self.alloc_node(),
            span: left.merge(ty.span),
            kind: ExpressionKind::As {
                expression: Box::new(missing),
                ty,
            },
        }
    }

    fn parse_type_assertion(&mut self) -> Expression {
        let left = self.bump().span;
        let ty = self.parse_type();
        self.expect_type_close();
        let expression = self.parse_unary_expression();
        let span = left.merge(expression.span);
        let assertion = Expression {
            id: self.alloc_node(),
            span,
            kind: ExpressionKind::As {
                expression: Box::new(expression),
                ty,
            },
        };
        self.retain_parser_recovery(ParserRecoveryKind::AngleAssertion, left, span);
        assertion
    }

    fn primary_parenthesized_arrow_index(&mut self) -> Option<usize> {
        if self.parenthesis_follows_recovered_generic_prefix()
            || self.parenthesis_continues_recovered_function_declaration()
        {
            return None;
        }
        self.parenthesized_arrow_head_certainty()
            .and_then(|definite| self.paren_expression_arrow_token(definite))
            .map(|token| match token {
                ParenthesizedArrowToken::Present(index) => index,
                ParenthesizedArrowToken::Missing => self.index,
            })
    }

    fn parenthesized_arrow_head_certainty(&self) -> Option<bool> {
        match self.peek_kind(1) {
            TokenKind::RightParen
                if matches!(
                    self.peek_kind(2),
                    TokenKind::FatArrow | TokenKind::Colon | TokenKind::LeftBrace
                ) =>
            {
                Some(true)
            }
            TokenKind::RightParen => None,
            TokenKind::DotDotDot => Some(true),
            kind if kind != TokenKind::Async
                && parameter_modifier(kind).is_some()
                && self.peek_kind(2).is_identifier()
                && (kind != TokenKind::Const
                    || matches!(
                        self.peek_kind(3),
                        TokenKind::Colon
                            | TokenKind::Question
                            | TokenKind::Equals
                            | TokenKind::Comma
                            | TokenKind::RightParen
                    )) =>
            {
                (self.peek_kind(2) != TokenKind::As).then_some(true)
            }
            kind if kind.is_identifier() || kind == TokenKind::This => match self.peek_kind(2) {
                TokenKind::Colon => Some(true),
                TokenKind::Question
                    if matches!(
                        self.peek_kind(3),
                        TokenKind::Colon
                            | TokenKind::Comma
                            | TokenKind::Equals
                            | TokenKind::RightParen
                    ) =>
                {
                    Some(true)
                }
                TokenKind::Comma | TokenKind::Equals | TokenKind::RightParen => Some(false),
                _ => None,
            },
            TokenKind::LeftBrace | TokenKind::LeftBracket => Some(false),
            _ => None,
        }
    }

    fn parse_parenthesized_arrow(&mut self, generic: bool) -> Expression {
        let diagnostic_count = self.diagnostics.len();
        let left = self.current().span;
        let type_parameters = if generic {
            self.parse_type_parameters()
        } else {
            Vec::new()
        };
        let arrow_token = self.paren_expression_arrow_token(true);
        let arrow_index = match arrow_token {
            Some(ParenthesizedArrowToken::Present(index)) => Some(index),
            Some(ParenthesizedArrowToken::Missing) | None => None,
        };
        let parameters = self.parse_arrow_parameters();
        let return_type = self.eat(TokenKind::Colon).then(|| {
            let token = *self.current();
            if token.kind == TokenKind::FatArrow && arrow_index == Some(self.index) {
                self.recover_missing_type(token, false)
            } else {
                self.parse_type()
            }
        });
        self.expect(TokenKind::FatArrow, "'=>' expected.", 1005);
        let header_diagnostic_recovered = self.diagnostics.len() != diagnostic_count;
        if header_diagnostic_recovered && let Some(arrow_index) = arrow_index {
            self.index = arrow_index + 1;
        }
        let body_diagnostic_count = self.diagnostics.len();
        let body_recovery_count = self.parser_recovery_facts.len();
        let authored_body_extent = self
            .at(TokenKind::LeftBrace)
            .then(|| self.balanced_recovery_brace_extent(self.index))
            .flatten();
        let body = self.parse_recovered_arrow_body(arrow_index.is_some());
        let body_recovered = self.diagnostics.len() != body_diagnostic_count
            || self.parser_recovery_facts.len() != body_recovery_count;
        if body_recovered && let Some(extent) = authored_body_extent {
            while self.current().span.start < extent.end {
                self.bump();
            }
        }
        let span = left.merge(self.previous().span);
        let expression = Expression {
            id: self.alloc_node(),
            span,
            kind: ExpressionKind::FunctionLike(Box::new(FunctionLikeExpression {
                type_parameters,
                parameters,
                return_type,
                syntax: FunctionLikeSyntax::Arrow(body),
            })),
        };
        if header_diagnostic_recovered || body_recovered {
            self.retain_parser_recovery(ParserRecoveryKind::Expression, left, span);
        }
        expression
    }

    fn generic_arrow_is_parenthesized_arrow(&mut self) -> bool {
        if !self.source.kind().supports_expression_type_arguments()
            || !self.at(TokenKind::LessThan)
            || !self.generic_arrow_is_unambiguous_in_jsx()
            || self.current_continues_recovered_function_declaration()
        {
            return false;
        }
        let saved_index = self.index;
        let saved_next_node = self.next_node;
        let saved_diagnostics = self.diagnostics.len();
        let saved_speculating = self.speculating;
        let saved_rewrites = self.speculative_token_rewrites.len();
        self.speculating = true;
        let type_parameters = self.parse_type_parameters();
        let parenthesized = self.at(TokenKind::LeftParen);
        let allow_missing_return = self.source.kind() == SourceKind::TypeScriptJsx;
        let arrow = parenthesized
            .then(|| self.paren_expression_arrow_token(allow_missing_return))
            .flatten();
        let is_arrow = !type_parameters.is_empty() && arrow.is_some();
        let rejected = !allow_missing_return
            && !type_parameters.is_empty()
            && parenthesized
            && arrow.is_none()
            && matches!(
                self.paren_expression_arrow_token(true),
                Some(ParenthesizedArrowToken::Present(_))
            );
        for (index, token) in self
            .speculative_token_rewrites
            .drain(saved_rewrites..)
            .rev()
        {
            self.tokens[index] = token;
        }
        self.speculating = saved_speculating;
        self.index = saved_index;
        self.next_node = saved_next_node;
        self.diagnostics.truncate(saved_diagnostics);
        if rejected && !saved_speculating {
            let authored_span = self.current().span;
            self.retain_parser_recovery(
                ParserRecoveryKind::RejectedGenericArrowPrefix,
                authored_span,
                self.recovery_extent_from_current(authored_span),
            );
        }
        is_arrow
    }

    fn generic_arrow_is_unambiguous_in_jsx(&self) -> bool {
        let mut cursor = self.index + 1;
        if self.source.kind() != SourceKind::TypeScriptJsx {
            let first = self.token_kind_at(cursor);
            return first.is_identifier() || first == TokenKind::Const;
        }
        if self.token_kind_at(cursor) == TokenKind::Const {
            cursor += 1;
        }
        if !self.token_kind_at(cursor).is_identifier() {
            return false;
        }
        cursor += 1;
        match self.token_kind_at(cursor) {
            TokenKind::Extends => !matches!(
                self.token_kind_at(cursor + 1),
                TokenKind::Equals | TokenKind::GreaterThan | TokenKind::Slash
            ),
            TokenKind::Comma | TokenKind::Equals => true,
            _ => false,
        }
    }

    fn parse_object_literal(&mut self) -> Expression {
        let object_extent = self.balanced_recovery_brace_extent(self.index);
        let left = self.bump().span;
        let mut properties = Vec::new();
        let mut member_recoveries = Vec::new();
        while !self.at_any(&[TokenKind::RightBrace, TokenKind::EndOfFile]) {
            if matches!(self.kind(), TokenKind::Get | TokenKind::Set)
                && self.class_member_starts_accessor()
            {
                let authored_span = self.bump().span;
                self.parse_property_name();
                self.parse_parameters();
                if self.eat(TokenKind::Colon) {
                    self.parse_type();
                }
                let member_extent = if self.at(TokenKind::LeftBrace) {
                    self.consume_balanced_tokens(
                        TokenKind::LeftBrace,
                        TokenKind::RightBrace,
                        "'}' expected.",
                    )
                } else {
                    self.error_current("'{' expected.", 1005);
                    self.recovery_extent_from_current(authored_span)
                };
                member_recoveries.push((
                    ParserRecoveryKind::ObjectMember,
                    authored_span,
                    Span {
                        start: authored_span.start,
                        ..member_extent
                    },
                ));
                self.eat(TokenKind::Comma);
                continue;
            }
            let start = self.current().span;
            let (name, name_span, name_kind) = self.parse_property_name();
            let has_colon = self.eat(TokenKind::Colon);
            if !has_colon
                && !self.at_any(&[
                    TokenKind::Comma,
                    TokenKind::Equals,
                    TokenKind::RightBrace,
                    TokenKind::EndOfFile,
                ])
            {
                let continuation = self.recovery_extent_from_current(name_span);
                let extent =
                    object_extent.map_or(continuation, |extent| extent.merge(continuation));
                member_recoveries.push((
                    ParserRecoveryKind::Expression,
                    name_span,
                    Span {
                        start: name_span.start,
                        ..extent
                    },
                ));
            }
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
            if name_kind == PropertyNameKind::Computed {
                member_recoveries.push((
                    ParserRecoveryKind::ObjectMember,
                    name_span,
                    Span {
                        start: name_span.start,
                        ..span
                    },
                ));
            }
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
        for (kind, authored_span, recovery_extent) in member_recoveries {
            self.record_parser_recovery_for_analysis(kind, authored_span, recovery_extent);
        }
        Expression {
            id: self.alloc_node(),
            span: left.merge(right),
            kind: ExpressionKind::Object(properties),
        }
    }

    fn parse_unsupported_class_expression(&mut self) -> Expression {
        use TokenKind::{LeftBrace, RightBrace};

        let start = self.bump().span;
        let implements_clause =
            self.at(TokenKind::Implements) && self.peek_kind(1).is_identifier_name();
        let _ = (self.kind().is_identifier() && !implements_clause).then(|| self.bump());
        self.parse_type_parameters();
        let _ = self
            .eat(TokenKind::Extends)
            .then(|| self.parse_class_heritage_element());
        if self.eat(TokenKind::Implements) {
            self.parse_class_heritage_element();
            while self.eat(TokenKind::Comma) {
                self.parse_class_heritage_element();
            }
        }
        let body = if self.at(LeftBrace) {
            self.consume_balanced_tokens(LeftBrace, RightBrace, "'}' expected.")
        } else {
            self.error_current("'{' expected.", 1005);
            self.recovery_extent_from_current(start)
        };
        let span = start.merge(body);
        self.record_parser_recovery_for_analysis(ParserRecoveryKind::ClassExpression, start, span);
        Expression {
            id: self.alloc_node(),
            span,
            kind: ExpressionKind::Missing,
        }
    }

    fn parse_class_heritage_element(&mut self) {
        let _ = (self.parse_postfix_expression(), self.parse_type_arguments());
    }

    pub(super) fn finish_no_substitution_template_source(&mut self, statements: &[Statement]) {
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
            self.observe_literal_source_context(AuthoredLiteralKind::Template);
        }
    }

    pub(super) fn observe_template_expression_semantics(&mut self, expression: &Expression) {
        if expression_contains_no_substitution_template(expression) {
            self.observe_literal_unsupported_host(AuthoredLiteralKind::Template);
        }
    }

    pub(super) fn observe_class_template_semantics(&mut self, declaration: &ClassDeclaration) {
        if class_contains_no_substitution_template(declaration) {
            self.observe_literal_unsupported_host(AuthoredLiteralKind::Template);
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
        self.observe_literal_unsupported_host(AuthoredLiteralKind::Template);
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
            self.source_syntax_facts
                .insert(SourceSyntaxFact::ExplicitNewTypeArguments);
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
        self.observe_literal_unsupported_host(AuthoredLiteralKind::Template);
        let template = *self.current();
        let authored_span = await_token.span.merge(template.span);
        let recovery_extent = self.recovery_extent_from_current(authored_span);
        self.retain_parser_recovery(ParserRecoveryKind::Template, authored_span, recovery_extent);
        let template_span = self.consume_template_extent();
        Some(Expression {
            id: self.alloc_node(),
            span: await_token.span.merge(template_span),
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
            self.observe_literal_unsupported_host(AuthoredLiteralKind::Template);
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
            self.observe_literal_lexical_recovery(AuthoredLiteralKind::Template);
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

    fn parse_unsupported_template_expression(&mut self) -> Expression {
        let head = *self.current();
        self.observe_literal_unsupported_host(AuthoredLiteralKind::Template);
        let span = self.consume_template_extent();
        self.retain_parser_recovery(ParserRecoveryKind::Template, head.span, span);
        Expression {
            id: self.alloc_node(),
            span,
            kind: ExpressionKind::Missing,
        }
    }

    fn consume_template_extent(&mut self) -> Span {
        let first = *self.current();
        if first.kind != TokenKind::TemplateHead {
            self.bump();
            return first.span;
        }
        let mut nesting = 0_u32;
        let mut span = first.span;
        loop {
            let token = *self.current();
            if token.kind.is_identifier() {
                self.source_syntax_facts
                    .insert(SourceSyntaxFact::TemplateExpressionIdentifier);
            }
            match token.kind {
                TokenKind::TemplateHead => nesting += 1,
                TokenKind::TemplateTail => nesting = nesting.saturating_sub(1),
                TokenKind::EndOfFile => break,
                _ => {}
            }
            span = span.merge(token.span);
            self.bump();
            if nesting == 0 {
                break;
            }
        }
        span
    }

    pub(super) fn reject_tagged_template(&mut self, tag_span: Span) -> bool {
        if !matches!(
            self.kind(),
            TokenKind::NoSubstitutionTemplateLiteral | TokenKind::TemplateHead
        ) {
            return false;
        }
        self.observe_literal_unsupported_host(AuthoredLiteralKind::Template);
        let recovery_extent = self.recovery_extent_from_current(tag_span);
        self.retain_parser_recovery(ParserRecoveryKind::Template, tag_span, recovery_extent);
        self.consume_template_extent();
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
            self.observe_literal_unsupported_host(AuthoredLiteralKind::Template);
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
