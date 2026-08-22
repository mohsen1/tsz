use super::super::{BinaryOperator, Expression, ExpressionKind, TokenKind};

pub(super) const fn binary_operator(kind: TokenKind) -> Option<(BinaryOperator, u8)> {
    let operator = match kind {
        TokenKind::BarBar => (BinaryOperator::LogicalOr, 1),
        TokenKind::QuestionQuestion => (BinaryOperator::NullishCoalesce, 1),
        TokenKind::AmpersandAmpersand => (BinaryOperator::LogicalAnd, 2),
        TokenKind::EqualsEquals => (BinaryOperator::Equals, 3),
        TokenKind::BangEquals => (BinaryOperator::NotEquals, 3),
        TokenKind::EqualsEqualsEquals => (BinaryOperator::StrictEquals, 3),
        TokenKind::BangEqualsEquals => (BinaryOperator::StrictNotEquals, 3),
        TokenKind::LessThan => (BinaryOperator::LessThan, 4),
        TokenKind::LessThanEquals => (BinaryOperator::LessThanEquals, 4),
        TokenKind::GreaterThan => (BinaryOperator::GreaterThan, 4),
        TokenKind::GreaterThanEquals => (BinaryOperator::GreaterThanEquals, 4),
        TokenKind::In => (BinaryOperator::In, 4),
        TokenKind::InstanceOf => (BinaryOperator::InstanceOf, 4),
        TokenKind::Plus => (BinaryOperator::Add, 5),
        TokenKind::Minus => (BinaryOperator::Subtract, 5),
        TokenKind::Star => (BinaryOperator::Multiply, 6),
        TokenKind::Slash => (BinaryOperator::Divide, 6),
        TokenKind::Percent => (BinaryOperator::Remainder, 6),
        _ => return None,
    };
    Some(operator)
}

pub(super) fn expression_has_recovered_left_edge(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Missing => true,
        ExpressionKind::Call { callee, .. } | ExpressionKind::New { callee, .. } => {
            expression_has_recovered_left_edge(callee)
        }
        ExpressionKind::Member { object, .. }
        | ExpressionKind::Unary {
            operand: object, ..
        }
        | ExpressionKind::Parenthesized(object) => expression_has_recovered_left_edge(object),
        ExpressionKind::Binary { left, .. }
        | ExpressionKind::Assignment { left, .. }
        | ExpressionKind::As {
            expression: left, ..
        } => expression_has_recovered_left_edge(left),
        ExpressionKind::Identifier { .. }
        | ExpressionKind::Literal(_)
        | ExpressionKind::RegularExpression(_)
        | ExpressionKind::Object(_)
        | ExpressionKind::Array(_)
        | ExpressionKind::Arrow { .. } => false,
    }
}
