use super::super::{AuthoredLiteralKind, Expression, ExpressionKind, SourceSyntaxFact, TokenKind};
use super::Parser;

impl Parser<'_> {
    pub(super) fn finish_regular_expression_source(&mut self) {
        if self.regular_expression_literals.is_empty()
            && !self.source_syntax_facts.iter().any(|fact| {
                matches!(
                    fact,
                    SourceSyntaxFact::LiteralBoundary(AuthoredLiteralKind::RegularExpression, _)
                )
            })
        {
            return;
        }
        self.source_syntax_facts
            .insert(SourceSyntaxFact::AuthoredRegularExpression);
        if self
            .regular_expression_literals
            .iter()
            .any(|literal| !literal.syntax_literal().validation_supported())
        {
            self.observe_literal_validation_gap(AuthoredLiteralKind::RegularExpression);
        }
    }

    pub(super) fn parse_regular_expression_literal(&mut self) -> Expression {
        let token = *self.current();
        let literal = self
            .regular_expression_literals
            .binary_search_by_key(&token.span.start, |literal| literal.span.start)
            .ok()
            .map(|index| self.regular_expression_literals[index].syntax_literal());
        self.bump();
        let kind = literal.map_or_else(
            || {
                self.observe_literal_lexical_recovery(AuthoredLiteralKind::RegularExpression);
                ExpressionKind::Missing
            },
            ExpressionKind::RegularExpression,
        );
        Expression {
            id: self.alloc_node(),
            span: token.span,
            kind,
        }
    }

    pub(super) fn observe_unmodeled_regular_expression_if_current(&mut self) {
        if matches!(self.kind(), TokenKind::Slash | TokenKind::SlashEquals) {
            self.observe_literal_unsupported_host(AuthoredLiteralKind::RegularExpression);
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
        let header_end = self.tokens[self.index + 1..]
            .iter()
            .position(|token| {
                let closes_header = token.kind == TokenKind::RightParen && depth == 1;
                depth += u32::from(token.kind == TokenKind::LeftParen);
                depth = depth.saturating_sub(u32::from(token.kind == TokenKind::RightParen));
                closes_header
            })
            .map(|offset| self.index + 1 + offset);
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
            self.observe_literal_unsupported_host(AuthoredLiteralKind::RegularExpression);
        }
    }
}
