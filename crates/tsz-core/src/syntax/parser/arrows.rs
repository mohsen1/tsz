use super::Parser;
use crate::syntax::{ArrowBody, Expression, ExpressionKind, TokenKind, TypeNode, TypeNodeKind};

#[derive(Clone, Copy)]
pub(super) enum ParenthesizedArrowToken {
    Present(usize),
    Missing,
}

impl Parser<'_> {
    pub(super) fn parse_arrow_body(&mut self) -> (ArrowBody, Option<crate::source::Span>) {
        if self.at(TokenKind::LeftBrace) {
            let (statements, span) = self.parse_block();
            (ArrowBody::Block(statements), span)
        } else {
            (
                ArrowBody::Expression(Box::new(self.parse_expression())),
                None,
            )
        }
    }

    pub(super) fn parse_recovered_arrow_body(
        &mut self,
        has_arrow: bool,
    ) -> (ArrowBody, Option<crate::source::Span>) {
        if self.at(TokenKind::LeftBrace) {
            let (statements, span) = self.parse_block();
            return (ArrowBody::Block(statements), span);
        }
        if has_arrow {
            return (
                ArrowBody::Expression(Box::new(self.parse_expression())),
                None,
            );
        }
        let token = *self.current();
        let expression = if token.kind.is_identifier() {
            self.parse_primary_expression()
        } else {
            Expression {
                id: self.alloc_node(),
                span: token.span,
                kind: ExpressionKind::Missing,
            }
        };
        (ArrowBody::Expression(Box::new(expression)), None)
    }

    pub(super) fn paren_expression_arrow_token(
        &mut self,
        definite: bool,
    ) -> Option<ParenthesizedArrowToken> {
        if !self.at(TokenKind::LeftParen) {
            return None;
        }
        if !definite {
            return self.speculative_parenthesized_arrow_token();
        }
        let mut depth = 0_u32;
        for (cursor, token) in self.tokens.iter().enumerate().skip(self.index) {
            match token.kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let following = self.tokens.get(cursor + 1).map(|token| token.kind);
                        if following == Some(TokenKind::FatArrow) {
                            return Some(ParenthesizedArrowToken::Present(cursor + 1));
                        }
                        if following != Some(TokenKind::Colon) {
                            return (definite || following == Some(TokenKind::LeftBrace))
                                .then_some(ParenthesizedArrowToken::Missing);
                        }
                        let annotation = cursor + 2;
                        return (definite && self.token_kind_at(annotation) == TokenKind::FatArrow)
                            .then_some(ParenthesizedArrowToken::Present(annotation))
                            .or_else(|| self.type_annotation_arrow_token(annotation, definite));
                    }
                }
                TokenKind::EndOfFile => break,
                _ => {}
            }
        }
        None
    }

    fn speculative_parenthesized_arrow_token(&mut self) -> Option<ParenthesizedArrowToken> {
        if self.not_parenthesized_arrows.contains(&self.index) {
            return None;
        }
        let saved_index = self.index;
        let saved_next_node = self.next_node;
        let saved_diagnostics = self.diagnostics.len();
        let saved_speculating = self.speculating;
        let saved_rewrites = self.speculative_token_rewrites.len();
        self.speculating = true;
        self.bump();
        let mut compatible = true;
        while !self.at_any(&[TokenKind::RightParen, TokenKind::EndOfFile]) {
            if !self.parameter_starts_arrow_speculation() {
                compatible = false;
                break;
            }
            self.parse_parameter();
            if self.eat(TokenKind::Comma) || self.at(TokenKind::RightParen) {
                continue;
            }
            if !self.parameter_starts_arrow_speculation() {
                compatible = false;
                break;
            }
        }
        compatible &= self.eat(TokenKind::RightParen);
        let arrow = if !compatible {
            None
        } else if self.at(TokenKind::FatArrow) {
            Some(ParenthesizedArrowToken::Present(self.index))
        } else if self.eat(TokenKind::Colon) {
            let annotation = self.parse_type();
            if annotation.blocks_arrow_parse() {
                None
            } else if self.at(TokenKind::FatArrow) {
                Some(ParenthesizedArrowToken::Present(self.index))
            } else {
                self.at(TokenKind::LeftBrace)
                    .then_some(ParenthesizedArrowToken::Missing)
            }
        } else {
            self.at(TokenKind::LeftBrace)
                .then_some(ParenthesizedArrowToken::Missing)
        };
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
        if arrow.is_none() {
            self.not_parenthesized_arrows.insert(saved_index);
        }
        arrow
    }

    pub(super) fn parameter_starts_arrow_speculation(&self) -> bool {
        matches!(
            self.kind(),
            TokenKind::DotDotDot | TokenKind::This | TokenKind::LeftBrace | TokenKind::LeftBracket
        ) || self.kind().is_identifier()
            || self.kind() == TokenKind::Export && self.peek_kind(1).is_identifier()
            || self.kind() == TokenKind::In
                && self.peek_kind(1).is_identifier()
                && self.tokens_are_on_same_line(self.index, self.index + 1)
    }

    pub(super) fn parse_parenthesized_or_function_type(&mut self) -> TypeNode {
        let left = self.bump().span;
        if self.paren_is_parameter_list() {
            let mut parameters = Vec::new();
            while !self.at_any(&[TokenKind::RightParen, TokenKind::EndOfFile]) {
                parameters.push(self.parse_parameter_with_this(true));
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RightParen, "')' expected.", 1005);
            self.expect(TokenKind::FatArrow, "'=>' expected.", 1005);
            let return_type = self.parse_type();
            return TypeNode {
                span: left.merge(return_type.span),
                kind: TypeNodeKind::Function {
                    id: self.alloc_node(),
                    type_parameters: Vec::new(),
                    parameters,
                    parameter_list_recovered: false,
                    return_type: Box::new(return_type),
                },
            };
        }
        let inner = self.parse_type();
        let right = self.current().span;
        self.expect(TokenKind::RightParen, "')' expected.", 1005);
        TypeNode {
            span: left.merge(right),
            kind: TypeNodeKind::Parenthesized(Box::new(inner)),
        }
    }

    fn paren_is_parameter_list(&self) -> bool {
        if !self.at(TokenKind::RightParen) && !self.parameter_starts_arrow_speculation() {
            return false;
        }
        let mut depth = 1_u32;
        let mut cursor = self.index;
        while let Some(token) = self.tokens.get(cursor) {
            match token.kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        return self.tokens.get(cursor + 1).map(|token| token.kind)
                            == Some(TokenKind::FatArrow);
                    }
                }
                TokenKind::EndOfFile => return false,
                _ => {}
            }
            cursor += 1;
        }
        false
    }

    fn type_annotation_arrow_token(
        &mut self,
        start: usize,
        definite: bool,
    ) -> Option<ParenthesizedArrowToken> {
        let saved_index = self.index;
        let saved_next_node = self.next_node;
        let saved_diagnostics = self.diagnostics.len();
        let saved_speculating = self.speculating;
        let saved_rewrites = self.speculative_token_rewrites.len();
        self.index = start;
        self.speculating = true;
        let annotation = self.parse_type();
        let arrow = if self.at(TokenKind::FatArrow) {
            (definite || !annotation.blocks_arrow_parse())
                .then_some(ParenthesizedArrowToken::Present(self.index))
        } else if definite || self.at(TokenKind::LeftBrace) && !annotation.blocks_arrow_parse() {
            Some(ParenthesizedArrowToken::Missing)
        } else {
            None
        };
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
        arrow
    }
}
