use crate::syntax::{BinaryOperator, UnaryOperator};

pub(super) const PREC_LOWEST: u8 = 0;
pub(super) const PREC_ASSIGNMENT: u8 = 1;
pub(super) const PREC_LOGICAL_OR: u8 = 2;
pub(super) const PREC_LOGICAL_AND: u8 = 3;
pub(super) const PREC_BITWISE_OR: u8 = 4;
pub(super) const PREC_BITWISE_AND: u8 = 5;
pub(super) const PREC_EQUALITY: u8 = 6;
pub(super) const PREC_RELATIONAL: u8 = 7;
pub(super) const PREC_SHIFT: u8 = 8;
pub(super) const PREC_ADDITIVE: u8 = 9;
pub(super) const PREC_MULTIPLICATIVE: u8 = 10;
pub(super) const PREC_UNARY: u8 = 11;
pub(super) const PREC_POSTFIX: u8 = 12;
pub(super) const PREC_PRIMARY: u8 = 13;

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
        BinaryOperator::UnsignedRightShift => ">>>",
        BinaryOperator::Equals => "==",
        BinaryOperator::NotEquals => "!=",
        BinaryOperator::StrictEquals => "===",
        BinaryOperator::StrictNotEquals => "!==",
        BinaryOperator::LogicalAnd => "&&",
        BinaryOperator::LogicalOr => "||",
        BinaryOperator::BitwiseAnd => "&",
        BinaryOperator::BitwiseOr => "|",
        BinaryOperator::NullishCoalesce => "??",
        BinaryOperator::In => "in",
        BinaryOperator::InstanceOf => "instanceof",
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
