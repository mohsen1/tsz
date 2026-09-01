//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-wasm/src/wasm_api/type_checker.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 60f4dd1bc4a988ea109c19f471b114de1cdba89447c3d69f6d4234c434f0f9bc 293 predicates_match_type_data_for_unions_and_intersections
    #[test]
    fn predicates_match_type_data_for_unions_and_intersections() {
        let (interner, checker) = checker();

        let union_id = interner.union2(TypeId::STRING, TypeId::NUMBER);
        assert!(checker.is_union_type(union_id.0));
        assert!(!checker.is_intersection_type(union_id.0));
        assert!(!checker.is_array_type(union_id.0));
        assert!(!checker.is_tuple_type(union_id.0));
        assert!(!checker.is_type_parameter(union_id.0));

        // `intersect_types_raw2` skips most normalization, but it still
        // collapses pairs that the solver knows are vacuous (e.g., disjoint
        // primitives -> never). Two type-parameter references survive raw
        // intersection unchanged, so we use those.
        let tp_a = interner.type_param(make_type_param(&interner, "A"));
        let tp_b = interner.type_param(make_type_param(&interner, "B"));
        let raw_intersection = interner.intersect_types_raw2(tp_a, tp_b);
        assert!(checker.is_intersection_type(raw_intersection.0));
        assert!(!checker.is_union_type(raw_intersection.0));
    }
// TSZ_INLINE_TEST_END 60f4dd1bc4a988ea109c19f471b114de1cdba89447c3d69f6d4234c434f0f9bc

// TSZ_INLINE_TEST_BEGIN 2f197b8932edb17bead144894eaeec38311637b94ea3972774f04d43215a61d2 315 predicates_recognize_arrays_and_tuples
    #[test]
    fn predicates_recognize_arrays_and_tuples() {
        let (interner, checker) = checker();

        let array_id = interner.array(TypeId::NUMBER);
        assert!(checker.is_array_type(array_id.0));
        assert!(!checker.is_tuple_type(array_id.0));
        assert!(!checker.is_union_type(array_id.0));

        let tuple_id = interner.tuple(vec![tuple_elem(TypeId::STRING), tuple_elem(TypeId::NUMBER)]);
        assert!(checker.is_tuple_type(tuple_id.0));
        assert!(!checker.is_array_type(tuple_id.0));
    }
// TSZ_INLINE_TEST_END 2f197b8932edb17bead144894eaeec38311637b94ea3972774f04d43215a61d2

// TSZ_INLINE_TEST_BEGIN 0cc24f0071f90f2df665d2f4a8a21ade3248f4764a1b29fa735e786e36568c84 329 intrinsic_predicates_return_false
    #[test]
    fn intrinsic_predicates_return_false() {
        let (_interner, checker) = checker();

        for &intrinsic in &[
            TypeId::ANY,
            TypeId::UNKNOWN,
            TypeId::STRING,
            TypeId::NUMBER,
            TypeId::BOOLEAN,
            TypeId::VOID,
            TypeId::NEVER,
        ] {
            assert!(!checker.is_union_type(intrinsic.0));
            assert!(!checker.is_intersection_type(intrinsic.0));
            assert!(!checker.is_array_type(intrinsic.0));
            assert!(!checker.is_tuple_type(intrinsic.0));
            assert!(!checker.is_type_parameter(intrinsic.0));
        }
    }
// TSZ_INLINE_TEST_END 0cc24f0071f90f2df665d2f4a8a21ade3248f4764a1b29fa735e786e36568c84

// TSZ_INLINE_TEST_BEGIN 7da3bbf2c0f7df6b83427ad13a8825d679b233a8a1da154c6c78e08d7d8b41cf 350 is_nullable_type_covers_unions_with_null_or_undefined
    #[test]
    fn is_nullable_type_covers_unions_with_null_or_undefined() {
        let (interner, checker) = checker();

        assert!(checker.is_nullable_type(TypeId::NULL.0));
        assert!(checker.is_nullable_type(TypeId::UNDEFINED.0));
        assert!(!checker.is_nullable_type(TypeId::STRING.0));

        let nullable_string = interner.union2(TypeId::STRING, TypeId::NULL);
        assert!(checker.is_nullable_type(nullable_string.0));

        let optional_string = interner.union2(TypeId::STRING, TypeId::UNDEFINED);
        assert!(checker.is_nullable_type(optional_string.0));

        let plain_union = interner.union2(TypeId::STRING, TypeId::NUMBER);
        assert!(!checker.is_nullable_type(plain_union.0));
    }
// TSZ_INLINE_TEST_END 7da3bbf2c0f7df6b83427ad13a8825d679b233a8a1da154c6c78e08d7d8b41cf

// TSZ_INLINE_TEST_BEGIN 588dba7bc550269b1329858b6ef6d2c31144fa379cdf56a30edbe99b80243551 368 type_flags_match_typescript_constants
    #[test]
    fn type_flags_match_typescript_constants() {
        let (interner, checker) = checker();

        assert_eq!(checker.get_type_flags(TypeId::ANY.0), type_flags::ANY);
        assert_eq!(
            checker.get_type_flags(TypeId::UNKNOWN.0),
            type_flags::UNKNOWN
        );
        assert_eq!(checker.get_type_flags(TypeId::STRING.0), type_flags::STRING);
        assert_eq!(checker.get_type_flags(TypeId::NUMBER.0), type_flags::NUMBER);
        assert_eq!(
            checker.get_type_flags(TypeId::BOOLEAN.0),
            type_flags::BOOLEAN
        );
        assert_eq!(
            checker.get_type_flags(TypeId::BIGINT.0),
            type_flags::BIG_INT
        );
        assert_eq!(
            checker.get_type_flags(TypeId::SYMBOL.0),
            type_flags::ES_SYMBOL
        );
        assert_eq!(checker.get_type_flags(TypeId::VOID.0), type_flags::VOID);
        assert_eq!(
            checker.get_type_flags(TypeId::UNDEFINED.0),
            type_flags::UNDEFINED
        );
        assert_eq!(checker.get_type_flags(TypeId::NULL.0), type_flags::NULL);
        assert_eq!(checker.get_type_flags(TypeId::NEVER.0), type_flags::NEVER);
        assert_eq!(
            checker.get_type_flags(TypeId::BOOLEAN_TRUE.0),
            type_flags::BOOLEAN_LITERAL | type_flags::BOOLEAN
        );
        assert_eq!(
            checker.get_type_flags(TypeId::BOOLEAN_FALSE.0),
            type_flags::BOOLEAN_LITERAL | type_flags::BOOLEAN
        );

        let str_lit = interner.literal_string("hello");
        assert_eq!(
            checker.get_type_flags(str_lit.0),
            type_flags::STRING_LITERAL
        );

        let num_lit = interner.literal_number(42.0);
        assert_eq!(
            checker.get_type_flags(num_lit.0),
            type_flags::NUMBER_LITERAL
        );

        let union_id = interner.union2(TypeId::STRING, TypeId::NUMBER);
        assert_eq!(checker.get_type_flags(union_id.0), type_flags::UNION);

        let array_id = interner.array(TypeId::NUMBER);
        assert_eq!(checker.get_type_flags(array_id.0), type_flags::OBJECT);

        let tuple_id = interner.tuple(vec![tuple_elem(TypeId::NUMBER)]);
        assert_eq!(checker.get_type_flags(tuple_id.0), type_flags::OBJECT);
    }
// TSZ_INLINE_TEST_END 588dba7bc550269b1329858b6ef6d2c31144fa379cdf56a30edbe99b80243551
