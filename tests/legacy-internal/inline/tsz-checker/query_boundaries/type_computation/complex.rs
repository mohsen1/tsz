//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/type_computation/complex.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 59a0bd6d925504d067c561a6b05057a1946dc9a1dba50a4c7dde0a64a22de9e0 243 application_infos_for_type_returns_direct_application
    #[test]
    fn application_infos_for_type_returns_direct_application() {
        let db = TypeInterner::new();
        let app = db.application(TypeId::STRING, vec![TypeId::NUMBER]);

        let applications = application_infos_for_type(&db, app);

        assert_eq!(applications, vec![(TypeId::STRING, vec![TypeId::NUMBER])]);
    }
// TSZ_INLINE_TEST_END 59a0bd6d925504d067c561a6b05057a1946dc9a1dba50a4c7dde0a64a22de9e0

// TSZ_INLINE_TEST_BEGIN a685dad1c6d492c28934c55e5a454ec791e39574835f01cd1bcbe36bd6fae307 253 application_infos_for_type_returns_display_alias_application
    #[test]
    fn application_infos_for_type_returns_display_alias_application() {
        let db = TypeInterner::new();
        let evaluated = fresh_object(&db, "value", TypeId::NUMBER);
        let alias_app = db.application(TypeId::STRING, vec![TypeId::NUMBER]);
        db.store_display_alias(evaluated, alias_app);

        let applications = application_infos_for_type(&db, evaluated);

        assert_eq!(applications, vec![(TypeId::STRING, vec![TypeId::NUMBER])]);
    }
// TSZ_INLINE_TEST_END a685dad1c6d492c28934c55e5a454ec791e39574835f01cd1bcbe36bd6fae307

// TSZ_INLINE_TEST_BEGIN 9f24eacb3cdee7da324a1ab099a609adf90fc0fbc6efa8bfdbeb74d844a77f3b 265 application_infos_for_type_includes_direct_and_distinct_alias_application
    #[test]
    fn application_infos_for_type_includes_direct_and_distinct_alias_application() {
        let db = TypeInterner::new();
        let direct_app = db.application(TypeId::STRING, vec![TypeId::NUMBER]);
        let alias_app = db.application(TypeId::NUMBER, vec![TypeId::STRING]);
        db.store_display_alias(direct_app, alias_app);

        let applications = application_infos_for_type(&db, direct_app);

        assert_eq!(
            applications,
            vec![
                (TypeId::STRING, vec![TypeId::NUMBER]),
                (TypeId::NUMBER, vec![TypeId::STRING]),
            ]
        );
    }
// TSZ_INLINE_TEST_END 9f24eacb3cdee7da324a1ab099a609adf90fc0fbc6efa8bfdbeb74d844a77f3b
