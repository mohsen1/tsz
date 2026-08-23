use crate::program::SemanticCompletion;
use crate::syntax::{
    ExpressionKind, Literal, NumberLiteral, StringLiteral, VariableDeclaration, VariableKind,
};

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
            StringLiteral::Extended(_) => return Completion::Deferred,
        };
        Completion::Complete(
            self.store
                .intern(TypeKind::LiteralString(value, provenance)),
        )
    }

    /// Own the only mutable-string inference shape graduated by this
    /// campaign. The direct `var` path widens without ever allocating a
    /// surrogate-bearing literal type. Every other extended-string variable
    /// host remains a typed deferred demand.
    pub(super) fn extended_unicode_variable_type(
        &mut self,
        declaration: &VariableDeclaration,
    ) -> Option<Completion<TypeId>> {
        let literal = match declaration
            .initializer
            .as_ref()
            .map(|expression| &expression.kind)
        {
            Some(ExpressionKind::Literal(Literal::String(StringLiteral::Extended(literal)))) => {
                literal
            }
            _ => return None,
        };
        if declaration.declaration_kind == VariableKind::Var
            && !declaration.exported
            && declaration.annotation.is_none()
            && literal.validation_supported()
        {
            Some(Completion::Complete(self.store.builtins.string))
        } else {
            Some(Completion::Deferred)
        }
    }

    pub(super) fn widen(&mut self, ty: TypeId) -> TypeId {
        if self.is_symbolic_regular_expression_type(ty) {
            return ty;
        }
        if matches!(
            self.store.kind(ty),
            TypeKind::Deferred(crate::semantics::types::DeferredType::BigIntLiteral)
        ) {
            return self.store.builtins.bigint;
        }
        let completion = self.force_type(ty, 0);
        let ty = match self.require_completion(completion) {
            Completion::Complete(ty) => ty,
            Completion::Deferred | Completion::Cycle | Completion::Limit => return ty,
        };
        self.store.widened_literal_type(ty)
    }
}
