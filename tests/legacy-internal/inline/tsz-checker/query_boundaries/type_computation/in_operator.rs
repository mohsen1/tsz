//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/type_computation/in_operator.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN acc27372863084196ff990283b1721fef91fffde7243fd6b3315f95251ee2a72 365 object_and_object_like_are_valid_rhs
    #[test]
    fn object_and_object_like_are_valid_rhs() {
        let db = TypeInterner::new();
        let mut cx = classifier(&db);
        assert!(is_valid_in_operator_rhs(&mut cx, TypeId::OBJECT));
        assert!(is_valid_in_operator_rhs(&mut cx, TypeId::ANY));
        let obj = db.object_fresh(vec![]);
        assert!(is_valid_in_operator_rhs(&mut cx, obj));
    }
// TSZ_INLINE_TEST_END acc27372863084196ff990283b1721fef91fffde7243fd6b3315f95251ee2a72

// TSZ_INLINE_TEST_BEGIN 747ae64e5b4ea777478142ea72a2f9c0ea3756d0933ee6e7136c92ae58c4e538 375 bare_primitive_is_not_valid_rhs
    #[test]
    fn bare_primitive_is_not_valid_rhs() {
        let db = TypeInterner::new();
        let mut cx = classifier(&db);
        assert!(!is_valid_in_operator_rhs(&mut cx, TypeId::STRING));
        assert!(!is_valid_in_operator_rhs(&mut cx, TypeId::NUMBER));
    }
// TSZ_INLINE_TEST_END 747ae64e5b4ea777478142ea72a2f9c0ea3756d0933ee6e7136c92ae58c4e538

// TSZ_INLINE_TEST_BEGIN 8b997712ed64be6adb27047e94c2bf8a4eb570e409922bcdf536e79804d82398 383 union_requires_every_member_valid
    #[test]
    fn union_requires_every_member_valid() {
        let db = TypeInterner::new();
        let mut cx = classifier(&db);
        let obj = db.object_fresh(vec![]);
        let all_valid = db.union(vec![obj, TypeId::OBJECT]);
        assert!(is_valid_in_operator_rhs(&mut cx, all_valid));
        let mixed = db.union(vec![obj, TypeId::STRING]);
        assert!(!is_valid_in_operator_rhs(&mut cx, mixed));
    }
// TSZ_INLINE_TEST_END 8b997712ed64be6adb27047e94c2bf8a4eb570e409922bcdf536e79804d82398

// TSZ_INLINE_TEST_BEGIN adec139bf27741415eb366a7316488680eb0c26d73c14bfb348e45aafc101662 394 intersection_requires_any_member_valid
    #[test]
    fn intersection_requires_any_member_valid() {
        let db = TypeInterner::new();
        let mut cx = classifier(&db);
        let obj = db.object_fresh(vec![]);
        let inter = db.intersection(vec![TypeId::STRING, obj]);
        assert!(is_valid_in_operator_rhs(&mut cx, inter));
    }
// TSZ_INLINE_TEST_END adec139bf27741415eb366a7316488680eb0c26d73c14bfb348e45aafc101662

// TSZ_INLINE_TEST_BEGIN 91d062b18522b77b428e93423682f5982d195e9e58c46cdf5b70a59ef7ce09fc 403 unknown_may_represent_primitive
    #[test]
    fn unknown_may_represent_primitive() {
        let db = TypeInterner::new();
        let mut cx = classifier(&db);
        assert!(type_may_represent_primitive(&mut cx, TypeId::UNKNOWN));
        assert!(!type_may_represent_primitive(&mut cx, TypeId::OBJECT));
    }
// TSZ_INLINE_TEST_END 91d062b18522b77b428e93423682f5982d195e9e58c46cdf5b70a59ef7ce09fc

// TSZ_INLINE_TEST_BEGIN ece88ea1b46f9c1826a017f32f78be98780d4c28a4df24594668b2e87f72baac 411 concrete_object_does_not_represent_primitive
    #[test]
    fn concrete_object_does_not_represent_primitive() {
        let db = TypeInterner::new();
        let mut cx = classifier(&db);
        let obj = db.object_fresh(vec![]);
        assert!(!type_may_represent_primitive(&mut cx, obj));
    }
// TSZ_INLINE_TEST_END ece88ea1b46f9c1826a017f32f78be98780d4c28a4df24594668b2e87f72baac

// TSZ_INLINE_TEST_BEGIN 6b666a7add9e86cddd3b9c1c01c4869229b7f09e86fa65260bd8254c569e3f8a 419 empty_object_shape_detection_walks_unions
    #[test]
    fn empty_object_shape_detection_walks_unions() {
        let db = TypeInterner::new();
        let mut cx = classifier(&db);
        let empty = db.object_fresh(vec![]);
        let with_empty = db.union(vec![TypeId::NUMBER, empty]);
        assert!(in_operator_type_contains_empty_object_shape(
            &mut cx, with_empty
        ));
        assert!(!in_operator_type_contains_empty_object_shape(
            &mut cx,
            TypeId::NUMBER
        ));
    }
// TSZ_INLINE_TEST_END 6b666a7add9e86cddd3b9c1c01c4869229b7f09e86fa65260bd8254c569e3f8a

// TSZ_INLINE_TEST_BEGIN 0b884d5bb988d1254ca71c67f1cd74886c9e16442ef98626eb65196f5c71d45f 477 evaluate_cycle_string_number_terminates_conservatively
    #[test]
    fn evaluate_cycle_string_number_terminates_conservatively() {
        assert_cycle_terminates_conservatively(TypeId::STRING, TypeId::NUMBER);
    }
// TSZ_INLINE_TEST_END 0b884d5bb988d1254ca71c67f1cd74886c9e16442ef98626eb65196f5c71d45f

// TSZ_INLINE_TEST_BEGIN 6860c833248db5f8ab1ec208ff53235161f3ebbc98d19617b090e9411be73106 482 evaluate_cycle_boolean_bigint_terminates_conservatively
    #[test]
    fn evaluate_cycle_boolean_bigint_terminates_conservatively() {
        // A different leaf pair proves the cap is a depth mechanism, not a
        // fast-path keyed on specific `TypeId`s.
        assert_cycle_terminates_conservatively(TypeId::BOOLEAN, TypeId::BIGINT);
    }
// TSZ_INLINE_TEST_END 6860c833248db5f8ab1ec208ff53235161f3ebbc98d19617b090e9411be73106

// TSZ_INLINE_TEST_BEGIN aa1f29ebe1bd4b28a89169521645c33640cc2fa77f3bcc51106536cefe75d6e3 489 bare_primitive_keeps_ts2638_path_not_assignability_shape
    #[test]
    fn bare_primitive_keeps_ts2638_path_not_assignability_shape() {
        let db = TypeInterner::new();
        // A bare primitive is an assignability-shape only inside a generic
        // union/intersection; on its own `is_primitive_type` returns true.
        assert!(in_rhs_is_type_parameter_assignability_shape(
            &db,
            TypeId::STRING
        ));
        let obj = db.object_fresh(vec![PropertyInfo::new(
            db.intern_string("a"),
            TypeId::NUMBER,
        )]);
        assert!(!in_rhs_is_type_parameter_assignability_shape(&db, obj));
    }
// TSZ_INLINE_TEST_END aa1f29ebe1bd4b28a89169521645c33640cc2fa77f3bcc51106536cefe75d6e3
