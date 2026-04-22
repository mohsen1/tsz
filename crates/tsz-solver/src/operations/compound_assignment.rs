use crate::{TypeDatabase, TypeId, type_queries};
use tsz_scanner::SyntaxKind;

pub const fn is_compound_assignment_operator(operator_token: u16) -> bool {
    matches!(
        operator_token,
        k if k == SyntaxKind::PlusEqualsToken as u16
            || k == SyntaxKind::MinusEqualsToken as u16
            || k == SyntaxKind::AsteriskEqualsToken as u16
            || k == SyntaxKind::AsteriskAsteriskEqualsToken as u16
            || k == SyntaxKind::SlashEqualsToken as u16
            || k == SyntaxKind::PercentEqualsToken as u16
            || k == SyntaxKind::LessThanLessThanEqualsToken as u16
            || k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
            || k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
            || k == SyntaxKind::AmpersandEqualsToken as u16
            || k == SyntaxKind::BarEqualsToken as u16
            || k == SyntaxKind::CaretEqualsToken as u16
            || k == SyntaxKind::AmpersandAmpersandEqualsToken as u16
            || k == SyntaxKind::BarBarEqualsToken as u16
            || k == SyntaxKind::QuestionQuestionEqualsToken as u16
    )
}

/// Check if a token is any assignment operator: simple `=` or one of the
/// compound forms (`+=`, `-=`, `**=`, `&&=`, `??=`, etc.).
pub const fn is_assignment_operator(operator_token: u16) -> bool {
    operator_token == SyntaxKind::EqualsToken as u16
        || is_compound_assignment_operator(operator_token)
}

pub const fn map_compound_assignment_to_binary(operator_token: u16) -> Option<&'static str> {
    match operator_token {
        k if k == SyntaxKind::PlusEqualsToken as u16 => Some("+"),
        k if k == SyntaxKind::MinusEqualsToken as u16 => Some("-"),
        k if k == SyntaxKind::AsteriskEqualsToken as u16 => Some("*"),
        k if k == SyntaxKind::AsteriskAsteriskEqualsToken as u16 => Some("**"),
        k if k == SyntaxKind::SlashEqualsToken as u16 => Some("/"),
        k if k == SyntaxKind::PercentEqualsToken as u16 => Some("%"),
        k if k == SyntaxKind::LessThanLessThanEqualsToken as u16 => Some("<<"),
        k if k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16 => Some(">>"),
        k if k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16 => Some(">>>"),
        k if k == SyntaxKind::AmpersandEqualsToken as u16 => Some("&"),
        k if k == SyntaxKind::BarEqualsToken as u16 => Some("|"),
        k if k == SyntaxKind::CaretEqualsToken as u16 => Some("^"),
        k if k == SyntaxKind::AmpersandAmpersandEqualsToken as u16 => Some("&&"),
        k if k == SyntaxKind::BarBarEqualsToken as u16 => Some("||"),
        k if k == SyntaxKind::QuestionQuestionEqualsToken as u16 => Some("??"),
        _ => None,
    }
}

pub fn fallback_compound_assignment_result(
    db: &dyn TypeDatabase,
    operator_token: u16,
    rhs_literal_type: Option<TypeId>,
) -> Option<TypeId> {
    match operator_token {
        k if k == SyntaxKind::MinusEqualsToken as u16
            || k == SyntaxKind::AsteriskEqualsToken as u16
            || k == SyntaxKind::AsteriskAsteriskEqualsToken as u16
            || k == SyntaxKind::SlashEqualsToken as u16
            || k == SyntaxKind::PercentEqualsToken as u16
            || k == SyntaxKind::LessThanLessThanEqualsToken as u16
            || k == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
            || k == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
            || k == SyntaxKind::AmpersandEqualsToken as u16
            || k == SyntaxKind::BarEqualsToken as u16
            || k == SyntaxKind::CaretEqualsToken as u16 =>
        {
            Some(TypeId::NUMBER)
        }
        k if k == SyntaxKind::PlusEqualsToken as u16 => rhs_literal_type.and_then(|literal| {
            if literal == TypeId::NUMBER || type_queries::is_number_literal(db, literal) {
                Some(TypeId::NUMBER)
            } else {
                None
            }
        }),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../tests/compound_assignment_tests.rs"]
mod tests;
