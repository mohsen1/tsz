use crate::semantics::types::{Completion, DeferredUnaryOperator, TypeId, TypeKind};

use super::Checker;

impl Checker<'_> {
    pub(super) fn evaluate_unary(
        &mut self,
        operator: DeferredUnaryOperator,
        operand: TypeId,
        depth: usize,
    ) -> Completion<TypeId> {
        let operand = completed!(self.force_operand(operand, depth));
        if operator == DeferredUnaryOperator::NonNull {
            return self.store.non_nullable(operand);
        }
        if operator == DeferredUnaryOperator::Await {
            return Completion::Complete(operand);
        }
        match self.store.kind(operand).clone() {
            TypeKind::LiteralNumber(_, _) if operator == DeferredUnaryOperator::Plus => {
                Completion::Complete(operand)
            }
            TypeKind::LiteralNumber(value, provenance)
                if operator == DeferredUnaryOperator::Minus =>
            {
                Completion::Complete(self.store.negated_numeric_literal(value, provenance))
            }
            TypeKind::Number | TypeKind::LiteralNumber(_, _) | TypeKind::Any => {
                Completion::Complete(self.store.builtins.number)
            }
            TypeKind::BigInt if operator != DeferredUnaryOperator::Plus => {
                Completion::Complete(self.store.builtins.bigint)
            }
            TypeKind::Error | TypeKind::Invalid(_) => Completion::Complete(operand),
            _ => Completion::Deferred,
        }
    }
}
