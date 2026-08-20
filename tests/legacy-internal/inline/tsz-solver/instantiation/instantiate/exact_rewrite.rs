//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/instantiation/instantiate/exact_rewrite.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN e2f7d0109cc8eeb37ee00bf6d98099f8612509a38457e5004cd51766f5e71e42 992 exact_rewrite_batches_shared_nodes_and_is_simultaneous
    #[test]
    fn exact_rewrite_batches_shared_nodes_and_is_simultaneous() {
        let db = TypeInterner::new();
        let first = fresh_param(&db, "First");
        let second = fresh_param(&db, "Second");
        let shared = db.application(TypeId::OBJECT, vec![first, second]);
        let root = db.tuple(vec![
            TupleElement::fixed(first),
            TupleElement::fixed(second),
            TupleElement::fixed(shared),
            TupleElement::fixed(shared),
        ]);

        let result = substitute_exact_types(&db, root, &[first, second], &[second, first]);
        let members = tuple_members(&db, result);
        assert_eq!(members[0], second);
        assert_eq!(members[1], first);
        assert_eq!(members[2], members[3]);

        let Some(TypeData::Application(app_id)) = db.lookup(members[2]) else {
            panic!("expected application");
        };
        let app = db.type_application(app_id);
        assert_eq!(app.args, vec![second, first]);
    }
// TSZ_INLINE_TEST_END e2f7d0109cc8eeb37ee00bf6d98099f8612509a38457e5004cd51766f5e71e42

// TSZ_INLINE_TEST_BEGIN 575abb89694cbeffdbb0e4f53c22471075474b5433950d5ac1bf7669663a5707 1018 exact_rewrite_uses_identity_not_same_named_binder
    #[test]
    fn exact_rewrite_uses_identity_not_same_named_binder() {
        let db = TypeInterner::new();
        let declaration = fresh_param(&db, "Tail");
        let foreign = fresh_param(&db, "Tail");
        assert_ne!(declaration, foreign);
        let root = db.tuple(vec![
            TupleElement::fixed(declaration),
            TupleElement::fixed(foreign),
        ]);

        let result = substitute_exact_type(&db, root, declaration, TypeId::STRING);
        assert_eq!(tuple_members(&db, result), vec![TypeId::STRING, foreign]);

        let no_match = db.array(foreign);
        assert_eq!(
            substitute_exact_type(&db, no_match, declaration, TypeId::STRING),
            no_match,
        );
    }
// TSZ_INLINE_TEST_END 575abb89694cbeffdbb0e4f53c22471075474b5433950d5ac1bf7669663a5707

// TSZ_INLINE_TEST_BEGIN 1739d435edcf497316d34ac57e90efdb7159322b55c800c536b390f4beb9d24a 1039 exact_rewrite_preserves_union_members_and_raw_intersection_shape
    #[test]
    fn exact_rewrite_preserves_union_members_and_raw_intersection_shape() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let subtype_member = db.literal_string("member");
        assert_eq!(
            db.union(vec![subtype_member, TypeId::STRING]),
            TypeId::STRING,
            "ordinary union construction absorbs the literal subtype",
        );
        let union = db.union_preserve_members(vec![outer, TypeId::STRING]);

        let rewritten_union = substitute_exact_type(&db, union, outer, subtype_member);
        let Some(TypeData::Union(list_id)) = db.lookup(rewritten_union) else {
            panic!("exact replay must not subtype-reduce the literal member");
        };
        assert_eq!(db.type_list(list_id).len(), 2);

        let left = db.object(vec![PropertyInfo::new(db.intern_string("left"), outer)]);
        let right = db.object(vec![PropertyInfo::new(
            db.intern_string("right"),
            TypeId::NUMBER,
        )]);
        let intersection = db.intersect_types_raw(vec![left, right]);
        let rewritten_intersection =
            substitute_exact_type(&db, intersection, outer, subtype_member);
        let Some(TypeData::Intersection(list_id)) = db.lookup(rewritten_intersection) else {
            panic!("exact replay must not normalize raw object intersections");
        };
        assert_eq!(db.type_list(list_id).len(), 2);
    }
// TSZ_INLINE_TEST_END 1739d435edcf497316d34ac57e90efdb7159322b55c800c536b390f4beb9d24a

// TSZ_INLINE_TEST_BEGIN 75fbdd559ae0aeff7e1bd8fbb30bf981fe61475773c8e340e2e913830a5ec6f3 1071 exact_rewrite_preserves_pre_sort_union_member_order
    #[test]
    fn exact_rewrite_preserves_pre_sort_union_member_order() {
        let db = TypeInterner::new();
        let source = fresh_param(&db, "Source");
        let other = fresh_param(&db, "Other");
        let union = db.union_preserve_members(vec![source, other]);
        assert_eq!(db.get_union_origin(union), None);

        let replacement = fresh_param(&db, "Replacement");
        let result = substitute_exact_type(&db, union, source, replacement);

        assert_eq!(
            db.get_union_origin(result).map(|origin| origin.to_vec()),
            Some(vec![replacement, other]),
        );
    }
// TSZ_INLINE_TEST_END 75fbdd559ae0aeff7e1bd8fbb30bf981fe61475773c8e340e2e913830a5ec6f3

// TSZ_INLINE_TEST_BEGIN ef3aeb4ee65462f6a9dc5d5ad58c9314a593d37d75e445147b997588de6908c7 1088 exact_rewrite_prefers_an_existing_union_origin
    #[test]
    fn exact_rewrite_prefers_an_existing_union_origin() {
        let db = TypeInterner::new();
        let first = fresh_param(&db, "First");
        let source = fresh_param(&db, "Source");
        let union = db.union_preserve_members(vec![first, source]);
        db.store_union_origin(union, vec![source, first]);
        assert_eq!(
            db.get_union_origin(union).map(|origin| origin.to_vec()),
            Some(vec![source, first]),
        );

        let replacement = fresh_param(&db, "Replacement");
        let result = substitute_exact_type(&db, union, source, replacement);

        assert_eq!(
            db.get_union_origin(result).map(|origin| origin.to_vec()),
            Some(vec![replacement, first]),
        );
    }
// TSZ_INLINE_TEST_END ef3aeb4ee65462f6a9dc5d5ad58c9314a593d37d75e445147b997588de6908c7

// TSZ_INLINE_TEST_BEGIN f7b7ad48e8a1bc6f5a323a7ed887b30dc6b48f3966683e1c2a417f486aaf2ecb 1109 exact_rewrite_complex_intersection_replay_does_not_signal_union_complexity
    #[test]
    fn exact_rewrite_complex_intersection_replay_does_not_signal_union_complexity() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let source = complex_replay_intersection(&db, outer);
        assert!(!db.is_union_too_complex());

        let result = substitute_exact_type(&db, source, outer, db.literal_string("replacement"));
        assert_ne!(result, source);
        assert!(matches!(db.lookup(result), Some(TypeData::Intersection(_))));
        assert!(
            !db.is_union_too_complex(),
            "replaying admitted structure must not request TS2590",
        );
    }
// TSZ_INLINE_TEST_END f7b7ad48e8a1bc6f5a323a7ed887b30dc6b48f3966683e1c2a417f486aaf2ecb

// TSZ_INLINE_TEST_BEGIN cde37110e5eb22e9835025ea7900e93afd25987a184b8ca69b6b6a136fe7ad06 1125 exact_rewrite_complex_intersection_before_depth_bail_does_not_leak_flag
    #[test]
    fn exact_rewrite_complex_intersection_before_depth_bail_does_not_leak_flag() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let intersection = complex_replay_intersection(&db, outer);
        assert!(!db.is_union_too_complex());

        let mut deep = outer;
        for _ in 0..=crate::recursion::MAX_SOLVER_STACK_FRAMES {
            deep = db.array(deep);
        }
        let root = db.tuple(vec![
            TupleElement::fixed(intersection),
            TupleElement::fixed(deep),
        ]);

        assert_eq!(
            substitute_exact_type(&db, root, outer, TypeId::STRING),
            root,
        );
        assert!(
            !db.is_union_too_complex(),
            "discarded replay work must not leak a TS2590 signal",
        );
    }
// TSZ_INLINE_TEST_END cde37110e5eb22e9835025ea7900e93afd25987a184b8ca69b6b6a136fe7ad06

// TSZ_INLINE_TEST_BEGIN d5f09240da799e45c3ee7c72a2fe8062577a6d98d1641d57019f0aad456d99e6 1151 exact_rewrite_reaches_mapped_binder_and_surface_fields
    #[test]
    fn exact_rewrite_reaches_mapped_binder_and_surface_fields() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let iter_info = TypeParamInfo {
            name: db.intern_string("Key"),
            constraint: Some(outer),
            default: Some(db.array(outer)),
            is_const: true,
            origin: crate::types::TypeParamOrigin::User,
        };
        let mapped = db.mapped(MappedType {
            type_param: iter_info,
            constraint: outer,
            name_type: Some(db.readonly_type(outer)),
            template: db.array(outer),
            readonly_modifier: None,
            optional_modifier: None,
        });

        let result = substitute_exact_type(&db, mapped, outer, TypeId::STRING);
        let Some(TypeData::Mapped(mapped_id)) = db.lookup(result) else {
            panic!("expected mapped type");
        };
        let mapped = db.get_mapped(mapped_id);
        assert_eq!(mapped.type_param.constraint, Some(TypeId::STRING));
        assert_eq!(mapped.type_param.default, Some(db.array(TypeId::STRING)));
        assert!(mapped.type_param.is_const);
        assert_eq!(mapped.constraint, TypeId::STRING);
        assert_eq!(mapped.name_type, Some(db.readonly_type(TypeId::STRING)));
        assert_eq!(mapped.template, db.array(TypeId::STRING));
    }
// TSZ_INLINE_TEST_END d5f09240da799e45c3ee7c72a2fe8062577a6d98d1641d57019f0aad456d99e6

// TSZ_INLINE_TEST_BEGIN 1749a3cd4b0e17c14bc37f2266647a00e7410b689d11d3b13a309fffef421967 1184 exact_rewrite_reaches_function_callable_and_index_metadata
    #[test]
    fn exact_rewrite_reaches_function_callable_and_index_metadata() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let signature_param = TypeParamInfo {
            name: db.intern_string("Inner"),
            constraint: Some(outer),
            default: Some(db.array(outer)),
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        };
        let predicate = TypePredicate {
            asserts: false,
            target: crate::types::TypePredicateTarget::This,
            type_id: Some(outer),
            parameter_index: None,
        };
        let function = db.function(FunctionShape {
            type_params: vec![signature_param],
            params: vec![ParamInfo {
                type_id: outer,
                ..ParamInfo::default()
            }],
            this_type: Some(outer),
            return_type: db.array(outer),
            type_predicate: Some(predicate),
            is_constructor: false,
            is_method: true,
        });
        let call_signature = CallSignature {
            type_params: vec![signature_param],
            params: vec![ParamInfo {
                type_id: outer,
                ..ParamInfo::default()
            }],
            this_type: Some(outer),
            return_type: outer,
            type_predicate: Some(predicate),
            is_method: true,
            declaration_group: 0,
        };
        let callable = db.callable(CallableShape {
            call_signatures: vec![call_signature],
            construct_signatures: Vec::new(),
            properties: vec![PropertyInfo::new(db.intern_string("value"), outer)],
            string_index: Some(IndexSignature {
                key_type: outer,
                value_type: db.array(outer),
                readonly: true,
                param_name: None,
            }),
            number_index: None,
            symbol: None,
            is_abstract: false,
        });
        let root = db.tuple(vec![
            TupleElement::fixed(function),
            TupleElement::fixed(callable),
        ]);

        let result = substitute_exact_type(&db, root, outer, TypeId::NUMBER);
        let members = tuple_members(&db, result);
        let Some(TypeData::Function(function_id)) = db.lookup(members[0]) else {
            panic!("expected function");
        };
        let function = db.function_shape(function_id);
        assert_eq!(function.type_params[0].constraint, Some(TypeId::NUMBER));
        assert_eq!(function.params[0].type_id, TypeId::NUMBER);
        assert_eq!(function.this_type, Some(TypeId::NUMBER));
        assert_eq!(function.return_type, db.array(TypeId::NUMBER));
        assert_eq!(
            function
                .type_predicate
                .expect("rewritten function should retain its predicate")
                .type_id,
            Some(TypeId::NUMBER)
        );

        let Some(TypeData::Callable(callable_id)) = db.lookup(members[1]) else {
            panic!("expected callable");
        };
        let callable = db.callable_shape(callable_id);
        assert_eq!(
            callable.call_signatures[0].type_params[0].default,
            Some(db.array(TypeId::NUMBER)),
        );
        assert_eq!(callable.properties[0].type_id, TypeId::NUMBER);
        let index = callable
            .string_index
            .expect("rewritten callable should retain its string index");
        assert_eq!(index.key_type, TypeId::NUMBER);
        assert_eq!(index.value_type, db.array(TypeId::NUMBER));
    }
// TSZ_INLINE_TEST_END 1749a3cd4b0e17c14bc37f2266647a00e7410b689d11d3b13a309fffef421967

// TSZ_INLINE_TEST_BEGIN ce81df66a9d4444a10bcaf3eab316c0c35e5a443ae04bffca9f054f110a33309 1278 exact_rewrite_concretizes_outer_constraint_and_keeps_nested_local_identity
    #[test]
    fn exact_rewrite_concretizes_outer_constraint_and_keeps_nested_local_identity() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let key = fresh_param(&db, "Key");
        let local_info = TypeParamInfo {
            name: db.intern_string("Local"),
            constraint: Some(db.union(vec![outer, key])),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        };
        let local = db.fresh_type_param(local_info);
        let method = db.function(FunctionShape {
            type_params: vec![local_info],
            params: vec![ParamInfo::required(db.intern_string("value"), local)],
            this_type: None,
            return_type: local,
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        });
        let concrete_key = db.literal_string("table");

        let result =
            substitute_exact_types(&db, method, &[outer, key], &[TypeId::NUMBER, concrete_key]);
        let Some(TypeData::Function(shape_id)) = db.lookup(result) else {
            panic!("expected materialized method, got {:?}", db.lookup(result));
        };
        let shape = db.function_shape(shape_id);
        let rewritten_local = shape.params[0].type_id;
        assert_eq!(shape.return_type, rewritten_local);
        assert_eq!(shape.type_params.len(), 1);
        assert_eq!(
            db.lookup(rewritten_local),
            Some(TypeData::TypeParameter(shape.type_params[0])),
        );
        assert_eq!(
            shape.type_params[0].constraint,
            Some(db.union(vec![TypeId::NUMBER, concrete_key])),
        );
        let constraint_members = crate::visitor::collect_all_types(
            &db,
            shape.type_params[0]
                .constraint
                .expect("local constraint should remain present"),
        );
        assert!(!constraint_members.contains(&outer));
        assert!(!constraint_members.contains(&key));
    }
// TSZ_INLINE_TEST_END ce81df66a9d4444a10bcaf3eab316c0c35e5a443ae04bffca9f054f110a33309

// TSZ_INLINE_TEST_BEGIN b0f0d47ca1a920737fd63025e988a1676f1a560b8615fa8cdea6ae6a1945e49a 1329 exact_rewrite_reaches_parameter_infer_enum_and_substitution_fields
    #[test]
    fn exact_rewrite_reaches_parameter_infer_enum_and_substitution_fields() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let base = fresh_param(&db, "Base");
        let info = TypeParamInfo {
            name: db.intern_string("Nested"),
            constraint: Some(outer),
            default: Some(db.array(outer)),
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        };
        let nested_param = db.type_param(info);
        let infer = db.infer(info);
        let enum_type = db.enum_type(DefId(7), outer);
        let substitution = db.substitution(base, outer);
        assert!(matches!(
            db.lookup(substitution),
            Some(TypeData::Substitution { .. })
        ));
        let root = db.tuple(vec![
            TupleElement::fixed(nested_param),
            TupleElement::fixed(infer),
            TupleElement::fixed(enum_type),
            TupleElement::fixed(substitution),
        ]);

        let result = substitute_exact_type(&db, root, outer, TypeId::STRING);
        let members = tuple_members(&db, result);
        for member in &members[..2] {
            let info = match db.lookup(*member) {
                Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info,
                other => panic!("expected parameter metadata, got {other:?}"),
            };
            assert_eq!(info.constraint, Some(TypeId::STRING));
            assert_eq!(info.default, Some(db.array(TypeId::STRING)));
        }
        assert_eq!(
            db.lookup(members[2]),
            Some(TypeData::Enum(DefId(7), TypeId::STRING))
        );
        assert_eq!(
            db.lookup(members[3]),
            Some(TypeData::Substitution {
                base_type: base,
                constraint: TypeId::STRING,
            }),
        );
    }
// TSZ_INLINE_TEST_END b0f0d47ca1a920737fd63025e988a1676f1a560b8615fa8cdea6ae6a1945e49a

// TSZ_INLINE_TEST_BEGIN ef01f9a880e46bd4a9580860c75a5b22c8544b85f899e60e0931ebe24f96670a 1379 exact_rewrite_preserves_distinct_fresh_type_parameter_identities
    #[test]
    fn exact_rewrite_preserves_distinct_fresh_type_parameter_identities() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let nested_info = TypeParamInfo {
            name: db.intern_string("Nested"),
            constraint: Some(outer),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        };
        let first = db.fresh_type_param(nested_info);
        let second = db.fresh_type_param(nested_info);
        assert_ne!(first, second);

        let function = db.function(FunctionShape {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                type_id: first,
                ..ParamInfo::default()
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let callable = db.callable(CallableShape {
            call_signatures: vec![CallSignature {
                type_params: Vec::new(),
                params: vec![ParamInfo {
                    type_id: second,
                    ..ParamInfo::default()
                }],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
                declaration_group: 0,
            }],
            construct_signatures: Vec::new(),
            properties: Vec::new(),
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });
        let root = db.tuple(vec![
            TupleElement::fixed(function),
            TupleElement::fixed(callable),
        ]);

        let result = substitute_exact_type(&db, root, outer, TypeId::STRING);
        let members = tuple_members(&db, result);
        let Some(TypeData::Function(function_id)) = db.lookup(members[0]) else {
            panic!("expected function");
        };
        let rewritten_first = db.function_shape(function_id).params[0].type_id;
        let Some(TypeData::Callable(callable_id)) = db.lookup(members[1]) else {
            panic!("expected callable");
        };
        let rewritten_second = db.callable_shape(callable_id).call_signatures[0].params[0].type_id;

        assert_ne!(rewritten_first, rewritten_second);
        for rewritten in [rewritten_first, rewritten_second] {
            let Some(TypeData::TypeParameter(info)) = db.lookup(rewritten) else {
                panic!("expected fresh type parameter");
            };
            assert_eq!(info.constraint, Some(TypeId::STRING));
        }
    }
// TSZ_INLINE_TEST_END ef01f9a880e46bd4a9580860c75a5b22c8544b85f899e60e0931ebe24f96670a

// TSZ_INLINE_TEST_BEGIN 9f61c8d02db68b086fe3a47bf1a397617c4e79ce82717020df04046eaadd6a57 1451 exact_rewrite_preserves_rewritten_object_provenance
    #[test]
    fn exact_rewrite_preserves_rewritten_object_provenance() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        // Application display aliases are preferred only when they predate the
        // evaluated structural result, matching normal evaluator allocation.
        let application_origin = db.application(db.lazy(DefId(11)), vec![outer]);
        let left = db.object(vec![PropertyInfo::new(db.intern_string("left"), outer)]);
        let right = db.object(vec![PropertyInfo::new(
            db.intern_string("right"),
            TypeId::NUMBER,
        )]);
        let source = db.intersection(vec![left, right]);
        assert!(db.get_merged_intersection_origin(source).is_some());

        db.store_display_properties(
            source,
            vec![PropertyInfo::new(db.intern_string("shown"), outer)],
        );
        db.record_application_eval_origin(source, application_origin);
        db.store_display_alias_preferring_application(source, application_origin);

        let result = substitute_exact_type(&db, source, outer, TypeId::STRING);
        assert_ne!(result, source);
        assert_eq!(
            db.get_display_properties(result)
                .expect("rewritten object should retain display properties")[0]
                .type_id,
            TypeId::STRING,
        );
        assert!(db.get_merged_intersection_origin(result).is_some());

        let origin = db
            .get_application_eval_origin(result)
            .expect("rewritten object should retain its application origin");
        let Some(TypeData::Application(app_id)) = db.lookup(origin) else {
            panic!("expected application origin");
        };
        assert_eq!(db.type_application(app_id).args, vec![TypeId::STRING]);
        assert_eq!(db.get_display_alias(result), Some(origin));
    }
// TSZ_INLINE_TEST_END 9f61c8d02db68b086fe3a47bf1a397617c4e79ce82717020df04046eaadd6a57

// TSZ_INLINE_TEST_BEGIN 67233be142aa354baf443da00e5c0212ab87ec7b387aa74b700a049f97947aa5 1493 exact_rewrite_transfers_generic_display_alias_in_both_allocation_orders
    #[test]
    fn exact_rewrite_transfers_generic_display_alias_in_both_allocation_orders() {
        fn run(alias_before_result: bool) {
            let db = TypeInterner::new();
            let source_param = fresh_param(&db, "Source");
            let replacement = fresh_param(&db, "Replacement");
            let base = db.lazy(DefId(21));

            // Seed valid source provenance in the ordinary evaluator order:
            // the application exists before its evaluated structural result.
            let source_alias = db.application(base, vec![source_param]);
            let source = db.array(source_param);
            db.store_display_alias_preferring_application(source, source_alias);
            assert_eq!(db.get_display_alias(source), Some(source_alias));

            let expected_alias =
                alias_before_result.then(|| db.application(base, vec![replacement]));
            let expected_result = (!alias_before_result).then(|| db.array(replacement));

            let result = substitute_exact_type(&db, source, source_param, replacement);
            let expected_result = expected_result.unwrap_or_else(|| db.array(replacement));
            let expected_alias =
                expected_alias.unwrap_or_else(|| db.application(base, vec![replacement]));

            assert_eq!(result, expected_result);
            assert_eq!(db.get_display_alias(result), Some(expected_alias));
        }

        run(true);
        run(false);
    }
// TSZ_INLINE_TEST_END 67233be142aa354baf443da00e5c0212ab87ec7b387aa74b700a049f97947aa5

// TSZ_INLINE_TEST_BEGIN 4290d7f5fb641c0f5eb9e5b4fe8b66267b4bd80888c4ab8a78d2136418803e7c 1525 exact_rewrite_does_not_repaint_an_existing_application_alias
    #[test]
    fn exact_rewrite_does_not_repaint_an_existing_application_alias() {
        let db = TypeInterner::new();
        let source_param = fresh_param(&db, "Source");
        let replacement = fresh_param(&db, "Replacement");
        let source_base = db.lazy(DefId(24));
        let existing_base = db.lazy(DefId(25));

        let source_alias = db.application(source_base, vec![source_param]);
        let source = db.array(source_param);
        db.store_display_alias_preferring_application(source, source_alias);

        let existing_alias = db.application(existing_base, vec![replacement]);
        let expected = db.array(replacement);
        db.store_display_alias_preferring_application(expected, existing_alias);
        assert_eq!(db.get_display_alias(expected), Some(existing_alias));

        let result = substitute_exact_type(&db, source, source_param, replacement);

        assert_eq!(result, expected);
        assert_eq!(db.get_display_alias(result), Some(existing_alias));
    }
// TSZ_INLINE_TEST_END 4290d7f5fb641c0f5eb9e5b4fe8b66267b4bd80888c4ab8a78d2136418803e7c

// TSZ_INLINE_TEST_BEGIN ce322155b55d0d27736e125dd0f9419c85676e26dab7420ce17e83bee6c64f02 1548 rewritten_display_alias_transfer_retains_global_identity_and_cycle_guards
    #[test]
    fn rewritten_display_alias_transfer_retains_global_identity_and_cycle_guards() {
        let db = TypeInterner::new();
        let parameter = fresh_param(&db, "Parameter");
        let base = db.lazy(DefId(23));
        let safe_alias = db.application(base, vec![TypeId::STRING]);

        db.transfer_rewritten_application_display_alias(TypeId::STRING, safe_alias);
        db.transfer_rewritten_application_display_alias(parameter, safe_alias);
        assert_eq!(db.get_display_alias(TypeId::STRING), None);
        assert_eq!(db.get_display_alias(parameter), None);

        let evaluated = db.array(TypeId::STRING);
        let cyclic_alias = db.application(base, vec![evaluated]);
        db.transfer_rewritten_application_display_alias(evaluated, cyclic_alias);
        assert_eq!(db.get_display_alias(evaluated), None);
    }
// TSZ_INLINE_TEST_END ce322155b55d0d27736e125dd0f9419c85676e26dab7420ce17e83bee6c64f02

// TSZ_INLINE_TEST_BEGIN b4a83b6213766c0149692d2f2571346557445437ce6b98777684ccc27757200a 1566 rewritten_application_alias_replaces_structural_provenance
    #[test]
    fn rewritten_application_alias_replaces_structural_provenance() {
        let db = TypeInterner::new();
        let evaluated = db.array(TypeId::STRING);
        let structural_alias = db.object(vec![PropertyInfo::new(
            db.intern_string("value"),
            TypeId::STRING,
        )]);
        db.store_display_alias(evaluated, structural_alias);
        assert_eq!(db.get_display_alias(evaluated), Some(structural_alias));

        let application = db.application(db.lazy(DefId(26)), vec![TypeId::STRING]);
        db.transfer_rewritten_application_display_alias(evaluated, application);

        assert_eq!(db.get_display_alias(evaluated), Some(application));
    }
// TSZ_INLINE_TEST_END b4a83b6213766c0149692d2f2571346557445437ce6b98777684ccc27757200a

// TSZ_INLINE_TEST_BEGIN 92868e2d3869d0f235bf724b508e7af2db1acbaac61e53376ef99c4760a5557d 1583 exact_rewrite_depth_bail_returns_original_without_provenance_writes
    #[test]
    fn exact_rewrite_depth_bail_returns_original_without_provenance_writes() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let base = db.lazy(DefId(22));
        let source_alias = db.application(base, vec![outer]);
        let property = db.intern_string("value");
        let shallow = db.object(vec![PropertyInfo::new(property, outer)]);
        db.store_display_alias_preferring_application(shallow, source_alias);
        assert_eq!(db.get_display_alias(shallow), Some(source_alias));

        // This canonical node is the speculative shallow rewrite the first
        // tuple slot would produce before the second slot exceeds the shared
        // solver-frame budget. Its provenance must remain untouched on bail.
        let rewritten_shallow = db.object(vec![PropertyInfo::new(property, TypeId::STRING)]);
        assert_eq!(db.get_display_alias(rewritten_shallow), None);

        let mut deep = outer;
        for _ in 0..=crate::recursion::MAX_SOLVER_STACK_FRAMES {
            deep = db.array(deep);
        }
        let root = db.tuple(vec![
            TupleElement::fixed(shallow),
            TupleElement::fixed(deep),
        ]);

        assert_eq!(
            substitute_exact_type(&db, root, outer, TypeId::STRING),
            root,
        );
        assert_eq!(db.get_display_alias(rewritten_shallow), None);

        // The RAII frame budget and sticky bailout are request-scoped.
        let shallow_array = db.array(outer);
        assert_eq!(
            substitute_exact_type(&db, shallow_array, outer, TypeId::STRING),
            db.array(TypeId::STRING),
        );
    }
// TSZ_INLINE_TEST_END 92868e2d3869d0f235bf724b508e7af2db1acbaac61e53376ef99c4760a5557d

// TSZ_INLINE_TEST_BEGIN 5a99bdb1d76544a1550dc3ec84569d49834a88eed0ac96ef798b066b8001c439 1623 exact_rewrite_memo_refreshes_late_provenance_and_converges_generation
    #[test]
    fn exact_rewrite_memo_refreshes_late_provenance_and_converges_generation() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let other = fresh_param(&db, "Other");
        let third = fresh_param(&db, "Third");
        let alias_base = db.lazy(DefId(31));
        let source_application = db.application(alias_base, vec![outer]);
        let nested_source = db.array(outer);
        let source = db.union_preserve_members(vec![outer, other, third]);
        let replacement = fresh_param(&db, "Replacement");

        let (result, mut memo) =
            substitute_exact_types_with_memo(&db, source, &[outer], &[replacement])
                .expect("the initial exact rewrite should complete");
        let rewritten_nested = db.array(replacement);
        let rewritten_application = db.application(alias_base, vec![replacement]);
        assert!(db.get_display_properties(result).is_none());
        let synthesized_union_fallback = db
            .get_union_origin(result)
            .expect("changed union should retain its pre-sort rewritten members");
        assert!(db.get_application_eval_origin(result).is_none());
        assert!(db.get_display_alias(result).is_none());

        let shown = db.intern_string("shown");
        db.store_display_properties(
            source,
            vec![PropertyInfo {
                declaration_order: 1,
                ..PropertyInfo::new(shown, nested_source)
            }],
        );
        db.replace_union_origin_for_display(source, vec![third, other, outer]);
        db.record_application_eval_origin(source, source_application);
        db.store_display_alias_preferring_application(source, source_application);

        memo.refresh_provenance(&db)
            .expect("late provenance replay should complete");
        let properties = db
            .get_display_properties(result)
            .expect("late display properties should reach the rewritten root");
        assert_eq!(properties[0].type_id, rewritten_nested);
        assert_eq!(properties[0].declaration_order, 1);
        assert_ne!(
            synthesized_union_fallback.as_slice(),
            &[third, other, replacement],
            "the test must replace a distinct synthesized fallback",
        );
        assert_eq!(
            db.get_union_origin(result)
                .expect("late union origin should reach the rewritten root")
                .as_slice(),
            &[third, other, replacement],
        );
        assert_eq!(
            db.get_application_eval_origin(result),
            Some(rewritten_application),
        );
        assert_eq!(db.get_display_alias(result), Some(rewritten_application));

        // A replay can advance the universe generation with its own target
        // writes. One no-op scan converges; later hits take the `O(1)` gate.
        let replay_generation = db.display_provenance_generation();
        memo.refresh_provenance(&db)
            .expect("the convergence replay should complete");
        assert_eq!(db.display_provenance_generation(), replay_generation);
        assert_eq!(memo.provenance_generation, replay_generation);
        memo.refresh_provenance(&db)
            .expect("an unchanged generation should be an immediate hit");
        assert_eq!(db.display_provenance_generation(), replay_generation);

        // `PropertyInfo` structural equality intentionally ignores declaration
        // order. The provenance epoch must still notice this display-only edit.
        db.store_display_properties(
            source,
            vec![PropertyInfo {
                declaration_order: 9,
                ..PropertyInfo::new(shown, nested_source)
            }],
        );
        assert_ne!(db.display_provenance_generation(), replay_generation);
        memo.refresh_provenance(&db)
            .expect("display-only metadata changes must replay");
        assert_eq!(
            db.get_display_properties(result)
                .expect("rewritten display properties should be replaced")[0]
                .declaration_order,
            9,
        );
    }
// TSZ_INLINE_TEST_END 5a99bdb1d76544a1550dc3ec84569d49834a88eed0ac96ef798b066b8001c439

// TSZ_INLINE_TEST_BEGIN 01a9ea6a19e452bb680ecbbaaa667045a8478bd54c9f76d6b6618ee19f5b7feb 1714 exact_rewrite_union_fallback_never_repaints_unrelated_real_origin
    #[test]
    fn exact_rewrite_union_fallback_never_repaints_unrelated_real_origin() {
        let db = TypeInterner::new();
        let source_param = fresh_param(&db, "Source");
        let other = fresh_param(&db, "Other");
        let replacement = fresh_param(&db, "Replacement");
        let source = db.union_preserve_members(vec![source_param, other]);
        let expected = db.union_preserve_members(vec![replacement, other]);
        let unrelated_target_origin = vec![replacement, other];
        db.replace_union_origin_for_display(expected, unrelated_target_origin.clone());

        let (result, mut memo) =
            substitute_exact_types_with_memo(&db, source, &[source_param], &[replacement])
                .expect("the initial exact rewrite should complete");
        assert_eq!(result, expected);
        assert_eq!(
            db.get_union_origin(result).map(|origin| origin.to_vec()),
            Some(unrelated_target_origin.clone()),
            "a synthesized fallback must not replace a real target origin",
        );

        db.replace_union_origin_for_display(source, vec![other, source_param]);
        memo.refresh_provenance(&db)
            .expect("late real source provenance should replay");
        assert_eq!(
            db.get_union_origin(result).map(|origin| origin.to_vec()),
            Some(unrelated_target_origin),
            "late provenance from another rewrite session must remain sticky",
        );
    }
// TSZ_INLINE_TEST_END 01a9ea6a19e452bb680ecbbaaa667045a8478bd54c9f76d6b6618ee19f5b7feb

// TSZ_INLINE_TEST_BEGIN 00a9128b8860f2d28d32c10fb8ac1f219ffff2e769048250a03404ca73a609af 1745 exact_rewrite_late_canonical_union_origin_clears_fallback
    #[test]
    fn exact_rewrite_late_canonical_union_origin_clears_fallback() {
        let db = TypeInterner::new();
        let source_param = fresh_param(&db, "Source");
        let other = fresh_param(&db, "Other");
        let replacement = fresh_param(&db, "Replacement");
        let source = db.union_preserve_members(vec![source_param, other]);
        let (result, mut memo) =
            substitute_exact_types_with_memo(&db, source, &[source_param], &[replacement])
                .expect("the initial exact rewrite should complete");
        assert!(db.get_union_origin(result).is_some());

        let Some(TypeData::Union(result_list)) = db.lookup(result) else {
            panic!("expected rewritten union");
        };
        let canonical_source_origin = db
            .type_list(result_list)
            .iter()
            .map(|&member| {
                if member == replacement {
                    source_param
                } else {
                    member
                }
            })
            .collect();
        db.replace_union_origin_for_display(source, canonical_source_origin);
        memo.refresh_provenance(&db)
            .expect("late canonical provenance should replay");
        assert_eq!(
            db.get_union_origin(result),
            None,
            "a canonical real origin should clear the stale tagged fallback",
        );
    }
// TSZ_INLINE_TEST_END 00a9128b8860f2d28d32c10fb8ac1f219ffff2e769048250a03404ca73a609af

// TSZ_INLINE_TEST_BEGIN a061feee2cf6f723cbc5929c174a9d18b1959673632f3fd379f25a2f60c5cb79 1781 exact_rewrite_memo_reuses_nested_fresh_binders_across_roots
    #[test]
    fn exact_rewrite_memo_reuses_nested_fresh_binders_across_roots() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let nested = db.fresh_type_param(TypeParamInfo {
            name: db.intern_string("Nested"),
            constraint: Some(outer),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let first_root = db.tuple(vec![
            TupleElement::fixed(nested),
            TupleElement::fixed(db.array(nested)),
        ]);

        let (first_result, mut memo) =
            substitute_exact_types_with_memo(&db, first_root, &[outer], &[TypeId::STRING])
                .expect("the initial exact rewrite should complete");
        let rewritten_nested = tuple_members(&db, first_result)[0];
        assert_ne!(rewritten_nested, nested);
        let Some(TypeData::TypeParameter(info)) = db.lookup(rewritten_nested) else {
            panic!("expected a rewritten fresh type parameter");
        };
        assert_eq!(info.constraint, Some(TypeId::STRING));

        db.store_display_properties(
            first_root,
            vec![PropertyInfo::new(db.intern_string("shown"), nested)],
        );
        memo.refresh_provenance(&db)
            .expect("late provenance replay should complete");
        assert_eq!(
            db.get_display_properties(first_result)
                .expect("rewritten root should receive late display properties")[0]
                .type_id,
            rewritten_nested,
        );
        memo.refresh_provenance(&db)
            .expect("the generation should converge after target writes");

        let second_root = db.array(nested);
        let second_result = memo
            .rewrite_root(&db, second_root)
            .expect("a second root should reuse the completed session");
        assert_eq!(second_result, db.array(rewritten_nested));
        assert_eq!(
            memo.rewrite_root(&db, second_root)
                .expect("a completed root should be reusable"),
            second_result,
        );
    }
// TSZ_INLINE_TEST_END a061feee2cf6f723cbc5929c174a9d18b1959673632f3fd379f25a2f60c5cb79

// TSZ_INLINE_TEST_BEGIN e82b0f93432240ae42f751e2ee1c1853e78276468905bcd75d7ba916354f9c1f 1834 exact_rewrite_direct_binder_pairs_do_not_repaint_replacements
    #[test]
    fn exact_rewrite_direct_binder_pairs_do_not_repaint_replacements() {
        let db = TypeInterner::new();
        let source_binder = fresh_param(&db, "Source");
        let active_binder = fresh_param(&db, "Active");
        let shown = db.intern_string("shown");
        db.store_display_properties(
            source_binder,
            vec![PropertyInfo::new(shown, TypeId::STRING)],
        );

        let root = db.array(source_binder);
        let (result, mut memo) =
            substitute_exact_types_with_memo(&db, root, &[source_binder], &[active_binder])
                .expect("the binder rewrite should complete");
        assert_eq!(result, db.array(active_binder));
        assert!(db.get_display_properties(active_binder).is_none());

        db.store_display_properties(
            source_binder,
            vec![PropertyInfo::new(shown, TypeId::NUMBER)],
        );
        memo.refresh_provenance(&db)
            .expect("late structural provenance should refresh");
        assert!(
            db.get_display_properties(active_binder).is_none(),
            "a terminal direct pair must not repaint the destination binder",
        );
    }
// TSZ_INLINE_TEST_END e82b0f93432240ae42f751e2ee1c1853e78276468905bcd75d7ba916354f9c1f

// TSZ_INLINE_TEST_BEGIN b33917e735d7e9a31f74c1369fa9128deb497b7682bf596e9b0ec4e0d5df5185 1864 exact_rewrite_abort_is_retryable_and_refresh_is_transactional
    #[test]
    fn exact_rewrite_abort_is_retryable_and_refresh_is_transactional() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let shown = db.intern_string("shown");
        let source = db.array(db.array(outer));
        let expected = db.array(db.array(TypeId::STRING));
        db.store_display_properties(source, vec![PropertyInfo::new(shown, outer)]);

        let held_frames: Vec<_> = (0..crate::recursion::MAX_SOLVER_STACK_FRAMES - 1)
            .map(|_| {
                crate::recursion::try_enter_solver_frame()
                    .expect("test should reserve all but one solver frame")
            })
            .collect();
        assert!(matches!(
            substitute_exact_types_with_memo(&db, source, &[outer], &[TypeId::STRING]),
            Err(ExactRewriteAborted),
        ));
        assert!(db.get_display_properties(expected).is_none());
        drop(held_frames);

        let (result, mut memo) =
            substitute_exact_types_with_memo(&db, source, &[outer], &[TypeId::STRING])
                .expect("the same rewrite should retry under a fresh frame budget");
        assert_eq!(result, expected);
        assert_eq!(
            db.get_display_properties(result)
                .expect("the completed retry should commit provenance")[0]
                .type_id,
            TypeId::STRING,
        );

        let late_source = db.readonly_type(db.tuple(vec![TupleElement::fixed(outer)]));
        let late_result = db.readonly_type(db.tuple(vec![TupleElement::fixed(TypeId::STRING)]));
        db.store_display_properties(source, vec![PropertyInfo::new(shown, late_source)]);
        let mapped_before = memo.mapped.clone();
        let sources_before = memo.provenance_sources.clone();
        let roots_before = memo.root_results.clone();
        let generation_before = memo.provenance_generation;
        let held_frames: Vec<_> = (0..crate::recursion::MAX_SOLVER_STACK_FRAMES - 1)
            .map(|_| {
                crate::recursion::try_enter_solver_frame()
                    .expect("test should reserve all but one solver frame")
            })
            .collect();
        assert_eq!(memo.refresh_provenance(&db), Err(ExactRewriteAborted));
        assert_eq!(memo.mapped, mapped_before);
        assert_eq!(memo.provenance_sources, sources_before);
        assert_eq!(memo.root_results, roots_before);
        assert_eq!(memo.provenance_generation, generation_before);
        assert_eq!(
            db.get_display_properties(result)
                .expect("failed refresh must preserve prior target provenance")[0]
                .type_id,
            TypeId::STRING,
        );
        drop(held_frames);

        memo.refresh_provenance(&db)
            .expect("the provenance refresh should retry after frames unwind");
        assert_eq!(
            db.get_display_properties(result)
                .expect("successful retry should commit late provenance")[0]
                .type_id,
            late_result,
        );
    }
// TSZ_INLINE_TEST_END b33917e735d7e9a31f74c1369fa9128deb497b7682bf596e9b0ec4e0d5df5185
