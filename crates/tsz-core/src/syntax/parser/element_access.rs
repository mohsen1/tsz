use super::Parser;
use super::operators::expression_has_recovered_left_edge;
use crate::syntax::{AuthoredLiteralKind, Expression, ExpressionKind, TokenKind};

impl Parser<'_> {
    pub(super) fn at_recovered_element_access(&self, expression: &Expression) -> bool {
        self.at(TokenKind::LeftBracket)
            && (expression_has_recovered_left_edge(expression)
                || self.postfix_continues_retained_recovery(expression.span))
    }

    pub(super) fn parse_member_access(&mut self, expression: Expression) -> Expression {
        match self.kind() {
            TokenKind::Dot => self.parse_property_access(expression),
            TokenKind::LeftBracket => self.parse_element_access(expression),
            _ => unreachable!("member access must start with '.' or '['"),
        }
    }

    fn parse_property_access(&mut self, object: Expression) -> Expression {
        let dot = self.bump().span;
        if super::super::erased_expression_separated_number(&object).is_some()
            && object.span.end != dot.start
        {
            self.observe_literal_unsupported_host(AuthoredLiteralKind::NumericSeparator);
        }
        let (name, name_span) = self.parse_identifier_name();
        Expression {
            id: self.alloc_node(),
            span: object.span.merge(name_span),
            kind: ExpressionKind::Member {
                object: Box::new(object),
                name,
                name_span,
            },
        }
    }

    pub(super) fn parse_element_access(&mut self, object: Expression) -> Expression {
        let left = self.bump().span;
        let index = if self.at(TokenKind::RightBracket) {
            self.error_current(
                "An element access expression should take an argument.",
                1011,
            );
            Expression {
                id: self.alloc_node(),
                span: self.current().span,
                kind: ExpressionKind::Missing,
            }
        } else {
            self.parse_expression()
        };
        let right = self.current().span;
        self.expect(TokenKind::RightBracket, "']' expected.", 1005);
        Expression {
            id: self.alloc_node(),
            span: object.span.merge(left).merge(right),
            kind: ExpressionKind::ElementAccess {
                object: Box::new(object),
                index: Box::new(index),
            },
        }
    }

    pub(super) fn parse_array_literal(&mut self) -> Expression {
        let left = self.bump().span;
        let mut elements = Vec::new();
        while !self.at_any(&[TokenKind::RightBracket, TokenKind::EndOfFile]) {
            elements.push(self.parse_expression());
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        let right = self.current().span;
        self.expect(TokenKind::RightBracket, "']' expected.", 1005);
        Expression {
            id: self.alloc_node(),
            span: left.merge(right),
            kind: ExpressionKind::Array(elements),
        }
    }
}
