use crate::source::{SourceText, Span};

use super::scanner::is_plain_strict_binding_identifier;
use super::{
    ArrowBody, ClassDeclaration, ClassMemberKind, Expression, ExpressionKind, FunctionDeclaration,
    Literal, Parameter, Statement, StatementKind, SwitchClauseKind, VariableKind,
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
    match &expression.kind {
        ExpressionKind::Literal(Literal::NoSubstitutionTemplate(_)) => true,
        ExpressionKind::Identifier { .. }
        | ExpressionKind::This
        | ExpressionKind::Literal(_)
        | ExpressionKind::RegularExpression(_)
        | ExpressionKind::Missing => false,
        ExpressionKind::Object(properties) => properties
            .iter()
            .any(|property| expression_contains_no_substitution_template(&property.value)),
        ExpressionKind::Array(elements) => elements
            .iter()
            .any(expression_contains_no_substitution_template),
        ExpressionKind::Call {
            callee, arguments, ..
        }
        | ExpressionKind::New {
            callee, arguments, ..
        } => {
            expression_contains_no_substitution_template(callee)
                || arguments
                    .iter()
                    .any(expression_contains_no_substitution_template)
        }
        ExpressionKind::Member { object, .. }
        | ExpressionKind::Unary {
            operand: object, ..
        }
        | ExpressionKind::As {
            expression: object, ..
        }
        | ExpressionKind::Parenthesized(object) => {
            expression_contains_no_substitution_template(object)
        }
        ExpressionKind::ElementAccess { object, index } => {
            expression_contains_no_substitution_template(object)
                || expression_contains_no_substitution_template(index)
        }
        ExpressionKind::Arrow {
            parameters, body, ..
        } => {
            parameters_contain_template(parameters)
                || match body {
                    ArrowBody::Expression(expression) => {
                        expression_contains_no_substitution_template(expression)
                    }
                    ArrowBody::Block(statements) => {
                        statements_contain_no_substitution_template(statements)
                    }
                }
        }
        ExpressionKind::Binary { left, right, .. } | ExpressionKind::Assignment { left, right } => {
            expression_contains_no_substitution_template(left)
                || expression_contains_no_substitution_template(right)
        }
    }
}

pub(crate) fn statements_contain_no_substitution_template(statements: &[Statement]) -> bool {
    statements
        .iter()
        .any(statement_contains_no_substitution_template)
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
    declaration.members.iter().any(|member| match &member.kind {
        ClassMemberKind::Property { initializer, .. } => initializer
            .as_ref()
            .is_some_and(expression_contains_no_substitution_template),
        ClassMemberKind::Constructor {
            parameters, body, ..
        }
        | ClassMemberKind::Method {
            parameters, body, ..
        } => {
            parameters_contain_template(parameters)
                || statements_contain_no_substitution_template(body)
        }
    })
}

fn statement_contains_no_substitution_template(statement: &Statement) -> bool {
    match &statement.kind {
        StatementKind::Variable(declaration) => declaration
            .initializer
            .as_ref()
            .is_some_and(expression_contains_no_substitution_template),
        StatementKind::Expression(expression) => {
            expression_contains_no_substitution_template(expression)
        }
        StatementKind::Return(expression) => expression
            .as_ref()
            .is_some_and(expression_contains_no_substitution_template),
        StatementKind::If(statement) => {
            expression_contains_no_substitution_template(&statement.condition)
                || statement_contains_no_substitution_template(&statement.then_statement)
                || statement
                    .else_statement
                    .as_deref()
                    .is_some_and(statement_contains_no_substitution_template)
        }
        StatementKind::Switch(statement) => {
            expression_contains_no_substitution_template(&statement.expression)
                || statement.clauses.iter().any(|clause| {
                    matches!(
                        &clause.kind,
                        SwitchClauseKind::Case(expression)
                            if expression_contains_no_substitution_template(expression)
                    ) || statements_contain_no_substitution_template(&clause.statements)
                })
        }
        StatementKind::Block(statements) => statements_contain_no_substitution_template(statements),
        StatementKind::Function(declaration) => function_contains_template(declaration),
        StatementKind::Class(declaration) => class_contains_no_substitution_template(declaration),
        StatementKind::Export(declaration) => declaration
            .assignment
            .as_ref()
            .is_some_and(expression_contains_no_substitution_template),
        StatementKind::Import(_)
        | StatementKind::TypeAlias(_)
        | StatementKind::Interface(_)
        | StatementKind::Break(_)
        | StatementKind::Continue(_)
        | StatementKind::Empty
        | StatementKind::Unknown => false,
    }
}

fn function_contains_template(declaration: &FunctionDeclaration) -> bool {
    parameters_contain_template(&declaration.parameters)
        || statements_contain_no_substitution_template(&declaration.body)
}

fn parameters_contain_template(parameters: &[Parameter]) -> bool {
    parameters.iter().any(|parameter| {
        parameter
            .initializer
            .as_ref()
            .is_some_and(expression_contains_no_substitution_template)
    })
}

const fn hex_value(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some((byte - b'0') as u32),
        b'a'..=b'f' => Some((byte - b'a' + 10) as u32),
        b'A'..=b'F' => Some((byte - b'A' + 10) as u32),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FileId;

    fn cooked(raw: &str) -> Option<String> {
        ScannedTemplateLiteral::terminated(Span::new(FileId(0), 0, raw.len()), raw)
            .syntax_literal()
            .map(|literal| literal.cooked)
    }

    #[test]
    fn cooks_control_unicode_identity_and_line_continuation_escapes() {
        assert_eq!(
            cooked(r"`\0\x19\u001f\u{20}\t\v\f\b\r\n\world\``"),
            Some("\0\u{19}\u{1f} \t\u{b}\u{c}\u{8}\r\nworld`".to_string())
        );
        assert_eq!(cooked("`a\\\r\nb\\\nc`").as_deref(), Some("abc"));
        assert_eq!(cooked("`a\rb\r\nc`").as_deref(), Some("a\nb\nc"));
    }

    #[test]
    fn rejects_legacy_octal_malformed_unicode_and_unrepresentable_surrogates() {
        for raw in [
            r"`\00`",
            r"`\8`",
            r"`\x0`",
            r"`\u123`",
            r"`\u{}`",
            r"`\u{110000}`",
            r"`\uD800`",
        ] {
            assert_eq!(cooked(raw), None, "{raw}");
        }
    }
}
