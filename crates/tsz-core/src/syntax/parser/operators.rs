use super::super::{
    BinaryOperator, Expression, ExpressionKind, ParserRecoveryKind, SourceSyntaxFact, TokenKind,
};
use super::Parser;
use crate::source::Span;
impl Parser<'_> {
    pub(super) fn parse_expression(&mut self) -> Expression {
        let mut expression = self.parse_assignment_expression();
        while self.at(TokenKind::Comma) {
            let operator_span = self.bump().span;
            let right = self.parse_assignment_expression();
            expression = Expression {
                id: self.alloc_node(),
                span: expression.span.merge(right.span),
                kind: ExpressionKind::Binary {
                    left: Box::new(expression),
                    operator: BinaryOperator::Comma,
                    operator_span,
                    right: Box::new(right),
                },
            };
        }
        expression
    }
    pub(super) fn parse_conditional_expression(&mut self) -> Expression {
        let condition = self.parse_binary_expression(0);
        if !self.speculating && self.at(TokenKind::GreaterThanGreaterThanGreaterThanEquals) {
            self.source_syntax_facts
                .insert(SourceSyntaxFact::UnsignedRightShiftAssignmentRecovery);
        }
        if !self.at(TokenKind::Question) {
            return condition;
        }
        let question_span = self.bump().span;
        let when_true = self.parse_assignment_expression();
        let (colon_span, when_false) = if self.at(TokenKind::Colon) {
            let colon_span = self.bump().span;
            (Some(colon_span), self.parse_assignment_expression())
        } else {
            self.error_current("':' expected.", 1005);
            let end = self.previous().span.end as usize;
            let missing = Span::new(self.source.id, end, end);
            (None, self.missing_expression(missing))
        };
        let span = condition.span.merge(when_false.span);
        if colon_span.is_none() {
            self.note_recovery(
                ParserRecoveryKind::ConditionalExpression,
                question_span,
                span,
            );
        }
        Expression {
            id: self.alloc_node(),
            span,
            kind: ExpressionKind::Conditional {
                condition: Box::new(condition),
                question_span,
                when_true: Box::new(when_true),
                colon_span,
                when_false: Box::new(when_false),
            },
        }
    }
    pub(super) fn observe_unsigned_shift_prefix_recovery(&mut self, kind: TokenKind) {
        if self.speculating {
            return;
        }
        let fact = match kind {
            TokenKind::GreaterThanGreaterThanGreaterThan => {
                Some(SourceSyntaxFact::UnsignedRightShiftOperandRecovery)
            }
            TokenKind::GreaterThanGreaterThanGreaterThanEquals => {
                Some(SourceSyntaxFact::UnsignedRightShiftAssignmentRecovery)
            }
            _ => None,
        };
        self.source_syntax_facts.extend(fact);
    }
}
pub(super) const fn binary_operator(kind: TokenKind) -> Option<(BinaryOperator, u8)> {
    let operator = match kind {
        TokenKind::BarBar => (BinaryOperator::LogicalOr, 1),
        TokenKind::QuestionQuestion => (BinaryOperator::NullishCoalesce, 1),
        TokenKind::AmpersandAmpersand => (BinaryOperator::LogicalAnd, 2),
        TokenKind::Bar => (BinaryOperator::BitwiseOr, 3),
        TokenKind::Caret => (BinaryOperator::BitwiseXor, 4),
        TokenKind::Ampersand => (BinaryOperator::BitwiseAnd, 5),
        TokenKind::EqualsEquals => (BinaryOperator::Equals, 6),
        TokenKind::BangEquals => (BinaryOperator::NotEquals, 6),
        TokenKind::EqualsEqualsEquals => (BinaryOperator::StrictEquals, 6),
        TokenKind::BangEqualsEquals => (BinaryOperator::StrictNotEquals, 6),
        TokenKind::LessThan => (BinaryOperator::LessThan, 7),
        TokenKind::LessThanEquals => (BinaryOperator::LessThanEquals, 7),
        TokenKind::GreaterThan => (BinaryOperator::GreaterThan, 7),
        TokenKind::GreaterThanEquals => (BinaryOperator::GreaterThanEquals, 7),
        TokenKind::In => (BinaryOperator::In, 7),
        TokenKind::InstanceOf => (BinaryOperator::InstanceOf, 7),
        TokenKind::LessThanLessThan => (BinaryOperator::LeftShift, 8),
        TokenKind::GreaterThanGreaterThan => (BinaryOperator::SignedRightShift, 8),
        TokenKind::GreaterThanGreaterThanGreaterThan => (BinaryOperator::UnsignedRightShift, 8),
        TokenKind::Plus => (BinaryOperator::Add, 9),
        TokenKind::Minus => (BinaryOperator::Subtract, 9),
        TokenKind::Star => (BinaryOperator::Multiply, 10),
        TokenKind::Slash => (BinaryOperator::Divide, 10),
        TokenKind::Percent => (BinaryOperator::Remainder, 10),
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
        | ExpressionKind::NonNull(object)
        | ExpressionKind::Unary {
            operand: object, ..
        }
        | ExpressionKind::Parenthesized(object) => expression_has_recovered_left_edge(object),
        ExpressionKind::Binary { left, .. }
        | ExpressionKind::Conditional {
            condition: left, ..
        }
        | ExpressionKind::Assignment { left, .. }
        | ExpressionKind::As {
            expression: left, ..
        } => expression_has_recovered_left_edge(left),
        ExpressionKind::Identifier { .. }
        | ExpressionKind::This
        | ExpressionKind::Literal(_)
        | ExpressionKind::Template(_)
        | ExpressionKind::RegularExpression(_)
        | ExpressionKind::Object(_)
        | ExpressionKind::Array(_)
        | ExpressionKind::FunctionLike(_) => false,
    }
}
