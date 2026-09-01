//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/objects/element_access.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN cb96d0b9e81b3a3cdc88dcb464a8d7150dd5f9ad7161cb1df9290ad10fe5bc2c 284 mapped_type_is_indexable
    #[test]
    fn mapped_type_is_indexable() {
        let interner = TypeInterner::new();

        // Create a mapped type: { [P in K as `get${P}`]: { a: P } }
        let type_param = interner.type_param(TypeParamInfo {
            name: Atom::NONE,
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let mapped_with_as = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: Atom::NONE,
                constraint: Some(TypeId::STRING),
                default: None,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            },
            constraint: type_param,
            name_type: Some(TypeId::STRING), // has as-clause
            template: TypeId::STRING,
            readonly_modifier: None,
            optional_modifier: None,
        });

        let evaluator = ElementAccessEvaluator::new(&interner);
        assert!(
            evaluator.is_indexable(mapped_with_as),
            "Mapped type with as-clause should be indexable"
        );
    }
// TSZ_INLINE_TEST_END cb96d0b9e81b3a3cdc88dcb464a8d7150dd5f9ad7161cb1df9290ad10fe5bc2c

// TSZ_INLINE_TEST_BEGIN 31ecf5a12825fcccaddbc1d59886bed0393d0d935be1d2976c09b35d644a2b98 318 mapped_type_without_as_clause_is_indexable
    #[test]
    fn mapped_type_without_as_clause_is_indexable() {
        let interner = TypeInterner::new();

        let type_param = interner.type_param(TypeParamInfo {
            name: Atom::NONE,
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let mapped_no_as = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: Atom::NONE,
                constraint: Some(TypeId::STRING),
                default: None,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            },
            constraint: type_param,
            name_type: None, // no as-clause
            template: TypeId::STRING,
            readonly_modifier: None,
            optional_modifier: None,
        });

        let evaluator = ElementAccessEvaluator::new(&interner);
        assert!(
            evaluator.is_indexable(mapped_no_as),
            "Mapped type without as-clause should be indexable"
        );
    }
// TSZ_INLINE_TEST_END 31ecf5a12825fcccaddbc1d59886bed0393d0d935be1d2976c09b35d644a2b98

// TSZ_INLINE_TEST_BEGIN 11924bacdb15f86f27f7ac84397bc3d322e8fdbbbd077c548f98846ebbf375be 351 union_of_mapped_types_is_indexable
    #[test]
    fn union_of_mapped_types_is_indexable() {
        let interner = TypeInterner::new();

        let type_param = interner.type_param(TypeParamInfo {
            name: Atom::NONE,
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let mapped = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: Atom::NONE,
                constraint: Some(TypeId::STRING),
                default: None,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            },
            constraint: type_param,
            name_type: Some(TypeId::STRING),
            template: TypeId::NUMBER,
            readonly_modifier: None,
            optional_modifier: None,
        });

        let obj = interner.object(vec![]);
        let union = interner.union2(mapped, obj);

        let evaluator = ElementAccessEvaluator::new(&interner);
        assert!(
            evaluator.is_indexable(union),
            "Union of mapped type and object should be indexable"
        );
    }
// TSZ_INLINE_TEST_END 11924bacdb15f86f27f7ac84397bc3d322e8fdbbbd077c548f98846ebbf375be

// TSZ_INLINE_TEST_BEGIN 430b6af23a8527400835301101105f63fd77527e784662aaf5e825ff2f689c27 387 readonly_array_is_indexable
    #[test]
    fn readonly_array_is_indexable() {
        let interner = TypeInterner::new();
        let arr = interner.array(TypeId::NUMBER);
        let readonly_arr = interner.readonly_type(arr);

        let evaluator = ElementAccessEvaluator::new(&interner);
        assert!(
            evaluator.is_indexable(readonly_arr),
            "ReadonlyType(Array(number)) should be indexable"
        );
    }
// TSZ_INLINE_TEST_END 430b6af23a8527400835301101105f63fd77527e784662aaf5e825ff2f689c27

// TSZ_INLINE_TEST_BEGIN c9ab9550cfd17d11e232aa31941babbb58a97c6c6b10d6f892feff9c34018f53 419 readonly_tuple_is_indexable
    #[test]
    fn readonly_tuple_is_indexable() {
        let interner = TypeInterner::new();
        let readonly_tuple = make_readonly_num_str_tuple(&interner);
        let evaluator = ElementAccessEvaluator::new(&interner);
        assert!(
            evaluator.is_indexable(readonly_tuple),
            "ReadonlyType(Tuple) should be indexable"
        );
    }
// TSZ_INLINE_TEST_END c9ab9550cfd17d11e232aa31941babbb58a97c6c6b10d6f892feff9c34018f53

// TSZ_INLINE_TEST_BEGIN 663837783154b50f72f6934db1960041b459d75b3a583316ba1e80a18c41b1cb 430 union_of_readonly_arrays_is_indexable
    #[test]
    fn union_of_readonly_arrays_is_indexable() {
        let interner = TypeInterner::new();
        let arr_num = interner.array(TypeId::NUMBER);
        let readonly_arr_num = interner.readonly_type(arr_num);
        let arr_str = interner.array(TypeId::STRING);
        let readonly_arr_str = interner.readonly_type(arr_str);
        let union = interner.union2(readonly_arr_num, readonly_arr_str);

        let evaluator = ElementAccessEvaluator::new(&interner);
        assert!(
            evaluator.is_indexable(union),
            "Union of ReadonlyType(Array) members should be indexable"
        );
    }
// TSZ_INLINE_TEST_END 663837783154b50f72f6934db1960041b459d75b3a583316ba1e80a18c41b1cb

// TSZ_INLINE_TEST_BEGIN 43513bc33b251c2eb408e7cc048e1b095229efa45bf9b99c3792c200bdbbbba2 446 readonly_array_element_access_returns_element_type
    #[test]
    fn readonly_array_element_access_returns_element_type() {
        let interner = TypeInterner::new();
        let arr = interner.array(TypeId::NUMBER);
        let readonly_arr = interner.readonly_type(arr);

        let evaluator = ElementAccessEvaluator::new(&interner);
        let result = evaluator.resolve_element_access(readonly_arr, TypeId::NUMBER, None);
        assert!(
            matches!(result, ElementAccessResult::Success(t) if t == TypeId::NUMBER),
            "Element access on ReadonlyType(Array(number)) with number index should succeed with number type"
        );
    }
// TSZ_INLINE_TEST_END 43513bc33b251c2eb408e7cc048e1b095229efa45bf9b99c3792c200bdbbbba2

// TSZ_INLINE_TEST_BEGIN cb0167ee7eaa995d5ffde3f091faa7f555a37fac1a5e880b4af6f168dd898993 460 readonly_tuple_element_access_in_bounds_succeeds
    #[test]
    fn readonly_tuple_element_access_in_bounds_succeeds() {
        let interner = TypeInterner::new();
        let readonly_tuple = make_readonly_num_str_tuple(&interner);
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_0 = interner.literal_number(0.0);
        let result = evaluator.resolve_element_access(readonly_tuple, literal_0, Some(0));
        assert!(
            matches!(result, ElementAccessResult::Success(_)),
            "In-bounds access on ReadonlyType(Tuple) should succeed"
        );
    }
// TSZ_INLINE_TEST_END cb0167ee7eaa995d5ffde3f091faa7f555a37fac1a5e880b4af6f168dd898993

// TSZ_INLINE_TEST_BEGIN 7f9ae21a9a71eee8d5772bd065261bc9180461cb9906382b7c17acad4ab325aa 473 readonly_tuple_element_access_out_of_bounds
    #[test]
    fn readonly_tuple_element_access_out_of_bounds() {
        let interner = TypeInterner::new();
        let readonly_tuple = make_readonly_num_str_tuple(&interner);
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_2 = interner.literal_number(2.0);
        let result = evaluator.resolve_element_access(readonly_tuple, literal_2, Some(2));
        assert!(
            matches!(
                result,
                ElementAccessResult::IndexOutOfBounds {
                    index: 2,
                    length: 2,
                    ..
                }
            ),
            "Out-of-bounds access on ReadonlyType(Tuple) should return IndexOutOfBounds"
        );
    }
// TSZ_INLINE_TEST_END 7f9ae21a9a71eee8d5772bd065261bc9180461cb9906382b7c17acad4ab325aa

// TSZ_INLINE_TEST_BEGIN 27dfd74d44c805d2c67c209368d2b84fe047a37b44e2a45c8855287c244e74b4 506 type_param_constrained_to_array_resolves_element_type
    /// `T extends number[]` ⇒ `T[0]` must resolve to `number`, not collapse
    /// to `ERROR` via the indexability gate. This is the structural rule
    /// behind issue #9716.
    #[test]
    fn type_param_constrained_to_array_resolves_element_type() {
        let interner = TypeInterner::new();
        let arr = interner.array(TypeId::NUMBER);
        let t = type_param_with_constraint(&interner, arr);
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_0 = interner.literal_number(0.0);
        let result = evaluator.resolve_element_access(t, literal_0, Some(0));
        assert!(
            matches!(result, ElementAccessResult::Success(t) if t == TypeId::NUMBER),
            "T[0] for T extends number[] should evaluate to number, got {result:?}",
        );
    }
// TSZ_INLINE_TEST_END 27dfd74d44c805d2c67c209368d2b84fe047a37b44e2a45c8855287c244e74b4

// TSZ_INLINE_TEST_BEGIN 76d1ce2c3a9012ca3173bf7f4d53806b52175385a639b631a11a0447cc93d4ce 522 renamed_type_param_constrained_to_array_resolves_element_type
    /// Renamed type parameter (`P` instead of `T`) must behave identically:
    /// the rule is structural, not name-based.
    #[test]
    fn renamed_type_param_constrained_to_array_resolves_element_type() {
        let interner = TypeInterner::new();
        let arr = interner.array(TypeId::STRING);
        let p = interner.type_param(TypeParamInfo {
            name: Atom::NONE, // identifier name is irrelevant
            constraint: Some(arr),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_0 = interner.literal_number(0.0);
        let result = evaluator.resolve_element_access(p, literal_0, Some(0));
        assert!(
            matches!(result, ElementAccessResult::Success(t) if t == TypeId::STRING),
            "P[0] for P extends string[] should evaluate to string, got {result:?}",
        );
    }
// TSZ_INLINE_TEST_END 76d1ce2c3a9012ca3173bf7f4d53806b52175385a639b631a11a0447cc93d4ce

// TSZ_INLINE_TEST_BEGIN de1a9d0b67ce67b53f0618ffc006d1c983be814f4614b63e540243278f659384 544 type_param_constrained_to_unknown_array_resolves_unknown
    /// Unconstrained `T` (or `T extends unknown[]`) keeps the element type
    /// of the constraint, which is `unknown` for the bottom case.
    #[test]
    fn type_param_constrained_to_unknown_array_resolves_unknown() {
        let interner = TypeInterner::new();
        let arr = interner.array(TypeId::UNKNOWN);
        let t = type_param_with_constraint(&interner, arr);
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_0 = interner.literal_number(0.0);
        let result = evaluator.resolve_element_access(t, literal_0, Some(0));
        assert!(
            matches!(result, ElementAccessResult::Success(t) if t == TypeId::UNKNOWN),
            "T[0] for T extends unknown[] should evaluate to unknown, got {result:?}",
        );
    }
// TSZ_INLINE_TEST_END de1a9d0b67ce67b53f0618ffc006d1c983be814f4614b63e540243278f659384

// TSZ_INLINE_TEST_BEGIN cc63cd478adefe8890f5b09b93f2b0a10dc3867cbc0bc80cf781a50703b9183e 560 type_param_constrained_to_tuple_resolves_positional_element
    /// Tuple constraint: `T extends [string, number]` ⇒ `T[0]` resolves to
    /// the constrained tuple's element type.
    #[test]
    fn type_param_constrained_to_tuple_resolves_positional_element() {
        let interner = TypeInterner::new();
        let tuple = interner.tuple(vec![
            crate::types::TupleElement {
                type_id: TypeId::STRING,
                name: None,
                optional: false,
                rest: false,
            },
            crate::types::TupleElement {
                type_id: TypeId::NUMBER,
                name: None,
                optional: false,
                rest: false,
            },
        ]);
        let t = type_param_with_constraint(&interner, tuple);
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_0 = interner.literal_number(0.0);
        let r0 = evaluator.resolve_element_access(t, literal_0, Some(0));
        assert!(
            matches!(r0, ElementAccessResult::Success(t) if t == TypeId::STRING),
            "T[0] for T extends [string, number] should be string, got {r0:?}",
        );
        let literal_1 = interner.literal_number(1.0);
        let r1 = evaluator.resolve_element_access(t, literal_1, Some(1));
        assert!(
            matches!(r1, ElementAccessResult::Success(t) if t == TypeId::NUMBER),
            "T[1] for T extends [string, number] should be number, got {r1:?}",
        );
    }
// TSZ_INLINE_TEST_END cc63cd478adefe8890f5b09b93f2b0a10dc3867cbc0bc80cf781a50703b9183e

// TSZ_INLINE_TEST_BEGIN 700b490405c5381632410d9c85994fb2a1650aabda8fcc3710915ed6aea2252a 596 type_param_constrained_to_tuple_reports_out_of_bounds
    /// Tuple constraint with an out-of-bounds literal index must surface
    /// `IndexOutOfBounds` (TS2493), not collapse to `NotIndexable` because
    /// the receiver is a type parameter.
    #[test]
    fn type_param_constrained_to_tuple_reports_out_of_bounds() {
        let interner = TypeInterner::new();
        let tuple = interner.tuple(vec![crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        }]);
        let t = type_param_with_constraint(&interner, tuple);
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_5 = interner.literal_number(5.0);
        let result = evaluator.resolve_element_access(t, literal_5, Some(5));
        assert!(
            matches!(
                result,
                ElementAccessResult::IndexOutOfBounds {
                    index: 5,
                    length: 1,
                    ..
                }
            ),
            "T[5] for T extends [string] should be IndexOutOfBounds, got {result:?}",
        );
    }
// TSZ_INLINE_TEST_END 700b490405c5381632410d9c85994fb2a1650aabda8fcc3710915ed6aea2252a

// TSZ_INLINE_TEST_BEGIN 5917bb5cc0762ee13378a51c0062c5bcf5ef295ce97472dd7e6659d3f66177c2 624 nested_type_param_chain_resolves_through_apparent_type
    /// Indirect type-parameter chain: `T extends number[]`, `U extends T` ⇒
    /// `U[0]` must still see the apparent element type through the chain.
    #[test]
    fn nested_type_param_chain_resolves_through_apparent_type() {
        let interner = TypeInterner::new();
        let arr = interner.array(TypeId::NUMBER);
        let t = type_param_with_constraint(&interner, arr);
        let u = type_param_with_constraint(&interner, t);
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_0 = interner.literal_number(0.0);
        let result = evaluator.resolve_element_access(u, literal_0, Some(0));
        assert!(
            matches!(result, ElementAccessResult::Success(t) if t == TypeId::NUMBER),
            "U[0] for U extends T extends number[] should be number, got {result:?}",
        );
    }
// TSZ_INLINE_TEST_END 5917bb5cc0762ee13378a51c0062c5bcf5ef295ce97472dd7e6659d3f66177c2

// TSZ_INLINE_TEST_BEGIN 1aad03897609920aa9d55889071ebbbd2fa2475f9d0ee4500989ca4ef215f8c4 642 type_param_constrained_to_non_indexable_reports_not_indexable
    /// `T` with a non-indexable constraint (`number`) still reports
    /// `NotIndexable`. The apparent-type walk must preserve the original
    /// negative gate for non-indexable apparent shapes.
    #[test]
    fn type_param_constrained_to_non_indexable_reports_not_indexable() {
        let interner = TypeInterner::new();
        let t = type_param_with_constraint(&interner, TypeId::NUMBER);
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_0 = interner.literal_number(0.0);
        let result = evaluator.resolve_element_access(t, literal_0, Some(0));
        assert!(
            matches!(result, ElementAccessResult::NotIndexable { .. }),
            "T[0] for T extends number should be NotIndexable, got {result:?}",
        );
    }
// TSZ_INLINE_TEST_END 1aad03897609920aa9d55889071ebbbd2fa2475f9d0ee4500989ca4ef215f8c4

// TSZ_INLINE_TEST_BEGIN a9a0cdf8514d9206ef9425a4ca0c3fd0bd6282962faa35a69055e2d0c9d86b89 657 unconstrained_type_param_reports_not_indexable
    /// Unconstrained `T` is the implicit-`unknown` case: still not
    /// indexable, no regression.
    #[test]
    fn unconstrained_type_param_reports_not_indexable() {
        let interner = TypeInterner::new();
        let t = interner.type_param(TypeParamInfo {
            name: Atom::NONE,
            constraint: None,
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_0 = interner.literal_number(0.0);
        let result = evaluator.resolve_element_access(t, literal_0, Some(0));
        assert!(
            matches!(result, ElementAccessResult::NotIndexable { .. }),
            "T[0] for unconstrained T should be NotIndexable, got {result:?}",
        );
    }
// TSZ_INLINE_TEST_END a9a0cdf8514d9206ef9425a4ca0c3fd0bd6282962faa35a69055e2d0c9d86b89

// TSZ_INLINE_TEST_BEGIN d5d67fe07d11af49a82811e5111991e37f5e10af3e80702b6f87b2ed2cb12eec 679 type_param_extends_any_normalizes_to_unknown_and_not_indexable
    /// An explicit `extends any` constraint must be normalized to `unknown`
    /// for the apparent-type walk, matching `getConstraintFromTypeParameter`.
    /// The receiver is not indexable in this case.
    #[test]
    fn type_param_extends_any_normalizes_to_unknown_and_not_indexable() {
        let interner = TypeInterner::new();
        let t = type_param_with_constraint(&interner, TypeId::ANY);
        let evaluator = ElementAccessEvaluator::new(&interner);
        let literal_0 = interner.literal_number(0.0);
        let result = evaluator.resolve_element_access(t, literal_0, Some(0));
        // The fast `evaluated_object == ANY` short-circuit at the top of
        // resolve_element_access does not fire here because the receiver is
        // a `TypeParameter` whose constraint is `any`. Apparent-type walking
        // normalizes `any` to `unknown`, which is correctly not indexable.
        assert!(
            matches!(result, ElementAccessResult::NotIndexable { .. }),
            "T[0] for T extends any should be NotIndexable after apparent-type \
             normalization, got {result:?}",
        );
    }
// TSZ_INLINE_TEST_END d5d67fe07d11af49a82811e5111991e37f5e10af3e80702b6f87b2ed2cb12eec
