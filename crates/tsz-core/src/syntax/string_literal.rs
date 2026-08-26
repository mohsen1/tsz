use crate::diagnostics::Diagnostic;
use crate::source::{SourceText, Span};

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

    pub(crate) fn as_string(&self) -> Option<String> {
        String::from_utf16(&self.0).ok()
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringLiteral {
    Plain(String),
    Extended(ExtendedUnicodeStringLiteral),
}

/// Structural result of scanning one authored escape. String and template
/// consumers retain their distinct recovery diagnostics and value domains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AuthoredEscape {
    Empty,
    CodePoint(u32),
    LegacyOctal(u8),
    NonOctalDecimal(u8),
    MissingFixedHex,
    ExtendedUnicode {
        digits_start: usize,
        digits_end: usize,
        value: u64,
        closed: bool,
    },
    MissingCharacter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StringDiagnosticEvent {
    byte_span: Span,
    diagnostic: Diagnostic,
}

impl StringDiagnosticEvent {
    fn new(source: &SourceText, span: Span, message: impl Into<String>, code: u32) -> Self {
        Self {
            byte_span: span,
            diagnostic: Diagnostic::at(source, span, message.into(), code),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ScannedStringLiteral {
    pub span: Span,
    literal: ExtendedUnicodeStringLiteral,
}

impl ScannedStringLiteral {
    pub(super) fn syntax_literal(&self) -> ExtendedUnicodeStringLiteral {
        self.literal.clone()
    }
}

pub(super) struct ScannedStringToken {
    pub end: usize,
    pub diagnostics: Vec<Diagnostic>,
    pub extended_literal: Option<ScannedStringLiteral>,
    pub cooked_literal: Option<ScannedCookedStringLiteral>,
}

/// Scanner-owned semantic value for a terminated string. The authored token
/// remains in source text for emit; parser consumers use this cooked value for
/// identity, including lone UTF-16 surrogate units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ScannedCookedStringLiteral {
    pub span: Span,
    pub cooked: Utf16String,
}

pub(super) fn scan_ordinary_string_literal(
    source: &SourceText,
    start: usize,
    quote: u8,
) -> ScannedStringToken {
    let bytes = source.text.as_bytes();
    let probe = probe_ordinary_string_literal(source, start, quote);
    let mut ordinary_events = scan_ordinary_escape_diagnostics(source, start, probe);
    if !probe.has_extended_escape {
        let span = Span::new(source.id, start, probe.end);
        let cooked_literal = probe
            .terminated
            .then(|| cook_terminated_ordinary_string(source.slice(span)))
            .flatten()
            .map(|cooked| ScannedCookedStringLiteral { span, cooked });
        if !probe.terminated {
            ordinary_events.push(StringDiagnosticEvent::new(
                source,
                span,
                "Unterminated string literal.",
                1002,
            ));
        }
        return ScannedStringToken {
            end: probe.end,
            diagnostics: ordinary_events
                .into_iter()
                .map(|event| event.diagnostic)
                .collect(),
            extended_literal: None,
            cooked_literal,
        };
    }

    let mut offset = start + 1;
    let mut segment_start = offset;
    let mut cooked = Vec::new();
    let mut events = Vec::new();
    let mut contains_invalid_escape = false;
    let mut contains_extended_unicode_escape = false;
    let mut terminated = false;

    while let Some(byte) = bytes.get(offset).copied() {
        if byte == quote {
            append_utf16(&source.text[segment_start..offset], &mut cooked);
            offset += 1;
            terminated = true;
            break;
        }
        if matches!(byte, b'\n' | b'\r') {
            append_utf16(&source.text[segment_start..offset], &mut cooked);
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

    ordinary_events.extend(events);
    ordinary_events.sort_by_key(|event| event.byte_span.start);
    let events = ordinary_events;

    let span = Span::new(source.id, start, offset);
    debug_assert_eq!(probe.end, offset);
    debug_assert_eq!(probe.terminated, terminated);
    let raw = source.slice(span).to_string();
    let cooked_literal = terminated
        .then(|| cook_terminated_ordinary_string(&raw))
        .flatten()
        .map(|cooked| ScannedCookedStringLiteral { span, cooked });
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
            },
        }),
        cooked_literal,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StringProbe {
    end: usize,
    terminated: bool,
    has_extended_escape: bool,
}

fn probe_ordinary_string_literal(source: &SourceText, start: usize, quote: u8) -> StringProbe {
    let bytes = source.text.as_bytes();
    let mut offset = start + 1;
    let mut has_extended_escape = false;
    while let Some(byte) = bytes.get(offset).copied() {
        if byte == quote {
            return StringProbe {
                end: offset + 1,
                terminated: true,
                has_extended_escape,
            };
        }
        if matches!(byte, b'\n' | b'\r') {
            break;
        }
        if byte == b'\\' {
            let escape_start = offset;
            offset += 1;
            if let Some(length) = line_continuation_len_at(bytes, offset) {
                offset += length;
            } else if let Some(character) = source.text[offset..].chars().next() {
                has_extended_escape |= bytes.get(escape_start..escape_start + 3) == Some(b"\\u{");
                offset += character.len_utf8();
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
    }
}

fn scan_ordinary_escape_diagnostics(
    source: &SourceText,
    start: usize,
    probe: StringProbe,
) -> Vec<StringDiagnosticEvent> {
    let bytes = source.text.as_bytes();
    let body_end = probe.end.saturating_sub(usize::from(probe.terminated));
    let mut offset = start + 1;
    let mut diagnostics = Vec::new();
    while offset < body_end {
        if bytes[offset] != b'\\' {
            offset += source.text[offset..]
                .chars()
                .next()
                .map_or(1, char::len_utf8);
            continue;
        }
        let escape_start = offset;
        match decode_authored_escape(&source.text, &mut offset, body_end) {
            AuthoredEscape::LegacyOctal(value) => diagnostics.push(StringDiagnosticEvent::new(
                source,
                Span::new(source.id, escape_start, offset),
                format!("Octal escape sequences are not allowed. Use the syntax '\\x{value:02x}'."),
                1487,
            )),
            AuthoredEscape::NonOctalDecimal(digit) => diagnostics.push(StringDiagnosticEvent::new(
                source,
                Span::new(source.id, escape_start, offset),
                format!("Escape sequence '\\{}' is not allowed.", char::from(digit)),
                1488,
            )),
            AuthoredEscape::MissingFixedHex => diagnostics.push(StringDiagnosticEvent::new(
                source,
                Span::new(source.id, offset, offset),
                "Hexadecimal digit expected.",
                1125,
            )),
            _ => {}
        }
    }
    diagnostics
}

pub(super) fn decode_authored_escape(
    text: &str,
    offset: &mut usize,
    limit: usize,
) -> AuthoredEscape {
    let bytes = text.as_bytes();
    debug_assert_eq!(bytes.get(*offset), Some(&b'\\'));
    *offset += 1;
    if let Some(length) = line_continuation_len_at(bytes, *offset)
        && *offset + length <= limit
    {
        *offset += length;
        return AuthoredEscape::Empty;
    }
    let Some(escaped) = bytes.get(*offset).copied().filter(|_| *offset < limit) else {
        return AuthoredEscape::MissingCharacter;
    };
    *offset += 1;
    let fixed_hex = |offset: &mut usize, width| {
        let mut value = 0;
        for _ in 0..width {
            let digit = (*offset < limit)
                .then(|| bytes[*offset])
                .and_then(hex_value)?;
            value = value * 16 + digit;
            *offset += 1;
        }
        Some(value)
    };
    match escaped {
        b'0' if *offset >= limit || !bytes[*offset].is_ascii_digit() => {
            AuthoredEscape::CodePoint(0)
        }
        b'0'..=b'7' => {
            let digit_start = *offset - 1;
            if escaped <= b'3' && *offset < limit && matches!(bytes[*offset], b'0'..=b'7') {
                *offset += 1;
            }
            if *offset < limit && matches!(bytes[*offset], b'0'..=b'7') {
                *offset += 1;
            }
            let value = bytes[digit_start..*offset]
                .iter()
                .fold(0_u8, |value, digit| value * 8 + (*digit - b'0'));
            AuthoredEscape::LegacyOctal(value)
        }
        b'8' | b'9' => AuthoredEscape::NonOctalDecimal(escaped),
        b'b' => AuthoredEscape::CodePoint(0x08),
        b'f' => AuthoredEscape::CodePoint(0x0c),
        b'n' => AuthoredEscape::CodePoint(u32::from(b'\n')),
        b'r' => AuthoredEscape::CodePoint(u32::from(b'\r')),
        b't' => AuthoredEscape::CodePoint(u32::from(b'\t')),
        b'v' => AuthoredEscape::CodePoint(0x0b),
        b'x' => {
            fixed_hex(offset, 2).map_or(AuthoredEscape::MissingFixedHex, AuthoredEscape::CodePoint)
        }
        b'u' if *offset >= limit || bytes[*offset] != b'{' => {
            fixed_hex(offset, 4).map_or(AuthoredEscape::MissingFixedHex, AuthoredEscape::CodePoint)
        }
        b'u' => {
            *offset += 1;
            let digits_start = *offset;
            let mut value = 0_u64;
            while *offset < limit {
                let Some(digit) = hex_value(bytes[*offset]) else {
                    break;
                };
                value = value.saturating_mul(16).saturating_add(u64::from(digit));
                *offset += 1;
            }
            let digits_end = *offset;
            let closed = *offset < limit && bytes[*offset] == b'}';
            *offset += usize::from(closed);
            AuthoredEscape::ExtendedUnicode {
                digits_start,
                digits_end,
                value,
                closed,
            }
        }
        _ => {
            let character = text[*offset - 1..limit]
                .chars()
                .next()
                .expect("the escaped source byte starts a character");
            *offset += character.len_utf8() - 1;
            AuthoredEscape::CodePoint(character as u32)
        }
    }
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

fn push_decoded_escape(escape: AuthoredEscape, cooked: &mut Vec<u16>) -> Option<()> {
    let value = match escape {
        AuthoredEscape::Empty => return Some(()),
        AuthoredEscape::CodePoint(value) => value,
        AuthoredEscape::LegacyOctal(value) | AuthoredEscape::NonOctalDecimal(value) => {
            u32::from(value)
        }
        AuthoredEscape::ExtendedUnicode {
            digits_start,
            digits_end,
            value,
            closed: true,
        } if digits_start < digits_end && value <= 0x10_ffff => value as u32,
        AuthoredEscape::MissingFixedHex
        | AuthoredEscape::ExtendedUnicode { .. }
        | AuthoredEscape::MissingCharacter => return None,
    };
    push_code_point(value, cooked);
    Some(())
}

fn cook_terminated_ordinary_string(raw: &str) -> Option<Utf16String> {
    if raw.len() < 2 || raw.as_bytes().first() != raw.as_bytes().last() {
        return None;
    }
    let bytes = raw.as_bytes();
    let body_end = bytes.len() - 1;
    let mut cooked = Vec::with_capacity(body_end.saturating_sub(1));
    let mut offset = 1;
    let mut segment_start = offset;
    while offset < body_end {
        if bytes[offset] == b'\\' {
            append_utf16(&raw[segment_start..offset], &mut cooked);
            let escape = decode_authored_escape(raw, &mut offset, body_end);
            push_decoded_escape(escape, &mut cooked)?;
            segment_start = offset;
        } else {
            let character = raw[offset..]
                .chars()
                .next()
                .expect("offset remains on a character boundary");
            offset += character.len_utf8();
        }
    }
    append_utf16(&raw[segment_start..body_end], &mut cooked);
    Some(Utf16String(cooked))
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

pub(super) const fn hex_value(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some((byte - b'0') as u32),
        b'a'..=b'f' => Some((byte - b'a' + 10) as u32),
        b'A'..=b'F' => Some((byte - b'A' + 10) as u32),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../rewrite-tests/ordinary_string_escape_unit.rs"]
mod tests;
