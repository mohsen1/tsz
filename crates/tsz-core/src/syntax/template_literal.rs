use crate::source::{SourceText, Span};

use super::descendant_walk::{ExpressionRoot, ExpressionTraversal, contains_matching_expression};
use super::scanner::is_plain_strict_binding_identifier;
use super::string_literal::hex_value;
use super::{
    ClassDeclaration, Expression, ExpressionKind, Literal, Statement, StatementKind, VariableKind,
};

/// Syntax-owned spelling and template value for a complete, valid,
/// no-substitution template literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoSubstitutionTemplateLiteral {
    /// The exact authored token, including both backticks.
    pub raw: String,
    /// The ECMAScript template value after escape cooking and newline folding.
    pub cooked: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ScannedTemplateLiteral {
    pub span: Span,
    raw: String,
    cooked: TemplateCooked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TemplateCooked {
    Valid(String),
    InvalidEscape(TemplateEscapeDiagnostic),
    Unrepresentable,
    Unterminated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TemplateEscapeDiagnostic {
    pub relative_start: u32,
    pub length: u32,
    pub code: u32,
    pub message: String,
}

impl ScannedTemplateLiteral {
    pub(super) fn terminated(span: Span, raw: &str) -> Self {
        let cooked = match cook(raw) {
            Ok(cooked) => TemplateCooked::Valid(cooked),
            Err(CookError::Diagnostic(diagnostic)) => TemplateCooked::InvalidEscape(diagnostic),
            Err(CookError::Unrepresentable) => TemplateCooked::Unrepresentable,
        };
        Self {
            span,
            raw: raw.to_string(),
            cooked,
        }
    }

    pub(super) fn unterminated(span: Span, raw: &str) -> Self {
        Self {
            span,
            raw: raw.to_string(),
            cooked: TemplateCooked::Unterminated,
        }
    }

    pub(super) fn syntax_literal(&self) -> Option<NoSubstitutionTemplateLiteral> {
        let TemplateCooked::Valid(cooked) = &self.cooked else {
            return None;
        };
        Some(NoSubstitutionTemplateLiteral {
            raw: self.raw.clone(),
            cooked: cooked.clone(),
        })
    }

    pub(super) fn escape_diagnostic(&self) -> Option<TemplateEscapeDiagnostic> {
        let TemplateCooked::InvalidEscape(diagnostic) = &self.cooked else {
            return None;
        };
        Some(diagnostic.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CookError {
    Diagnostic(TemplateEscapeDiagnostic),
    Unrepresentable,
}

fn cook(raw: &str) -> Result<String, CookError> {
    if raw.len() < 2 || !raw.starts_with('`') || !raw.ends_with('`') {
        return Err(CookError::Unrepresentable);
    }
    let body = &raw[1..raw.len() - 1];
    let bytes = body.as_bytes();
    let mut cooked = String::with_capacity(body.len());
    let mut offset = 0;
    while offset < bytes.len() {
        match bytes[offset] {
            b'\r' => {
                offset += 1;
                if bytes.get(offset) == Some(&b'\n') {
                    offset += 1;
                }
                cooked.push('\n');
            }
            b'\\' => {
                offset += 1;
                cook_escape(body, bytes, &mut offset, &mut cooked)?;
            }
            _ => push_character(body, &mut offset, &mut cooked)?,
        }
    }
    Ok(cooked)
}

fn cook_escape(
    body: &str,
    bytes: &[u8],
    offset: &mut usize,
    cooked: &mut String,
) -> Result<(), CookError> {
    let Some(byte) = bytes.get(*offset).copied() else {
        return Err(unterminated_unicode_escape(*offset));
    };
    match byte {
        b'\r' => {
            *offset += 1;
            if bytes.get(*offset) == Some(&b'\n') {
                *offset += 1;
            }
        }
        b'\n' => *offset += 1,
        b'0' if !bytes.get(*offset + 1).is_some_and(u8::is_ascii_digit) => {
            *offset += 1;
            cooked.push('\0');
        }
        b'0'..=b'7' => return Err(legacy_octal_escape(bytes, *offset)),
        b'8' | b'9' => {
            let escaped = char::from(byte);
            return Err(template_diagnostic(
                offset.saturating_sub(1) + 1,
                2,
                1488,
                format!("Escape sequence '\\{escaped}' is not allowed."),
            ));
        }
        b'b' => push_ascii_escape(offset, cooked, '\u{0008}'),
        b'f' => push_ascii_escape(offset, cooked, '\u{000c}'),
        b'n' => push_ascii_escape(offset, cooked, '\n'),
        b'r' => push_ascii_escape(offset, cooked, '\r'),
        b't' => push_ascii_escape(offset, cooked, '\t'),
        b'v' => push_ascii_escape(offset, cooked, '\u{000b}'),
        b'x' => {
            *offset += 1;
            let value = take_fixed_hex(bytes, offset, 2)?;
            cooked.push(char::from_u32(value).ok_or(CookError::Unrepresentable)?);
        }
        b'u' => {
            *offset += 1;
            let value = if bytes.get(*offset) == Some(&b'{') {
                take_code_point_escape(bytes, offset)?
            } else {
                take_fixed_hex(bytes, offset, 4)?
            };
            cooked.push(char::from_u32(value).ok_or(CookError::Unrepresentable)?);
        }
        _ => {
            let start = *offset;
            let character = body[start..]
                .chars()
                .next()
                .ok_or_else(|| unterminated_unicode_escape(start))?;
            *offset += character.len_utf8();
            if matches!(character, '\u{2028}' | '\u{2029}') {
                return Ok(());
            }
            cooked.push(character);
        }
    }
    Ok(())
}

fn push_ascii_escape(offset: &mut usize, cooked: &mut String, character: char) {
    *offset += 1;
    cooked.push(character);
}

fn take_fixed_hex(bytes: &[u8], offset: &mut usize, width: usize) -> Result<u32, CookError> {
    let mut value = 0;
    for _ in 0..width {
        let Some(digit) = bytes.get(*offset).copied().and_then(hex_value) else {
            return Err(hexadecimal_digit_expected(*offset));
        };
        value = value * 16 + digit;
        *offset += 1;
    }
    Ok(value)
}

fn take_code_point_escape(bytes: &[u8], offset: &mut usize) -> Result<u32, CookError> {
    debug_assert_eq!(bytes.get(*offset), Some(&b'{'));
    *offset += 1;
    let digits_start = *offset;
    let mut value = 0_u64;
    while let Some(digit) = bytes.get(*offset).copied().and_then(hex_value) {
        value = value.saturating_mul(16).saturating_add(u64::from(digit));
        *offset += 1;
    }
    if *offset == digits_start {
        return Err(hexadecimal_digit_expected(*offset));
    }
    if bytes.get(*offset).is_none() {
        return Err(unterminated_unicode_escape(*offset));
    }
    if bytes.get(*offset) != Some(&b'}') {
        return Err(unterminated_unicode_escape(*offset));
    }
    if value > 0x10_ffff {
        return Err(template_diagnostic(
            digits_start + 1,
            *offset - digits_start,
            1198,
            "An extended Unicode escape value must be between 0x0 and 0x10FFFF inclusive."
                .to_string(),
        ));
    }
    *offset += 1;
    Ok(value as u32)
}

fn legacy_octal_escape(bytes: &[u8], digit_start: usize) -> CookError {
    let first = bytes[digit_start];
    let mut end = digit_start + 1;
    if first <= b'3'
        && bytes
            .get(end)
            .is_some_and(|byte| matches!(byte, b'0'..=b'7'))
    {
        end += 1;
    }
    if bytes
        .get(end)
        .is_some_and(|byte| matches!(byte, b'0'..=b'7'))
    {
        end += 1;
    }
    let value = bytes[digit_start..end]
        .iter()
        .fold(0_u32, |value, byte| value * 8 + u32::from(*byte - b'0'));
    template_diagnostic(
        digit_start,
        end - digit_start + 1,
        1487,
        format!("Octal escape sequences are not allowed. Use the syntax '\\x{value:02x}'."),
    )
}

fn hexadecimal_digit_expected(body_offset: usize) -> CookError {
    template_diagnostic(
        body_offset + 1,
        1,
        1125,
        "Hexadecimal digit expected.".to_string(),
    )
}

fn unterminated_unicode_escape(body_offset: usize) -> CookError {
    template_diagnostic(
        body_offset + 1,
        1,
        1199,
        "Unterminated Unicode escape sequence.".to_string(),
    )
}

const fn template_diagnostic(
    relative_start: usize,
    length: usize,
    code: u32,
    message: String,
) -> CookError {
    CookError::Diagnostic(TemplateEscapeDiagnostic {
        relative_start: relative_start as u32,
        length: length as u32,
        code,
        message,
    })
}

fn push_character(body: &str, offset: &mut usize, cooked: &mut String) -> Result<(), CookError> {
    let character = body[*offset..]
        .chars()
        .next()
        .ok_or(CookError::Unrepresentable)?;
    *offset += character.len_utf8();
    cooked.push(character);
    Ok(())
}

pub(crate) fn expression_contains_no_substitution_template(expression: &Expression) -> bool {
    contains_matching_expression(
        ExpressionRoot::Expression(expression),
        ExpressionTraversal::All,
        is_template_literal,
    )
}

pub(crate) fn statements_contain_no_substitution_template(statements: &[Statement]) -> bool {
    contains_matching_expression(
        ExpressionRoot::Statements(statements),
        ExpressionTraversal::All,
        is_template_literal,
    )
}

pub(crate) fn statements_form_no_substitution_template_safe_file(
    source: &SourceText,
    statements: &[Statement],
    valid_template_literal_count: usize,
) -> bool {
    source.is_regular_typescript_source()
        && statements.len() == valid_template_literal_count
        && (statements_form_no_substitution_template_expression_file(statements)
            || statements_form_no_substitution_template_variable_file(source, statements))
}

pub(crate) fn statements_form_no_substitution_template_expression_file(
    statements: &[Statement],
) -> bool {
    !statements.is_empty()
        && statements.iter().all(|statement| {
            matches!(
                &statement.kind,
                StatementKind::Expression(Expression {
                    kind: ExpressionKind::Literal(Literal::NoSubstitutionTemplate(_)),
                    ..
                })
            )
        })
}

pub(crate) fn statements_form_no_substitution_template_variable_file(
    source: &SourceText,
    statements: &[Statement],
) -> bool {
    !statements.is_empty()
        && statements.iter().all(|statement| {
            let StatementKind::Variable(declaration) = &statement.kind else {
                return false;
            };
            declaration.declaration_kind == VariableKind::Var
                && !declaration.exported
                && declaration.annotation.is_none()
                && is_plain_strict_binding_identifier(source.slice(declaration.name_span))
                && matches!(
                    declaration.initializer.as_ref(),
                    Some(Expression {
                        kind: ExpressionKind::Literal(Literal::NoSubstitutionTemplate(_)),
                        ..
                    })
                )
        })
}

pub(crate) fn class_contains_no_substitution_template(declaration: &ClassDeclaration) -> bool {
    contains_matching_expression(
        ExpressionRoot::Class(declaration),
        ExpressionTraversal::All,
        is_template_literal,
    )
}

const fn is_template_literal(expression: &Expression) -> bool {
    matches!(
        &expression.kind,
        ExpressionKind::Literal(Literal::NoSubstitutionTemplate(_))
    )
}

#[cfg(test)]
#[path = "../../rewrite-tests/template_literal_unit.rs"]
mod tests;
