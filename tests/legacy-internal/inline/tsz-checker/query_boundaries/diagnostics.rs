//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/diagnostics.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 8f2085b7e9ffde2f43c9421aeaf5ca4cf74f11d4f4b2bbf501ad41e9d5cdd792 1517 non_class_nominal_application_surface_matches_by_def_id_for_renamed_interfaces
    #[test]
    fn non_class_nominal_application_surface_matches_by_def_id_for_renamed_interfaces() {
        for name in ["Carrier", "RenamedCarrier"] {
            let db = TypeInterner::new();
            let store = DefinitionStore::new();
            let base = register_interface_base(&db, &store, name);
            let source = db.application(base, vec![TypeId::STRING]);
            let target = db.application(base, vec![TypeId::STRING]);

            assert!(
                same_non_class_nominal_application_surface(&db, &db, &store, &[source], &[target],),
                "same interface application surface should match structurally for {name}"
            );
        }
    }
// TSZ_INLINE_TEST_END 8f2085b7e9ffde2f43c9421aeaf5ca4cf74f11d4f4b2bbf501ad41e9d5cdd792

// TSZ_INLINE_TEST_BEGIN fe643dd250cda3e856e4ef63dec49618959c8575361d197bcd0d68f41c8c7f74 1533 non_class_nominal_application_surface_rejects_different_type_args
    #[test]
    fn non_class_nominal_application_surface_rejects_different_type_args() {
        let db = TypeInterner::new();
        let store = DefinitionStore::new();
        let base = register_interface_base(&db, &store, "Carrier");
        let source = db.application(base, vec![TypeId::STRING]);
        let target = db.application(base, vec![TypeId::NUMBER]);

        assert!(
            !same_non_class_nominal_application_surface(&db, &db, &store, &[source], &[target]),
            "same generic base with different type arguments must not suppress TS2345"
        );
    }
// TSZ_INLINE_TEST_END fe643dd250cda3e856e4ef63dec49618959c8575361d197bcd0d68f41c8c7f74

// TSZ_INLINE_TEST_BEGIN bcdaebca51ac60fb1a03f90b7e53f6063f34edce2bf368976b91bd54fde93b0f 1547 class_and_type_query_application_surfaces_do_not_match
    #[test]
    fn class_and_type_query_application_surfaces_do_not_match() {
        let db = TypeInterner::new();
        let store = DefinitionStore::new();
        let class_def = store.register(DefinitionInfo::class(
            db.intern_string("Box"),
            vec![TypeParamInfo::simple(db.intern_string("T"))],
            vec![PropertyInfo::new(db.intern_string("value"), TypeId::STRING)],
            vec![],
        ));
        let class_app = db.application(db.lazy(class_def), vec![TypeId::STRING]);
        assert!(!same_non_class_nominal_application_surface(
            &db,
            &db,
            &store,
            &[class_app],
            &[class_app]
        ));

        let query_app = db.application(db.type_query(SymbolRef(7)), vec![TypeId::STRING]);
        assert!(!same_non_class_nominal_application_surface(
            &db,
            &db,
            &store,
            &[query_app],
            &[query_app]
        ));
    }
// TSZ_INLINE_TEST_END bcdaebca51ac60fb1a03f90b7e53f6063f34edce2bf368976b91bd54fde93b0f
