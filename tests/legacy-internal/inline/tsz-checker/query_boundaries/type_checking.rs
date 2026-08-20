//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/query_boundaries/type_checking.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 324e0421be9e20a4f1daea0fb935c0cba5558eed8e97eb5133ad8e7d507b7d43 132 constructs_type_checking_surfaces
    #[test]
    fn constructs_type_checking_surfaces() {
        let db = TypeInterner::new();
        let name = db.intern_string("T");
        let type_param = user_type_param(&db, name, Some(TypeId::STRING), None, true);

        assert_eq!(
            type_param,
            db.type_param(TypeParamInfo {
                name,
                constraint: Some(TypeId::STRING),
                default: None,
                is_const: true,
                origin: TypeParamOrigin::User,
            })
        );
        assert_eq!(
            type_checking_union(&db, vec![TypeId::STRING, TypeId::NUMBER]),
            db.union(vec![TypeId::STRING, TypeId::NUMBER])
        );
        assert_eq!(
            type_checking_index_access(&db, TypeId::STRING, TypeId::NUMBER),
            db.index_access(TypeId::STRING, TypeId::NUMBER)
        );
        assert_eq!(
            type_checking_literal_number(&db, 1.0),
            db.literal_number(1.0)
        );

        let param = param_info(
            Some(db.intern_string("value")),
            TypeId::BOOLEAN,
            true,
            false,
        );
        assert_eq!(param.type_id, TypeId::BOOLEAN);
        assert!(param.optional);
        assert!(!param.rest);

        let global_function = global_function_fallback_type(&db, db.intern_string("args"));
        assert!(has_function_shape(&db, global_function));

        let method = method_function_type(&db, vec![], vec![param], TypeId::NUMBER);
        let shape = tsz_solver::type_queries::get_function_shape(&db, method)
            .expect("method function should have shape");
        assert!(shape.is_method);
        assert_eq!(shape.return_type, TypeId::NUMBER);
    }
// TSZ_INLINE_TEST_END 324e0421be9e20a4f1daea0fb935c0cba5558eed8e97eb5133ad8e7d507b7d43
