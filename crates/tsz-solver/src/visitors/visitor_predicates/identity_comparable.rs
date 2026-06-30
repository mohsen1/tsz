//! Identity-comparable type predicates.

use crate::construction::TypeDatabase;
use crate::{TypeData, TypeId};

/// NOTE: This does NOT handle `ReadonlyType` - readonly tuples must be checked separately
/// because `["a"]` is a subtype of `readonly ["a"]` even though they have different `TypeIds`.
pub fn is_identity_comparable_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_identity_comparable_type_impl(types, type_id, 0)
}

const MAX_IDENTITY_COMPARABLE_DEPTH: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdentityComparableDepthState {
    Continue,
    LimitExceeded,
}

impl IdentityComparableDepthState {
    const fn for_depth(depth: u32) -> Self {
        if depth > MAX_IDENTITY_COMPARABLE_DEPTH {
            Self::LimitExceeded
        } else {
            Self::Continue
        }
    }
}

fn is_identity_comparable_type_impl(types: &dyn TypeDatabase, type_id: TypeId, depth: u32) -> bool {
    // Prevent stack overflow on pathological types
    match IdentityComparableDepthState::for_depth(depth) {
        IdentityComparableDepthState::Continue => {}
        IdentityComparableDepthState::LimitExceeded => return false,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intern::TypeInterner;

    #[test]
    fn identity_comparable_depth_state_names_exact_cap_and_limit() {
        assert_eq!(
            IdentityComparableDepthState::for_depth(MAX_IDENTITY_COMPARABLE_DEPTH),
            IdentityComparableDepthState::Continue
        );
        assert_eq!(
            IdentityComparableDepthState::for_depth(MAX_IDENTITY_COMPARABLE_DEPTH + 1),
            IdentityComparableDepthState::LimitExceeded
        );
    }

    #[test]
    fn identity_comparable_depth_limit_preserves_false_fallback() {
        let interner = TypeInterner::new();

        assert!(is_identity_comparable_type(&interner, TypeId::BOOLEAN_TRUE));
        assert!(!is_identity_comparable_type_impl(
            &interner,
            TypeId::BOOLEAN_TRUE,
            MAX_IDENTITY_COMPARABLE_DEPTH + 1
        ));
    }
}
