use super::descendant_walk::{ExpressionRoot, ExpressionTraversal, contains_matching_expression};
use super::string_literal::{AuthoredEscape, decode_authored_escape};
use super::{Expression, ExpressionKind, Literal, TokenKind};

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
pub(super) struct TemplateEscapeDiagnostic {
    pub relative_start: u32,
    pub length: u32,
    pub code: u32,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CookError {
    Diagnostic(TemplateEscapeDiagnostic),
    Unrepresentable,
}

pub(super) fn scan_no_substitution_template(
    raw: &str,
) -> Result<NoSubstitutionTemplateLiteral, CookError> {
    let body = raw
        .strip_prefix('`')
        .and_then(|raw| raw.strip_suffix('`'))
        .ok_or(CookError::Unrepresentable)?;
    Ok(NoSubstitutionTemplateLiteral {
        raw: raw.to_string(),
        cooked: cook(body)?,
    })
}

pub(super) fn scan_template_chunk(raw: &str, kind: TokenKind) -> Result<String, CookError> {
    let body = match kind {
        TokenKind::TemplateHead => raw.strip_prefix('`').and_then(|raw| raw.strip_suffix("${")),
        TokenKind::TemplateMiddle => raw.strip_prefix('}').and_then(|raw| raw.strip_suffix("${")),
        TokenKind::TemplateTail => raw.strip_prefix('}').and_then(|raw| raw.strip_suffix('`')),
        _ => None,
    }
    .ok_or(CookError::Unrepresentable)?;
    cook(body)
}

fn cook(body: &str) -> Result<String, CookError> {
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
                let escape_start = offset;
                let escape = decode_authored_escape(body, &mut offset, bytes.len());
                cook_decoded_escape(escape, escape_start, offset, &mut cooked)?;
            }
            _ => push_character(body, &mut offset, &mut cooked)?,
        }
    }
    Ok(cooked)
}

fn cook_decoded_escape(
    escape: AuthoredEscape,
    escape_start: usize,
    escape_end: usize,
    cooked: &mut String,
) -> Result<(), CookError> {
    let diagnostic =
        |start, length, code, message| Err(template_diagnostic(start, length, code, message));
    let value = match escape {
        AuthoredEscape::Empty => return Ok(()),
        AuthoredEscape::CodePoint(value) => value,
        AuthoredEscape::LegacyOctal(value) => {
            return diagnostic(
                escape_start + 1,
                escape_end - escape_start,
                1487,
                format!("Octal escape sequences are not allowed. Use the syntax '\\x{value:02x}'."),
            );
        }
        AuthoredEscape::NonOctalDecimal(digit) => {
            return diagnostic(
                escape_start + 1,
                escape_end - escape_start,
                1488,
                format!("Escape sequence '\\{}' is not allowed.", char::from(digit)),
            );
        }
        AuthoredEscape::MissingFixedHex => {
            return diagnostic(
                escape_end + 1,
                1,
                1125,
                "Hexadecimal digit expected.".to_string(),
            );
        }
        AuthoredEscape::ExtendedUnicode {
            digits_start,
            digits_end,
            value,
            closed,
        } => {
            if digits_start == digits_end {
                return diagnostic(
                    digits_end + 1,
                    1,
                    1125,
                    "Hexadecimal digit expected.".to_string(),
                );
            }
            if !closed {
                return diagnostic(
                    digits_end + 1,
                    1,
                    1199,
                    "Unterminated Unicode escape sequence.".to_string(),
                );
            }
            if value > 0x10_ffff {
                return diagnostic(
                    digits_start + 1,
                    digits_end - digits_start,
                    1198,
                    "An extended Unicode escape value must be between 0x0 and 0x10FFFF inclusive."
                        .to_string(),
                );
            }
            value as u32
        }
        AuthoredEscape::MissingCharacter => {
            return diagnostic(
                escape_end + 1,
                1,
                1199,
                "Unterminated Unicode escape sequence.".to_string(),
            );
        }
    };
    cooked.push(char::from_u32(value).ok_or(CookError::Unrepresentable)?);
    Ok(())
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

const fn is_template_literal(expression: &Expression) -> bool {
    matches!(
        &expression.kind,
        ExpressionKind::Literal(Literal::NoSubstitutionTemplate(_)) | ExpressionKind::Template(_)
    )
}

#[cfg(test)]
#[path = "../../rewrite-tests/template_literal_unit.rs"]
mod tests;
