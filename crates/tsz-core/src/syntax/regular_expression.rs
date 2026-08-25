use crate::source::{SourceText, Span};

use super::scanner::is_plain_strict_binding_identifier;
use super::{
    CommentTrivia, Expression, ExpressionKind, Statement, StatementKind, VariableKind,
    comments_form_contiguous_plain_leading_run, source_is_ascii_outside_comments,
    source_uses_supported_line_breaks, statement_starts_at_supported_column,
};

/// Scanner-owned syntax for one regular-expression token.
///
/// The checker consumes the authored pattern and flags directly. Keeping their
/// spans here avoids rescanning rendered output when it selects diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegularExpressionLiteral {
    /// Exact authored token, beginning with `/` and including flags when the
    /// closing delimiter was present.
    pub raw: String,
    /// Authored pattern text without slash delimiters.
    pub pattern: String,
    /// Authored flag run following the closing delimiter.
    pub flags: String,
    pub pattern_span: Span,
    pub flags_span: Span,
    pub terminated: bool,
    /// The unterminated token stopped immediately before a physical line
    /// terminator rather than at end of file.
    pub(crate) recovery_at_line_break: bool,
}

impl RegularExpressionLiteral {
    /// Whether the initial checker campaign owns validation for this syntax.
    ///
    /// This is intentionally narrower than ECMAScript regular expressions.
    /// Advanced groups, property/named/back-reference escapes, Unicode-set
    /// mode, and class-set operations stay Deferred. Unicode mode is limited
    /// to concatenated extended escapes whose shape is sufficient for the
    /// checker's owned diagnostics; ambiguous malformed payloads fail closed.
    #[must_use]
    pub fn validation_supported(&self) -> bool {
        if self.recovery_at_line_break
            || (!self.terminated
                && self
                    .pattern
                    .as_bytes()
                    .last()
                    .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b';'))
        {
            return false;
        }
        if self
            .flags
            .bytes()
            .any(|flag| matches!(flag, b'd' | b's' | b'v'))
            || !self
                .flags
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
        {
            return false;
        }
        if self.flags.contains('u') {
            return unicode_extended_escape_sequence_supported(&self.pattern);
        }
        if contains_extended_unicode_escape(&self.pattern) {
            return false;
        }
        basic_ascii_pattern_supported(&self.pattern) && class_ranges_supported(&self.pattern)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ScannedRegularExpressionLiteral {
    pub span: Span,
    literal: RegularExpressionLiteral,
}

impl ScannedRegularExpressionLiteral {
    pub(super) fn terminated(
        source: &SourceText,
        start: usize,
        pattern_end: usize,
        flags_start: usize,
        end: usize,
    ) -> Self {
        Self::new(source, start, pattern_end, flags_start, end, true, false)
    }

    pub(super) fn unterminated(source: &SourceText, start: usize, end: usize) -> Self {
        Self::new(source, start, end, end, end, false, false)
    }

    pub(super) fn unterminated_at_line_break(
        source: &SourceText,
        start: usize,
        end: usize,
    ) -> Self {
        Self::new(source, start, end, end, end, false, true)
    }

    fn new(
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

pub(crate) fn statements_form_regular_expression_expression_file(statements: &[Statement]) -> bool {
    matches!(
        statements,
        [Statement {
            kind: StatementKind::Expression(Expression {
                kind: ExpressionKind::RegularExpression(_),
                ..
            }),
            ..
        }]
    )
}

direct_var_literal_predicates!(
    statements_form_regular_expression_safe_file,
    statements_form_regular_expression_variable_file,
    comments_form_regular_expression_safe_file,
    ExpressionKind::RegularExpression(_),
    statements_form_regular_expression_expression_file
);

fn basic_ascii_pattern_supported(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut index = 0;
    let mut group_depth = 0_usize;
    let mut in_class = false;
    let mut class_has_content = false;
    let mut can_quantify = false;

    while let Some(byte) = bytes.get(index).copied() {
        if !byte.is_ascii() || matches!(byte, b'\n' | b'\r') {
            return false;
        }
        match byte {
            b'\\' => {
                let Some(escaped) = bytes.get(index + 1).copied() else {
                    return false;
                };
                if escaped == b'u' && bytes.get(index + 2) == Some(&b'{') {
                    index += 3;
                    while bytes.get(index).is_some_and(|byte| *byte != b'}') {
                        if !bytes[index].is_ascii() || matches!(bytes[index], b'\n' | b'\r') {
                            return false;
                        }
                        index += 1;
                    }
                    if bytes.get(index) != Some(&b'}') {
                        return false;
                    }
                    index += 1;
                    can_quantify = true;
                    class_has_content |= in_class;
                    continue;
                }
                if matches!(escaped, b'p' | b'P' | b'k' | b'u' | b'x' | b'1'..=b'9')
                    || !(escaped.is_ascii_alphabetic()
                        && matches!(
                            escaped,
                            b'd' | b'D'
                                | b's'
                                | b'S'
                                | b'w'
                                | b'W'
                                | b'b'
                                | b'B'
                                | b'f'
                                | b'n'
                                | b'r'
                                | b't'
                                | b'v'
                        ))
                        && !matches!(
                            escaped,
                            b'^' | b'$'
                                | b'\\'
                                | b'.'
                                | b'*'
                                | b'+'
                                | b'?'
                                | b'('
                                | b')'
                                | b'['
                                | b']'
                                | b'{'
                                | b'}'
                                | b'|'
                                | b'/'
                                | b'-'
                        )
                {
                    return false;
                }
                index += 2;
                can_quantify = in_class || !matches!(escaped, b'b' | b'B');
                class_has_content |= in_class;
                continue;
            }
            b'[' => {
                if in_class {
                    return false;
                }
                in_class = true;
                class_has_content = false;
                can_quantify = false;
            }
            b']' => {
                if !in_class || !class_has_content {
                    return false;
                }
                in_class = false;
                can_quantify = true;
            }
            b'&' if in_class && bytes.get(index + 1) == Some(&b'&') => return false,
            b'-' if in_class && bytes.get(index + 1) == Some(&b'-') => return false,
            _ if in_class => class_has_content = true,
            b'(' => {
                if bytes.get(index + 1) == Some(&b'?') {
                    return false;
                }
                group_depth += 1;
                can_quantify = false;
            }
            b')' => {
                if group_depth == 0 {
                    return false;
                }
                group_depth -= 1;
                can_quantify = true;
            }
            b'*' | b'+' | b'?' => {
                if !can_quantify {
                    return false;
                }
                can_quantify = false;
            }
            b'{' | b'}' => return false,
            b'|' | b'^' | b'$' => can_quantify = false,
            _ => can_quantify = true,
        }
        index += 1;
    }

    !in_class && group_depth == 0
}

fn contains_extended_unicode_escape(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut index = 0;
    while let Some(byte) = bytes.get(index) {
        if *byte != b'\\' {
            index += 1;
            continue;
        }
        if bytes.get(index + 1) == Some(&b'u') && bytes.get(index + 2) == Some(&b'{') {
            return true;
        }
        index += usize::from(bytes.get(index + 1).is_some()) + 1;
    }
    false
}

fn unicode_extended_escape_sequence_supported(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut index = 0;
    let mut atom_count = 0_usize;

    while index < bytes.len() {
        if bytes.get(index..index + 3) != Some(br"\u{") {
            return false;
        }
        index += 3;
        let payload_start = index;
        while bytes.get(index).is_some_and(|byte| *byte != b'}') {
            let byte = bytes[index];
            if !byte.is_ascii() || matches!(byte, b'\n' | b'\r') {
                return false;
            }
            index += 1;
        }
        if bytes.get(index) != Some(&b'}') {
            return false;
        }
        let payload = &bytes[payload_start..index];
        let all_hex = payload.iter().all(u8::is_ascii_hexdigit);
        let one_nonhex_letter = matches!(payload, [byte]
            if byte.is_ascii_alphabetic() && !byte.is_ascii_hexdigit());
        let negative_hex = matches!(payload, [b'-', rest @ ..]
            if !rest.is_empty() && rest.iter().all(u8::is_ascii_hexdigit));
        if !all_hex && !one_nonhex_letter && !negative_hex {
            return false;
        }
        index += 1;
        atom_count += 1;
    }

    atom_count != 0
}

#[derive(Clone, Copy)]
enum BasicClassItem {
    Literal(u8),
    CharacterClassEscape,
    OtherEscape,
    Hyphen,
}

fn class_ranges_supported(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut index = 0;
    while let Some(byte) = bytes.get(index).copied() {
        match byte {
            b'\\' if bytes.get(index + 1) == Some(&b'u') && bytes.get(index + 2) == Some(&b'{') => {
                index += 3;
                while bytes.get(index).is_some_and(|byte| *byte != b'}') {
                    index += 1;
                }
                if bytes.get(index) != Some(&b'}') {
                    return false;
                }
                index += 1;
            }
            b'\\' => index += usize::from(bytes.get(index + 1).is_some()) + 1,
            b'[' => {
                index += 1;
                let Some(end) = collect_supported_class_items(bytes, index) else {
                    return false;
                };
                index = end;
            }
            _ => index += 1,
        }
    }
    true
}

fn collect_supported_class_items(bytes: &[u8], mut index: usize) -> Option<usize> {
    let mut items = Vec::new();
    if bytes.get(index) == Some(&b'^') {
        index += 1;
    }
    while let Some(byte) = bytes.get(index).copied() {
        match byte {
            b']' => return ranges_are_supported(&items).then_some(index + 1),
            b'\\' => {
                let escaped = bytes.get(index + 1).copied()?;
                if escaped == b'u' && bytes.get(index + 2) == Some(&b'{') {
                    index += 3;
                    while bytes.get(index).is_some_and(|byte| *byte != b'}') {
                        index += 1;
                    }
                    if bytes.get(index) != Some(&b'}') {
                        return None;
                    }
                    index += 1;
                    items.push(BasicClassItem::OtherEscape);
                } else {
                    index += 2;
                    items.push(
                        if matches!(escaped, b'd' | b'D' | b's' | b'S' | b'w' | b'W') {
                            BasicClassItem::CharacterClassEscape
                        } else if escaped.is_ascii_punctuation() {
                            BasicClassItem::Literal(escaped)
                        } else {
                            BasicClassItem::OtherEscape
                        },
                    );
                }
            }
            b'-' => {
                items.push(BasicClassItem::Hyphen);
                index += 1;
            }
            b'[' => return None,
            literal => {
                items.push(BasicClassItem::Literal(literal));
                index += 1;
            }
        }
    }
    None
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

#[cfg(test)]
#[path = "../../rewrite-tests/regular_expression_unit.rs"]
mod tests;
