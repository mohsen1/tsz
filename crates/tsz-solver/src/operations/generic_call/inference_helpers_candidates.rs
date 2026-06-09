//! Inference candidate classification predicates.
//!
//! Contains type predicates used during generic call inference to classify
//! whether a `TypeId` is a suitable candidate for direct-argument inference,
//! return-position inference, or merge resolution.

use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{ObjectFlags, TypeData, TypeId};

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    pub(super) fn is_mergeable_direct_inference_candidate(&self, ty: TypeId) -> bool {
        let evaluated_ty = self.interner.evaluate_type(ty);
        // Primitives (null, undefined, string, number, boolean, void, never, etc.)
        // are always safe to merge into a union — they don't indicate structural
        // ambiguity. Without this, `equal(B, D | undefined)` would discard the
        // union and use only the first candidate, causing false TS2345 errors.
        if ty.is_nullish() || ty.is_any_or_unknown() || ty == TypeId::NEVER || ty == TypeId::VOID {
            return true;
        }
        // Primitive base types are safe to merge — they're just as unambiguous as
        // null/undefined. Literal types (string/number/boolean/bigint literals)
        // are also safe since they widen to their base primitive during resolution.
        if matches!(
            ty,
            TypeId::STRING
                | TypeId::NUMBER
                | TypeId::BOOLEAN
                | TypeId::BIGINT
                | TypeId::SYMBOL
                | TypeId::OBJECT
                | TypeId::BOOLEAN_TRUE
                | TypeId::BOOLEAN_FALSE
        ) {
            return true;
        }
        // Nominal private brands should never be merged into a union during
        // direct argument inference. TypeScript fixes `T` to the first such
        // candidate and reports the later mismatch (`C` vs `D`) instead of
        // inferring `C | D`.
        if crate::type_queries::get_private_brand_name(self.interner.as_type_database(), ty)
            .is_some()
            || crate::type_queries::get_private_field_name(self.interner.as_type_database(), ty)
                .is_some()
            || crate::type_queries::get_private_brand_name(
                self.interner.as_type_database(),
                evaluated_ty,
            )
            .is_some()
            || crate::type_queries::get_private_field_name(
                self.interner.as_type_database(),
                evaluated_ty,
            )
            .is_some()
        {
            return false;
        }
        match self.interner.lookup(ty) {
            Some(
                TypeData::Literal(_)
                | TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Function(_)
                | TypeData::Callable(_)
                | TypeData::Intersection(_)
                | TypeData::Enum(..)
                | TypeData::Lazy(_)
                | TypeData::Application(_)
                | TypeData::Conditional(_)
                | TypeData::IndexAccess(..)
                | TypeData::TemplateLiteral(_)
                | TypeData::ReadonlyType(_)
                | TypeData::KeyOf(_),
            ) => true,
            Some(TypeData::Union(members)) => {
                let members = self.interner.type_list(members);
                !members.is_empty()
                    && members
                        .iter()
                        .all(|member| self.is_mergeable_direct_inference_candidate(*member))
            }
            _ => false,
        }
    }

    pub(super) fn inference_type_contains_fresh_object_or_array(&self, ty: TypeId) -> bool {
        if ty.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(ty) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => self
                .interner
                .object_shape(shape_id)
                .flags
                .contains(ObjectFlags::FRESH_LITERAL),
            Some(TypeData::Array(_) | TypeData::Tuple(_)) => true,
            Some(TypeData::Union(members)) => self
                .interner
                .type_list(members)
                .iter()
                .any(|&member| self.inference_type_contains_fresh_object_or_array(member)),
            _ => false,
        }
    }

    pub(super) fn is_structural_return_inference_candidate(&self, ty: TypeId) -> bool {
        if ty.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(ty) {
            Some(
                TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Function(_)
                | TypeData::Callable(_)
                | TypeData::Intersection(_),
            ) => true,
            Some(TypeData::Union(members)) => {
                let members = self.interner.type_list(members);
                !members.is_empty()
                    && members
                        .iter()
                        .all(|member| self.is_structural_return_inference_candidate(*member))
            }
            _ => false,
        }
    }

    /// Returns `true` when the lower bounds contain literal types from different
    /// primitive families (e.g., a string literal and a number literal). This indicates
    /// heterogeneous candidates that tsc would NOT merge into a union.
    pub(super) fn has_conflicting_literal_bases(&self, lower_bounds: &[TypeId]) -> bool {
        // Direct-parameter inference should keep the leftmost candidate when
        // fresh candidates disagree on primitive base. That preserves TypeScript's
        // first-wins behavior for cases like `bar<T>(x: T, y: T); bar(1, "")`,
        // where `T` should settle on `number` and the second argument should
        // still produce TS2345 instead of broadening the call to `number | string`.
        let mut seen_base: Option<TypeId> = None;
        for &ty in lower_bounds {
            let base = self.primitive_base_of(ty);
            if let Some(b) = base {
                match seen_base {
                    None => seen_base = Some(b),
                    Some(prev) if prev != b => return true,
                    _ => {}
                }
            }
        }
        false
    }

    /// Returns the primitive base TypeId for a type if it's a literal or primitive,
    /// or `None` for non-primitive types (objects, arrays, etc.).
    pub(super) fn primitive_base_of(&self, ty: TypeId) -> Option<TypeId> {
        // Check well-known primitive TypeIds first
        if matches!(
            ty,
            TypeId::STRING | TypeId::NUMBER | TypeId::BOOLEAN | TypeId::BIGINT | TypeId::SYMBOL
        ) {
            return Some(ty);
        }
        if matches!(ty, TypeId::BOOLEAN_TRUE | TypeId::BOOLEAN_FALSE) {
            return Some(TypeId::BOOLEAN);
        }
        match self.interner.lookup(ty) {
            Some(TypeData::Literal(lit)) => Some(lit.primitive_type_id()),
            _ => None,
        }
    }
}
