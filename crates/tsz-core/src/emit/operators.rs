use super::Printer;
use crate::syntax::{
    AssignmentOperator, BinaryOperator, Expression, ExpressionKind, FunctionLikeSyntax,
    UnaryOperator,
};

pub(super) const PREC_LOWEST: u8 = 0;
pub(super) const PREC_ASSIGNMENT: u8 = 1;
pub(super) const PREC_LOGICAL_OR: u8 = 2;
pub(super) const PREC_LOGICAL_AND: u8 = 3;
pub(super) const PREC_BITWISE_OR: u8 = 4;
pub(super) const PREC_BITWISE_XOR: u8 = 5;
pub(super) const PREC_BITWISE_AND: u8 = 6;
pub(super) const PREC_EQUALITY: u8 = 7;
pub(super) const PREC_RELATIONAL: u8 = 8;
pub(super) const PREC_SHIFT: u8 = 9;
pub(super) const PREC_ADDITIVE: u8 = 10;
pub(super) const PREC_MULTIPLICATIVE: u8 = 11;
pub(super) const PREC_UNARY: u8 = 12;
pub(super) const PREC_POSTFIX: u8 = 13;
pub(super) const PREC_PRIMARY: u8 = 14;

impl Printer<'_> {
    pub(super) fn expression_precedence(&self, expression: &Expression) -> u8 {
        match &expression.kind {
            ExpressionKind::FunctionLike(function) => match &function.syntax {
                FunctionLikeSyntax::Arrow(_) => PREC_ASSIGNMENT,
                FunctionLikeSyntax::Function { .. } => PREC_PRIMARY,
            },
            ExpressionKind::Assignment { .. } => PREC_ASSIGNMENT,
            ExpressionKind::Binary { operator, .. } => match operator {
                BinaryOperator::LogicalOr | BinaryOperator::NullishCoalesce => PREC_LOGICAL_OR,
                BinaryOperator::LogicalAnd => PREC_LOGICAL_AND,
                BinaryOperator::BitwiseOr => PREC_BITWISE_OR,
                BinaryOperator::BitwiseXor => PREC_BITWISE_XOR,
                BinaryOperator::BitwiseAnd => PREC_BITWISE_AND,
                BinaryOperator::Equals
                | BinaryOperator::NotEquals
                | BinaryOperator::StrictEquals
                | BinaryOperator::StrictNotEquals => PREC_EQUALITY,
                BinaryOperator::LessThan
                | BinaryOperator::LessThanEquals
                | BinaryOperator::GreaterThan
                | BinaryOperator::GreaterThanEquals
                | BinaryOperator::In
                | BinaryOperator::InstanceOf => PREC_RELATIONAL,
                BinaryOperator::LeftShift
                | BinaryOperator::SignedRightShift
                | BinaryOperator::UnsignedRightShift => PREC_SHIFT,
                BinaryOperator::Add | BinaryOperator::Subtract => PREC_ADDITIVE,
                BinaryOperator::Multiply | BinaryOperator::Divide | BinaryOperator::Remainder => {
                    PREC_MULTIPLICATIVE
                }
            },
            ExpressionKind::Unary { .. } => PREC_UNARY,
            ExpressionKind::Call { .. }
            | ExpressionKind::New { .. }
            | ExpressionKind::Member { .. }
            | ExpressionKind::ElementAccess { .. } => PREC_POSTFIX,
            ExpressionKind::As { expression, .. } => self.expression_precedence(expression),
            ExpressionKind::NonNull(inner) => self.expression_precedence(inner),
            ExpressionKind::Identifier { .. }
            | ExpressionKind::This
            | ExpressionKind::Literal(_)
            | ExpressionKind::Template(_)
            | ExpressionKind::RegularExpression(_)
            | ExpressionKind::Object(_)
            | ExpressionKind::Array(_)
            | ExpressionKind::Parenthesized(_)
            | ExpressionKind::Missing => PREC_PRIMARY,
        }
    }
}

pub(super) const fn binary_operator_text(operator: BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::Add => "+",
        BinaryOperator::Subtract => "-",
        BinaryOperator::Multiply => "*",
        BinaryOperator::Divide => "/",
        BinaryOperator::Remainder => "%",
        BinaryOperator::LessThan => "<",
        BinaryOperator::LessThanEquals => "<=",
        BinaryOperator::GreaterThan => ">",
        BinaryOperator::GreaterThanEquals => ">=",
        BinaryOperator::LeftShift => "<<",
        BinaryOperator::SignedRightShift => ">>",
        BinaryOperator::UnsignedRightShift => ">>>",
        BinaryOperator::Equals => "==",
        BinaryOperator::NotEquals => "!=",
        BinaryOperator::StrictEquals => "===",
        BinaryOperator::StrictNotEquals => "!==",
        BinaryOperator::LogicalAnd => "&&",
        BinaryOperator::LogicalOr => "||",
        BinaryOperator::BitwiseAnd => "&",
        BinaryOperator::BitwiseXor => "^",
        BinaryOperator::BitwiseOr => "|",
        BinaryOperator::NullishCoalesce => "??",
        BinaryOperator::In => "in",
        BinaryOperator::InstanceOf => "instanceof",
    }
}

pub(super) const fn assignment_operator_text(operator: AssignmentOperator) -> &'static str {
    match operator {
        AssignmentOperator::Assign => "=",
        AssignmentOperator::AddAssign => "+=",
    }
}

pub(super) const fn unary_operator_text(operator: UnaryOperator) -> &'static str {
    match operator {
        UnaryOperator::Plus => "+",
        UnaryOperator::Minus => "-",
        UnaryOperator::Not => "!",
        UnaryOperator::BitwiseNot => "~",
        UnaryOperator::TypeOf => "typeof",
        UnaryOperator::Void => "void",
        UnaryOperator::Delete => "delete",
        UnaryOperator::Await => "await",
    }
}

pub(super) const fn unary_operator_is_keyword(operator: UnaryOperator) -> bool {
    matches!(
        operator,
        UnaryOperator::TypeOf | UnaryOperator::Void | UnaryOperator::Delete | UnaryOperator::Await
    )
}
