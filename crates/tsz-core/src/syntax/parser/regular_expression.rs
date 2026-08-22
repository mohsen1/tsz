use super::super::{
    Expression, ExpressionKind, Statement, TokenKind, comments_form_regular_expression_safe_file,
    statements_form_regular_expression_safe_file,
};
use super::Parser;

impl Parser<'_> {
    pub(super) fn finish_regular_expression_source(&mut self, statements: &[Statement]) -> bool {
        let has_authored_regular_expression = !self.regular_expression_literals.is_empty()
            || !self
                .product_capabilities
                .regular_expression_products_supported;
        let supported_literal_count = self
            .regular_expression_literals
            .iter()
            .filter(|literal| literal.syntax_literal().validation_supported())
            .count();
        if has_authored_regular_expression
            && (self.has_unmodeled_trivia
                || self.has_unmodeled_top_level_syntax
                || !comments_form_regular_expression_safe_file(
                    self.source,
                    statements,
                    &self.comments,
                )
                || !statements_form_regular_expression_safe_file(
                    self.source,
                    statements,
                    supported_literal_count,
                ))
        {
            self.product_capabilities
                .observe_unmodeled_regular_expression();
        }
        has_authored_regular_expression
    }

    pub(super) fn parse_regular_expression_literal(&mut self) -> Expression {
        let token = *self.current();
        let literal = self
            .regular_expression_literals
            .binary_search_by_key(&token.span.start, |literal| literal.span.start)
            .ok()
            .map(|index| self.regular_expression_literals[index].syntax_literal());
        self.bump();
        let Some(literal) = literal else {
            self.product_capabilities
                .observe_unmodeled_regular_expression();
            return Expression {
                id: self.alloc_node(),
                span: token.span,
                kind: ExpressionKind::Missing,
            };
        };
        Expression {
            id: self.alloc_node(),
            span: token.span,
            kind: ExpressionKind::RegularExpression(literal),
        }
    }

    pub(super) fn observe_unmodeled_regular_expression_if_current(&mut self) {
        if matches!(self.kind(), TokenKind::Slash | TokenKind::SlashEquals) {
            self.product_capabilities
                .observe_unmodeled_regular_expression();
        }
    }

    pub(super) fn observe_regular_expression_in_unsupported_statement(&mut self) {
        if !matches!(
            self.kind(),
            TokenKind::For | TokenKind::While | TokenKind::With
        ) {
            return;
        }
        let mut depth = 0_u32;
        let mut header_end = None;
        for cursor in self.index + 1..self.tokens.len() {
            match self.tokens[cursor].kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen if depth > 0 => {
                    depth -= 1;
                    if depth == 0 {
                        header_end = Some(cursor);
                        break;
                    }
                }
                TokenKind::EndOfFile => break,
                _ => {}
            }
        }
        let Some(header_end) = header_end else {
            return;
        };
        let body_starts_with_slash = self
            .tokens
            .get(header_end + 1)
            .is_some_and(|token| matches!(token.kind, TokenKind::Slash | TokenKind::SlashEquals));
        let for_of_operand_starts_with_slash = self.kind() == TokenKind::For
            && self.tokens[self.index + 1..header_end]
                .windows(2)
                .any(|tokens| {
                    tokens[0].kind == TokenKind::Of
                        && matches!(tokens[1].kind, TokenKind::Slash | TokenKind::SlashEquals)
                });
        if body_starts_with_slash || for_of_operand_starts_with_slash {
            self.product_capabilities
                .observe_unmodeled_regular_expression();
        }
    }
}
