//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/type_queries/extended.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN efb65fecabb43decb1933ab8ff4635590ed8bb028668e8f21896ab4e17cce513 1715 index_type_visit_state_records_first_entry
    #[test]
    fn index_type_visit_state_records_first_entry() {
        let mut visited = FxHashSet::default();

        let state = index_type_visit_state(&mut visited, TypeId::STRING);

        assert_eq!(state, IndexTypeVisitState::Entered);
        assert!(visited.contains(&TypeId::STRING));
    }
// TSZ_INLINE_TEST_END efb65fecabb43decb1933ab8ff4635590ed8bb028668e8f21896ab4e17cce513

// TSZ_INLINE_TEST_BEGIN f00e08482621b69789793c47e8258bf8710e555768b45fa9d27807f32af27087 1725 index_type_visit_state_records_reentry
    #[test]
    fn index_type_visit_state_records_reentry() {
        let mut visited = FxHashSet::default();

        assert_eq!(
            index_type_visit_state(&mut visited, TypeId::STRING),
            IndexTypeVisitState::Entered
        );
        assert_eq!(
            index_type_visit_state(&mut visited, TypeId::STRING),
            IndexTypeVisitState::AlreadyVisited
        );
        assert_eq!(visited.len(), 1);
    }
// TSZ_INLINE_TEST_END f00e08482621b69789793c47e8258bf8710e555768b45fa9d27807f32af27087

// TSZ_INLINE_TEST_BEGIN 66f68d90de95f88ca486a688ca8d096c0259e023da0c96eaddd509a0c50b9624 1740 branded_primitive_intersections_are_valid_index_types
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
// TSZ_INLINE_TEST_END 66f68d90de95f88ca486a688ca8d096c0259e023da0c96eaddd509a0c50b9624

// TSZ_INLINE_TEST_BEGIN 0e18ed7f63d8df5bf9e7bdc60cbbfb1713a663bfa54582ee1b1fc47750dd9b03 1758 object_only_intersections_remain_invalid_index_types
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
// TSZ_INLINE_TEST_END 0e18ed7f63d8df5bf9e7bdc60cbbfb1713a663bfa54582ee1b1fc47750dd9b03

// TSZ_INLINE_TEST_BEGIN d72d5eaee1582b04d936c11f132b1ae8e99a0c5e31800257a7ddc88104e42577 1771 dedup_alpha_equivalent_generic_signatures
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
                origin: crate::TypeParamOrigin::User,
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
            declaration_group: 0,
        };

        let sig2 = CallSignature {
            type_params: vec![TypeParamInfo {
                name: Atom(10),
                constraint: None,
                default: None,
                is_const: false,
                origin: crate::TypeParamOrigin::User,
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
            declaration_group: 0,
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
// TSZ_INLINE_TEST_END d72d5eaee1582b04d936c11f132b1ae8e99a0c5e31800257a7ddc88104e42577

// TSZ_INLINE_TEST_BEGIN 4928630228ed6bb3dc0e15de1a261c3759b2d12406ce31861feb02d9ecbedfea 1831 dedup_preserves_different_generic_signatures
    #[test]
    fn dedup_preserves_different_generic_signatures() {
        // Two genuinely different generic signatures should not be deduped
        let sig1 = CallSignature {
            type_params: vec![TypeParamInfo {
                name: Atom(10),
                constraint: None,
                default: None,
                is_const: false,
                origin: crate::TypeParamOrigin::User,
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
            declaration_group: 0,
        };

        let sig2 = CallSignature {
            type_params: vec![TypeParamInfo {
                name: Atom(11), // Different type param name
                constraint: None,
                default: None,
                is_const: false,
                origin: crate::TypeParamOrigin::User,
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
            declaration_group: 0,
        };

        let mut sigs = vec![sig1, sig2];
        dedup_alpha_equivalent_signatures(&mut sigs);
        assert_eq!(
            sigs.len(),
            2,
            "Different generic signatures should be preserved"
        );
    }
// TSZ_INLINE_TEST_END 4928630228ed6bb3dc0e15de1a261c3759b2d12406ce31861feb02d9ecbedfea

// TSZ_INLINE_TEST_BEGIN eaf312128013157a256d3c0314c21e7c68ca53b7b3961ad5078fe839141a5ed3 1885 dedup_skips_non_generic_signatures
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
            declaration_group: 0,
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
            declaration_group: 0,
        };

        let mut sigs = vec![sig1, sig2];
        dedup_alpha_equivalent_signatures(&mut sigs);
        assert_eq!(sigs.len(), 2, "Non-generic signatures should be preserved");
    }
// TSZ_INLINE_TEST_END eaf312128013157a256d3c0314c21e7c68ca53b7b3961ad5078fe839141a5ed3

// TSZ_INLINE_TEST_BEGIN 1243dd12336554614a00e4d56da79eb3b33bcef348625c3716e01629fcfab529 1926 widen_literal_to_primitive_widens_boolean_intrinsics
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
// TSZ_INLINE_TEST_END 1243dd12336554614a00e4d56da79eb3b33bcef348625c3716e01629fcfab529
