//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/types/queries/lib.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 2959de4d4cf8a14783cdd8c3317f71c98bc7426e7d42bd8d6bdabeffeb287dbc 1940 shared_array_resolution_reuses_registered_base_and_params
    #[test]
    fn shared_array_resolution_reuses_registered_base_and_params() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let array_base = types.factory().object(Vec::new());
        let array_param = TypeParamInfo {
            name: types.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
            origin: tsz_solver::TypeParamOrigin::User,
        };
        types.set_array_base_type(array_base, vec![array_param]);

        let mut checker = CheckerState::new(
            &arena,
            &binder,
            &types,
            "test.ts".to_string(),
            crate::context::CheckerOptions::default(),
        );
        checker.ctx.share_owner_symbol_type_results = true;

        let (resolved, params) = checker.resolve_lib_type_with_params("Array");

        assert_eq!(resolved, Some(array_base));
        assert_eq!(params, vec![array_param]);

        let (resolved_string, params_string) = checker.resolve_lib_type_with_params("String");
        assert_eq!(resolved_string, None);
        assert_eq!(params_string, Vec::<TypeParamInfo>::new());
    }
// TSZ_INLINE_TEST_END 2959de4d4cf8a14783cdd8c3317f71c98bc7426e7d42bd8d6bdabeffeb287dbc
