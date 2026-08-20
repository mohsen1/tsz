//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/relations/subtype/rules/functions/checking/context_instantiation.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 1668c8a7eaab5b73139a1752c3060948053f19259aceaab82395ab06ff480337 319 nested_retry_guard_follows_contravariant_parameter_direction
    #[test]
    fn nested_retry_guard_follows_contravariant_parameter_direction() {
        let interner = TypeInterner::new();
        let pack = interner.fresh_type_param(TypeParamInfo {
            name: interner.intern_string("Pack"),
            constraint: Some(interner.array(TypeId::UNKNOWN)),
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped {
                file: interner.intern_string("nested-rest-direction.ts"),
                node: 1,
            },
        });
        let rest_params = vec![ParamInfo {
            name: None,
            type_id: pack,
            optional: false,
            rest: true,
        }];
        let fixed_params = vec![ParamInfo::unnamed(pack)];
        let callback = |params| {
            interner.function(FunctionShape {
                type_params: vec![],
                params,
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_constructor: false,
                is_method: false,
            })
        };
        let outer = |callback_type| FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo::unnamed(callback_type)],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        };
        let written_source = outer(callback(fixed_params.clone()));
        let written_target = outer(callback(rest_params.clone()));

        let mut checker = SubtypeChecker::new(&interner).with_query_db(&interner);
        checker.strict_function_types = true;
        checker.allow_bivariant_rest = true;

        assert!(checker.rigid_bare_rest_params_mismatch(&rest_params, &fixed_params, false));
        assert!(!checker.rigid_bare_rest_params_mismatch(&fixed_params, &rest_params, false));
        assert!(checker.nested_rigid_rest_blocks_contextual_retry(
            &written_source,
            &written_target,
            false,
        ));
    }
// TSZ_INLINE_TEST_END 1668c8a7eaab5b73139a1752c3060948053f19259aceaab82395ab06ff480337
