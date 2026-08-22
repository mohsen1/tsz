use super::super::{
    ClassDeclaration, Expression, ExpressionKind, Literal, Statement, Token, TokenKind,
    class_contains_no_substitution_template, expression_contains_no_substitution_template,
    statements_form_no_substitution_template_safe_file,
};
use super::Parser;
use crate::diagnostics::Diagnostic;
use crate::source::Span;

impl Parser<'_> {
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
    }

    pub(super) fn parse_new_expression(&mut self) -> Expression {
        let left = self.bump().span;
        let callee = self.parse_primary_expression();
        let type_arguments = if self.at(TokenKind::LessThan) {
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
        let template = self.bump();
        Some(Expression {
            id: self.alloc_node(),
            span: await_token.span.merge(template.span),
            kind: ExpressionKind::Missing,
        })
    }

    pub(super) fn consume_non_null_template_host(&mut self) -> bool {
        if !self.source.kind().supports_expression_type_arguments()
            || !self.at(TokenKind::Bang)
            || !self.tokens_are_on_same_line(self.index.saturating_sub(1), self.index)
            || !matches!(
                self.peek_kind(1),
                TokenKind::NoSubstitutionTemplateLiteral | TokenKind::TemplateHead
            )
        {
            return false;
        }
        // A TypeScript non-null assertion remains a valid tag host. It is
        // syntax-erased, while the following template is still consumed as
        // parser-proven tagged rather than revisited as an ordinary literal.
        self.bump();
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

    pub(super) fn reject_tagged_template(&mut self) -> bool {
        if !matches!(
            self.kind(),
            TokenKind::NoSubstitutionTemplateLiteral | TokenKind::TemplateHead
        ) {
            return false;
        }
        self.product_capabilities.observe_unmodeled_template();
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
        }
    }

    pub(super) fn literal_from(&self, token: Token) -> Literal {
        match token.kind {
            TokenKind::True => Literal::Boolean(true),
            TokenKind::False => Literal::Boolean(false),
            TokenKind::Null => Literal::Null,
            TokenKind::StringLiteral => Literal::String(unquote(self.text(token.span))),
            TokenKind::BigIntLiteral => Literal::BigInt(self.text(token.span).to_string()),
            _ => Literal::Number(self.text(token.span).to_string()),
        }
    }

    pub(super) fn parse_module_specifier(&mut self) -> (String, Span) {
        let token = *self.current();
        if token.kind == TokenKind::StringLiteral {
            self.bump();
            (unquote(self.text(token.span)), token.span)
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
