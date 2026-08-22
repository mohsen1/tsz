use crate::diagnostics::Diagnostic;
use crate::source::{SourceText, Span};

use super::scanner::is_plain_strict_binding_identifier;
use super::{
    CommentTrivia, Expression, ExpressionKind, Literal, Statement, StatementKind, VariableKind,
    comments_form_contiguous_plain_leading_run, source_is_ascii_outside_comments,
    source_uses_supported_line_breaks, statement_starts_at_supported_column,
};

/// An ECMAScript string value represented as exact UTF-16 code units.
///
/// Rust `String` cannot represent authored escapes for lone surrogate code
/// units, which TypeScript retains as valid ordinary string literal values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utf16String(Vec<u16>);

impl Utf16String {
    #[must_use]
    pub fn units(&self) -> &[u16] {
        &self.0
    }
}

/// Syntax-owned spelling and value for an ordinary string containing at
/// least one authored extended Unicode escape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtendedUnicodeStringLiteral {
    /// Exact authored token, including an opening quote and a closing quote
    /// when the scanner found one.
    pub raw: String,
    /// ECMAScript value after escape cooking, including lone UTF-16 units.
    pub cooked: Utf16String,
    pub terminated: bool,
    pub contains_invalid_escape: bool,
    pub contains_extended_unicode_escape: bool,
    ownership: ExtendedUnicodeStringOwnership,
}

impl ExtendedUnicodeStringLiteral {
    /// Whether scanner recovery and semantic/product ownership are proven for
    /// this token shape. Invalid and unterminated escapes can still be owned.
    #[must_use]
    pub const fn validation_supported(&self) -> bool {
        !matches!(self.ownership, ExtendedUnicodeStringOwnership::Unowned)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringLiteral {
    Plain(String),
    Extended(ExtendedUnicodeStringLiteral),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExtendedUnicodeStringOwnership {
    ValidTerminatedRun,
    ValidUnterminatedSingle,
    HomogeneousInvalid(EscapeRecoveryClass),
    Unowned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EscapeRecoveryClass {
    OverRangeClosed,
    EmptyClosed,
    NegativeClosed,
    IdentifierUnitClosed,
    EmptyAtQuote,
    UnterminatedDigitsAtQuote,
    UnexpectedEndDigits,
    UnterminatedDigitsAtLineBreak,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StringDiagnosticEvent {
    byte_span: Span,
    diagnostic: Diagnostic,
}

impl StringDiagnosticEvent {
    fn new(source: &SourceText, span: Span, message: &'static str, code: u32) -> Self {
        Self {
            byte_span: span,
            diagnostic: Diagnostic::at(source, span, message.to_string(), code),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ScannedStringLiteral {
    pub span: Span,
    literal: ExtendedUnicodeStringLiteral,
    diagnostic_events: Vec<StringDiagnosticEvent>,
}

impl ScannedStringLiteral {
    pub(super) fn syntax_literal(&self) -> ExtendedUnicodeStringLiteral {
        self.literal.clone()
    }

    /// The scanner creates each diagnostic identity once. Parser recovery may
    /// only graduate the file when its complete diagnostic vector is exactly
    /// this token's ordered event vector, with no speculative re-emission.
    pub(super) fn owns_all_diagnostics(&self, diagnostics: &[Diagnostic]) -> bool {
        diagnostics.len() == self.diagnostic_events.len()
            && diagnostics
                .iter()
                .zip(&self.diagnostic_events)
                .all(|(diagnostic, event)| diagnostic == &event.diagnostic)
    }
}

pub(super) struct ScannedStringToken {
    pub end: usize,
    pub diagnostics: Vec<Diagnostic>,
    pub extended_literal: Option<ScannedStringLiteral>,
    pub line_continuation_literal: Option<ScannedLineContinuationStringLiteral>,
}

/// Sparse scanner-owned semantic value for the exact ordinary-string subset
/// whose only escapes are line continuations. The authored token remains in
/// source text for emit; parser consumers use this cooked value for identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ScannedLineContinuationStringLiteral {
    pub span: Span,
    pub cooked: String,
}

pub(super) fn scan_ordinary_string_literal(
    source: &SourceText,
    start: usize,
    quote: u8,
) -> ScannedStringToken {
    let bytes = source.text.as_bytes();
    let probe = probe_ordinary_string_literal(source, start, quote);
    if !probe.has_extended_escape {
        let line_continuation_literal = (probe.terminated
            && probe.has_line_continuation
            && !probe.has_non_line_continuation_escape)
            .then(|| ScannedLineContinuationStringLiteral {
                span: Span::new(source.id, start, probe.end),
                cooked: cook_line_continuation_string(&source.text[start..probe.end]),
            });
        return ScannedStringToken {
            end: probe.end,
            diagnostics: (!probe.terminated)
                .then(|| {
                    Diagnostic::at(
                        source,
                        Span::new(source.id, start, probe.end),
                        "Unterminated string literal.".to_string(),
                        1002,
                    )
                })
                .into_iter()
                .collect(),
            extended_literal: None,
            line_continuation_literal,
        };
    }

    let mut offset = start + 1;
    let mut segment_start = offset;
    let mut cooked = Vec::new();
    let mut events = Vec::new();
    let mut contains_invalid_escape = false;
    let mut contains_extended_unicode_escape = false;
    let mut terminated = false;
    let mut recovery_at_line_break = false;

    while let Some(byte) = bytes.get(offset).copied() {
        if byte == quote {
            append_utf16(&source.text[segment_start..offset], &mut cooked);
            offset += 1;
            terminated = true;
            break;
        }
        if matches!(byte, b'\n' | b'\r') {
            append_utf16(&source.text[segment_start..offset], &mut cooked);
            recovery_at_line_break = true;
            break;
        }
        if bytes.get(offset..offset + 3) == Some(b"\\u{") {
            append_utf16(&source.text[segment_start..offset], &mut cooked);
            scan_extended_escape(
                source,
                bytes,
                &mut offset,
                &mut cooked,
                &mut events,
                &mut contains_invalid_escape,
                &mut contains_extended_unicode_escape,
            );
            segment_start = offset;
            continue;
        }
        if byte == b'\\' {
            offset += 1;
            if bytes.get(offset..offset + 2) == Some(b"\r\n") {
                offset += 2;
            } else if let Some(character) = source.text[offset..].chars().next() {
                offset += character.len_utf8();
            }
            continue;
        }
        let Some(character) = source.text[offset..].chars().next() else {
            break;
        };
        offset += character.len_utf8();
    }

    if !terminated && offset >= bytes.len() && segment_start <= offset {
        append_utf16(&source.text[segment_start..offset], &mut cooked);
    }

    if !terminated {
        push_event(
            source,
            &mut events,
            Span::new(source.id, offset, offset),
            "Unterminated string literal.",
            1002,
        );
    }

    let span = Span::new(source.id, start, offset);
    debug_assert_eq!(probe.end, offset);
    debug_assert_eq!(probe.terminated, terminated);
    let raw = source.slice(span).to_string();
    let ownership =
        classify_extended_unicode_string(&raw, quote, terminated, recovery_at_line_break);
    let diagnostics = events
        .iter()
        .map(|event| event.diagnostic.clone())
        .collect();
    ScannedStringToken {
        end: offset,
        diagnostics,
        extended_literal: Some(ScannedStringLiteral {
            span,
            literal: ExtendedUnicodeStringLiteral {
                raw,
                cooked: Utf16String(cooked),
                terminated,
                contains_invalid_escape,
                contains_extended_unicode_escape,
                ownership,
            },
            diagnostic_events: events,
        }),
        line_continuation_literal: None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StringProbe {
    end: usize,
    terminated: bool,
    has_extended_escape: bool,
    has_line_continuation: bool,
    has_non_line_continuation_escape: bool,
}

fn probe_ordinary_string_literal(source: &SourceText, start: usize, quote: u8) -> StringProbe {
    let bytes = source.text.as_bytes();
    let mut offset = start + 1;
    let mut has_extended_escape = false;
    let mut has_line_continuation = false;
    let mut has_non_line_continuation_escape = false;
    while let Some(byte) = bytes.get(offset).copied() {
        if byte == quote {
            return StringProbe {
                end: offset + 1,
                terminated: true,
                has_extended_escape,
                has_line_continuation,
                has_non_line_continuation_escape,
            };
        }
        if matches!(byte, b'\n' | b'\r') {
            break;
        }
        if byte == b'\\' {
            let escape_start = offset;
            offset += 1;
            if let Some(length) = line_continuation_len_at(bytes, offset) {
                has_line_continuation = true;
                offset += length;
            } else if let Some(character) = source.text[offset..].chars().next() {
                has_extended_escape |= bytes.get(escape_start..escape_start + 3) == Some(b"\\u{");
                has_non_line_continuation_escape = true;
                offset += character.len_utf8();
            } else {
                has_non_line_continuation_escape = true;
            }
            continue;
        }
        let Some(character) = source.text[offset..].chars().next() else {
            break;
        };
        offset += character.len_utf8();
    }
    StringProbe {
        end: offset,
        terminated: false,
        has_extended_escape,
        has_line_continuation,
        has_non_line_continuation_escape,
    }
}

fn cook_line_continuation_string(raw: &str) -> String {
    debug_assert!(raw.len() >= 2);
    let bytes = raw.as_bytes();
    let body_end = raw.len() - 1;
    let mut cooked = String::with_capacity(body_end.saturating_sub(1));
    let mut offset = 1;
    let mut segment_start = offset;
    while offset < body_end {
        if bytes[offset] == b'\\' {
            cooked.push_str(&raw[segment_start..offset]);
            offset += 1;
            let length = line_continuation_len_at(bytes, offset)
                .expect("sparse string metadata contains only line-continuation escapes");
            offset += length;
            segment_start = offset;
        } else {
            let character = raw[offset..]
                .chars()
                .next()
                .expect("offset remains on a character boundary");
            offset += character.len_utf8();
        }
    }
    cooked.push_str(&raw[segment_start..body_end]);
    cooked
}

fn line_continuation_len_at(bytes: &[u8], offset: usize) -> Option<usize> {
    match bytes.get(offset).copied() {
        Some(b'\r') if bytes.get(offset + 1) == Some(&b'\n') => Some(2),
        Some(b'\r' | b'\n') => Some(1),
        _ if is_unicode_line_separator_at(bytes, offset) => Some(3),
        _ => None,
    }
}

fn is_unicode_line_separator_at(bytes: &[u8], offset: usize) -> bool {
    matches!(
        bytes.get(offset..offset + 3),
        Some([0xe2, 0x80, 0xa8 | 0xa9])
    )
}

fn classify_extended_unicode_string(
    raw: &str,
    quote: u8,
    terminated: bool,
    recovery_at_line_break: bool,
) -> ExtendedUnicodeStringOwnership {
    if quote != b'"' || !raw.is_ascii() || raw.as_bytes().first() != Some(&quote) {
        return ExtendedUnicodeStringOwnership::Unowned;
    }
    let body_end = if terminated {
        if raw.as_bytes().last() != Some(&quote) {
            return ExtendedUnicodeStringOwnership::Unowned;
        }
        raw.len() - 1
    } else {
        raw.len()
    };
    let body = &raw.as_bytes()[1..body_end];
    let mut offset = 0;
    let mut valid_count = 0_usize;
    let mut invalid_count = 0_usize;
    let mut invalid_class = None;
    while offset < body.len() {
        if body.get(offset..offset + 3) != Some(b"\\u{") {
            return ExtendedUnicodeStringOwnership::Unowned;
        }
        offset += 3;
        let digits_start = offset;
        let mut value = 0_u64;
        while let Some(digit) = body.get(offset).copied().and_then(hex_value) {
            value = value.saturating_mul(16).saturating_add(u64::from(digit));
            offset += 1;
        }
        let outcome = if offset > digits_start {
            if body.get(offset) == Some(&b'}') {
                offset += 1;
                if value <= 0x10_ffff {
                    None
                } else {
                    Some(EscapeRecoveryClass::OverRangeClosed)
                }
            } else if offset == body.len() && value <= 0x10_ffff {
                Some(if terminated {
                    EscapeRecoveryClass::UnterminatedDigitsAtQuote
                } else if recovery_at_line_break {
                    EscapeRecoveryClass::UnterminatedDigitsAtLineBreak
                } else {
                    EscapeRecoveryClass::UnexpectedEndDigits
                })
            } else {
                return ExtendedUnicodeStringOwnership::Unowned;
            }
        } else if body.get(offset) == Some(&b'}') {
            offset += 1;
            Some(EscapeRecoveryClass::EmptyClosed)
        } else if offset == body.len() && terminated {
            Some(EscapeRecoveryClass::EmptyAtQuote)
        } else if body.get(offset) == Some(&b'-') {
            offset += 1;
            let negative_digits = offset;
            while body.get(offset).is_some_and(u8::is_ascii_hexdigit) {
                offset += 1;
            }
            if offset == negative_digits || body.get(offset) != Some(&b'}') {
                return ExtendedUnicodeStringOwnership::Unowned;
            }
            offset += 1;
            Some(EscapeRecoveryClass::NegativeClosed)
        } else if body
            .get(offset)
            .is_some_and(|byte| is_ascii_nonhex_alpha(*byte))
            && body.get(offset + 1) == Some(&b'}')
        {
            offset += 2;
            Some(EscapeRecoveryClass::IdentifierUnitClosed)
        } else {
            return ExtendedUnicodeStringOwnership::Unowned;
        };

        match outcome {
            None if invalid_class.is_none() => valid_count += 1,
            Some(class) if valid_count == 0 && invalid_class.is_none_or(|owned| owned == class) => {
                invalid_count += 1;
                invalid_class = Some(class);
            }
            None | Some(_) => return ExtendedUnicodeStringOwnership::Unowned,
        }
    }

    if let Some(class) = invalid_class {
        if invalid_count > 1 && class != EscapeRecoveryClass::IdentifierUnitClosed {
            return ExtendedUnicodeStringOwnership::Unowned;
        }
        if !terminated
            && !matches!(
                class,
                EscapeRecoveryClass::UnexpectedEndDigits
                    | EscapeRecoveryClass::UnterminatedDigitsAtLineBreak
            )
        {
            return ExtendedUnicodeStringOwnership::Unowned;
        }
        ExtendedUnicodeStringOwnership::HomogeneousInvalid(class)
    } else if valid_count > 0 && terminated {
        ExtendedUnicodeStringOwnership::ValidTerminatedRun
    } else if valid_count == 1 {
        ExtendedUnicodeStringOwnership::ValidUnterminatedSingle
    } else {
        ExtendedUnicodeStringOwnership::Unowned
    }
}

const fn is_ascii_nonhex_alpha(byte: u8) -> bool {
    matches!(byte, b'g'..=b'z' | b'G'..=b'Z')
}

#[allow(clippy::too_many_arguments)]
fn scan_extended_escape(
    source: &SourceText,
    bytes: &[u8],
    offset: &mut usize,
    cooked: &mut Vec<u16>,
    events: &mut Vec<StringDiagnosticEvent>,
    contains_invalid_escape: &mut bool,
    contains_extended_unicode_escape: &mut bool,
) {
    let escape_start = *offset;
    *offset += 3;
    let digits_start = *offset;
    let mut value = 0_u64;
    while let Some(digit) = bytes.get(*offset).copied().and_then(hex_value) {
        value = value.saturating_mul(16).saturating_add(u64::from(digit));
        *offset += 1;
    }

    let mut invalid = false;
    if *offset == digits_start {
        push_event(
            source,
            events,
            Span::new(source.id, *offset, *offset),
            "Hexadecimal digit expected.",
            1125,
        );
        invalid = true;
    } else if value > 0x10_ffff {
        push_event(
            source,
            events,
            Span::new(source.id, digits_start, *offset),
            "An extended Unicode escape value must be between 0x0 and 0x10FFFF inclusive.",
            1198,
        );
        invalid = true;
    }

    if *offset >= bytes.len() {
        push_event(
            source,
            events,
            Span::new(source.id, *offset, *offset),
            "Unexpected end of text.",
            1126,
        );
        invalid = true;
    } else if bytes[*offset] == b'}' {
        *offset += 1;
    } else {
        push_event(
            source,
            events,
            Span::new(source.id, *offset, *offset),
            "Unterminated Unicode escape sequence.",
            1199,
        );
        invalid = true;
    }

    if invalid {
        *contains_invalid_escape = true;
        append_utf16(&source.text[escape_start..*offset], cooked);
    } else {
        *contains_extended_unicode_escape = true;
        push_code_point(value as u32, cooked);
    }
}

fn push_event(
    source: &SourceText,
    events: &mut Vec<StringDiagnosticEvent>,
    span: Span,
    message: &'static str,
    code: u32,
) {
    if events
        .last()
        .is_some_and(|event| event.byte_span.start == span.start)
    {
        return;
    }
    events.push(StringDiagnosticEvent::new(source, span, message, code));
}

fn append_utf16(text: &str, cooked: &mut Vec<u16>) {
    cooked.extend(text.encode_utf16());
}

fn push_code_point(value: u32, cooked: &mut Vec<u16>) {
    if value <= 0xffff {
        cooked.push(value as u16);
    } else {
        let adjusted = value - 0x1_0000;
        cooked.push(0xd800 + (adjusted >> 10) as u16);
        cooked.push(0xdc00 + (adjusted & 0x3ff) as u16);
    }
}

const fn hex_value(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some((byte - b'0') as u32),
        b'a'..=b'f' => Some((byte - b'a' + 10) as u32),
        b'A'..=b'F' => Some((byte - b'A' + 10) as u32),
        _ => None,
    }
}

pub(crate) fn statements_form_extended_unicode_string_safe_file(
    source: &SourceText,
    statements: &[Statement],
    supported_literal_count: usize,
) -> bool {
    source.is_regular_typescript_source()
        && source_uses_supported_line_breaks(source)
        && statement_starts_at_supported_column(source, statements)
        && supported_literal_count == 1
        && statements_form_extended_unicode_string_variable_file(source, statements)
}

pub(crate) fn statements_form_extended_unicode_string_variable_file(
    source: &SourceText,
    statements: &[Statement],
) -> bool {
    let [
        Statement {
            kind: StatementKind::Variable(declaration),
            ..
        },
    ] = statements
    else {
        return false;
    };
    declaration.declaration_kind == VariableKind::Var
        && !declaration.exported
        && declaration.annotation.is_none()
        && is_plain_strict_binding_identifier(source.slice(declaration.name_span))
        && matches!(
            declaration.initializer.as_ref(),
            Some(Expression {
                kind: ExpressionKind::Literal(Literal::String(StringLiteral::Extended(literal))),
                ..
            }) if literal.validation_supported()
        )
}

pub(crate) fn comments_form_extended_unicode_string_safe_file(
    source: &SourceText,
    statements: &[Statement],
    comments: &[CommentTrivia],
) -> bool {
    if comments.is_empty() {
        return source.text.is_ascii();
    }
    let [statement] = statements else {
        return false;
    };
    statements_form_extended_unicode_string_variable_file(source, statements)
        && comments_form_contiguous_plain_leading_run(source, statement, comments)
        && source_is_ascii_outside_comments(source, comments)
}
