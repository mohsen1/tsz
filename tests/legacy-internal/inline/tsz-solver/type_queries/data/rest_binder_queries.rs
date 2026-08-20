//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/type_queries/data/rest_binder_queries.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN d5dceef335e4c9de58059449b7663a1d516046d710a07f71068b538a599b1f1d 974 deep_no_infer_chain_has_no_sixteen_wrapper_cliff
    #[test]
    fn deep_no_infer_chain_has_no_sixteen_wrapper_cliff() {
        let interner = TypeInterner::new();
        let (info, binder) = declared_pack(&interner, 1);
        let mut wrapped = binder;
        for _ in 0..64 {
            wrapped = interner.no_infer(wrapped);
        }

        assert!(matches!(
            transparent_bare_rest_type_parameter_with_resolver_query(
                &interner,
                &NoopResolver,
                wrapped,
            ),
            RestBinderQuery::Complete(Some(found)) if found.is_same_binder(info)
        ));
    }
// TSZ_INLINE_TEST_END d5dceef335e4c9de58059449b7663a1d516046d710a07f71068b538a599b1f1d

// TSZ_INLINE_TEST_BEGIN 0c36b601b8ede0fffed43249cb13702e87a2909178befc52a86714089a3dc374 993 repeated_identity_alias_chain_has_no_256_reentrance_cliff
    #[test]
    fn repeated_identity_alias_chain_has_no_256_reentrance_cliff() {
        let interner = TypeInterner::new();
        let (pack_info, pack) = declared_pack(&interner, 4);
        let alias_param = TypeParamInfo {
            name: interner.intern_string("AliasPack"),
            constraint: Some(interner.array(TypeId::UNKNOWN)),
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped {
                file: interner.intern_string("deep-rest-query.ts"),
                node: 5,
            },
        };
        let alias_body = interner.fresh_type_param(alias_param);
        let def_id = DefId(9_001);
        let alias = interner.lazy(def_id);
        let resolver = IdentityAliasResolver {
            def_id,
            body: alias_body,
            type_param: alias_param,
        };
        let mut nested = pack;
        for _ in 0..300 {
            nested = interner.application(alias, vec![nested]);
        }

        assert!(matches!(
            transparent_bare_rest_type_parameter_with_resolver_query(
                &interner,
                &resolver,
                nested,
            ),
            RestBinderQuery::Complete(Some(found)) if found.is_same_binder(pack_info)
        ));
    }
// TSZ_INLINE_TEST_END 0c36b601b8ede0fffed43249cb13702e87a2909178befc52a86714089a3dc374

// TSZ_INLINE_TEST_BEGIN 6c95a11335d79a5b5c44fc89136afbc84cbd16a97858afabffbbb47af68ac7a1 1030 conditional_identity_requires_the_declared_constraint_surface
    #[test]
    fn conditional_identity_requires_the_declared_constraint_surface() {
        let interner = TypeInterner::new();
        let (info, binder) = declared_pack(&interner, 2);
        let constraint = info.constraint.expect("declared pack has a constraint");
        let identity = interner.conditional(ConditionalType {
            check_type: binder,
            extends_type: constraint,
            true_type: binder,
            false_type: TypeId::NEVER,
            is_distributive: true,
        });
        assert!(matches!(
            transparent_bare_rest_type_parameter_with_resolver_query(
                &interner,
                &NoopResolver,
                identity,
            ),
            RestBinderQuery::Complete(Some(found)) if found.is_same_binder(info)
        ));

        let non_identity = interner.conditional(ConditionalType {
            check_type: binder,
            extends_type: interner.tuple(vec![]),
            true_type: binder,
            false_type: TypeId::NEVER,
            is_distributive: true,
        });
        assert!(matches!(
            transparent_bare_rest_type_parameter_with_resolver_query(
                &interner,
                &NoopResolver,
                non_identity,
            ),
            RestBinderQuery::Complete(None)
        ));

        let different_false_branch = interner.conditional(ConditionalType {
            check_type: binder,
            extends_type: constraint,
            true_type: binder,
            false_type: TypeId::STRING,
            is_distributive: true,
        });
        assert!(matches!(
            transparent_bare_rest_type_parameter_with_resolver_query(
                &interner,
                &NoopResolver,
                different_false_branch,
            ),
            RestBinderQuery::Complete(None)
        ));
    }
// TSZ_INLINE_TEST_END 6c95a11335d79a5b5c44fc89136afbc84cbd16a97858afabffbbb47af68ac7a1

// TSZ_INLINE_TEST_BEGIN 848a794bb3d41224755e8832ac063230d77f1df42c70f01910f12b12bdde75d6 1084 single_variadic_tuple_query_distinguishes_spread_from_array_and_fixed_tuple
    #[test]
    fn single_variadic_tuple_query_distinguishes_spread_from_array_and_fixed_tuple() {
        let interner = TypeInterner::new();
        let (info, binder) = declared_pack(&interner, 7);
        let spread_tuple = interner.tuple(vec![TupleElement {
            type_id: binder,
            name: None,
            optional: false,
            rest: true,
        }]);
        let fixed_tuple = interner.tuple(vec![TupleElement {
            type_id: binder,
            name: None,
            optional: false,
            rest: false,
        }]);

        assert!(matches!(
            single_variadic_tuple_rest_type_parameter_with_resolver_query(
                &interner,
                &NoopResolver,
                spread_tuple,
            ),
            RestBinderQuery::Complete(Some(found)) if found.is_same_binder(info)
        ));
        for non_spread in [interner.array(binder), fixed_tuple] {
            assert_eq!(
                single_variadic_tuple_rest_type_parameter_with_resolver_query(
                    &interner,
                    &NoopResolver,
                    non_spread,
                ),
                RestBinderQuery::Complete(None),
            );
        }
    }
// TSZ_INLINE_TEST_END 848a794bb3d41224755e8832ac063230d77f1df42c70f01910f12b12bdde75d6

// TSZ_INLINE_TEST_BEGIN 8e814b8c0ccfa976793e4794f0dda807b59cd8f8dc51e8b29b2b1ed96a50fa6e 1121 shared_identity_conditional_dag_reuses_completed_binder_results
    #[test]
    fn shared_identity_conditional_dag_reuses_completed_binder_results() {
        let interner = TypeInterner::new();
        let (info, binder) = declared_pack(&interner, 6);
        let constraint = info.constraint.expect("declared pack has a constraint");
        let mut identity = binder;
        for _ in 0..32 {
            identity = interner.conditional(ConditionalType {
                check_type: identity,
                extends_type: constraint,
                true_type: identity,
                false_type: TypeId::NEVER,
                is_distributive: true,
            });
        }

        assert!(matches!(
            transparent_bare_rest_type_parameter_with_resolver_query(
                &interner,
                &NoopResolver,
                identity,
            ),
            RestBinderQuery::Complete(Some(found)) if found.is_same_binder(info)
        ));
    }
// TSZ_INLINE_TEST_END 8e814b8c0ccfa976793e4794f0dda807b59cd8f8dc51e8b29b2b1ed96a50fa6e

// TSZ_INLINE_TEST_BEGIN 2c2b27b57708497753680645cdf69228a114fb3f14a3d4c74fc0b6a8adae4476 1147 declared_rest_visitor_crosses_deep_structural_wrappers
    #[test]
    fn declared_rest_visitor_crosses_deep_structural_wrappers() {
        let interner = TypeInterner::new();
        let (_, binder) = declared_pack(&interner, 3);
        let mut nested = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: None,
                type_id: binder,
                optional: false,
                rest: true,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        for level in 0..64 {
            nested = interner.object(vec![PropertyInfo::new(
                interner.intern_string(&format!("level{level}")),
                nested,
            )]);
        }

        assert_eq!(
            contains_declared_bare_function_rest_with_resolver_query(
                &interner,
                &NoopResolver,
                nested,
            ),
            RestBinderQuery::Complete(true)
        );
    }
// TSZ_INLINE_TEST_END 2c2b27b57708497753680645cdf69228a114fb3f14a3d4c74fc0b6a8adae4476

// TSZ_INLINE_TEST_BEGIN 14ad18bc98fd748006286216ccdda9a17e9074a7562492ceaf4ddba7dcd72429 1182 structural_fanout_uses_one_operation_wide_budget
    #[test]
    fn structural_fanout_uses_one_operation_wide_budget() {
        let interner = TypeInterner::new();
        let properties = (0..MAX_REST_BINDER_QUERY_STEPS)
            .map(|index| {
                PropertyInfo::new(
                    interner.intern_string(&format!("branch{index}")),
                    TypeId::STRING,
                )
            })
            .collect();
        let wide = interner.object(properties);

        assert_eq!(
            contains_declared_bare_function_rest_with_resolver_query(
                &interner,
                &NoopResolver,
                wide,
            ),
            RestBinderQuery::Incomplete,
            "cloned branch states must share one global traversal budget"
        );
    }
// TSZ_INLINE_TEST_END 14ad18bc98fd748006286216ccdda9a17e9074a7562492ceaf4ddba7dcd72429
