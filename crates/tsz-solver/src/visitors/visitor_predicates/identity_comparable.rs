//! Identity-comparable type predicates.

use crate::construction::TypeDatabase;
use crate::{TypeData, TypeId};

/// NOTE: This does NOT handle `ReadonlyType` - readonly tuples must be checked separately
/// because `["a"]` is a subtype of `readonly ["a"]` even though they have different `TypeIds`.
pub fn is_identity_comparable_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_identity_comparable_type_impl(types, type_id, 0)
}

const MAX_IDENTITY_COMPARABLE_DEPTH: u32 = 10;

fn is_identity_comparable_type_impl(types: &dyn TypeDatabase, type_id: TypeId, depth: u32) -> bool {
    // Prevent stack overflow on pathological types
    if depth > MAX_IDENTITY_COMPARABLE_DEPTH {
        return false;
    }

    // Check well-known singleton types first.
    if matches!(
        type_id,
        TypeId::NULL | TypeId::UNDEFINED | TypeId::VOID | TypeId::NEVER
    ) {
        return true;
    }
    // Fast path: BOOLEAN_TRUE / BOOLEAN_FALSE are reserved intrinsic TypeIds
    // whose `TypeData::lookup` returns `Literal(Boolean)` -- identity-comparable.
    // All other intrinsics lookup to `Intrinsic(_)` which falls to `_ => false`.
    if type_id.is_intrinsic() {
        return type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE;
    }

    match types.lookup(type_id) {
        // Identity-comparable scalar types.
        Some(TypeData::Literal(_) | TypeData::Enum(_, _) | TypeData::UniqueSymbol(_)) => true,

        // Tuples are NOT identity-comparable because labeled tuples like [a: 1]
        // and [b: 1] are compatible despite having different TypeIds.
        // Similarly, [1, 2?] and [a: 1, b?: 2] must go through structural comparison
        // (`check_tuple_subtype`) which correctly ignores labels.

        // Everything else is not identity-comparable.
        _ => false,
    }
}
