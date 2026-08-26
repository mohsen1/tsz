use crate::program::SemanticCompletion;
use crate::syntax::{Literal, NumberLiteral, StringLiteral};

use super::super::relation::RelationContext;
use super::{Checker, Completion, LiteralProvenance, TypeId, TypeKind};

impl<'a> Checker<'a> {
    pub(super) fn literal_type(
        &mut self,
        literal: &Literal,
        provenance: LiteralProvenance,
    ) -> TypeId {
        match literal {
            Literal::String(value) => match self.string_literal_type(value, provenance) {
                Completion::Complete(ty) => ty,
                Completion::Deferred | Completion::Cycle | Completion::Limit => {
                    self.observe_completion(SemanticCompletion::Deferred);
                    self.store.deferred_utf16_string_literal()
                }
            },
            Literal::NoSubstitutionTemplate(literal) => self
                .store
                .intern(TypeKind::LiteralString(literal.cooked.clone(), provenance)),
            Literal::Number(NumberLiteral::Plain(value)) => {
                self.store.numeric_literal(value, provenance)
            }
            Literal::Number(NumberLiteral::Separated(value)) => {
                self.store.numeric_literal(value.raw(), provenance)
            }
            Literal::Number(NumberLiteral::Recovery(value)) if value.validation_supported() => self
                .store
                .numeric_literal(value.semantic_text(), provenance),
            Literal::Number(NumberLiteral::Recovery(_)) => {
                self.observe_completion(SemanticCompletion::Deferred);
                self.store.deferred_numeric_recovery()
            }
            Literal::BigInt(_) => self.store.deferred_bigint_literal(),
            Literal::Boolean(value) => self
                .store
                .intern(TypeKind::LiteralBoolean(*value, provenance)),
            Literal::Null => self.store.builtins.null,
        }
    }

    fn string_literal_type(
        &mut self,
        literal: &StringLiteral,
        provenance: LiteralProvenance,
    ) -> Completion<TypeId> {
        let value = match literal {
            StringLiteral::Plain(value) => value.clone(),
            StringLiteral::Extended(literal)
                if literal.terminated && !literal.contains_invalid_escape =>
            {
                let Some(value) = literal.cooked.as_string() else {
                    return Completion::Complete(self.store.deferred_utf16_string_literal());
                };
                value
            }
            StringLiteral::Extended(_) => return Completion::Deferred,
        };
        Completion::Complete(
            self.store
                .intern(TypeKind::LiteralString(value, provenance)),
        )
    }

    pub(super) fn widen(&mut self, ty: TypeId) -> TypeId {
        if self.is_symbolic_regular_expression_type(ty) {
            return ty;
        }
        let widened = self.store.widened_literal_type(ty);
        if widened != ty {
            return widened;
        }
        let completion = self.force_type(ty, 0);
        let ty = match self.require_completion(completion) {
            Completion::Complete(ty) => ty,
            Completion::Deferred | Completion::Cycle | Completion::Limit => return ty,
        };
        self.store.widened_literal_type(ty)
    }
}
