//! Candidate shape helpers used while resolving inference variables.

use crate::inference::infer::InferenceContext;
use crate::types::{ObjectFlags, TypeData, TypeId};

impl<'a> InferenceContext<'a> {
    /// Match tsc's `unionObjectAndArrayLiteralCandidates`: extract all object
    /// and array/tuple literal candidates, union them into a single type, and
    /// return the updated candidate list. Non-literal candidates are kept as-is.
    pub(super) fn union_object_and_array_literal_candidates(
        &self,
        candidates: &[TypeId],
    ) -> Vec<TypeId> {
        if candidates.len() <= 1 {
            return candidates.to_vec();
        }
        let mut object_or_array_literals = Vec::new();
        let mut other_candidates = Vec::new();
        for &ty in candidates {
            if self.is_object_or_array_literal_type(ty) {
                object_or_array_literals.push(ty);
            } else {
                other_candidates.push(ty);
            }
        }
        if object_or_array_literals.is_empty() {
            return candidates.to_vec();
        }
        let literals_union = if object_or_array_literals.len() == 1 {
            object_or_array_literals[0]
        } else {
            self.interner.union(object_or_array_literals)
        };
        other_candidates.push(literals_union);
        other_candidates
    }

    /// Check if a type is an object or array literal type (anonymous object or tuple).
    fn is_object_or_array_literal_type(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                shape.symbol.is_none()
            }
            Some(TypeData::Tuple(_)) => true,
            _ => false,
        }
    }

    pub(super) fn is_fresh_object_literal_candidate(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                shape.flags.contains(ObjectFlags::FRESH_LITERAL)
            }
            _ => false,
        }
    }

    pub(super) fn is_non_fresh_object_candidate(&self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        match self.interner.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                !shape.flags.contains(ObjectFlags::FRESH_LITERAL)
            }
            _ => false,
        }
    }
}
