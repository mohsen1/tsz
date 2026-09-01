//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/type_computation/core.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN cdeb73a1aff5979a5bd7ce4adb2f197654f40e260b90ff2d3b3494390c9dd6d0 384 write_target_logical_or_normalizes_object_union_members
    #[test]
    fn write_target_logical_or_normalizes_object_union_members() {
        let db = TypeInterner::new();
        let left_object = fresh_object(&db, "left", TypeId::STRING);
        let right_object = fresh_object(&db, "right", TypeId::NUMBER);
        let nullable_left = db.union(vec![left_object, TypeId::NULL]);

        let result = write_target_logical_result_type(
            &db,
            WriteTargetLogicalOperator::LogicalOr,
            nullable_left,
            right_object,
        )
        .expect("nullable object || object should normalize write-target union");
        let WriteTargetLogicalResult::Type(result) = result else {
            panic!("expected normalized write-target type");
        };

        let members = union_members(&db, result);
        assert_eq!(members.len(), 2);
        for member in members {
            assert!(tsz_solver::type_queries::type_has_property_by_str(
                &db, member, "left"
            ));
            assert!(tsz_solver::type_queries::type_has_property_by_str(
                &db, member, "right"
            ));
        }
    }
// TSZ_INLINE_TEST_END cdeb73a1aff5979a5bd7ce4adb2f197654f40e260b90ff2d3b3494390c9dd6d0

// TSZ_INLINE_TEST_BEGIN e3b437686482bea592a7d60f396a424aac1a746feb0d070da1b2842e48cb38c7 414 union_context_prefers_tuple_when_all_array_shapes_are_tuples
    #[test]
    fn union_context_prefers_tuple_when_all_array_shapes_are_tuples() {
        let db = TypeInterner::new();
        let first = tuple(&db, TypeId::STRING);
        let second = tuple(&db, TypeId::NUMBER);
        let contextual = db.union(vec![first, second]);

        assert!(union_context_prefers_tuple_array_literal(&db, contextual));
    }
// TSZ_INLINE_TEST_END e3b437686482bea592a7d60f396a424aac1a746feb0d070da1b2842e48cb38c7

// TSZ_INLINE_TEST_BEGIN 4f39cf9d2f02032d0330ec12f56a8549ed12cee8877267cbe010936aca227d78 427 union_context_prefers_tuple_alongside_array_member
    /// `tsc` 7.0.2 accepts `const b1: [string] | number[] = [s]`: `someType(…,
    /// isTupleLikeType)` is satisfied by the tuple arm alone, so the literal is
    /// a tuple and the array arm never has to accept `string[]`.
    #[test]
    fn union_context_prefers_tuple_alongside_array_member() {
        let db = TypeInterner::new();
        let contextual = db.union(vec![tuple(&db, TypeId::STRING), db.array(TypeId::NUMBER)]);

        assert!(union_context_prefers_tuple_array_literal(&db, contextual));
    }
// TSZ_INLINE_TEST_END 4f39cf9d2f02032d0330ec12f56a8549ed12cee8877267cbe010936aca227d78

// TSZ_INLINE_TEST_BEGIN 9753ac4cca3eb0f7936261ab3a0758b75e172f82f18e01153ae1321d93481e7b 437 union_context_prefers_tuple_alongside_non_applicable_member
    /// A non-array constituent does not veto the tuple arm either — `tsc`
    /// accepts `const a: [string] | number = [s]`.
    #[test]
    fn union_context_prefers_tuple_alongside_non_applicable_member() {
        let db = TypeInterner::new();
        let contextual = db.union(vec![tuple(&db, TypeId::STRING), TypeId::NUMBER]);

        assert!(union_context_prefers_tuple_array_literal(&db, contextual));
    }
// TSZ_INLINE_TEST_END 9753ac4cca3eb0f7936261ab3a0758b75e172f82f18e01153ae1321d93481e7b

// TSZ_INLINE_TEST_BEGIN 9db68d91bb178f96a6acdc8997bdc998a67cb1d9cbb13682050a6351636adc32 447 union_context_does_not_prefer_tuple_without_a_tuple_member
    /// No tuple constituent, no tuple context: a union of plain arrays stays in
    /// array context so the ambiguous-union element machinery still runs.
    #[test]
    fn union_context_does_not_prefer_tuple_without_a_tuple_member() {
        let db = TypeInterner::new();
        let contextual = db.union(vec![db.array(TypeId::STRING), db.array(TypeId::NUMBER)]);

        assert!(!union_context_prefers_tuple_array_literal(&db, contextual));
    }
// TSZ_INLINE_TEST_END 9db68d91bb178f96a6acdc8997bdc998a67cb1d9cbb13682050a6351636adc32

// TSZ_INLINE_TEST_BEGIN 90c4207cd555dc7af5b94a7492af333544e0da271826f91c069c5bf58f87247a 455 non_union_context_does_not_prefer_tuple_array_literal
    #[test]
    fn non_union_context_does_not_prefer_tuple_array_literal() {
        let db = TypeInterner::new();

        assert!(!union_context_prefers_tuple_array_literal(
            &db,
            tuple(&db, TypeId::STRING)
        ));
    }
// TSZ_INLINE_TEST_END 90c4207cd555dc7af5b94a7492af333544e0da271826f91c069c5bf58f87247a

// TSZ_INLINE_TEST_BEGIN 87c6509a77babf2cdfa66df37674a0741033869d355d6ca0fb9c910ad739aa4d 465 literal_permissive_object_context_accepts_top_like_contexts
    #[test]
    fn literal_permissive_object_context_accepts_top_like_contexts() {
        assert!(is_literal_permissive_object_context(TypeId::UNKNOWN));
        assert!(is_literal_permissive_object_context(TypeId::ANY));
        assert!(is_literal_permissive_object_context(TypeId::NEVER));
    }
// TSZ_INLINE_TEST_END 87c6509a77babf2cdfa66df37674a0741033869d355d6ca0fb9c910ad739aa4d

// TSZ_INLINE_TEST_BEGIN 3cb3f88c772bf21de139bfa6b85eaaa669c50f0b9b9b64e4cadfb71e0a8c3f9a 472 literal_permissive_object_context_rejects_constraining_contexts
    #[test]
    fn literal_permissive_object_context_rejects_constraining_contexts() {
        assert!(!is_literal_permissive_object_context(TypeId::STRING));
        assert!(!is_literal_permissive_object_context(TypeId::NUMBER));
        assert!(!is_literal_permissive_object_context(TypeId::BOOLEAN));
    }
// TSZ_INLINE_TEST_END 3cb3f88c772bf21de139bfa6b85eaaa669c50f0b9b9b64e4cadfb71e0a8c3f9a

// TSZ_INLINE_TEST_BEGIN 9755dd2e3304f97e0f3b9fb7f057500edada6f178164b29598e2f7fe3c6b48bd 479 generic_application_literal_expected_rebuilds_argument_union
    #[test]
    fn generic_application_literal_expected_rebuilds_argument_union() {
        let db = TypeInterner::new();
        let first = db.literal_string("first");
        let second = db.literal_string("second");
        let expected = db.application(TypeId::STRING, vec![TypeId::STRING]);

        let result = generic_application_literal_expected_for_mismatch(
            &db,
            true,
            expected,
            &[first],
            &[second],
        )
        .expect("two string literal candidates should rebuild display expectation");

        let (base, args) = tsz_solver::type_queries::get_application_info(&db, result)
            .expect("result should remain an application");
        assert_eq!(base, TypeId::STRING);
        assert_eq!(args.len(), 1);
        let members = union_members(&db, args[0]);
        assert_eq!(members.len(), 2);
        assert!(members.contains(&first));
        assert!(members.contains(&second));
    }
// TSZ_INLINE_TEST_END 9755dd2e3304f97e0f3b9fb7f057500edada6f178164b29598e2f7fe3c6b48bd

// TSZ_INLINE_TEST_BEGIN 602dc241792c6cbc2f2c743931adce40d9e0891cf9a44ce0f205279b4904f160 505 generic_application_literal_expected_uses_display_alias_application
    #[test]
    fn generic_application_literal_expected_uses_display_alias_application() {
        let db = TypeInterner::new();
        let first = db.literal_number(1.0);
        let second = db.literal_number(2.0);
        let expected = fresh_object(&db, "value", TypeId::NUMBER);
        let alias_expected = db.application(TypeId::NUMBER, vec![TypeId::NUMBER]);
        db.store_display_alias(expected, alias_expected);

        let result = generic_application_literal_expected_for_mismatch(
            &db,
            true,
            expected,
            &[first],
            &[second],
        )
        .expect("display alias application should drive rebuilt expectation");

        let (base, args) = tsz_solver::type_queries::get_application_info(&db, result)
            .expect("result should remain an application");
        assert_eq!(base, TypeId::NUMBER);
        let members = union_members(&db, args[0]);
        assert!(members.contains(&first));
        assert!(members.contains(&second));
    }
// TSZ_INLINE_TEST_END 602dc241792c6cbc2f2c743931adce40d9e0891cf9a44ce0f205279b4904f160

// TSZ_INLINE_TEST_BEGIN f5b9aa1ef29bf7ae67affcb2fa87fd3b480bbe4f4432f0ef7c84ce30f24e8464 531 generic_application_literal_expected_rejects_single_unique_candidate
    #[test]
    fn generic_application_literal_expected_rejects_single_unique_candidate() {
        let db = TypeInterner::new();
        let first = db.literal_string("first");
        let expected = db.application(TypeId::STRING, vec![TypeId::STRING]);

        let result = generic_application_literal_expected_for_mismatch(
            &db,
            true,
            expected,
            &[first],
            &[first],
        );

        assert_eq!(result, None);
    }
// TSZ_INLINE_TEST_END f5b9aa1ef29bf7ae67affcb2fa87fd3b480bbe4f4432f0ef7c84ce30f24e8464

// TSZ_INLINE_TEST_BEGIN e3fc841a9908043b027a26f9deb96a70d6af5133d19b2bb0a2d7d0aaded42499 548 write_target_nullish_coalescing_normalizes_object_union_members
    #[test]
    fn write_target_nullish_coalescing_normalizes_object_union_members() {
        let db = TypeInterner::new();
        let left_object = fresh_object(&db, "value", TypeId::STRING);
        let right_object = fresh_object(&db, "fallback", TypeId::BOOLEAN);
        let nullish_left = db.union(vec![left_object, TypeId::NULL, TypeId::UNDEFINED]);

        let result = write_target_logical_result_type(
            &db,
            WriteTargetLogicalOperator::NullishCoalescing,
            nullish_left,
            right_object,
        )
        .expect("nullish object ?? object should normalize write-target union");
        let WriteTargetLogicalResult::Type(result) = result else {
            panic!("expected normalized write-target type");
        };

        let members = union_members(&db, result);
        assert_eq!(members.len(), 2);
        for member in members {
            assert!(tsz_solver::type_queries::type_has_property_by_str(
                &db, member, "value"
            ));
            assert!(tsz_solver::type_queries::type_has_property_by_str(
                &db, member, "fallback"
            ));
        }
    }
// TSZ_INLINE_TEST_END e3fc841a9908043b027a26f9deb96a70d6af5133d19b2bb0a2d7d0aaded42499

// TSZ_INLINE_TEST_BEGIN 44cd8abf64b4a844c912233532a7a3c494b547c5a42cc0f953775fd54348a323 578 write_target_logical_result_falls_back_for_primitive_members
    #[test]
    fn write_target_logical_result_falls_back_for_primitive_members() {
        let db = TypeInterner::new();
        let nullable_left = db.union(vec![TypeId::STRING, TypeId::NULL]);

        let result = write_target_logical_result_type(
            &db,
            WriteTargetLogicalOperator::LogicalOr,
            nullable_left,
            TypeId::NUMBER,
        );

        assert_eq!(result, None);
    }
// TSZ_INLINE_TEST_END 44cd8abf64b4a844c912233532a7a3c494b547c5a42cc0f953775fd54348a323

// TSZ_INLINE_TEST_BEGIN d482d2c522cd832b9def5c0acba1cf263a3b950dc2d6bd25382e43eb181da99c 593 write_target_logical_result_requests_logical_fallback_when_split_is_impossible
    #[test]
    fn write_target_logical_result_requests_logical_fallback_when_split_is_impossible() {
        let db = TypeInterner::new();

        let result = write_target_logical_result_type(
            &db,
            WriteTargetLogicalOperator::LogicalOr,
            TypeId::NULL,
            TypeId::NUMBER,
        );

        assert_eq!(
            result,
            Some(WriteTargetLogicalResult::FallbackToLogicalExpression)
        );
    }
// TSZ_INLINE_TEST_END d482d2c522cd832b9def5c0acba1cf263a3b950dc2d6bd25382e43eb181da99c
