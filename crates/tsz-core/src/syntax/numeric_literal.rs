use crate::diagnostics::Diagnostic;
use crate::source::{SourceText, Span};

use super::{
    Expression, ExpressionKind, Literal, Statement, StatementKind, Token, TokenKind, UnaryOperator,
    source_uses_supported_line_breaks, statement_starts_at_supported_column,
};

/// Syntax-owned spelling for an ordinary or scanner-recovered number token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumberLiteral {
    Plain(String),
    Recovery(NumericRecoveryLiteral),
}

impl NumberLiteral {
    #[must_use]
    pub fn raw(&self) -> &str {
        match self {
            Self::Plain(raw) => raw,
            Self::Recovery(literal) => &literal.raw,
        }
    }

    /// A syntactically valid spelling with the same numeric value as the
    /// recovered token. Malformed exponent text never reaches the type store.
    #[must_use]
    pub fn semantic_text(&self) -> &str {
        match self {
            Self::Plain(raw) => raw,
            Self::Recovery(literal) => &literal.semantic_text,
        }
    }

    /// JavaScript spelling selected by scanner-owned recovery syntax.
    #[must_use]
    pub fn emit_text(&self) -> &str {
        match self {
            Self::Plain(raw) => raw,
            Self::Recovery(literal) => &literal.emit_text,
        }
    }

    #[must_use]
    pub const fn validation_supported(&self) -> bool {
        match self {
            Self::Plain(_) => true,
            Self::Recovery(literal) => literal.validation_supported,
        }
    }

    pub(crate) const fn recovery_kind(&self) -> Option<NumericRecoveryKind> {
        match self {
            Self::Plain(_) => None,
            Self::Recovery(literal) => Some(literal.kind),
        }
    }
}

/// Scanner-owned numeric recovery. The three spellings stay separate so emit
/// never derives JavaScript text from a checker display or a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumericRecoveryLiteral {
    raw: String,
    semantic_text: String,
    emit_text: String,
    kind: NumericRecoveryKind,
    validation_supported: bool,
}

impl NumericRecoveryLiteral {
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    #[must_use]
    pub fn semantic_text(&self) -> &str {
        &self.semantic_text
    }

    #[must_use]
    pub fn emit_text(&self) -> &str {
        &self.emit_text
    }

    #[must_use]
    pub const fn validation_supported(&self) -> bool {
        self.validation_supported
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumericRecoveryKind {
    LegacyOctal,
    LeadingZeroDecimal,
    MissingExponentDigits,
    InvalidSeparator,
    IncompleteRadix,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NumericDiagnosticEvent {
    diagnostic: Diagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ScannedNumericLiteral {
    pub span: Span,
    literal: NumericRecoveryLiteral,
    diagnostic_events: Vec<NumericDiagnosticEvent>,
}

impl ScannedNumericLiteral {
    pub(super) fn syntax_literal(&self) -> NumericRecoveryLiteral {
        self.literal.clone()
    }

    pub(super) fn owns_diagnostics(
        &self,
        diagnostics: &[Diagnostic],
        parser_diagnostics: &[Diagnostic],
    ) -> bool {
        diagnostics.len() == self.diagnostic_events.len() + parser_diagnostics.len()
            && diagnostics
                .iter()
                .zip(
                    self.diagnostic_events
                        .iter()
                        .map(|event| &event.diagnostic)
                        .chain(parser_diagnostics),
                )
                .all(|(actual, expected)| actual == expected)
    }
}

pub(super) struct ScannedNumericToken {
    pub end: usize,
    pub kind: TokenKind,
    pub diagnostics: Vec<Diagnostic>,
    pub recovery_literal: Option<ScannedNumericLiteral>,
}

pub(super) fn scan_numeric_literal(
    source: &SourceText,
    start: usize,
    previous_token: Option<Token>,
) -> ScannedNumericToken {
    let bytes = source.text.as_bytes();
    let mut offset = start;
    let starts_with_dot = bytes.get(offset) == Some(&b'.');
    if starts_with_dot {
        offset += 1;
        let fraction_run = consume_digits(bytes, &mut offset, 10);
        let integer = &source.text[start..offset];
        finish_decimal_numeric(
            source,
            start,
            offset,
            integer,
            false,
            true,
            true,
            fraction_run.invalid_separator,
        )
    } else if bytes.get(offset..offset + 2) == Some(b"0x")
        || bytes.get(offset..offset + 2) == Some(b"0X")
    {
        offset += 2;
        scan_prefixed_numeric(source, start, offset, 16)
    } else if bytes.get(offset..offset + 2) == Some(b"0b")
        || bytes.get(offset..offset + 2) == Some(b"0B")
    {
        offset += 2;
        scan_prefixed_numeric(source, start, offset, 2)
    } else if bytes.get(offset..offset + 2) == Some(b"0o")
        || bytes.get(offset..offset + 2) == Some(b"0O")
    {
        offset += 2;
        scan_prefixed_numeric(source, start, offset, 8)
    } else {
        let leading_zero = bytes.get(start) == Some(&b'0');
        let mut invalid_separator = false;
        if leading_zero {
            consume_ascii_digits(bytes, &mut offset);
            if offset == start + 1 && bytes.get(offset) == Some(&b'_') {
                invalid_separator = consume_digits(bytes, &mut offset, 10).invalid_separator;
            }
        } else {
            invalid_separator = consume_digits(bytes, &mut offset, 10).invalid_separator;
        }

        let integer_end = offset;
        let integer = &source.text[start..integer_end];
        let has_legacy_prefix = integer.len() > 1 && leading_zero && !integer.contains('_');
        if has_legacy_prefix && integer.bytes().all(|byte| matches!(byte, b'0'..=b'7')) {
            let negative = previous_token.is_some_and(|token| token.kind == TokenKind::Minus);
            let replacement_digits = legacy_octal_replacement_digits(integer);
            let replacement = if negative {
                format!("-0o{replacement_digits}")
            } else {
                format!("0o{replacement_digits}")
            };
            let mut diagnostic = Diagnostic::at(
                source,
                Span::new(source.id, start, integer_end),
                format!("Octal literals are not allowed. Use the syntax '{replacement}'."),
                1121,
            );
            if negative {
                diagnostic.start = diagnostic.start.saturating_sub(1);
                diagnostic.length = diagnostic.length.saturating_add(1);
            }
            let canonical_text = canonical_bounded_integer(integer, 8);
            return recovered_numeric_token(
                source,
                start,
                integer_end,
                canonical_text
                    .clone()
                    .unwrap_or_else(|| integer.to_string()),
                canonical_text
                    .clone()
                    .unwrap_or_else(|| integer.to_string()),
                NumericRecoveryKind::LegacyOctal,
                canonical_text.is_some(),
                vec![diagnostic],
            );
        }

        let leading_zero_decimal = has_legacy_prefix;
        let mut has_fraction_or_exponent = false;
        let mut mantissa_has_fraction = false;
        if bytes.get(offset) == Some(&b'.') {
            has_fraction_or_exponent = true;
            mantissa_has_fraction = true;
            offset += 1;
            invalid_separator |= consume_digits(bytes, &mut offset, 10).invalid_separator;
        }

        finish_decimal_numeric(
            source,
            start,
            offset,
            integer,
            leading_zero_decimal,
            has_fraction_or_exponent,
            mantissa_has_fraction,
            invalid_separator,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn finish_decimal_numeric(
    source: &SourceText,
    start: usize,
    mut offset: usize,
    integer: &str,
    leading_zero_decimal: bool,
    mut has_fraction_or_exponent: bool,
    mantissa_has_fraction: bool,
    mut invalid_separator: bool,
) -> ScannedNumericToken {
    let bytes = source.text.as_bytes();
    let mut missing_exponent_digits = false;
    let mut semantic_end = offset;
    if matches!(bytes.get(offset), Some(b'e' | b'E')) {
        has_fraction_or_exponent = true;
        semantic_end = offset;
        offset += 1;
        if matches!(bytes.get(offset), Some(b'+' | b'-')) {
            offset += 1;
        }
        let exponent_run = consume_digits(bytes, &mut offset, 10);
        missing_exponent_digits = exponent_run.digit_count == 0;
        invalid_separator |= exponent_run.invalid_separator;
    }

    if !has_fraction_or_exponent && !leading_zero_decimal && !invalid_separator {
        return plain_numeric_token(bytes, offset);
    }

    let mut diagnostics = Vec::new();
    if leading_zero_decimal {
        diagnostics.push(Diagnostic::at(
            source,
            Span::new(source.id, start, offset),
            "Decimals with leading zeros are not allowed.".to_string(),
            1489,
        ));
    }
    if missing_exponent_digits {
        diagnostics.push(Diagnostic::at(
            source,
            Span::new(source.id, offset, offset),
            "Digit expected.".to_string(),
            1124,
        ));
    }
    if diagnostics.is_empty() && !invalid_separator {
        return ScannedNumericToken {
            end: offset,
            kind: TokenKind::NumericLiteral,
            diagnostics,
            recovery_literal: None,
        };
    }

    let missing_exponent_suffix = missing_exponent_digits
        && !invalid_separator
        && bytes.get(offset) == Some(&b'n')
        && !conservatively_continues_identifier(bytes, offset + 1);
    let token_kind =
        if invalid_separator && !has_fraction_or_exponent && bytes.get(offset) == Some(&b'n') {
            offset += 1;
            TokenKind::BigIntLiteral
        } else {
            if missing_exponent_suffix {
                offset += 1;
            }
            TokenKind::NumericLiteral
        };

    let raw = source.text[start..offset].to_string();
    let (semantic_text, emit_text, kind, validation_supported) = if missing_exponent_digits {
        let semantic_text = source.text[start..semantic_end].replace('_', "");
        (
            semantic_text,
            raw,
            NumericRecoveryKind::MissingExponentDigits,
            !leading_zero_decimal
                && !mantissa_has_fraction
                && !invalid_separator
                && !missing_exponent_suffix,
        )
    } else if invalid_separator && !leading_zero_decimal {
        (
            raw.replace('_', ""),
            raw.clone(),
            NumericRecoveryKind::InvalidSeparator,
            false,
        )
    } else {
        let canonical = (!has_fraction_or_exponent)
            .then(|| canonical_bounded_integer(integer, 10))
            .flatten();
        (
            canonical.clone().unwrap_or_else(|| raw.replace('_', "")),
            canonical.clone().unwrap_or_else(|| raw.clone()),
            NumericRecoveryKind::LeadingZeroDecimal,
            canonical.is_some(),
        )
    };
    recovered_numeric_token_with_kind(
        source,
        start,
        offset,
        semantic_text,
        emit_text,
        kind,
        validation_supported,
        diagnostics,
        token_kind,
    )
}

fn recovered_numeric_token(
    source: &SourceText,
    start: usize,
    end: usize,
    semantic_text: String,
    emit_text: String,
    kind: NumericRecoveryKind,
    validation_supported: bool,
    diagnostics: Vec<Diagnostic>,
) -> ScannedNumericToken {
    recovered_numeric_token_with_kind(
        source,
        start,
        end,
        semantic_text,
        emit_text,
        kind,
        validation_supported,
        diagnostics,
        TokenKind::NumericLiteral,
    )
}

#[allow(clippy::too_many_arguments)]
fn recovered_numeric_token_with_kind(
    source: &SourceText,
    start: usize,
    end: usize,
    semantic_text: String,
    emit_text: String,
    kind: NumericRecoveryKind,
    validation_supported: bool,
    diagnostics: Vec<Diagnostic>,
    token_kind: TokenKind,
) -> ScannedNumericToken {
    let raw = source.text[start..end].to_string();
    let diagnostic_events = diagnostics
        .iter()
        .cloned()
        .map(|diagnostic| NumericDiagnosticEvent { diagnostic })
        .collect();
    ScannedNumericToken {
        end,
        kind: token_kind,
        diagnostics,
        recovery_literal: Some(ScannedNumericLiteral {
            span: Span::new(source.id, start, end),
            literal: NumericRecoveryLiteral {
                raw,
                semantic_text,
                emit_text,
                kind,
                validation_supported,
            },
            diagnostic_events,
        }),
    }
}

fn scan_prefixed_numeric(
    source: &SourceText,
    start: usize,
    mut offset: usize,
    radix: u32,
) -> ScannedNumericToken {
    let bytes = source.text.as_bytes();
    let run = consume_digits(bytes, &mut offset, radix);
    if run.digit_count == 0 && !run.invalid_separator {
        let token_kind = if bytes.get(offset) == Some(&b'n') {
            offset += 1;
            TokenKind::BigIntLiteral
        } else {
            TokenKind::NumericLiteral
        };
        return recovered_numeric_token_with_kind(
            source,
            start,
            offset,
            "0".to_string(),
            source.text[start..offset].to_string(),
            NumericRecoveryKind::IncompleteRadix,
            false,
            Vec::new(),
            token_kind,
        );
    }
    let plain = plain_numeric_token(bytes, offset);
    if run.digit_count != 0 && !run.invalid_separator {
        return plain;
    }
    let kind = if run.digit_count == 0 {
        NumericRecoveryKind::IncompleteRadix
    } else {
        NumericRecoveryKind::InvalidSeparator
    };
    let semantic_text = if run.digit_count == 0 || plain.kind == TokenKind::BigIntLiteral {
        "0".to_string()
    } else {
        source.text[start..plain.end].replace('_', "")
    };
    recovered_numeric_token_with_kind(
        source,
        start,
        plain.end,
        semantic_text,
        source.text[start..plain.end].to_string(),
        kind,
        false,
        Vec::new(),
        plain.kind,
    )
}

fn plain_numeric_token(bytes: &[u8], mut offset: usize) -> ScannedNumericToken {
    let kind = if bytes.get(offset) == Some(&b'n') {
        offset += 1;
        TokenKind::BigIntLiteral
    } else {
        TokenKind::NumericLiteral
    };
    ScannedNumericToken {
        end: offset,
        kind,
        diagnostics: Vec::new(),
        recovery_literal: None,
    }
}

#[derive(Debug, Clone, Copy)]
struct DigitRun {
    digit_count: usize,
    invalid_separator: bool,
}

fn consume_ascii_digits(bytes: &[u8], offset: &mut usize) {
    while bytes.get(*offset).is_some_and(u8::is_ascii_digit) {
        *offset += 1;
    }
}

fn conservatively_continues_identifier(bytes: &[u8], offset: usize) -> bool {
    bytes.get(offset).is_some_and(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$' | b'\\') || *byte >= 0x80
    })
}

fn consume_digits(bytes: &[u8], offset: &mut usize, radix: u32) -> DigitRun {
    let mut digit_count = 0;
    let mut saw_separator = false;
    let mut previous_was_digit = false;
    let mut invalid_separator = false;
    while bytes.get(*offset).is_some_and(|byte| {
        *byte == b'_'
            || match radix {
                2 => matches!(byte, b'0' | b'1'),
                8 => matches!(byte, b'0'..=b'7'),
                10 => byte.is_ascii_digit(),
                16 => byte.is_ascii_hexdigit(),
                _ => false,
            }
    }) {
        if bytes[*offset] == b'_' {
            saw_separator = true;
            invalid_separator |= !previous_was_digit;
            previous_was_digit = false;
        } else {
            digit_count += 1;
            previous_was_digit = true;
        }
        *offset += 1;
    }
    invalid_separator |= saw_separator && !previous_was_digit;
    DigitRun {
        digit_count,
        invalid_separator,
    }
}

fn canonical_bounded_integer(digits: &str, radix: u32) -> Option<String> {
    const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
    let value = u64::from_str_radix(digits, radix).ok()?;
    (value <= MAX_SAFE_INTEGER).then(|| value.to_string())
}

fn legacy_octal_replacement_digits(digits: &str) -> String {
    const MAX_SIGNED_OCTAL: &str = "777777777777777777777";
    let value = u64::from_str_radix(digits, 8).ok();
    if value.is_none_or(|value| value > i64::MAX as u64) {
        return MAX_SIGNED_OCTAL.to_string();
    }
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        "0".to_string()
    } else {
        digits.to_string()
    }
}

pub(crate) fn statements_form_numeric_recovery_safe_file(
    source: &SourceText,
    statements: &[Statement],
    supported_literal_count: usize,
) -> bool {
    source.is_regular_typescript_source()
        && source.text.is_ascii()
        && source_uses_supported_line_breaks(source)
        && statements
            .get(..1)
            .is_some_and(|first| statement_starts_at_supported_column(source, first))
        && supported_literal_count == 1
        && numeric_recovery_family(statements).is_some()
}

pub(crate) fn numeric_recovery_family(statements: &[Statement]) -> Option<NumericRecoveryKind> {
    match statements {
        [
            Statement {
                kind:
                    StatementKind::Expression(Expression {
                        kind: ExpressionKind::Literal(Literal::Number(number)),
                        ..
                    }),
                ..
            },
        ] => number
            .validation_supported()
            .then(|| number.recovery_kind())
            .flatten(),
        [
            Statement {
                kind:
                    StatementKind::Expression(Expression {
                        span,
                        kind:
                            ExpressionKind::Unary {
                                operator: UnaryOperator::Minus,
                                operand,
                            },
                        ..
                    }),
                ..
            },
        ] => {
            let ExpressionKind::Literal(Literal::Number(number)) = &operand.kind else {
                return None;
            };
            (number.validation_supported()
                && number.recovery_kind() == Some(NumericRecoveryKind::LegacyOctal)
                && span.start.saturating_add(1) == operand.span.start)
                .then_some(NumericRecoveryKind::LegacyOctal)
        }
        [
            Statement {
                kind:
                    StatementKind::Expression(Expression {
                        kind: ExpressionKind::Literal(Literal::Number(first)),
                        span: first_span,
                        ..
                    }),
                ..
            },
            Statement {
                kind:
                    StatementKind::Expression(Expression {
                        kind: ExpressionKind::Literal(Literal::Number(second)),
                        span: second_span,
                        ..
                    }),
                ..
            },
        ] => (first.validation_supported()
            && first.recovery_kind() == Some(NumericRecoveryKind::LegacyOctal)
            && second.recovery_kind().is_none()
            && second.raw().starts_with('.')
            && first_span.end == second_span.start)
            .then_some(NumericRecoveryKind::LegacyOctal),
        _ => None,
    }
}
