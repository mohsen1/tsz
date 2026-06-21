//! Empty-key indexed access support.

use crate::relations::subtype::TypeResolver;
use crate::types::{TypeData, TypeId};
use crate::visitor::union_list_id;

use super::super::evaluate::TypeEvaluator;

impl<R: TypeResolver> TypeEvaluator<'_, R> {
    /// True for the narrow `T[never]` / empty `T[keyof T]` path.
    pub(crate) fn is_empty_key_index_access(
        &mut self,
        object_type: TypeId,
        evaluated_object: TypeId,
        index_type: TypeId,
        evaluated_index: TypeId,
    ) -> bool {
        evaluated_index == TypeId::NEVER
            && (index_type == TypeId::NEVER
                || matches!(
                    self.interner().lookup(index_type),
                    Some(TypeData::KeyOf(inner))
                        if inner == object_type
                            || inner == evaluated_object
                            || self.evaluate(inner) == object_type
                            || self.evaluate(inner) == evaluated_object
                ))
    }

    /// Evaluate `T[K]` for an empty (`never`) key set, mirroring tsc's
    /// `getIndexedAccessType` over an empty key. `object_type` must already be
    /// evaluated; union members are constituents of an evaluated union.
    ///
    /// `never` is assignable to every index key, so tsc reads `T`'s index
    /// *sources*: the number index first (an array's element type, a tuple's
    /// element union, or a numeric index signature), then the string and symbol
    /// index signatures. The access collapses to `never` only when `T` exposes no
    /// index source. Probing the concrete `number`/`string`/`symbol` keys reuses
    /// the full per-shape index machinery; a missing index source surfaces as
    /// `error`/`undefined`, which is treated as "no contribution".
    pub(crate) fn evaluate_empty_key_index_access(&mut self, object_type: TypeId) -> TypeId {
        if object_type == TypeId::NEVER {
            return TypeId::NEVER;
        }

        // `(A | B)[never]` distributes to `A[never] | B[never]`, dropping members
        // that contribute no index source (`union` of an empty set is `never`).
        if let Some(members_id) = union_list_id(self.interner(), object_type) {
            let members = self.interner().type_list(members_id);
            let mut results = Vec::new();
            for &member in members.iter() {
                let contribution = self.evaluate_empty_key_index_access(member);
                if contribution != TypeId::NEVER {
                    results.push(contribution);
                }
            }
            return self.interner().union(results);
        }

        // tsc priority: the number index (array/tuple element, numeric signature)
        // first, then the string and symbol index signatures.
        for key in [TypeId::NUMBER, TypeId::STRING, TypeId::SYMBOL] {
            let value = self.recurse_index_access(object_type, key);
            if value == TypeId::ERROR || value == TypeId::UNDEFINED || value == TypeId::NEVER {
                continue;
            }
            // A deferred access that indexes straight back onto `object_type` made
            // no progress for this key; skip it so a later key can still resolve.
            if matches!(
                self.interner().lookup(value),
                Some(TypeData::IndexAccess(obj, _)) if obj == object_type
            ) {
                continue;
            }
            return value;
        }

        TypeId::NEVER
    }
}
