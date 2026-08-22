use super::super::{
    Expression, ExpressionKind, Literal, NumberLiteral, NumericRecoveryKind, Statement, Token,
    TokenKind, statements_form_numeric_recovery_safe_file,
};
use super::Parser;
use crate::diagnostics::Diagnostic;

impl Parser<'_> {
    pub(super) fn number_literal(&self, token: Token) -> NumberLiteral {
        self.numeric_literals
            .binary_search_by_key(&token.span.start, |literal| literal.span.start)
            .ok()
            .filter(|index| self.numeric_literals[*index].span == token.span)
            .map_or_else(
                || NumberLiteral::Plain(self.text(token.span).to_string()),
                |index| NumberLiteral::Recovery(self.numeric_literals[index].syntax_literal()),
            )
    }

    /// TypeScript terminates a legacy-octal token before a following decimal
    /// fraction. The parser owns the resulting missing-semicolon diagnostic
    /// and deliberately leaves the dot-leading token for the next statement.
    pub(super) fn finish_numeric_recovery_expression_statement(
        &mut self,
        expression: &Expression,
    ) -> bool {
        let recovery_kind = match &expression.kind {
            ExpressionKind::Literal(Literal::Number(number)) => number.recovery_kind(),
            _ => None,
        };
        let recovery_can_terminate_before_next_token = matches!(
            recovery_kind,
            Some(NumericRecoveryKind::LegacyOctal | NumericRecoveryKind::LeadingZeroDecimal)
        );
        let next_token_requires_separator = self.kind() == TokenKind::Identifier
            || self.kind() == TokenKind::NumericLiteral
                && self.text(self.current().span).starts_with('.');
        if self.statement_nesting_depth != 0
            || !recovery_can_terminate_before_next_token
            || !next_token_requires_separator
            || !self.tokens_are_on_same_line(self.index.saturating_sub(1), self.index)
        {
            return false;
        }
        let diagnostic = Diagnostic::at(
            self.source,
            self.current().span,
            "';' expected.".to_string(),
            1005,
        );
        self.numeric_parser_diagnostics.push(diagnostic.clone());
        self.diagnostics.push(diagnostic);
        true
    }

    pub(super) fn finish_numeric_recovery_source(&mut self, statements: &[Statement]) -> bool {
        let has_authored = !self.numeric_literals.is_empty();
        if !has_authored {
            return false;
        }
        let owned = match self.numeric_literals.as_slice() {
            [literal] => {
                literal.syntax_literal().validation_supported()
                    && literal.owns_diagnostics(&self.diagnostics, &self.numeric_parser_diagnostics)
                    && self.comments.is_empty()
                    && !self.has_unmodeled_trivia
                    && !self.has_unmodeled_top_level_syntax
                    && statements_form_numeric_recovery_safe_file(self.source, statements, 1)
            }
            _ => false,
        };
        if !owned {
            self.product_capabilities
                .observe_unmodeled_numeric_recovery();
        }
        true
    }
}
