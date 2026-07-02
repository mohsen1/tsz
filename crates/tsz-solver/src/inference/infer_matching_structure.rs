//! Outer-structure classification for inference union-arm routing.

use super::infer::InferenceContext;
use crate::types::{LiteralValue, TypeData, TypeId};

/// Coarse outer structural kind used to decide whether a source member and a
/// union target arm share enough structure to be inferred against each other.
/// Array and tuple collapse to [`StructuralKind::ArrayLike`]; function and
/// callable collapse to [`StructuralKind::Callable`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum StructuralKind {
    ArrayLike,
    Object,
    Callable,
    IndexAccess,
    KeyOf,
}

impl<'a> InferenceContext<'a> {
    /// Check if two types share the same outer structure (same kind / same generic base).
    ///
    /// Used to match source union members to the best target union member during
    /// inference. For example, `Foo<U>` and `Foo<V>` share outer structure (both
    /// are applications of `Foo`), but `U` and `Foo<V>` do not.
    pub(super) fn types_share_outer_structure(&self, source: TypeId, target: TypeId) -> bool {
        if source.is_intrinsic() || target.is_intrinsic() {
            return false;
        }
        if self.type_has_own_then_property(source) && self.type_has_own_then_property(target) {
            return true;
        }
        let application_outer = |type_id| {
            let app_id = match self.interner.lookup(type_id) {
                Some(TypeData::Application(app_id)) => Some(app_id),
                _ => {
                    let evaluated = self.evaluate_type_for_inference_probe(type_id);
                    if evaluated == type_id {
                        None
                    } else {
                        match self.interner.lookup(evaluated) {
                            Some(TypeData::Application(app_id)) => Some(app_id),
                            _ => None,
                        }
                    }
                }
            }?;
            let app = self.interner.type_application(app_id);
            let lazy_def = crate::type_queries::get_lazy_def_id(self.interner, app.base);
            Some((app.base, lazy_def))
        };

        if let (Some((source_base, source_def)), Some((target_base, target_def))) =
            (application_outer(source), application_outer(target))
            && (source_base == target_base || (source_def.is_some() && source_def == target_def))
        {
            return true;
        }

        let (Some(s_key), Some(t_key)) =
            (self.interner.lookup(source), self.interner.lookup(target))
        else {
            return false;
        };
        match (s_key, t_key) {
            // Both are applications of the same base type
            (TypeData::Application(s_app_id), TypeData::Application(t_app_id)) => {
                let s_app = self.interner.type_application(s_app_id);
                let t_app = self.interner.type_application(t_app_id);
                s_app.base == t_app.base
            }
            // Both share the same structural kind
            (TypeData::Object(_), TypeData::Object(_))
            | (TypeData::Callable(_), TypeData::Callable(_))
            | (TypeData::Function(_), TypeData::Function(_))
            | (TypeData::Tuple(_), TypeData::Tuple(_))
            | (TypeData::Array(_), TypeData::Array(_))
            | (TypeData::IndexAccess(_, _), TypeData::IndexAccess(_, _))
            | (TypeData::KeyOf(_), TypeData::KeyOf(_))
            | (TypeData::Literal(LiteralValue::String(_)), TypeData::TemplateLiteral(_))
            | (TypeData::TemplateLiteral(_), TypeData::TemplateLiteral(_)) => true,
            // One side is a type-alias application / lazy reference whose
            // structural form matches the other side's kind. This is the key
            // case for recursive array utilities: a target arm like
            // `RecArray<T> = Array<T | RecArray<T>>` is stored as an
            // `Application`, so a `number[]` source would otherwise look
            // unrelated and be routed to the naked type variable instead of
            // being decomposed through the array arm. Comparing the *evaluated*
            // structural kinds lets such aliases participate in structured
            // inference just like their expanded forms.
            _ => self.evaluated_structural_kinds_match(source, target),
        }
    }

    /// Best-effort structural kind of `type_id`, expanding a single layer of
    /// type-alias application / lazy reference so that aliases compare equal to
    /// the structural type they evaluate to (e.g. `RecArray<T>` -> array-like).
    ///
    /// This is the `InferenceContext` counterpart of the constraint walker's
    /// `types_share_outer_structure_for_constraint`. It is intentionally coarse:
    /// it does not special-case promise-like (`then`) shapes the way the walker
    /// does, because those flow through dedicated arms in this engine. Returns
    /// `None` for intrinsics, unions, type parameters, and anything without an
    /// outer structural shape.
    fn evaluated_structural_kind(&self, type_id: TypeId) -> Option<StructuralKind> {
        if type_id.is_intrinsic() {
            return None;
        }
        let mut key = self.interner.lookup(type_id)?;
        if matches!(key, TypeData::Application(_) | TypeData::Lazy(_)) {
            let evaluated = self.evaluate_type_for_inference_probe(type_id);
            if evaluated != type_id {
                key = self.interner.lookup(evaluated)?;
            }
        }
        match key {
            // Array and tuple are both array-like for inference decomposition.
            TypeData::Array(_) | TypeData::Tuple(_) => Some(StructuralKind::ArrayLike),
            TypeData::Object(_) | TypeData::ObjectWithIndex(_) => Some(StructuralKind::Object),
            // Function and callable are both signature-bearing.
            TypeData::Function(_) | TypeData::Callable(_) => Some(StructuralKind::Callable),
            TypeData::IndexAccess(_, _) => Some(StructuralKind::IndexAccess),
            TypeData::KeyOf(_) => Some(StructuralKind::KeyOf),
            _ => None,
        }
    }

    /// True when `source` and `target` resolve to the same coarse structural
    /// kind after expanding a single alias layer. Used as a fallback in
    /// [`Self::types_share_outer_structure`] so alias arms route through
    /// structured inference rather than the naked type variable.
    fn evaluated_structural_kinds_match(&self, source: TypeId, target: TypeId) -> bool {
        match (
            self.evaluated_structural_kind(source),
            self.evaluated_structural_kind(target),
        ) {
            (Some(source_kind), Some(target_kind)) => source_kind == target_kind,
            _ => false,
        }
    }

    fn type_has_own_then_property(&self, type_id: TypeId) -> bool {
        if self.object_type_has_own_then_property(type_id) {
            return true;
        }
        let evaluated = self.evaluate_type_for_inference_probe(type_id);
        evaluated != type_id && self.object_type_has_own_then_property(evaluated)
    }

    fn evaluate_type_for_inference_probe(&self, type_id: TypeId) -> TypeId {
        if let Some(query_db) = self.query_db {
            return query_db.evaluate_type(type_id);
        }
        crate::evaluation::evaluate::evaluate_type(self.interner, type_id)
    }

    fn object_type_has_own_then_property(&self, type_id: TypeId) -> bool {
        match self.interner.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => matches!(
                self.interner
                    .object_property_index(shape_id, self.interner.intern_string("then")),
                crate::types::PropertyLookup::Found(_)
            ),
            _ => false,
        }
    }
}
