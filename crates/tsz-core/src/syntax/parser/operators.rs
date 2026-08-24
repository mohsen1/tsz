use super::super::{BinaryOperator, Expression, ExpressionKind, TokenKind};
use super::Parser;

impl Parser<'_> {
    pub(super) fn parse_expression(&mut self) -> Expression {
        let expression = self.parse_assignment_expression();
        self.observe_unmodeled_template_tail(&expression);
        expression
    }
}

pub(super) const fn binary_operator(kind: TokenKind) -> Option<(BinaryOperator, u8)> {
    let operator = match kind {
        TokenKind::BarBar => (BinaryOperator::LogicalOr, 1),
        TokenKind::QuestionQuestion => (BinaryOperator::NullishCoalesce, 1),
        TokenKind::AmpersandAmpersand => (BinaryOperator::LogicalAnd, 2),
        TokenKind::Bar => (BinaryOperator::BitwiseOr, 3),
        TokenKind::Ampersand => (BinaryOperator::BitwiseAnd, 4),
        TokenKind::EqualsEquals => (BinaryOperator::Equals, 5),
        TokenKind::BangEquals => (BinaryOperator::NotEquals, 5),
        TokenKind::EqualsEqualsEquals => (BinaryOperator::StrictEquals, 5),
        TokenKind::BangEqualsEquals => (BinaryOperator::StrictNotEquals, 5),
        TokenKind::LessThan => (BinaryOperator::LessThan, 6),
        TokenKind::LessThanEquals => (BinaryOperator::LessThanEquals, 6),
        TokenKind::GreaterThan => (BinaryOperator::GreaterThan, 6),
        TokenKind::GreaterThanEquals => (BinaryOperator::GreaterThanEquals, 6),
        TokenKind::In => (BinaryOperator::In, 6),
        TokenKind::InstanceOf => (BinaryOperator::InstanceOf, 6),
        TokenKind::Plus => (BinaryOperator::Add, 7),
        TokenKind::Minus => (BinaryOperator::Subtract, 7),
        TokenKind::Star => (BinaryOperator::Multiply, 8),
        TokenKind::Slash => (BinaryOperator::Divide, 8),
        TokenKind::Percent => (BinaryOperator::Remainder, 8),
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
        | ExpressionKind::ElementAccess { object, .. }
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
        | ExpressionKind::This
        | ExpressionKind::Literal(_)
        | ExpressionKind::RegularExpression(_)
        | ExpressionKind::Object(_)
        | ExpressionKind::Array(_)
        | ExpressionKind::FunctionLike(_) => false,
    }
}
