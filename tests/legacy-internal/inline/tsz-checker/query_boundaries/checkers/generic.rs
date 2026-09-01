//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/checkers/generic.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 5babf7bdf2efff18ea81e6c6cd056f7b39521f1535a7627b56cda69950cb8499 944 constraint_keyof_surface_detects_direct_keyof
    #[test]
    fn constraint_keyof_surface_detects_direct_keyof() {
        let db = TypeInterner::new();
        let object = object_with_property(&db, "alpha");
        let keyof = db.keyof(object);

        assert!(constraint_has_keyof_surface(&db, keyof));
    }
// TSZ_INLINE_TEST_END 5babf7bdf2efff18ea81e6c6cd056f7b39521f1535a7627b56cda69950cb8499

// TSZ_INLINE_TEST_BEGIN 582ffddd5c0a96d2c6083cbaed7fcf5a3a09474ccaf8635b814632dd93b97691 953 constraint_keyof_surface_ignores_non_keyof_alias
    #[test]
    fn constraint_keyof_surface_ignores_non_keyof_alias() {
        let db = TypeInterner::new();
        let object = object_with_property(&db, "gamma");
        let evaluated = db.union(vec![TypeId::STRING, TypeId::NUMBER]);
        db.store_display_alias(evaluated, object);

        assert!(!constraint_has_keyof_surface(&db, evaluated));
    }
// TSZ_INLINE_TEST_END 582ffddd5c0a96d2c6083cbaed7fcf5a3a09474ccaf8635b814632dd93b97691

// TSZ_INLINE_TEST_BEGIN 56c8ba73e4e1ea390b00b036abc387bc95611da0953172feabe4329bae411484 963 constraint_keyof_surface_ignores_keyof_display_alias
    #[test]
    fn constraint_keyof_surface_ignores_keyof_display_alias() {
        let db = TypeInterner::new();
        let object = object_with_property(&db, "delta");
        let keyof = db.keyof(object);
        let evaluated = db.union(vec![TypeId::STRING, TypeId::NUMBER]);
        db.store_display_alias(evaluated, keyof);

        assert!(!constraint_has_keyof_surface(&db, evaluated));
    }
// TSZ_INLINE_TEST_END 56c8ba73e4e1ea390b00b036abc387bc95611da0953172feabe4329bae411484

// TSZ_INLINE_TEST_BEGIN 249df2f570eae6710a94a7bd453ab90d3a03954109154dcf7e17b3d39d6d8c12 974 indexed_object_map_value_accepts_intersection_containing_constraint
    #[test]
    fn indexed_object_map_value_accepts_intersection_containing_constraint() {
        let db = TypeInterner::new();
        let value = db.intersection(vec![TypeId::OBJECT, TypeId::STRING]);

        assert!(indexed_object_map_value_structurally_satisfies_constraint(
            &db,
            value,
            TypeId::STRING,
        ));
    }
// TSZ_INLINE_TEST_END 249df2f570eae6710a94a7bd453ab90d3a03954109154dcf7e17b3d39d6d8c12

// TSZ_INLINE_TEST_BEGIN 70b382a4e6fd3b52182c84bad80cdd07730bedd74c12d0704a6e72e430094fcf 986 indexed_object_map_value_rejects_intersection_without_constraint
    #[test]
    fn indexed_object_map_value_rejects_intersection_without_constraint() {
        let db = TypeInterner::new();
        let param = db.type_param(tsz_solver::TypeParamInfo {
            name: db.intern_string("Item"),
            constraint: None,
            default: None,
            is_const: false,
            origin: tsz_solver::TypeParamOrigin::User,
        });
        let value = db.intersection(vec![param, TypeId::NUMBER]);

        assert!(!indexed_object_map_value_structurally_satisfies_constraint(
            &db,
            value,
            TypeId::STRING,
        ));
    }
// TSZ_INLINE_TEST_END 70b382a4e6fd3b52182c84bad80cdd07730bedd74c12d0704a6e72e430094fcf

// TSZ_INLINE_TEST_BEGIN e7410965d08044fd786f226c1e89d896148b466067106cea048fa902d9e88d1c 1005 mapped_key_constraint_filtering_uses_relation_outcome_boundary
    #[test]
    fn mapped_key_constraint_filtering_uses_relation_outcome_boundary() {
        let source = include_str!("generic.rs");
        let helper_end = source
            .find("#[cfg(test)]")
            .expect("missing generic query-boundary test module marker");
        let helper_source = &source[..helper_end];
        let legacy = concat!("diagnostic_relation", "_boolean_guard(");

        assert!(
            helper_source.contains(
                ".mapped_key_constraint_relation_outcome(constraint_eval, keyof_object_param)"
            ) && helper_source
                .contains(".mapped_key_constraint_relation_outcome(next_eval, keyof_object_param)")
                && helper_source
                    .contains(".mapped_key_constraint_relation_outcome(evaluated, keyof_object)"),
            "mapped-key constraint filtering should route relation probes through \
             the mapped-key constraint relation outcome boundary"
        );
        assert!(
            !helper_source
                .contains(".assign_relation_outcome(constraint_eval, keyof_object_param)")
                && !helper_source
                    .contains(".assign_relation_outcome(next_eval, keyof_object_param)")
                && !helper_source.contains(".assign_relation_outcome(evaluated, keyof_object)"),
            "mapped-key constraint filtering should not use generic assignment request routing"
        );
        assert!(
            !helper_source.contains(legacy),
            "generic query-boundary helpers should not use raw diagnostic relation \
             boolean guards"
        );
    }
// TSZ_INLINE_TEST_END e7410965d08044fd786f226c1e89d896148b466067106cea048fa902d9e88d1c
