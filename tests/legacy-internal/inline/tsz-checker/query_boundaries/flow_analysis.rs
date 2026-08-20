//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/flow_analysis.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN cc73ab4403d52b5661a4293e0d3833ceab3ebd177a897e18c3f7bd346d7d5688 1386 assignment_reduction_preserves_top_like_initial_types
    #[test]
    fn assignment_reduction_preserves_top_like_initial_types() {
        let db = TypeInterner::new();

        assert_eq!(
            narrow_assignment(&db, None, TypeId::ANY, TypeId::NUMBER),
            TypeId::ANY
        );
        assert_eq!(
            narrow_assignment(&db, None, TypeId::UNKNOWN, TypeId::NUMBER),
            TypeId::UNKNOWN
        );
        assert_eq!(
            narrow_assignment(&db, None, TypeId::ERROR, TypeId::NUMBER),
            TypeId::ERROR
        );
    }
// TSZ_INLINE_TEST_END cc73ab4403d52b5661a4293e0d3833ceab3ebd177a897e18c3f7bd346d7d5688

// TSZ_INLINE_TEST_BEGIN 034404584b9b6960b56ee83f3bc5ac732588544102f781217f0e9a388c251045 1404 assignment_reduction_keeps_non_union_initial_type
    #[test]
    fn assignment_reduction_keeps_non_union_initial_type() {
        let db = TypeInterner::new();

        assert_eq!(
            narrow_assignment(&db, None, TypeId::STRING, TypeId::NUMBER),
            TypeId::STRING
        );
    }
// TSZ_INLINE_TEST_END 034404584b9b6960b56ee83f3bc5ac732588544102f781217f0e9a388c251045

// TSZ_INLINE_TEST_BEGIN 613df711ff09feb15e7e45af3f2e3d151cdb0622618e586c946e5f48200ad5e7 1414 assignment_reduction_uses_non_nullish_type_parameter_constraint_surface
    #[test]
    fn assignment_reduction_uses_non_nullish_type_parameter_constraint_surface() {
        let db = TypeInterner::new();
        let nullable_string = db.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
        let type_param = type_param_with_constraint(&db, "T", nullable_string);
        let assigned = tsz_solver::narrowing::remove_nullish(&db, type_param);

        assert_eq!(
            narrow_assignment(&db, None, type_param, assigned),
            TypeId::STRING
        );
    }
// TSZ_INLINE_TEST_END 613df711ff09feb15e7e45af3f2e3d151cdb0622618e586c946e5f48200ad5e7

// TSZ_INLINE_TEST_BEGIN 08a9342a87b24b6d18909f2eed357fe0ba4f72c2d6585fec413762b116060b67 1427 assignment_reduction_uses_non_nullish_indexed_access_constraint_surface
    #[test]
    fn assignment_reduction_uses_non_nullish_indexed_access_constraint_surface() {
        let db = TypeInterner::new();
        let nullable_string = db.union(vec![TypeId::STRING, TypeId::UNDEFINED]);
        let object = db.object(vec![property(&db, "x", nullable_string)]);
        let type_param = type_param_with_constraint(&db, "T", object);
        let indexed = db.index_access(type_param, db.literal_string("x"));
        let assigned = db.intersection(vec![indexed, db.object(Vec::new())]);

        assert_eq!(
            narrow_assignment(&db, None, indexed, assigned),
            TypeId::STRING
        );
    }
// TSZ_INLINE_TEST_END 08a9342a87b24b6d18909f2eed357fe0ba4f72c2d6585fec413762b116060b67

// TSZ_INLINE_TEST_BEGIN 0d45215073fb6118c59487f8f2837edcdb4af3739cfdf81e020477f92cc7fbc2 1442 assignment_reduction_filters_union_by_literal_source_assignability
    #[test]
    fn assignment_reduction_filters_union_by_literal_source_assignability() {
        let db = TypeInterner::new();
        let initial = db.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let assigned = tsz_solver::type_queries::create_number_literal_type(&db, 42.0);

        assert_eq!(
            narrow_assignment(&db, None, initial, assigned),
            TypeId::NUMBER
        );
    }
// TSZ_INLINE_TEST_END 0d45215073fb6118c59487f8f2837edcdb4af3739cfdf81e020477f92cc7fbc2

// TSZ_INLINE_TEST_BEGIN e085a66278684fe1f8d7c27615eff9f24a9881e69951e89561f94ec00800c125 1454 assignment_reduction_keeps_original_union_when_no_member_matches
    #[test]
    fn assignment_reduction_keeps_original_union_when_no_member_matches() {
        let db = TypeInterner::new();
        let initial = db.union(vec![TypeId::STRING, TypeId::BOOLEAN]);

        assert_eq!(
            narrow_assignment(&db, None, initial, TypeId::NUMBER),
            initial
        );
    }
// TSZ_INLINE_TEST_END e085a66278684fe1f8d7c27615eff9f24a9881e69951e89561f94ec00800c125

// TSZ_INLINE_TEST_BEGIN 6b1e13256900cf158d000a8d7f410a846d3ebc40b9720e7e7f52d49ab8c91512 1465 typeof_switch_domain_rejects_error_operands
    #[test]
    fn typeof_switch_domain_rejects_error_operands() {
        let db = TypeInterner::new();

        assert_eq!(typeof_switch_domain(&db, None, TypeId::ERROR), None);
    }
// TSZ_INLINE_TEST_END 6b1e13256900cf158d000a8d7f410a846d3ebc40b9720e7e7f52d49ab8c91512

// TSZ_INLINE_TEST_BEGIN ce4726da2779f4b221372ef43cf8f56491e09cb9e1b274a6ced118777ea0bacd 1472 typeof_switch_domain_returns_single_literal_for_primitive_operand
    #[test]
    fn typeof_switch_domain_returns_single_literal_for_primitive_operand() {
        let db = TypeInterner::new();

        assert_eq!(
            typeof_switch_domain(&db, None, TypeId::STRING),
            Some(db.literal_string("string"))
        );
    }
// TSZ_INLINE_TEST_END ce4726da2779f4b221372ef43cf8f56491e09cb9e1b274a6ced118777ea0bacd

// TSZ_INLINE_TEST_BEGIN 0432b338e1222891b307a21a5d26032cd71e85d4ca2260361cf6b0eff5575ab2 1482 typeof_switch_domain_returns_union_for_union_operand
    #[test]
    fn typeof_switch_domain_returns_union_for_union_operand() {
        let db = TypeInterner::new();
        let operand = db.union(vec![TypeId::STRING, TypeId::NUMBER]);

        let Some(domain) = typeof_switch_domain(&db, None, operand) else {
            panic!("expected typeof domain for string | number");
        };
        let members = union_members_for_type(&db, domain).unwrap_or_else(|| vec![domain].into());
        assert_eq!(members.len(), 2);
        assert!(members.contains(&db.literal_string("string")));
        assert!(members.contains(&db.literal_string("number")));
    }
// TSZ_INLINE_TEST_END 0432b338e1222891b307a21a5d26032cd71e85d4ca2260361cf6b0eff5575ab2

// TSZ_INLINE_TEST_BEGIN 8277055f5f5759e0ecc029006e52f64d209958acbf8d59e778b0740a938dbf68 1496 narrow_by_typeof_result_routes_positive_and_negative_branches
    #[test]
    fn narrow_by_typeof_result_routes_positive_and_negative_branches() {
        let db = TypeInterner::new();
        let source = db.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);

        assert_eq!(
            narrow_by_typeof_result(&db, None, source, "string", true),
            TypeId::STRING
        );

        let negative = narrow_by_typeof_result(&db, None, source, "string", false);
        let members =
            union_members_for_type(&db, negative).unwrap_or_else(|| vec![negative].into());
        assert_eq!(members.len(), 2);
        assert!(members.contains(&TypeId::NUMBER));
        assert!(members.contains(&TypeId::BOOLEAN));
    }
// TSZ_INLINE_TEST_END 8277055f5f5759e0ecc029006e52f64d209958acbf8d59e778b0740a938dbf68

// TSZ_INLINE_TEST_BEGIN 63b8c0c728ea33cbabb11b3960c7e53420b8a82ec62e99aee1e960ccacc762bc 1514 cases_exhaust_type_uses_exact_literal_union_coverage
    #[test]
    fn cases_exhaust_type_uses_exact_literal_union_coverage() {
        let db = TypeInterner::new();
        let first = db.literal_string("first");
        let second = db.literal_string("second");
        let third = db.literal_string("third");
        let switch_type = db.union(vec![first, second, third]);

        assert!(cases_exhaust_type(
            &db,
            None,
            switch_type,
            &[second, first, third],
        ));
        assert!(!cases_exhaust_type(&db, None, switch_type, &[first, third]));
    }
// TSZ_INLINE_TEST_END 63b8c0c728ea33cbabb11b3960c7e53420b8a82ec62e99aee1e960ccacc762bc

// TSZ_INLINE_TEST_BEGIN 6e9c822be7aba8f8c93e23afa2d91bc65a4ef9c3ab4569e26e0c28844143e99f 1531 enum_member_union_domain_keeps_plain_union_identity
    #[test]
    fn enum_member_union_domain_keeps_plain_union_identity() {
        let db = TypeInterner::new();
        let union = db.union(vec![TypeId::STRING, TypeId::NUMBER]);

        assert_eq!(enum_member_union_domain(&db, union), union);
    }
// TSZ_INLINE_TEST_END 6e9c822be7aba8f8c93e23afa2d91bc65a4ef9c3ab4569e26e0c28844143e99f

// TSZ_INLINE_TEST_BEGIN 67bdb013c36b865239844c35ef2b659ebe69b89f7f11d83871c643a9acde671f 1539 enum_member_union_domain_rewrites_only_enum_members
    #[test]
    fn enum_member_union_domain_rewrites_only_enum_members() {
        let db = TypeInterner::new();
        let literal = db.literal_string("ready");
        let enum_member = db.enum_type(tsz_solver::def::DefId(701), literal);
        let union = db.union(vec![enum_member, TypeId::NUMBER]);

        let domain = enum_member_union_domain(&db, union);
        let members = union_members_for_type(&db, domain).unwrap_or_else(|| vec![domain].into());

        assert_eq!(members.len(), 2);
        assert!(members.contains(&literal));
        assert!(members.contains(&TypeId::NUMBER));
        assert!(!members.contains(&enum_member));
    }
// TSZ_INLINE_TEST_END 67bdb013c36b865239844c35ef2b659ebe69b89f7f11d83871c643a9acde671f

// TSZ_INLINE_TEST_BEGIN ff99a51920ac75c2b22d30052048bdb21d90f60aa2c5028879f48ba2464c1433 1555 has_enum_components_tracks_enum_identity
    #[test]
    fn has_enum_components_tracks_enum_identity() {
        let db = TypeInterner::new();
        let literal = db.literal_string("ready");
        let enum_member = db.enum_type(tsz_solver::def::DefId(702), literal);

        assert!(has_enum_components(&db, enum_member));
        assert!(!has_enum_components(&db, literal));
        assert!(!has_enum_components(&db, TypeId::NUMBER));
    }
// TSZ_INLINE_TEST_END ff99a51920ac75c2b22d30052048bdb21d90f60aa2c5028879f48ba2464c1433

// TSZ_INLINE_TEST_BEGIN 93f6dba148a2a73ce5ac5f87255abe9d2fa522b0b24096ce863d684bf3fe2d56 1566 property_access_function_returns_never_recognizes_never_returning_property
    #[test]
    fn property_access_function_returns_never_recognizes_never_returning_property() {
        let db = TypeInterner::new();
        let never_fn = function_returning(&db, TypeId::NEVER);
        let void_fn = function_returning(&db, TypeId::VOID);
        let object = db.object(vec![
            property(&db, "bail", never_fn),
            property(&db, "continue", void_fn),
        ]);

        assert!(property_access_function_returns_never(&db, object, "bail"));
        assert!(!property_access_function_returns_never(
            &db, object, "continue"
        ));
        assert!(!property_access_function_returns_never(
            &db, object, "missing"
        ));
    }
// TSZ_INLINE_TEST_END 93f6dba148a2a73ce5ac5f87255abe9d2fa522b0b24096ce863d684bf3fe2d56

// TSZ_INLINE_TEST_BEGIN f7b2d6e0674bd5dda80e50e80c4cb834b115c14c29322778de79f3f1d02a854e 1585 property_access_function_returns_never_is_structural_not_name_based
    #[test]
    fn property_access_function_returns_never_is_structural_not_name_based() {
        let db = TypeInterner::new();
        let never_fn = function_returning(&db, TypeId::NEVER);
        let first_object = db.object(vec![property(&db, "abort", never_fn)]);
        let second_object = db.object(vec![property(&db, "halt", never_fn)]);
        let value_object = db.object(vec![property(&db, "abort", TypeId::NUMBER)]);

        assert!(property_access_function_returns_never(
            &db,
            first_object,
            "abort"
        ));
        assert!(property_access_function_returns_never(
            &db,
            second_object,
            "halt"
        ));
        assert!(!property_access_function_returns_never(
            &db,
            value_object,
            "abort"
        ));
    }
// TSZ_INLINE_TEST_END f7b2d6e0674bd5dda80e50e80c4cb834b115c14c29322778de79f3f1d02a854e

// TSZ_INLINE_TEST_BEGIN 90ed8e0c0933b82eb47e98da0e148fbb559f6ce883254e3b9eda51b05b8980cf 1610 nullish_coalescing_switch_domain_rejects_error_operands
    #[test]
    fn nullish_coalescing_switch_domain_rejects_error_operands() {
        let db = TypeInterner::new();

        assert_eq!(
            nullish_coalescing_switch_domain(&db, TypeId::ERROR, TypeId::STRING),
            None
        );
        assert_eq!(
            nullish_coalescing_switch_domain(&db, TypeId::STRING, TypeId::ERROR),
            None
        );
    }
// TSZ_INLINE_TEST_END 90ed8e0c0933b82eb47e98da0e148fbb559f6ce883254e3b9eda51b05b8980cf

// TSZ_INLINE_TEST_BEGIN e344f1cf074de8cd835361482ad23cd58728d821638e15710985cb7037247b56 1624 nullish_coalescing_switch_domain_uses_right_when_left_is_nullish
    #[test]
    fn nullish_coalescing_switch_domain_uses_right_when_left_is_nullish() {
        let db = TypeInterner::new();
        let left = db.union(vec![TypeId::NULL, TypeId::UNDEFINED]);

        assert_eq!(
            nullish_coalescing_switch_domain(&db, left, TypeId::STRING),
            Some(TypeId::STRING)
        );
    }
// TSZ_INLINE_TEST_END e344f1cf074de8cd835361482ad23cd58728d821638e15710985cb7037247b56

// TSZ_INLINE_TEST_BEGIN 8497c78d818d45562708f9b1045227632f6cdcbdbdf90029a18742d9c24e8831 1635 nullish_coalescing_switch_domain_unions_non_nullish_left_and_right
    #[test]
    fn nullish_coalescing_switch_domain_unions_non_nullish_left_and_right() {
        let db = TypeInterner::new();
        let left = db.union(vec![TypeId::NULL, TypeId::NUMBER]);

        let Some(domain) = nullish_coalescing_switch_domain(&db, left, TypeId::STRING) else {
            panic!("expected switch domain for number | null ?? string");
        };
        let members = union_members_for_type(&db, domain).unwrap_or_else(|| vec![domain].into());
        assert_eq!(members.len(), 2);
        assert!(members.contains(&TypeId::NUMBER));
        assert!(members.contains(&TypeId::STRING));
    }
// TSZ_INLINE_TEST_END 8497c78d818d45562708f9b1045227632f6cdcbdbdf90029a18742d9c24e8831
