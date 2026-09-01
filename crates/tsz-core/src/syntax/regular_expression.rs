use crate::source::{SourceText, Span};

use super::string_literal::{AuthoredEscape, decode_authored_escape};

/// Scanner-owned spelling and spans for one regular-expression token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegularExpressionLiteral {
    /// Exact authored token, including delimiters and flags when present.
    pub raw: String,
    /// Authored pattern text without slash delimiters.
    pub pattern: String,
    /// Authored flag run following the closing delimiter.
    pub flags: String,
    pub pattern_span: Span,
    pub flags_span: Span,
    pub terminated: bool,
    /// Whether unterminated recovery stopped at a physical line break.
    pub(crate) recovery_at_line_break: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegularExpressionIssue {
    UnknownFlag(Span),
    DuplicateFlag(Span),
    HexDigit(Span),
    UnicodeRange(Span),
    CloseBrace(Span),
}

impl RegularExpressionLiteral {
    /// Whether the bounded checker campaign owns this authored grammar.
    /// Advanced groups, escapes, and Unicode-set operations fail closed.
    #[must_use]
    pub fn validation_supported(&self) -> bool {
        self.analyze(&mut |_| {}).is_some()
    }

    pub(crate) fn validation_issues(&self) -> Option<Vec<RegularExpressionIssue>> {
        let mut issues = Vec::new();
        self.analyze(&mut |issue| issues.push(issue))?;
        Some(issues)
    }

    fn analyze(&self, report: &mut impl FnMut(RegularExpressionIssue)) -> Option<()> {
        (!self.recovery_at_line_break
            && !(!self.terminated
                && self
                    .pattern
                    .as_bytes()
                    .last()
                    .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b';')))
        .then_some(())?;
        analyze_flags(&self.flags, self.flags_span, report)?;
        if self.flags.contains('u') {
            analyze_extended_unicode_escapes(&self.pattern, self.pattern_span, report)
        } else {
            basic_ascii_pattern_supported(&self.pattern)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ScannedRegularExpressionLiteral {
    pub span: Span,
    literal: RegularExpressionLiteral,
}

impl ScannedRegularExpressionLiteral {
    pub(super) fn from_source(
        source: &SourceText,
        start: usize,
        pattern_end: usize,
        flags_start: usize,
        end: usize,
        terminated: bool,
        recovery_at_line_break: bool,
    ) -> Self {
        let span = Span::new(source.id, start, end);
        let pattern_span = Span::new(source.id, start.saturating_add(1), pattern_end);
        let flags_span = Span::new(source.id, flags_start, end);
        Self {
            span,
            literal: RegularExpressionLiteral {
                raw: source.slice(span).to_string(),
                pattern: source.slice(pattern_span).to_string(),
                flags: source.slice(flags_span).to_string(),
                pattern_span,
                flags_span,
                terminated,
                recovery_at_line_break,
            },
        }
    }

    pub(super) fn syntax_literal(&self) -> RegularExpressionLiteral {
        self.literal.clone()
    }
}

fn analyze_flags(
    flags: &str,
    span: Span,
    report: &mut impl FnMut(RegularExpressionIssue),
) -> Option<()> {
    let mut seen = [false; 5];
    for (offset, flag) in flags.char_indices() {
        (!matches!(flag, 'd' | 's' | 'v')
            && (flag.is_ascii_alphanumeric() || matches!(flag, '_' | '$')))
        .then_some(())?;
        let flag_span = relative_span(span, offset, flag.len_utf8());
        let Some(index) = "gimuy".find(flag) else {
            report(RegularExpressionIssue::UnknownFlag(flag_span));
            continue;
        };
        if std::mem::replace(&mut seen[index], true) {
            report(RegularExpressionIssue::DuplicateFlag(flag_span));
        }
    }
    Some(())
}

fn analyze_extended_unicode_escapes(
    pattern: &str,
    span: Span,
    report: &mut impl FnMut(RegularExpressionIssue),
) -> Option<()> {
    let mut index = 0;
    while index < pattern.len() {
        (pattern.as_bytes().get(index) == Some(&b'\\')).then_some(())?;
        let AuthoredEscape::ExtendedUnicode {
            digits_start,
            digits_end,
            value,
            closed,
        } = decode_authored_escape(pattern, &mut index, pattern.len())
        else {
            return None;
        };
        if closed {
            if digits_start == digits_end {
                report(RegularExpressionIssue::HexDigit(relative_span(
                    span, digits_end, 1,
                )));
            } else if value > 0x10_ffff {
                report(RegularExpressionIssue::UnicodeRange(relative_span(
                    span,
                    digits_start,
                    digits_end - digits_start,
                )));
            }
            continue;
        }
        let close = pattern.as_bytes()[digits_end..]
            .iter()
            .position(|byte| *byte == b'}')
            .map(|close| digits_end + close)?;
        let payload = &pattern.as_bytes()[digits_start..close];
        let one_nonhex_letter = matches!(payload, [byte]
            if byte.is_ascii_alphabetic() && !byte.is_ascii_hexdigit());
        let negative_hex = matches!(payload, [b'-', rest @ ..]
            if !rest.is_empty() && rest.iter().all(u8::is_ascii_hexdigit));
        (one_nonhex_letter || negative_hex).then_some(())?;
        report(RegularExpressionIssue::HexDigit(relative_span(
            span,
            digits_start,
            1,
        )));
        report(RegularExpressionIssue::CloseBrace(relative_span(
            span, close, 1,
        )));
        index = close + 1;
    }
    (!pattern.is_empty()).then_some(())
}

fn basic_ascii_pattern_supported(pattern: &str) -> Option<()> {
    let bytes = pattern.as_bytes();
    let (mut index, mut group_depth, mut can_quantify) = (0, 0_usize, false);
    while let Some(byte) = bytes.get(index).copied() {
        (byte.is_ascii() && !matches!(byte, b'\n' | b'\r')).then_some(())?;
        match byte {
            b'\\' => {
                let escaped = bytes
                    .get(index + 1)
                    .copied()
                    .filter(|escaped| basic_escape_supported(*escaped))?;
                can_quantify = !matches!(escaped, b'b' | b'B');
                index += 2;
            }
            b'[' => {
                index = scan_basic_class(bytes, index + 1)?;
                can_quantify = true;
            }
            b']' | b'{' | b'}' => return None,
            b'(' => {
                (bytes.get(index + 1) != Some(&b'?')).then_some(())?;
                group_depth += 1;
                can_quantify = false;
                index += 1;
            }
            b')' => {
                (group_depth != 0).then_some(())?;
                group_depth -= 1;
                can_quantify = true;
                index += 1;
            }
            b'*' | b'+' | b'?' => {
                can_quantify.then_some(())?;
                can_quantify = false;
                index += 1;
            }
            b'|' | b'^' | b'$' => {
                can_quantify = false;
                index += 1;
            }
            _ => {
                can_quantify = true;
                index += 1;
            }
        }
    }
    (group_depth == 0).then_some(())
}

fn basic_escape_supported(escaped: u8) -> bool {
    b"dDsSwWbBfnrtv^$\\.*+?()[]{}|/-".contains(&escaped)
}

fn scan_basic_class(bytes: &[u8], mut index: usize) -> Option<usize> {
    let mut items = Vec::new();
    let mut has_content = false;
    while let Some(byte) = bytes.get(index).copied() {
        if !byte.is_ascii() || matches!(byte, b'\n' | b'\r') {
            return None;
        }
        match byte {
            b']' => return (has_content && ranges_are_supported(&items)).then_some(index + 1),
            b'[' => return None,
            b'&' if bytes.get(index + 1) == Some(&b'&') => return None,
            b'-' if bytes.get(index + 1) == Some(&b'-') => return None,
            b'\\' => {
                let escaped = bytes
                    .get(index + 1)
                    .copied()
                    .filter(|escaped| basic_escape_supported(*escaped))?;
                items.push(classify_class_escape(escaped));
                has_content = true;
                index += 2;
            }
            b'^' if !has_content => {
                has_content = true;
                index += 1;
            }
            literal => {
                items.push(if literal == b'-' {
                    BasicClassItem::Hyphen
                } else {
                    BasicClassItem::Literal(literal)
                });
                has_content = true;
                index += 1;
            }
        }
    }
    None
}

#[derive(Clone, Copy)]
enum BasicClassItem {
    Literal(u8),
    CharacterClassEscape,
    OtherEscape,
    Hyphen,
}

const fn classify_class_escape(escaped: u8) -> BasicClassItem {
    if matches!(escaped, b'd' | b'D' | b's' | b'S' | b'w' | b'W') {
        BasicClassItem::CharacterClassEscape
    } else if escaped.is_ascii_punctuation() {
        BasicClassItem::Literal(escaped)
    } else {
        BasicClassItem::OtherEscape
    }
}

fn ranges_are_supported(items: &[BasicClassItem]) -> bool {
    for (index, item) in items.iter().enumerate() {
        if !matches!(item, BasicClassItem::Hyphen) || index == 0 || index + 1 == items.len() {
            continue;
        }
        match (items[index - 1], items[index + 1]) {
            (BasicClassItem::CharacterClassEscape, _) => {}
            (BasicClassItem::Literal(left), BasicClassItem::Literal(right)) if left <= right => {}
            _ => return false,
        }
    }
    true
}

const fn relative_span(base: Span, start: usize, length: usize) -> Span {
    let start = base.start.saturating_add(start as u32);
    Span {
        file: base.file,
        start,
        end: start.saturating_add(length as u32),
    }
}

#[cfg(test)]
#[path = "../../rewrite-tests/regular_expression_unit.rs"]
mod tests;
