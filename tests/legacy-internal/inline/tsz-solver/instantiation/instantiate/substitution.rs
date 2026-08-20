//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/instantiation/instantiate/substitution.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN fedcfa554140e23e4777d1ee90f5d23ebff1636f5af7a0ef3829fac565b03c04 484 exact_domain_substitutes_only_the_owned_same_surface_binder
    #[test]
    fn exact_domain_substitutes_only_the_owned_same_surface_binder() {
        let interner = TypeInterner::new();
        let name = interner.intern_string("U");
        let file = interner.intern_string("identity.ts");
        let local_info = TypeParamInfo {
            name,
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 1 },
        };
        let foreign_info = TypeParamInfo {
            origin: TypeParamOrigin::DeclScoped { file, node: 2 },
            ..local_info
        };
        let local = interner.fresh_type_param(local_info);
        let foreign = interner.fresh_type_param(foreign_info);
        assert_ne!(local, foreign);
        let root = interner.tuple(vec![
            TupleElement::fixed(local),
            TupleElement::fixed(foreign),
        ]);

        let mut substitution = TypeSubstitution::new();
        substitution.insert(name, TypeId::NUMBER);
        substitution.protect_type_parameters(&[local_info]);
        let result = instantiate_type(&interner, root, &substitution);

        assert_eq!(
            tuple_members(&interner, result),
            vec![TypeId::NUMBER, foreign]
        );

        // The name/value cache component is identical, but changing the exact
        // owner must produce a distinct result rather than hitting a name-only
        // project-cache entry.
        let mut other_owner = TypeSubstitution::new();
        other_owner.insert(name, TypeId::NUMBER);
        other_owner.protect_type_parameters(&[foreign_info]);
        let other_result = instantiate_type(&interner, root, &other_owner);
        assert_eq!(
            tuple_members(&interner, other_result),
            vec![local, TypeId::NUMBER],
        );
    }
// TSZ_INLINE_TEST_END fedcfa554140e23e4777d1ee90f5d23ebff1636f5af7a0ef3829fac565b03c04

// TSZ_INLINE_TEST_BEGIN d06c68095e0d4bc72c3075b9cfa97a0995e506bd3e6afbee258fc30cfac33154 531 jsdoc_exact_domain_uses_comment_position_and_origin_kind
    #[test]
    fn jsdoc_exact_domain_uses_comment_position_and_origin_kind() {
        let interner = TypeInterner::new();
        let name = interner.intern_string("Value");
        let file = interner.intern_string("identity.js");
        let owned_info = TypeParamInfo {
            name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::JsdocCommentScoped { file, pos: 10 },
        };
        let reconstructed_info = TypeParamInfo {
            constraint: Some(TypeId::STRING),
            ..owned_info
        };
        let foreign_jsdoc_info = TypeParamInfo {
            origin: TypeParamOrigin::JsdocCommentScoped { file, pos: 20 },
            ..owned_info
        };
        let ast_info = TypeParamInfo {
            origin: TypeParamOrigin::DeclScoped { file, node: 10 },
            ..owned_info
        };
        let legacy_info = TypeParamInfo::simple(name);

        assert!(owned_info.is_same_binder(reconstructed_info));
        assert!(!owned_info.is_same_binder(foreign_jsdoc_info));
        assert!(!owned_info.is_same_binder(ast_info));
        assert!(legacy_info.is_same_binder(TypeParamInfo {
            constraint: Some(TypeId::NUMBER),
            ..legacy_info
        }));

        let reconstructed = interner.fresh_type_param(reconstructed_info);
        let foreign_jsdoc = interner.fresh_type_param(foreign_jsdoc_info);
        let ast = interner.fresh_type_param(ast_info);
        let root = interner.tuple(vec![
            TupleElement::fixed(reconstructed),
            TupleElement::fixed(foreign_jsdoc),
            TupleElement::fixed(ast),
        ]);
        let substitution =
            TypeSubstitution::from_signature_args(&interner, &[owned_info], &[TypeId::NUMBER]);

        assert_eq!(
            tuple_members(&interner, instantiate_type(&interner, root, &substitution)),
            vec![TypeId::NUMBER, foreign_jsdoc, ast],
        );
    }
// TSZ_INLINE_TEST_END d06c68095e0d4bc72c3075b9cfa97a0995e506bd3e6afbee258fc30cfac33154

// TSZ_INLINE_TEST_BEGIN 6e386c256b491ff6b6df84640b906224d683c5764f559a840d6ab82cca0e9d92 582 exact_domain_distinguishes_sibling_binders_at_one_jsdoc_site
    #[test]
    fn exact_domain_distinguishes_sibling_binders_at_one_jsdoc_site() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("siblings.js");
        let t_name = interner.intern_string("T");
        let u_name = interner.intern_string("U");

        for origin in [
            TypeParamOrigin::DeclScoped { file, node: 10 },
            TypeParamOrigin::JsdocOwnerScoped { file, node: 10 },
            TypeParamOrigin::JsdocCommentScoped { file, pos: 20 },
        ] {
            let t_info = TypeParamInfo {
                origin,
                ..TypeParamInfo::simple(t_name)
            };
            let u_info = TypeParamInfo {
                origin,
                ..TypeParamInfo::simple(u_name)
            };
            assert!(!t_info.is_same_binder(u_info));

            let reconstructed_t = interner.fresh_type_param(TypeParamInfo {
                constraint: Some(TypeId::OBJECT),
                ..t_info
            });
            let reconstructed_u = interner.fresh_type_param(TypeParamInfo {
                default: Some(TypeId::UNKNOWN),
                ..u_info
            });
            let tuple = interner.tuple(vec![
                TupleElement::fixed(reconstructed_t),
                TupleElement::fixed(reconstructed_u),
            ]);
            let substitution = TypeSubstitution::from_signature_args(
                &interner,
                &[t_info, u_info],
                &[TypeId::NUMBER, TypeId::STRING],
            );

            assert_eq!(
                tuple_members(&interner, instantiate_type(&interner, tuple, &substitution)),
                vec![TypeId::NUMBER, TypeId::STRING],
            );
        }
    }
// TSZ_INLINE_TEST_END 6e386c256b491ff6b6df84640b906224d683c5764f559a840d6ab82cca0e9d92

// TSZ_INLINE_TEST_BEGIN 20fa416b78bd6a7d08edc965606bc71a996aa96385db1969caac60dcee8c6608 629 exact_domain_scratch_shares_the_out_of_line_domain
    #[test]
    fn exact_domain_scratch_shares_the_out_of_line_domain() {
        let interner = TypeInterner::new();
        let info = TypeParamInfo {
            name: interner.intern_string("U"),
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped {
                file: interner.intern_string("scratch-domain.ts"),
                node: 1,
            },
        };
        let substitution = TypeSubstitution::for_signature_domain(&[info]);
        let scratch = substitution.empty_with_same_domain();

        assert!(std::sync::Arc::ptr_eq(
            substitution
                .identity_domain
                .as_ref()
                .expect("scoped signature must have an exact domain"),
            scratch
                .identity_domain
                .as_ref()
                .expect("scratch substitution must preserve the exact domain"),
        ));
        assert!(
            TypeSubstitution::for_signature_domain(&[TypeParamInfo::simple(info.name)])
                .identity_domain
                .is_none(),
            "the common unstamped path must not allocate an exact domain",
        );
    }
// TSZ_INLINE_TEST_END 20fa416b78bd6a7d08edc965606bc71a996aa96385db1969caac60dcee8c6608

// TSZ_INLINE_TEST_BEGIN 259325a7397a802266dc9ec96632ca7d182cc07715de54faeffd2cd275f03607 663 exact_domain_foreign_binder_skips_constraint_fallback
    #[test]
    fn exact_domain_foreign_binder_skips_constraint_fallback() {
        let interner = TypeInterner::new();
        let u = interner.intern_string("U");
        let v = interner.intern_string("V");
        let file = interner.intern_string("identity.ts");
        let dependency = interner.fresh_type_param(TypeParamInfo {
            name: v,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 3 },
        });
        let local_info = TypeParamInfo {
            name: u,
            constraint: Some(dependency),
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 1 },
        };
        let foreign = interner.fresh_type_param(TypeParamInfo {
            name: u,
            constraint: Some(dependency),
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 2 },
        });

        let mut substitution = TypeSubstitution::new();
        substitution.insert(u, TypeId::STRING);
        substitution.insert(v, TypeId::NUMBER);
        substitution.protect_type_parameters(&[local_info]);

        assert_eq!(instantiate_type(&interner, foreign, &substitution), foreign);
    }
// TSZ_INLINE_TEST_END 259325a7397a802266dc9ec96632ca7d182cc07715de54faeffd2cd275f03607

// TSZ_INLINE_TEST_BEGIN 08c02bba9bc097196fb1fb60f935c9be30174e40c7275843121324a720d85509 699 legacy_unstamped_substitution_remains_name_keyed
    #[test]
    fn legacy_unstamped_substitution_remains_name_keyed() {
        let interner = TypeInterner::new();
        let name = interner.intern_string("T");
        let info = param(name, None);
        let first = interner.fresh_type_param(info);
        let second = interner.fresh_type_param(info);
        let root = interner.tuple(vec![
            TupleElement::fixed(first),
            TupleElement::fixed(second),
        ]);

        let substitution = TypeSubstitution::single(name, TypeId::BOOLEAN);
        let result = instantiate_type(&interner, root, &substitution);

        assert_eq!(
            tuple_members(&interner, result),
            vec![TypeId::BOOLEAN, TypeId::BOOLEAN],
        );
    }
// TSZ_INLINE_TEST_END 08c02bba9bc097196fb1fb60f935c9be30174e40c7275843121324a720d85509

// TSZ_INLINE_TEST_BEGIN d1013e4334838201f3a7528ae904802ef9e6beed01b4e5078e23dd1ef85df4ec 720 exact_domain_does_not_affect_a_renamed_foreign_binder
    #[test]
    fn exact_domain_does_not_affect_a_renamed_foreign_binder() {
        let interner = TypeInterner::new();
        let local_name = interner.intern_string("Local");
        let foreign_name = interner.intern_string("Foreign");
        let file = interner.intern_string("identity.ts");
        let local_info = TypeParamInfo {
            name: local_name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 1 },
        };
        let local = interner.fresh_type_param(local_info);
        let foreign = interner.fresh_type_param(TypeParamInfo {
            name: foreign_name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 2 },
        });
        let root = interner.tuple(vec![
            TupleElement::fixed(local),
            TupleElement::fixed(foreign),
        ]);

        let mut substitution = TypeSubstitution::new();
        substitution.insert(local_name, TypeId::STRING);
        substitution.protect_type_parameters(&[local_info]);

        assert_eq!(
            tuple_members(&interner, instantiate_type(&interner, root, &substitution)),
            vec![TypeId::STRING, foreign],
        );
    }
// TSZ_INLINE_TEST_END d1013e4334838201f3a7528ae904802ef9e6beed01b4e5078e23dd1ef85df4ec

// TSZ_INLINE_TEST_BEGIN fb4d1ca33742819614e223e7fd9398b8d6012bce8832bb5438212816a2cd69f1 756 signature_exact_domain_descends_into_nested_generic_return
    #[test]
    fn signature_exact_domain_descends_into_nested_generic_return() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("nested.ts");
        let u = interner.intern_string("U");
        let v = interner.intern_string("V");
        let owned_info = TypeParamInfo {
            name: u,
            constraint: Some(TypeId::STRING),
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 1 },
        };
        let foreign = interner.fresh_type_param(TypeParamInfo {
            origin: TypeParamOrigin::DeclScoped { file, node: 2 },
            ..owned_info
        });
        // A reconstructed occurrence of the owned declaration deliberately has
        // a distinct `TypeId`; its declaration origin is the stable identity.
        let reconstructed_owned = interner.fresh_type_param(owned_info);
        let nested_param = TypeParamInfo {
            name: v,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 3 },
        };
        let nested = interner.function(FunctionShape {
            type_params: vec![nested_param],
            params: Vec::new(),
            this_type: None,
            return_type: interner.tuple(vec![
                TupleElement::fixed(foreign),
                TupleElement::fixed(reconstructed_owned),
            ]),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        let substitution =
            TypeSubstitution::from_signature_args(&interner, &[owned_info], &[TypeId::NUMBER]);
        let result = instantiate_type(&interner, nested, &substitution);
        let Some(TypeData::Function(shape_id)) = interner.lookup(result) else {
            panic!(
                "expected nested function, got {:?}",
                interner.lookup(result)
            );
        };
        let shape = interner.function_shape(shape_id);

        assert_eq!(shape.type_params, vec![nested_param]);
        assert_eq!(
            tuple_members(&interner, shape.return_type),
            vec![foreign, TypeId::NUMBER],
        );
    }
// TSZ_INLINE_TEST_END fb4d1ca33742819614e223e7fd9398b8d6012bce8832bb5438212816a2cd69f1

// TSZ_INLINE_TEST_BEGIN 864bb780bd2f847e348cd0d17c7b6f60f84138b8d8d43bb112b1606101439a4a 814 nested_same_named_binder_shadows_only_its_own_identity
    #[test]
    fn nested_same_named_binder_shadows_only_its_own_identity() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("nested-shadow.ts");
        let name = interner.intern_string("U");
        let owned_info = TypeParamInfo {
            name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 1 },
        };
        let nested_info = TypeParamInfo {
            origin: TypeParamOrigin::DeclScoped { file, node: 2 },
            ..owned_info
        };
        let captured_outer = interner.fresh_type_param(owned_info);
        let nested_local = interner.fresh_type_param(nested_info);
        let nested = interner.function(FunctionShape {
            type_params: vec![nested_info],
            params: vec![crate::types::ParamInfo::unnamed(nested_local)],
            this_type: None,
            return_type: interner.tuple(vec![
                TupleElement::fixed(captured_outer),
                TupleElement::fixed(nested_local),
            ]),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        let substitution =
            TypeSubstitution::from_signature_args(&interner, &[owned_info], &[TypeId::NUMBER]);
        let result = instantiate_type(&interner, nested, &substitution);
        let Some(TypeData::Function(shape_id)) = interner.lookup(result) else {
            panic!(
                "expected nested function, got {:?}",
                interner.lookup(result)
            );
        };
        let shape = interner.function_shape(shape_id);

        assert_eq!(shape.type_params, vec![nested_info]);
        assert_eq!(shape.params[0].type_id, nested_local);
        assert_eq!(
            tuple_members(&interner, shape.return_type),
            vec![TypeId::NUMBER, nested_local],
        );
    }
// TSZ_INLINE_TEST_END 864bb780bd2f847e348cd0d17c7b6f60f84138b8d8d43bb112b1606101439a4a

// TSZ_INLINE_TEST_BEGIN 2a5bbd08dc72118c2dad3341f2208cfbf7ce5781ea260717b0841076dff14c07 864 rewritten_nested_local_lookup_is_declaration_aware
    #[test]
    fn rewritten_nested_local_lookup_is_declaration_aware() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("nested-local.ts");
        let u = interner.intern_string("U");
        let v = interner.intern_string("V");
        let owned_u = TypeParamInfo {
            name: u,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 1 },
        };
        let owned_v = TypeParamInfo {
            name: v,
            origin: TypeParamOrigin::DeclScoped { file, node: 2 },
            ..owned_u
        };
        let owned_v_occurrence = interner.fresh_type_param(owned_v);
        let nested_u = TypeParamInfo {
            name: u,
            constraint: Some(owned_v_occurrence),
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 3 },
        };
        let captured_u = interner.fresh_type_param(owned_u);
        let nested_u_occurrence = interner.fresh_type_param(nested_u);
        let nested = interner.function(FunctionShape {
            type_params: vec![nested_u],
            params: vec![crate::types::ParamInfo::unnamed(nested_u_occurrence)],
            this_type: None,
            return_type: interner.tuple(vec![
                TupleElement::fixed(captured_u),
                TupleElement::fixed(nested_u_occurrence),
            ]),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        let substitution = TypeSubstitution::from_signature_args(
            &interner,
            &[owned_u, owned_v],
            &[TypeId::NUMBER, TypeId::STRING],
        );
        let result = instantiate_type(&interner, nested, &substitution);
        let Some(TypeData::Function(shape_id)) = interner.lookup(result) else {
            panic!(
                "expected nested function, got {:?}",
                interner.lookup(result)
            );
        };
        let shape = interner.function_shape(shape_id);
        let rewritten_local = shape.type_params[0];

        assert_eq!(rewritten_local.origin, nested_u.origin);
        assert_eq!(rewritten_local.constraint, Some(TypeId::STRING));
        assert_eq!(
            shape.params[0].type_id,
            interner.type_param(rewritten_local)
        );
        assert_eq!(
            tuple_members(&interner, shape.return_type),
            vec![TypeId::NUMBER, interner.type_param(rewritten_local)],
        );
    }
// TSZ_INLINE_TEST_END 2a5bbd08dc72118c2dad3341f2208cfbf7ce5781ea260717b0841076dff14c07

// TSZ_INLINE_TEST_BEGIN d4fb25112ed9a70a8199e25df47d34b514b11855736fc25df19793483e378e6c 932 signature_default_can_capture_foreign_same_named_binder
    #[test]
    fn signature_default_can_capture_foreign_same_named_binder() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("default-capture.ts");
        let name = interner.intern_string("U");
        let foreign_info = TypeParamInfo {
            name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 1 },
        };
        let foreign = interner.fresh_type_param(foreign_info);
        let owned_info = TypeParamInfo {
            default: Some(foreign),
            origin: TypeParamOrigin::DeclScoped { file, node: 2 },
            ..foreign_info
        };

        let substitution = TypeSubstitution::from_signature_args(&interner, &[owned_info], &[]);

        assert_eq!(substitution.get(name), Some(foreign));
    }
// TSZ_INLINE_TEST_END d4fb25112ed9a70a8199e25df47d34b514b11855736fc25df19793483e378e6c

// TSZ_INLINE_TEST_BEGIN 952cafb40280eafc5b485913dc455018533fb05b08dcbfd6cecd82abf5be866c 959 from_args_all_supplied_maps_directly
    /// When every type parameter has a corresponding argument, `from_args`
    /// must map each name to the supplied argument and never enter the
    /// default-resolution phase.
    #[test]
    fn from_args_all_supplied_maps_directly() {
        let interner = TypeInterner::new();
        let t = interner.intern_string("T");
        let u = interner.intern_string("U");
        let params = vec![param(t, None), param(u, None)];

        let subst =
            TypeSubstitution::from_args(&interner, &params, &[TypeId::NUMBER, TypeId::STRING]);

        assert_eq!(subst.get(t), Some(TypeId::NUMBER));
        assert_eq!(subst.get(u), Some(TypeId::STRING));
        assert_eq!(subst.len(), 2);
    }
// TSZ_INLINE_TEST_END 952cafb40280eafc5b485913dc455018533fb05b08dcbfd6cecd82abf5be866c

// TSZ_INLINE_TEST_BEGIN d96b637bcacb6322eeeeab416fbfdf4a3d6954f61c17e66f4084144e2521911c 981 from_args_error_sentinel_arg_falls_back_to_any
    /// A supplied argument that is the `TypeId::ERROR` cycle/fuel sentinel must
    /// never be baked into the substitution as `error` (the cross-arena
    /// base-class poison cycle #13044/#13484), nor left free (which leaks the
    /// raw parameter into a contextual signature and degrades checking of the
    /// remaining arguments, regressing `thislessFunctionsNotContextSensitive2`).
    /// It is treated exactly like an unsupplied argument: bound to `any`, the
    /// no-candidate fallback. Real arguments in other positions are unaffected.
    #[test]
    fn from_args_error_sentinel_arg_falls_back_to_any() {
        let interner = TypeInterner::new();
        let t = interner.intern_string("T");
        let u = interner.intern_string("U");
        let params = vec![param(t, None), param(u, None)];

        let subst =
            TypeSubstitution::from_args(&interner, &params, &[TypeId::ERROR, TypeId::STRING]);

        // The ERROR-sentinel position resolves to `any`, never `error`.
        assert_eq!(subst.get(t), Some(TypeId::ANY));
        assert_ne!(subst.get(t), Some(TypeId::ERROR));
        // A genuine argument in another position is bound normally.
        assert_eq!(subst.get(u), Some(TypeId::STRING));
    }
// TSZ_INLINE_TEST_END d96b637bcacb6322eeeeab416fbfdf4a3d6954f61c17e66f4084144e2521911c

// TSZ_INLINE_TEST_BEGIN ff169c4d84441c8d8f8265629174a13b51c531dd52dd48d2be65514d51c04511 1001 from_args_unsupplied_without_default_is_removed
    /// A parameter with neither an argument nor a default must be left
    /// unsubstituted: the `any` placeholder seeded in phase 2 is removed in
    /// phase 3 so the body keeps the raw parameter.
    #[test]
    fn from_args_unsupplied_without_default_is_removed() {
        let interner = TypeInterner::new();
        let t = interner.intern_string("T");
        let params = vec![param(t, None)];

        let subst = TypeSubstitution::from_args(&interner, &params, &[]);

        assert_eq!(subst.get(t), None);
        assert!(subst.is_empty());
    }
// TSZ_INLINE_TEST_END ff169c4d84441c8d8f8265629174a13b51c531dd52dd48d2be65514d51c04511

// TSZ_INLINE_TEST_BEGIN 4cd41f9f36abc887e3a271a56020058316b5fbf24f478b5140d66f8a849b19cc 1016 from_args_default_references_earlier_supplied_param
    /// A default that references an earlier parameter must be instantiated
    /// against the argument supplied for that earlier parameter. This is the
    /// case the in-place (clone-free) accumulation must preserve.
    #[test]
    fn from_args_default_references_earlier_supplied_param() {
        let interner = TypeInterner::new();
        let t = interner.intern_string("T");
        let u = interner.intern_string("U");
        // U defaults to the type parameter `T`.
        let t_param_ty = interner.type_param(param(t, None));
        let params = vec![param(t, None), param(u, Some(t_param_ty))];

        let subst = TypeSubstitution::from_args(&interner, &params, &[TypeId::NUMBER]);

        assert_eq!(subst.get(t), Some(TypeId::NUMBER));
        // U's default `T` resolves through the substitution built so far.
        assert_eq!(subst.get(u), Some(TypeId::NUMBER));
    }
// TSZ_INLINE_TEST_END 4cd41f9f36abc887e3a271a56020058316b5fbf24f478b5140d66f8a849b19cc

// TSZ_INLINE_TEST_BEGIN fea4ecf4fcde27b9ea8db52c215eb65858d153b142c5898563e97b8ea774e44c 1036 from_args_default_chain_propagates_through_in_place_map
    /// A chain of defaults (`U = T`, `V = U`) must propagate the supplied
    /// argument all the way down. This exercises the in-place accumulation
    /// across multiple iterations: each default observes the resolved value of
    /// the previous one, exactly as the prior per-iteration map clone did.
    #[test]
    fn from_args_default_chain_propagates_through_in_place_map() {
        let interner = TypeInterner::new();
        let t = interner.intern_string("T");
        let u = interner.intern_string("U");
        let v = interner.intern_string("V");
        let t_param_ty = interner.type_param(param(t, None));
        let u_param_ty = interner.type_param(param(u, None));
        let params = vec![
            param(t, None),
            param(u, Some(t_param_ty)),
            param(v, Some(u_param_ty)),
        ];

        let subst = TypeSubstitution::from_args(&interner, &params, &[TypeId::BOOLEAN]);

        assert_eq!(subst.get(t), Some(TypeId::BOOLEAN));
        assert_eq!(subst.get(u), Some(TypeId::BOOLEAN));
        assert_eq!(subst.get(v), Some(TypeId::BOOLEAN));
    }
// TSZ_INLINE_TEST_END fea4ecf4fcde27b9ea8db52c215eb65858d153b142c5898563e97b8ea774e44c

// TSZ_INLINE_TEST_BEGIN 2a9ad437917f7d6d663eaafa77cbd4675846cfa5e35a7791ebaf778fb5ba820e 1061 from_args_self_referential_default_falls_back_to_any
    /// A self-referential default (`X = X`) resolves to `any`: phase 2 seeds
    /// `X -> any`, and instantiating the default against that map substitutes
    /// the self-reference away, matching tsc's any-fallback for circular
    /// defaults.
    #[test]
    fn from_args_self_referential_default_falls_back_to_any() {
        let interner = TypeInterner::new();
        let x = interner.intern_string("X");
        let x_param_ty = interner.type_param(param(x, None));
        let params = vec![param(x, Some(x_param_ty))];

        let subst = TypeSubstitution::from_args(&interner, &params, &[]);

        assert_eq!(subst.get(x), Some(TypeId::ANY));
    }
// TSZ_INLINE_TEST_END 2a9ad437917f7d6d663eaafa77cbd4675846cfa5e35a7791ebaf778fb5ba820e

// TSZ_INLINE_TEST_BEGIN 04b2cb1a1e2579e4a83d6b14425ba7c91b5ca506954cecc0fc8e60393684196d 1077 from_args_forward_reference_default_is_any_like
    /// A forward reference (`U = V` where `V` is a later, unsupplied parameter)
    /// must resolve to an any-like type rather than leaking an unresolved
    /// placeholder, because phase 2 pre-seeds every unsupplied parameter with
    /// `any` before defaults are processed in declaration order.
    #[test]
    fn from_args_forward_reference_default_is_any_like() {
        let interner = TypeInterner::new();
        let u = interner.intern_string("U");
        let v = interner.intern_string("V");
        let v_param_ty = interner.type_param(param(v, None));
        // U defaults to the *later* parameter V; V has no default/arg.
        let params = vec![param(u, Some(v_param_ty)), param(v, None)];

        let subst = TypeSubstitution::from_args(&interner, &params, &[]);

        // U sees V's phase-2 `any` seed.
        assert_eq!(subst.get(u), Some(TypeId::ANY));
        // V itself has no default and no arg, so it is removed.
        assert_eq!(subst.get(v), None);
    }
// TSZ_INLINE_TEST_END 04b2cb1a1e2579e4a83d6b14425ba7c91b5ca506954cecc0fc8e60393684196d
