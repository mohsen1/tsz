//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/operations/constraints/walker_helpers.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 2a0a3f022c48c1ca2d61cf1df03e52e4a25229604ec10f6a84fd51a3ce663256 715 signature_constraint_variance_uses_target_declaration_kind
    #[test]
    fn signature_constraint_variance_uses_target_declaration_kind() {
        for is_construct in [false, true] {
            for use_explicit_this in [false, true] {
                assert_signature_candidate_routing(is_construct, true, use_explicit_this);
                assert_signature_candidate_routing(is_construct, false, use_explicit_this);
            }
        }
    }
// TSZ_INLINE_TEST_END 2a0a3f022c48c1ca2d61cf1df03e52e4a25229604ec10f6a84fd51a3ce663256

// TSZ_INLINE_TEST_BEGIN 05b9f938c07bda4f8e1a49c156e2f526c19805104977693f4ce7f695ca211899 725 method_property_hint_does_not_loosen_constructor_bridges
    #[test]
    fn method_property_hint_does_not_loosen_constructor_bridges() {
        for bridge in 0..3 {
            assert_method_hint_does_not_loosen_constructor_bridge(bridge);
        }
    }
// TSZ_INLINE_TEST_END 05b9f938c07bda4f8e1a49c156e2f526c19805104977693f4ce7f695ca211899

// TSZ_INLINE_TEST_BEGIN 5b62499c38f823fbf4a7b92d445344343d28d387a132e749884dda57eca9a3b4 732 nested_strict_signature_toggles_back_to_covariant_candidates
    #[test]
    fn nested_strict_signature_toggles_back_to_covariant_candidates() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let resolver = TypeEnvironment::new();
        let mut checker = ResolverBackedChecker {
            resolver: &resolver,
            assignable: true,
        };
        let mut evaluator = CallEvaluator::new(&cache, &mut checker);
        let t_param = TypeParamInfo::simple(interner.intern_string("Nested"));
        let t_type = interner.type_param(t_param);
        let inner = |ty| {
            interner.function(FunctionShape::new(
                unary_signature(&interner, ty, false).params,
                TypeId::UNKNOWN,
            ))
        };
        let source = interner.callable(CallableShape {
            call_signatures: vec![unary_signature(&interner, inner(TypeId::STRING), false)],
            ..CallableShape::default()
        });
        let target = interner.callable(CallableShape {
            call_signatures: vec![unary_signature(&interner, inner(t_type), false)],
            ..CallableShape::default()
        });
        let mut ctx = InferenceContext::new(&interner);
        let var = ctx.fresh_type_param(t_param.name, false);
        let mut var_map = FxHashMap::default();
        var_map.insert(t_type, var);

        evaluator.constrain_types(
            &mut ctx,
            &var_map,
            source,
            target,
            InferencePriority::NakedTypeVariable,
        );

        assert_eq!(
            ctx.get_constraints(var)
                .expect("double contravariance must produce a regular candidate")
                .lower_bounds,
            vec![TypeId::STRING]
        );
        assert!(ctx.get_contra_candidate_types(var).is_empty());
    }
// TSZ_INLINE_TEST_END 5b62499c38f823fbf4a7b92d445344343d28d387a132e749884dda57eca9a3b4

// TSZ_INLINE_TEST_BEGIN 2ed46efa5c2e9c5a6a0b1645f8e171f18c7b0e4cebd76c3229c0a0c7e4b1df8f 780 triple_nested_strict_signature_routes_to_contravariant_candidates
    #[test]
    fn triple_nested_strict_signature_routes_to_contravariant_candidates() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let resolver = TypeEnvironment::new();
        let mut checker = ResolverBackedChecker {
            resolver: &resolver,
            assignable: true,
        };
        let mut evaluator = CallEvaluator::new(&cache, &mut checker);
        let t_param = TypeParamInfo::simple(interner.intern_string("TripleNested"));
        let t_type = interner.type_param(t_param);
        let inner = |ty| {
            interner.function(FunctionShape::new(
                unary_signature(&interner, ty, false).params,
                TypeId::UNKNOWN,
            ))
        };
        let source = interner.callable(CallableShape {
            call_signatures: vec![unary_signature(
                &interner,
                inner(inner(TypeId::STRING)),
                false,
            )],
            ..CallableShape::default()
        });
        let target = interner.callable(CallableShape {
            call_signatures: vec![unary_signature(&interner, inner(inner(t_type)), false)],
            ..CallableShape::default()
        });
        let mut ctx = InferenceContext::new(&interner);
        let var = ctx.fresh_type_param(t_param.name, false);
        let mut var_map = FxHashMap::default();
        var_map.insert(t_type, var);

        evaluator.constrain_types(
            &mut ctx,
            &var_map,
            source,
            target,
            InferencePriority::NakedTypeVariable,
        );

        assert!(
            ctx.get_constraints(var)
                .map(|constraints| constraints.lower_bounds.is_empty())
                .unwrap_or(true)
        );
        assert_eq!(ctx.get_contra_candidate_types(var), vec![TypeId::STRING]);
    }
// TSZ_INLINE_TEST_END 2ed46efa5c2e9c5a6a0b1645f8e171f18c7b0e4cebd76c3229c0a0c7e4b1df8f

// TSZ_INLINE_TEST_BEGIN c02ce0703263db991e7904ac6e0c45354acc12c4bb442eaec8ffaa52dad2cde7 831 method_property_metadata_reaches_rebuilt_constraint_signature
    #[test]
    fn method_property_metadata_reaches_rebuilt_constraint_signature() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let resolver = TypeEnvironment::new();
        let mut checker = ResolverBackedChecker {
            resolver: &resolver,
            assignable: true,
        };
        let mut evaluator = CallEvaluator::new(&cache, &mut checker);
        let t_param = TypeParamInfo::simple(interner.intern_string("Rebuilt"));
        let t_type = interner.type_param(t_param);
        let function = |ty| {
            interner.function(FunctionShape::new(
                unary_signature(&interner, ty, false).params,
                TypeId::UNKNOWN,
            ))
        };
        let member = interner.intern_string("consume");
        let source = interner.object(vec![PropertyInfo::new(member, function(TypeId::STRING))]);
        let mut target_property = PropertyInfo::new(member, function(t_type));
        target_property.is_method = true;
        let target = interner.object(vec![target_property]);
        let mut ctx = InferenceContext::new(&interner);
        let var = ctx.fresh_type_param(t_param.name, false);
        let mut var_map = FxHashMap::default();
        var_map.insert(t_type, var);

        evaluator.constrain_types(
            &mut ctx,
            &var_map,
            source,
            target,
            InferencePriority::NakedTypeVariable,
        );

        assert_eq!(
            ctx.get_constraints(var)
                .expect("property method metadata must reach the signature boundary")
                .lower_bounds,
            vec![TypeId::STRING]
        );
        assert!(ctx.get_contra_candidate_types(var).is_empty());
    }
// TSZ_INLINE_TEST_END c02ce0703263db991e7904ac6e0c45354acc12c4bb442eaec8ffaa52dad2cde7

// TSZ_INLINE_TEST_BEGIN 7d15a7d649f9704eda9ba9076dce2becc88c8f7cf54c73f973764ac92f707036 876 method_property_metadata_does_not_reach_callable_number_index
    #[test]
    fn method_property_metadata_does_not_reach_callable_number_index() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let resolver = TypeEnvironment::new();
        let mut checker = ResolverBackedChecker {
            resolver: &resolver,
            assignable: true,
        };
        let mut evaluator = CallEvaluator::new(&cache, &mut checker);
        let t_param = TypeParamInfo::simple(interner.intern_string("Indexed"));
        let t_type = interner.type_param(t_param);
        let function = |ty| {
            interner.function(FunctionShape::new(
                unary_signature(&interner, ty, false).params,
                TypeId::UNKNOWN,
            ))
        };
        let callable = |value_type| {
            interner.callable(CallableShape {
                number_index: Some(IndexSignature {
                    key_type: TypeId::NUMBER,
                    value_type,
                    readonly: false,
                    param_name: None,
                }),
                ..CallableShape::default()
            })
        };
        let source = callable(function(TypeId::STRING));
        let target = callable(function(t_type));
        let mut ctx = InferenceContext::new(&interner);
        ctx.pending_target_method = true;
        let var = ctx.fresh_type_param(t_param.name, false);
        let mut var_map = FxHashMap::default();
        var_map.insert(t_type, var);

        evaluator.constrain_types(
            &mut ctx,
            &var_map,
            source,
            target,
            InferencePriority::NakedTypeVariable,
        );

        assert!(
            ctx.get_constraints(var)
                .map(|constraints| constraints.lower_bounds.is_empty())
                .unwrap_or(true)
        );
        assert_eq!(ctx.get_contra_candidate_types(var), vec![TypeId::STRING]);
        assert!(ctx.pending_target_method);
    }
// TSZ_INLINE_TEST_END 7d15a7d649f9704eda9ba9076dce2becc88c8f7cf54c73f973764ac92f707036

// TSZ_INLINE_TEST_BEGIN 1ffbd8da0cbbb7ea2e05f76f8abbf8d62678ce116ff1eba6c735729386d0ddd3 930 method_property_metadata_does_not_reach_type_predicate
    #[test]
    fn method_property_metadata_does_not_reach_type_predicate() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let resolver = TypeEnvironment::new();
        let mut checker = ResolverBackedChecker {
            resolver: &resolver,
            assignable: true,
        };
        let mut evaluator = CallEvaluator::new(&cache, &mut checker);
        let t_param = TypeParamInfo::simple(interner.intern_string("Predicate"));
        let t_type = interner.type_param(t_param);
        let predicate_name = interner.intern_string("value");
        let predicate = |ty| TypePredicate {
            asserts: false,
            target: TypePredicateTarget::Identifier(predicate_name),
            type_id: Some(interner.function(FunctionShape::new(
                unary_signature(&interner, ty, false).params,
                TypeId::BOOLEAN,
            ))),
            parameter_index: Some(0),
        };
        let signature = |ty| {
            let mut signature = CallSignature::new(Vec::new(), TypeId::BOOLEAN);
            signature.type_predicate = Some(predicate(ty));
            signature
        };
        let mut ctx = InferenceContext::new(&interner);
        ctx.pending_target_method = true;
        let var = ctx.fresh_type_param(t_param.name, false);
        let mut var_map = FxHashMap::default();
        var_map.insert(t_type, var);

        evaluator.constrain_call_signature_to_call_signature(
            &mut ctx,
            &var_map,
            &signature(TypeId::STRING),
            &signature(t_type),
            InferencePriority::ReturnType,
            false,
        );

        assert!(
            ctx.get_constraints(var)
                .map(|constraints| constraints.lower_bounds.is_empty())
                .unwrap_or(true)
        );
        assert_eq!(ctx.get_contra_candidate_types(var), vec![TypeId::STRING]);
        assert!(ctx.pending_target_method);
    }
// TSZ_INLINE_TEST_END 1ffbd8da0cbbb7ea2e05f76f8abbf8d62678ce116ff1eba6c735729386d0ddd3

// TSZ_INLINE_TEST_BEGIN cd5c93deff21df3aca168da3b63aa4e732de3a97b898b7c018bedad80c16599c 981 method_property_metadata_does_not_reach_return_signature
    #[test]
    fn method_property_metadata_does_not_reach_return_signature() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let resolver = TypeEnvironment::new();
        let mut checker = ResolverBackedChecker {
            resolver: &resolver,
            assignable: true,
        };
        let mut evaluator = CallEvaluator::new(&cache, &mut checker);
        let t_param = TypeParamInfo::simple(interner.intern_string("Returned"));
        let t_type = interner.type_param(t_param);
        let function = |ty| {
            interner.function(FunctionShape::new(
                unary_signature(&interner, ty, false).params,
                TypeId::BOOLEAN,
            ))
        };
        let source = CallSignature::new(Vec::new(), function(TypeId::STRING));
        let target = CallSignature::new(Vec::new(), function(t_type));
        let mut ctx = InferenceContext::new(&interner);
        ctx.pending_target_method = true;
        let var = ctx.fresh_type_param(t_param.name, false);
        let mut var_map = FxHashMap::default();
        var_map.insert(t_type, var);

        evaluator.constrain_call_signature_to_call_signature(
            &mut ctx,
            &var_map,
            &source,
            &target,
            InferencePriority::ReturnType,
            false,
        );

        assert!(
            ctx.get_constraints(var)
                .map(|constraints| constraints.lower_bounds.is_empty())
                .unwrap_or(true)
        );
        assert_eq!(ctx.get_contra_candidate_types(var), vec![TypeId::STRING]);
        assert!(ctx.pending_target_method);
    }
// TSZ_INLINE_TEST_END cd5c93deff21df3aca168da3b63aa4e732de3a97b898b7c018bedad80c16599c

// TSZ_INLINE_TEST_BEGIN 61bbb5c5d21fc47e2c9ab95b43000818416b1942de63d3f024dd44d703a6ac92 1025 compute_application_variances_reuses_query_cache
    #[test]
    fn compute_application_variances_reuses_query_cache() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let t_param = TypeParamInfo::simple(interner.intern_string("T"));
        let t_type = interner.type_param(t_param);
        let body = interner.object(vec![PropertyInfo::new(
            interner.intern_string("value"),
            t_type,
        )]);
        let def_id = DefId(91_001);
        let base = interner.lazy(def_id);

        let mut env = TypeEnvironment::new();
        env.insert_def_with_params(def_id, body, vec![t_param]);
        let mut checker = ResolverBackedChecker {
            resolver: &env,
            assignable: true,
        };
        let evaluator = CallEvaluator::new(&cache, &mut checker);

        assert_eq!(cache.statistics().variance_cache_entries, 0);
        assert!(evaluator.compute_application_variances(base).is_some());
        assert_eq!(cache.statistics().variance_cache_entries, 1);
        assert!(evaluator.compute_application_variances(base).is_some());
        assert_eq!(cache.statistics().variance_cache_entries, 1);
    }
// TSZ_INLINE_TEST_END 61bbb5c5d21fc47e2c9ab95b43000818416b1942de63d3f024dd44d703a6ac92

// TSZ_INLINE_TEST_BEGIN a79c78042ae2720339abd0bc12b111e22ed94db8aeac62c01f14a3d9d6efd459 1053 equivalent_application_bases_constrain_type_args
    #[test]
    fn equivalent_application_bases_constrain_type_args() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let source_def = DefId(143_570);
        let target_def = DefId(143_571);
        let resolver = PairEquivalentResolver {
            left: source_def,
            right: target_def,
        };
        let mut checker = ResolverBackedChecker {
            resolver: &resolver,
            assignable: false,
        };
        let mut evaluator = CallEvaluator::new(&cache, &mut checker);

        let t_param = TypeParamInfo::simple(interner.intern_string("T"));
        let t_type = interner.type_param(t_param);
        let mut infer_ctx = InferenceContext::new(&interner);
        let var_t = infer_ctx.fresh_type_param(t_param.name, false);
        let mut var_map = FxHashMap::default();
        var_map.insert(t_type, var_t);

        let source = interner.application(interner.lazy(source_def), vec![TypeId::STRING]);
        let target = interner.application(interner.lazy(target_def), vec![t_type]);
        evaluator.constrain_types(
            &mut infer_ctx,
            &var_map,
            source,
            target,
            InferencePriority::NakedTypeVariable,
        );

        assert_eq!(
            infer_ctx
                .resolve_with_constraints(var_t)
                .expect("equivalent application bases must constrain the type arg"),
            TypeId::STRING
        );
    }
// TSZ_INLINE_TEST_END a79c78042ae2720339abd0bc12b111e22ed94db8aeac62c01f14a3d9d6efd459

// TSZ_INLINE_TEST_BEGIN eb1bb90880e179dc076fb95123f56a67d8c4673b264351b2e626b6646614c98c 1094 unrelated_application_bases_do_not_constrain_type_args
    #[test]
    fn unrelated_application_bases_do_not_constrain_type_args() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let resolver = PairEquivalentResolver {
            left: DefId(143_580),
            right: DefId(143_581),
        };
        let mut checker = ResolverBackedChecker {
            resolver: &resolver,
            assignable: false,
        };
        let mut evaluator = CallEvaluator::new(&cache, &mut checker);

        let t_param = TypeParamInfo::simple(interner.intern_string("T"));
        let t_type = interner.type_param(t_param);
        let mut infer_ctx = InferenceContext::new(&interner);
        let var_t = infer_ctx.fresh_type_param(t_param.name, false);
        let mut var_map = FxHashMap::default();
        var_map.insert(t_type, var_t);

        let source = interner.application(interner.lazy(DefId(143_582)), vec![TypeId::STRING]);
        let target = interner.application(interner.lazy(DefId(143_583)), vec![t_type]);
        evaluator.constrain_types(
            &mut infer_ctx,
            &var_map,
            source,
            target,
            InferencePriority::NakedTypeVariable,
        );

        assert_eq!(
            infer_ctx
                .resolve_with_constraints(var_t)
                .expect("unconstrained inference var must still resolve (to unknown)"),
            TypeId::UNKNOWN
        );
    }
// TSZ_INLINE_TEST_END eb1bb90880e179dc076fb95123f56a67d8c4673b264351b2e626b6646614c98c
