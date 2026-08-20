//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/classes/class_summary.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 916971e4a46a3cf60ca59568e92a564db0f73bcef41eb2e33495ca805d6963bf 38 active_scope_rebind_rejects_same_named_foreign_binder
    #[test]
    fn active_scope_rebind_rejects_same_named_foreign_binder() {
        let db = TypeInterner::new();
        let name = db.intern_string("Outer");
        let file = db.intern_string("scope.ts");
        let source_info = TypeParamInfo {
            origin: TypeParamOrigin::DeclScoped { file, node: 10 },
            ..TypeParamInfo::simple(name)
        };
        let source = db.fresh_type_param(source_info);
        let equivalent = db.fresh_type_param(source_info);
        let foreign = db.fresh_type_param(TypeParamInfo {
            origin: TypeParamOrigin::DeclScoped { file, node: 20 },
            ..source_info
        });
        let summary = ClassChainSummary {
            root_type_params: vec![source],
            ..ClassChainSummary::default()
        };

        let mut scope = FxHashMap::default();
        scope.insert("Outer".to_string(), equivalent);
        assert_eq!(
            summary.root_type_params_from_active_scope(&db, &scope),
            Some(vec![equivalent]),
        );

        scope.insert("Outer".to_string(), foreign);
        assert_eq!(
            summary.root_type_params_from_active_scope(&db, &scope),
            None,
            "a same-named nested binder must not replace the class binder",
        );
    }
// TSZ_INLINE_TEST_END 916971e4a46a3cf60ca59568e92a564db0f73bcef41eb2e33495ca805d6963bf

// TSZ_INLINE_TEST_BEGIN 8274eb9baa3f7dec9c973f6a5f05dd8344df2d41856fb3e2164c74d14bb3f925 73 repeated_member_rebind_reuses_nested_generic_identity
    #[test]
    fn repeated_member_rebind_reuses_nested_generic_identity() {
        let db = TypeInterner::new();
        let source_outer = db.fresh_type_param(TypeParamInfo::simple(db.intern_string("Outer")));
        let active_outer = db.fresh_type_param(TypeParamInfo::simple(db.intern_string("Active")));
        let nested = db.fresh_type_param(TypeParamInfo {
            name: db.intern_string("Nested"),
            constraint: Some(source_outer),
            default: None,
            is_const: false,
            origin: TypeParamOrigin::User,
        });
        let member = crate::query_boundaries::construct_signatures::function_type_from_parts(
            &db,
            Vec::new(),
            vec![ParamInfo {
                type_id: nested,
                ..ParamInfo::default()
            }],
            None,
            TypeId::VOID,
            None,
            false,
            true,
        );
        let summary = ClassChainSummary {
            root_type_params: vec![source_outer],
            ..ClassChainSummary::default()
        };

        let first = summary.rebind_root_type_params(&db, &[active_outer], member);
        let second = summary.rebind_root_type_params(&db, &[active_outer], member);

        assert_eq!(first, second);
        assert_ne!(first, member);
    }
// TSZ_INLINE_TEST_END 8274eb9baa3f7dec9c973f6a5f05dd8344df2d41856fb3e2164c74d14bb3f925

// TSZ_INLINE_TEST_BEGIN 1af03590bbfa7d913917e5ad87249fa4a3147083ccf87bdad09d83c940c82c40 110 identical_or_empty_class_binder_scopes_skip_rewrite_sessions
    #[test]
    fn identical_or_empty_class_binder_scopes_skip_rewrite_sessions() {
        let db = TypeInterner::new();
        let outer = db.fresh_type_param(TypeParamInfo::simple(db.intern_string("Outer")));
        let member = db.array(outer);
        let generic_summary = ClassChainSummary {
            root_type_params: vec![outer],
            ..ClassChainSummary::default()
        };

        assert_eq!(
            generic_summary.rebind_root_type_params(&db, &[outer], member),
            member,
        );
        assert!(generic_summary.rebind_sessions.borrow().is_empty());

        let plain_summary = ClassChainSummary::default();
        assert_eq!(
            plain_summary.rebind_root_type_params(&db, &[], TypeId::STRING),
            TypeId::STRING,
        );
        assert!(plain_summary.rebind_sessions.borrow().is_empty());
    }
// TSZ_INLINE_TEST_END 1af03590bbfa7d913917e5ad87249fa4a3147083ccf87bdad09d83c940c82c40

// TSZ_INLINE_TEST_BEGIN ad8bcca6e0231da1bed528d0ede25329d92fce50914d21f85b089a90c0cae3a5 134 cached_member_rebind_refreshes_late_application_display_alias
    #[test]
    fn cached_member_rebind_refreshes_late_application_display_alias() {
        let db = TypeInterner::new();
        let source_outer = db.fresh_type_param(TypeParamInfo::simple(db.intern_string("Outer")));
        let active_outer = db.fresh_type_param(TypeParamInfo::simple(db.intern_string("Active")));
        // The application is discovered before its evaluated structural result,
        // matching evaluator allocation order, but its provenance is attached
        // only after the first cached rewrite.
        let alias_base = db.lazy(DefId(27));
        let source_alias = db.application(alias_base, vec![source_outer]);
        let member = db.array(source_outer);
        let summary = ClassChainSummary {
            root_type_params: vec![source_outer],
            ..ClassChainSummary::default()
        };

        let first = summary.rebind_root_type_params(&db, &[active_outer], member);
        assert_eq!(db.get_display_alias(first), None);

        db.store_display_alias_preferring_application(member, source_alias);
        db.record_application_eval_origin(member, source_alias);
        assert_eq!(db.get_display_alias(member), Some(source_alias));

        let second = summary.rebind_root_type_params(&db, &[active_outer], member);
        let expected_alias = db.application(alias_base, vec![active_outer]);

        assert_eq!(second, first);
        assert_eq!(db.get_display_alias(second), Some(expected_alias));
        assert_eq!(db.get_application_eval_origin(second), Some(expected_alias));
    }
// TSZ_INLINE_TEST_END ad8bcca6e0231da1bed528d0ede25329d92fce50914d21f85b089a90c0cae3a5

// TSZ_INLINE_TEST_BEGIN d1d1520333dd23f348f9fdb9ea6301f307a5ca9aeb2ae478a0f09b83a8f22bf3 165 cached_rebind_session_shares_nested_binder_across_member_roots
    #[test]
    fn cached_rebind_session_shares_nested_binder_across_member_roots() {
        let db = TypeInterner::new();
        let source_outer = db.fresh_type_param(TypeParamInfo::simple(db.intern_string("Outer")));
        let active_outer = db.fresh_type_param(TypeParamInfo::simple(db.intern_string("Active")));
        let nested = db.fresh_type_param(TypeParamInfo {
            name: db.intern_string("Nested"),
            constraint: Some(source_outer),
            default: None,
            is_const: false,
            origin: TypeParamOrigin::User,
        });
        let function_member =
            crate::query_boundaries::construct_signatures::function_type_from_parts(
                &db,
                Vec::new(),
                vec![ParamInfo {
                    type_id: nested,
                    ..ParamInfo::default()
                }],
                None,
                TypeId::VOID,
                None,
                false,
                true,
            );
        let array_member = db.array(nested);
        let summary = ClassChainSummary {
            root_type_params: vec![source_outer],
            ..ClassChainSummary::default()
        };

        let rewritten_function =
            summary.rebind_root_type_params(&db, &[active_outer], function_member);
        let rewritten_array = summary.rebind_root_type_params(&db, &[active_outer], array_member);

        let rewritten_nested = crate::query_boundaries::exact_rewrite::function_parameter_type(
            &db,
            rewritten_function,
            0,
        )
        .expect("expected rewritten function member");
        let array_nested =
            crate::query_boundaries::exact_rewrite::array_element_type(&db, rewritten_array)
                .expect("expected rewritten array member");
        assert_eq!(rewritten_nested, array_nested);
        assert_ne!(rewritten_nested, nested);
    }
// TSZ_INLINE_TEST_END d1d1520333dd23f348f9fdb9ea6301f307a5ca9aeb2ae478a0f09b83a8f22bf3
