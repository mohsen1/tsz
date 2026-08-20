//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/assignability/shape.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 685a674639d534ed4f155253000b25648ac3518ade44411d3573d6a2e4cdc8ee 184 declaration_origin_queries_distinguish_identity_from_surface
    #[test]
    fn declaration_origin_queries_distinguish_identity_from_surface() {
        let db = TypeInterner::new();
        let file = db.intern_string("identity.js");
        let first = boxed(&db, declared_param(&db, file, "Value", 10));
        let second = boxed(&db, declared_param(&db, file, "Value", 20));
        let renamed = boxed(&db, declared_param(&db, file, "Other", 30));

        assert!(have_distinct_decl_scoped_free_type_parameters(
            &db, first, second
        ));
        assert!(have_same_surface_distinct_decl_scoped_free_type_parameters(
            &db, &db, first, second
        ));
        assert!(have_distinct_decl_scoped_free_type_parameters(
            &db, first, renamed
        ));
        assert!(
            !have_same_surface_distinct_decl_scoped_free_type_parameters(&db, &db, first, renamed)
        );
        assert!(!have_distinct_decl_scoped_free_type_parameters(
            &db, first, first
        ));
    }
// TSZ_INLINE_TEST_END 685a674639d534ed4f155253000b25648ac3518ade44411d3573d6a2e4cdc8ee

// TSZ_INLINE_TEST_BEGIN 0ea63a54968717a85ca719a75bbaf9e157e030136f142d663f505dc22e1d8e40 209 declaration_origin_queries_ignore_legacy_and_reminted_identity
    #[test]
    fn declaration_origin_queries_ignore_legacy_and_reminted_identity() {
        let db = TypeInterner::new();
        let file = db.intern_string("identity.js");
        let name = db.intern_string("Value");
        let origin = TypeParamOrigin::DeclScoped { file, node: 40 };
        let first_param = db.type_param(TypeParamInfo {
            origin,
            ..TypeParamInfo::simple(name)
        });
        let reminted_param = db.type_param(TypeParamInfo {
            constraint: Some(TypeId::STRING),
            origin,
            ..TypeParamInfo::simple(name)
        });
        assert_ne!(first_param, reminted_param);
        assert!(!have_distinct_decl_scoped_free_type_parameters(
            &db,
            boxed(&db, first_param),
            boxed(&db, reminted_param),
        ));

        let legacy_left = boxed(
            &db,
            db.type_param(TypeParamInfo {
                constraint: Some(TypeId::STRING),
                ..TypeParamInfo::simple(name)
            }),
        );
        let legacy_right = boxed(
            &db,
            db.type_param(TypeParamInfo {
                constraint: Some(TypeId::NUMBER),
                ..TypeParamInfo::simple(name)
            }),
        );
        assert!(!have_distinct_decl_scoped_free_type_parameters(
            &db,
            legacy_left,
            legacy_right,
        ));
    }
// TSZ_INLINE_TEST_END 0ea63a54968717a85ca719a75bbaf9e157e030136f142d663f505dc22e1d8e40

// TSZ_INLINE_TEST_BEGIN 601aa52594dc5639d3415246ba6299ae644e8768c2135d54c08c790a43ecec68 252 declaration_origin_queries_distinguish_siblings_at_one_owner_site
    #[test]
    fn declaration_origin_queries_distinguish_siblings_at_one_owner_site() {
        let db = TypeInterner::new();
        let file = db.intern_string("siblings.js");
        let left = boxed(&db, declared_param(&db, file, "T", 45));
        let right = boxed(&db, declared_param(&db, file, "U", 45));

        assert!(have_distinct_decl_scoped_free_type_parameters(
            &db, left, right
        ));
        assert!(
            !have_same_surface_distinct_decl_scoped_free_type_parameters(&db, &db, left, right,)
        );
    }
// TSZ_INLINE_TEST_END 601aa52594dc5639d3415246ba6299ae644e8768c2135d54c08c790a43ecec68

// TSZ_INLINE_TEST_BEGIN c1e8131b3307ed14460c825a39ba8bca859c37f17d6fd1e88c68dc6b36091a60 267 declaration_origin_surface_query_traverses_application_wrappers
    #[test]
    fn declaration_origin_surface_query_traverses_application_wrappers() {
        let db = TypeInterner::new();
        let file = db.intern_string("application.js");
        let base = db.lazy(tsz_solver::DefId(1));
        let first = db.application(
            base,
            vec![boxed(&db, declared_param(&db, file, "Element", 50))],
        );
        let second = db.application(
            base,
            vec![boxed(&db, declared_param(&db, file, "Element", 60))],
        );

        assert!(have_same_surface_distinct_decl_scoped_free_type_parameters(
            &db, &db, first, second
        ));
    }
// TSZ_INLINE_TEST_END c1e8131b3307ed14460c825a39ba8bca859c37f17d6fd1e88c68dc6b36091a60

// TSZ_INLINE_TEST_BEGIN cee39ac402993b8d0cabe09c8e23175cce68802bcb587a38ea396667d67c93ac 286 declaration_origin_surface_query_rejects_ambiguous_same_name_sets
    #[test]
    fn declaration_origin_surface_query_rejects_ambiguous_same_name_sets() {
        let db = TypeInterner::new();
        let file = db.intern_string("ambiguous.js");
        let name = "Repeated";
        let pair = |direct_node, nested_node| {
            db.object(vec![
                PropertyInfo::new(
                    db.intern_string("direct"),
                    declared_param(&db, file, name, direct_node),
                ),
                PropertyInfo::new(
                    db.intern_string("nested"),
                    boxed(&db, declared_param(&db, file, name, nested_node)),
                ),
            ])
        };
        let source = pair(70, 80);
        let target = pair(90, 100);

        assert!(have_distinct_decl_scoped_free_type_parameters(
            &db, source, target
        ));
        assert!(
            !have_same_surface_distinct_decl_scoped_free_type_parameters(&db, &db, source, target)
        );
    }
// TSZ_INLINE_TEST_END cee39ac402993b8d0cabe09c8e23175cce68802bcb587a38ea396667d67c93ac
