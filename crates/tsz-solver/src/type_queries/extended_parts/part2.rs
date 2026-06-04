#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallSignature, ParamInfo, TypeParamInfo};
    use tsz_common::Atom;

    #[test]
    fn branded_primitive_intersections_are_valid_index_types() {
        let interner = crate::construction::TypeInterner::new();
        let brand = interner.object(vec![]);

        let branded_string = interner.intersection(vec![TypeId::STRING, brand]);
        assert!(
            get_invalid_index_type_member(&interner, branded_string).is_none(),
            "string & Brand should stay usable as an element-access index"
        );

        let branded_number = interner.intersection(vec![TypeId::NUMBER, brand]);
        assert!(
            get_invalid_index_type_member(&interner, branded_number).is_none(),
            "number & Brand should stay usable as an element-access index"
        );
    }

    #[test]
    fn object_only_intersections_remain_invalid_index_types() {
        let interner = crate::construction::TypeInterner::new();
        let left = interner.object(vec![]);
        let right = interner.object(vec![]);
        let object_intersection = interner.intersection(vec![left, right]);

        assert!(
            get_invalid_index_type_member(&interner, object_intersection).is_some(),
            "object-only intersections should still be rejected as index types"
        );
    }

    #[test]
    fn dedup_alpha_equivalent_generic_signatures() {
        // Two signatures with the same generic structure but different TypeIds
        // for type parameters (as happens when resolving a generic method from
        // different union members).
        let sig1 = CallSignature {
            type_params: vec![TypeParamInfo {
                name: Atom(10),
                constraint: None,
                default: None,
                is_const: false,
            }],
            params: vec![ParamInfo {
                name: Some(Atom(20)),
                type_id: TypeId(100), // ReadonlyArray<T> with T=TypeId(100)
                optional: false,
                rest: false,
            }],
            this_type: Some(TypeId(100)),
            return_type: TypeId(8), // boolean
            type_predicate: None,
            is_method: true,
        };

        let sig2 = CallSignature {
            type_params: vec![TypeParamInfo {
                name: Atom(10),
                constraint: None,
                default: None,
                is_const: false,
            }],
            params: vec![ParamInfo {
                name: Some(Atom(20)),
                type_id: TypeId(200), // ReadonlyArray<T> with different T=TypeId(200)
                optional: false,
                rest: false,
            }],
            this_type: Some(TypeId(200)),
            return_type: TypeId(8),
            type_predicate: None,
            is_method: true,
        };

        let mut sigs = vec![sig1.clone(), sig2];
        dedup_alpha_equivalent_signatures(&mut sigs);
        assert_eq!(
            sigs.len(),
            1,
            "Alpha-equivalent generic signatures should deduplicate to 1"
        );
        assert_eq!(
            sigs[0].this_type, sig1.this_type,
            "Should keep the first signature"
        );
    }

    #[test]
    fn dedup_preserves_different_generic_signatures() {
        // Two genuinely different generic signatures should not be deduped
        let sig1 = CallSignature {
            type_params: vec![TypeParamInfo {
                name: Atom(10),
                constraint: None,
                default: None,
                is_const: false,
            }],
            params: vec![ParamInfo {
                name: Some(Atom(20)),
                type_id: TypeId(100),
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId(8),
            type_predicate: None,
            is_method: true,
        };

        let sig2 = CallSignature {
            type_params: vec![TypeParamInfo {
                name: Atom(11), // Different type param name
                constraint: None,
                default: None,
                is_const: false,
            }],
            params: vec![ParamInfo {
                name: Some(Atom(20)),
                type_id: TypeId(200),
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId(8),
            type_predicate: None,
            is_method: true,
        };

        let mut sigs = vec![sig1, sig2];
        dedup_alpha_equivalent_signatures(&mut sigs);
        assert_eq!(
            sigs.len(),
            2,
            "Different generic signatures should be preserved"
        );
    }

    #[test]
    fn dedup_skips_non_generic_signatures() {
        // Non-generic signatures should never be deduped
        let sig1 = CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(Atom(20)),
                type_id: TypeId(100),
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId(8),
            type_predicate: None,
            is_method: false,
        };

        let sig2 = CallSignature {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(Atom(20)),
                type_id: TypeId(200),
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId(8),
            type_predicate: None,
            is_method: false,
        };

        let mut sigs = vec![sig1, sig2];
        dedup_alpha_equivalent_signatures(&mut sigs);
        assert_eq!(sigs.len(), 2, "Non-generic signatures should be preserved");
    }

    /// Regression: an earlier intrinsic fast path returned `type_id` for any
    /// intrinsic, but `BOOLEAN_TRUE` / `BOOLEAN_FALSE` are intrinsic IDs that
    /// resolve to `Literal(Boolean)` and must widen to BOOLEAN.
    #[test]
    fn widen_literal_to_primitive_widens_boolean_intrinsics() {
        let interner = crate::construction::TypeInterner::new();
        assert_eq!(
            widen_literal_to_primitive(&interner, TypeId::BOOLEAN_TRUE),
            TypeId::BOOLEAN
        );
        assert_eq!(
            widen_literal_to_primitive(&interner, TypeId::BOOLEAN_FALSE),
            TypeId::BOOLEAN
        );
        // Other intrinsics are returned unchanged.
        assert_eq!(
            widen_literal_to_primitive(&interner, TypeId::NUMBER),
            TypeId::NUMBER
        );
        assert_eq!(
            widen_literal_to_primitive(&interner, TypeId::ANY),
            TypeId::ANY
        );
    }
}
