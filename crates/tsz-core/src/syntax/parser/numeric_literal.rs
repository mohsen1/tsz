use super::super::{
    AuthoredLiteralKind, Expression, ExpressionKind, Literal, NumberLiteral, NumericRecoveryKind,
    SourceSyntaxFact, Token, TokenKind,
};
use super::{Parser, scan_at};
use crate::diagnostics::Diagnostic;

impl Parser<'_> {
    pub(super) fn number_literal(&self, token: Token) -> NumberLiteral {
        if let Some(literal) = scan_at(&self.numeric_literals, token.span, |literal| literal.span) {
            return NumberLiteral::Recovery(literal.syntax_literal());
        }
        let literals = &self.separated_numeric_literals;
        scan_at(literals, token.span, |literal| literal.span).map_or_else(
            || NumberLiteral::Plain(self.text(token.span).to_string()),
            |literal| NumberLiteral::Separated(literal.syntax_literal()),
        )
    }

    pub(super) fn observe_unmodeled_numeric_separator_if_current(&mut self) {
        let span = self.current().span;
        let literals = &self.separated_numeric_literals;
        if scan_at(literals, span, |literal| literal.span).is_some() {
            self.observe_literal_unsupported_host(AuthoredLiteralKind::NumericSeparator);
        }
    }

    pub(super) fn observe_unmodeled_numeric_separator_in_span(
        &mut self,
        span: crate::source::Span,
    ) {
        if self.separated_numeric_literals.iter().any(|literal| {
            literal.span.file == span.file
                && literal.span.start >= span.start
                && literal.span.end <= span.end
        }) {
            self.observe_literal_unsupported_host(AuthoredLiteralKind::NumericSeparator);
        }
    }

    pub(super) fn finish_numeric_separator_source(&mut self) {
        if !self.numeric_separator_spans.is_empty() && self.has_unmodeled_numeric_separator {
            self.observe_literal_validation_gap(AuthoredLiteralKind::NumericSeparator);
        }
    }

    /// TypeScript terminates a legacy-octal token before a following decimal
    /// fraction. The parser owns the resulting missing-semicolon diagnostic
    /// and deliberately leaves the dot-leading token for the next statement.
    pub(super) fn finish_numeric_recovery_expression_statement(
        &mut self,
        expression: &Expression,
    ) -> bool {
        let same_line = self.tokens_are_on_same_line(self.index.saturating_sub(1), self.index);
        if same_line
            && matches!(expression.kind, ExpressionKind::Identifier { .. })
            && self.kind() == TokenKind::NumericLiteral
            && self.text(self.current().span).starts_with('.')
        {
            self.diagnostics.push(Diagnostic::at(
                self.source,
                expression.span,
                "Unexpected keyword or identifier.".to_string(),
                1434,
            ));
            return true;
        }
        if same_line
            && matches!(expression.kind, ExpressionKind::Literal(Literal::Number(_)))
            && self.kind() == TokenKind::Identifier
            && self.identifier_value(self.current().span).is_some()
        {
            self.diagnostics.push(Diagnostic::at(
                self.source,
                self.current().span,
                "';' expected.".to_string(),
                1005,
            ));
            return true;
        }
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
        if recovery_kind == Some(NumericRecoveryKind::MissingExponentDigits)
            && next_token_requires_separator
            && same_line
        {
            self.observe_literal_unsupported_host(AuthoredLiteralKind::NumericRecovery);
            self.retain_recovery_extent(
                super::super::ParserRecoveryKind::Expression,
                expression.span,
            );
            return false;
        }
        if self.statement_nesting_depth != 0
            || !recovery_can_terminate_before_next_token
            || !next_token_requires_separator
            || !same_line
        {
            return false;
        }
        let diagnostic = Diagnostic::at(
            self.source,
            self.current().span,
            "';' expected.".to_string(),
            1005,
        );
        self.diagnostics.push(diagnostic);
        self.source_syntax_facts
            .insert(SourceSyntaxFact::NumericRecoveryEmit(expression.id));
        true
    }

    pub(super) fn finish_numeric_recovery_source(&mut self) {
        if self
            .numeric_literals
            .iter()
            .any(|literal| !literal.syntax_literal().validation_supported())
        {
            self.observe_literal_validation_gap(AuthoredLiteralKind::NumericRecovery);
        }
    }
}
