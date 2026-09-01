//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/type_queries/shape_queries.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN c920d270abbc56e2010bffa5b439a4d97e5811b23580f8d6f297c0736914ea1d 382 application_base_is_mapped_type_detects_mapped_alias
    #[test]
    fn application_base_is_mapped_type_detects_mapped_alias() {
        let interner = TypeInterner::new();
        let def_id = DefId(7);
        let body = make_identity_mapped(&interner);
        let resolver = DefBodyResolver { def_id, body };
        let app = interner.application(interner.lazy(def_id), vec![TypeId::STRING]);
        assert!(application_base_is_mapped_type_db(
            &interner, &resolver, app
        ));
    }
// TSZ_INLINE_TEST_END c920d270abbc56e2010bffa5b439a4d97e5811b23580f8d6f297c0736914ea1d

// TSZ_INLINE_TEST_BEGIN 704c7c35129e34c7bdd4636f5b4d3a3929a586ceffaf04409b11bad2c9244bc5 394 application_base_is_mapped_type_rejects_non_mapped_alias
    #[test]
    fn application_base_is_mapped_type_rejects_non_mapped_alias() {
        let interner = TypeInterner::new();
        let def_id = DefId(7);
        // Alias body is a plain object-ish type, not a mapped type.
        let resolver = DefBodyResolver {
            def_id,
            body: TypeId::STRING,
        };
        let app = interner.application(interner.lazy(def_id), vec![TypeId::STRING]);
        assert!(!application_base_is_mapped_type_db(
            &interner, &resolver, app
        ));
    }
// TSZ_INLINE_TEST_END 704c7c35129e34c7bdd4636f5b4d3a3929a586ceffaf04409b11bad2c9244bc5

// TSZ_INLINE_TEST_BEGIN 5dde8a3ac1f832160af0880244abb95d88a4641ecd8aabbc41006fe2fd25e239 409 application_base_is_mapped_type_rejects_non_application
    #[test]
    fn application_base_is_mapped_type_rejects_non_application() {
        let interner = TypeInterner::new();
        let resolver = DefBodyResolver {
            def_id: DefId(7),
            body: make_identity_mapped(&interner),
        };
        assert!(!application_base_is_mapped_type_db(
            &interner,
            &resolver,
            TypeId::STRING
        ));
        assert!(!application_base_is_mapped_type_db(
            &interner,
            &resolver,
            make_identity_mapped(&interner)
        ));
    }
// TSZ_INLINE_TEST_END 5dde8a3ac1f832160af0880244abb95d88a4641ecd8aabbc41006fe2fd25e239

// TSZ_INLINE_TEST_BEGIN 738c017179340c9489496c74064a4a8ea2142e7bc16eeffeb206429ca72b25af 428 intrinsic_short_circuit
    #[test]
    fn intrinsic_short_circuit() {
        let interner = TypeInterner::new();
        assert!(!shape_contains_conditional_type_db(
            &interner,
            TypeId::STRING
        ));
        assert!(!shape_contains_conditional_type_db(
            &interner,
            TypeId::NUMBER
        ));
        assert!(!shape_contains_conditional_type_db(&interner, TypeId::ANY));
    }
// TSZ_INLINE_TEST_END 738c017179340c9489496c74064a4a8ea2142e7bc16eeffeb206429ca72b25af

// TSZ_INLINE_TEST_BEGIN 2bf8904c84299517f4ff10a80631c1f81da1c6fbbd3230ee748328f50a238c20 442 direct_conditional_matches
    #[test]
    fn direct_conditional_matches() {
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        assert!(shape_contains_conditional_type_db(&interner, cond));
    }
// TSZ_INLINE_TEST_END 2bf8904c84299517f4ff10a80631c1f81da1c6fbbd3230ee748328f50a238c20

// TSZ_INLINE_TEST_BEGIN ec25fc2e9ff5d949d5e7e7f737fbad05dba0248634e390222a8f35b0d815a330 449 conditional_inside_union_matches
    #[test]
    fn conditional_inside_union_matches() {
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let union = interner.union(vec![TypeId::STRING, cond]);
        assert!(shape_contains_conditional_type_db(&interner, union));
    }
// TSZ_INLINE_TEST_END ec25fc2e9ff5d949d5e7e7f737fbad05dba0248634e390222a8f35b0d815a330

// TSZ_INLINE_TEST_BEGIN 0b67c91bb7df044b7f6c67698190824ebe01621391fb68da1ca360f6aa550dab 457 conditional_inside_intersection_matches
    #[test]
    fn conditional_inside_intersection_matches() {
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let inter = interner.intersection(vec![TypeId::STRING, cond]);
        assert!(shape_contains_conditional_type_db(&interner, inter));
    }
// TSZ_INLINE_TEST_END 0b67c91bb7df044b7f6c67698190824ebe01621391fb68da1ca360f6aa550dab

// TSZ_INLINE_TEST_BEGIN 238f2d27853cca3cd3c5e2889da013b69d96d660c5fb3913949300066c64ad57 465 conditional_inside_application_arg_matches
    #[test]
    fn conditional_inside_application_arg_matches() {
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let base = interner.lazy(DefId(42));
        let app = interner.application(base, vec![cond]);
        assert!(shape_contains_conditional_type_db(&interner, app));
    }
// TSZ_INLINE_TEST_END 238f2d27853cca3cd3c5e2889da013b69d96d660c5fb3913949300066c64ad57

// TSZ_INLINE_TEST_BEGIN 268cd0261a6382297bd2ccb2c3f98e466a10257bddfc8f3521830b0485da27df 474 conditional_inside_index_access_object_matches
    #[test]
    fn conditional_inside_index_access_object_matches() {
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let ia = interner.index_access(cond, TypeId::STRING);
        assert!(shape_contains_conditional_type_db(&interner, ia));
    }
// TSZ_INLINE_TEST_END 268cd0261a6382297bd2ccb2c3f98e466a10257bddfc8f3521830b0485da27df

// TSZ_INLINE_TEST_BEGIN 44284fe78a61fa04a5b5404af9b48ec748335f232397ade9adc7dccc8899fd71 482 conditional_inside_index_access_index_matches
    #[test]
    fn conditional_inside_index_access_index_matches() {
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let base = interner.lazy(DefId(42));
        let ia = interner.index_access(base, cond);
        assert!(shape_contains_conditional_type_db(&interner, ia));
    }
// TSZ_INLINE_TEST_END 44284fe78a61fa04a5b5404af9b48ec748335f232397ade9adc7dccc8899fd71

// TSZ_INLINE_TEST_BEGIN 738ad48f38140d8763a3e8b2c9c9edc7a9470947bc1b1293b93c607250bc90c0 491 deeply_nested_projection_matches
    #[test]
    fn deeply_nested_projection_matches() {
        // union(intersection(application_args(conditional))) — every layer
        // is a projection path and must be walked.
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let base = interner.lazy(DefId(42));
        let app = interner.application(base, vec![cond]);
        let inter = interner.intersection(vec![TypeId::STRING, app]);
        let union = interner.union(vec![TypeId::NUMBER, inter]);
        assert!(shape_contains_conditional_type_db(&interner, union));
    }
// TSZ_INLINE_TEST_END 738ad48f38140d8763a3e8b2c9c9edc7a9470947bc1b1293b93c607250bc90c0

// TSZ_INLINE_TEST_BEGIN 460e1737f353feb77c4f93af51dc316b8c26944113985c4b3fce4eb0aa1b035e 504 conditional_buried_in_mapped_template_does_not_match
    #[test]
    fn conditional_buried_in_mapped_template_does_not_match() {
        // Mapped templates are NOT a projection path — descending into them
        // would over-suppress diagnostics.
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let mapped = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: interner.intern_string("K"),
                constraint: Some(TypeId::STRING),
                default: None,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            },
            constraint: TypeId::STRING,
            name_type: None,
            template: cond,
            readonly_modifier: None,
            optional_modifier: None,
        });
        assert!(!shape_contains_conditional_type_db(&interner, mapped));
    }
// TSZ_INLINE_TEST_END 460e1737f353feb77c4f93af51dc316b8c26944113985c4b3fce4eb0aa1b035e

// TSZ_INLINE_TEST_BEGIN 484eaec34e4be6e87226a9aca54c376805e57492e9bebe9af4b15ebf2643941e 527 no_conditional_returns_false
    #[test]
    fn no_conditional_returns_false() {
        let interner = TypeInterner::new();
        let base = interner.lazy(DefId(1));
        let app = interner.application(base, vec![TypeId::STRING, TypeId::NUMBER]);
        let union = interner.union(vec![TypeId::STRING, app]);
        assert!(!shape_contains_conditional_type_db(&interner, union));
    }
// TSZ_INLINE_TEST_END 484eaec34e4be6e87226a9aca54c376805e57492e9bebe9af4b15ebf2643941e

// TSZ_INLINE_TEST_BEGIN 15276122c9a97f470af7cf38478f381f7173e34676dad30a3dcb22ffb5efe616 536 shared_dag_subtree_is_walked_once
    #[test]
    fn shared_dag_subtree_is_walked_once() {
        // The same `Application` is reachable through two `Union` branches.
        // The memo proves we visit the shared subtree once — the result
        // is consistent regardless of sharing.
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let base = interner.lazy(DefId(1));
        let shared = interner.application(base, vec![cond]);
        let left = interner.intersection(vec![TypeId::STRING, shared]);
        let right = interner.intersection(vec![TypeId::NUMBER, shared]);
        let union = interner.union(vec![left, right]);
        assert!(shape_contains_conditional_type_db(&interner, union));
    }
// TSZ_INLINE_TEST_END 15276122c9a97f470af7cf38478f381f7173e34676dad30a3dcb22ffb5efe616

// TSZ_INLINE_TEST_BEGIN 9151fee37950fcb31a3c92a64ae9fdcc1d56607d3177ea3458e91072f51dbb84 551 generic_mapped_type_when_constraint_has_type_param
    #[test]
    fn generic_mapped_type_when_constraint_has_type_param() {
        let interner = TypeInterner::new();
        let t_param = make_type_param(&interner, "T");
        let keyof_t = interner.keyof(t_param);
        let mapped = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: interner.intern_string("K"),
                constraint: Some(keyof_t),
                default: None,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            },
            constraint: keyof_t,
            name_type: None,
            template: TypeId::NUMBER,
            readonly_modifier: None,
            optional_modifier: None,
        });
        assert!(is_generic_mapped_type_db(&interner, mapped));
    }
// TSZ_INLINE_TEST_END 9151fee37950fcb31a3c92a64ae9fdcc1d56607d3177ea3458e91072f51dbb84

// TSZ_INLINE_TEST_BEGIN 0a54dbdea4e5a168837886b510e62febab98b334adad4863abfe6494216d178d 573 concrete_mapped_type_is_not_generic
    #[test]
    fn concrete_mapped_type_is_not_generic() {
        let interner = TypeInterner::new();
        let mapped = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: interner.intern_string("K"),
                constraint: Some(TypeId::STRING),
                default: None,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            },
            constraint: TypeId::STRING,
            name_type: None,
            template: TypeId::NUMBER,
            readonly_modifier: None,
            optional_modifier: None,
        });
        assert!(!is_generic_mapped_type_db(&interner, mapped));
    }
// TSZ_INLINE_TEST_END 0a54dbdea4e5a168837886b510e62febab98b334adad4863abfe6494216d178d

// TSZ_INLINE_TEST_BEGIN cfa524627808a7f400597e67d49aa4d775f833c3065ef08d684636f53d0143b8 593 generic_mapped_type_when_name_type_has_type_param
    #[test]
    fn generic_mapped_type_when_name_type_has_type_param() {
        // {[K in "a" | "b" as `prefix-${T}`]: number} — name_type drives genericness.
        let interner = TypeInterner::new();
        let t_param = make_type_param(&interner, "T");
        let concrete_keys = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let mapped = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: interner.intern_string("K"),
                constraint: Some(concrete_keys),
                default: None,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            },
            constraint: concrete_keys,
            name_type: Some(t_param),
            template: TypeId::NUMBER,
            readonly_modifier: None,
            optional_modifier: None,
        });
        assert!(is_generic_mapped_type_db(&interner, mapped));
    }
// TSZ_INLINE_TEST_END cfa524627808a7f400597e67d49aa4d775f833c3065ef08d684636f53d0143b8

// TSZ_INLINE_TEST_BEGIN 29f10051e1007feeb349070cb1b4f669deb88e4cc11224b880ef52ce2e2eac70 616 renaming_iteration_var_does_not_change_classification
    #[test]
    fn renaming_iteration_var_does_not_change_classification() {
        // Confirm the rule is structural — different iteration var names
        // ("P" vs "K") give the same answer.
        let interner = TypeInterner::new();
        let t_param = make_type_param(&interner, "T");
        let keyof_t = interner.keyof(t_param);

        let mapped_k = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: interner.intern_string("K"),
                constraint: Some(keyof_t),
                default: None,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            },
            constraint: keyof_t,
            name_type: None,
            template: TypeId::NUMBER,
            readonly_modifier: None,
            optional_modifier: None,
        });

        let mapped_p = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: interner.intern_string("P"),
                constraint: Some(keyof_t),
                default: None,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            },
            constraint: keyof_t,
            name_type: None,
            template: TypeId::NUMBER,
            readonly_modifier: None,
            optional_modifier: None,
        });

        assert_eq!(
            is_generic_mapped_type_db(&interner, mapped_k),
            is_generic_mapped_type_db(&interner, mapped_p),
        );
    }
// TSZ_INLINE_TEST_END 29f10051e1007feeb349070cb1b4f669deb88e4cc11224b880ef52ce2e2eac70

// TSZ_INLINE_TEST_BEGIN 4dd35d1a270650e9c0cf560b44eb2127a66f652bd90b38c9ec5b3807e47123f0 660 non_mapped_type_is_not_generic_mapped
    #[test]
    fn non_mapped_type_is_not_generic_mapped() {
        let interner = TypeInterner::new();
        let t_param = make_type_param(&interner, "T");
        assert!(!is_generic_mapped_type_db(&interner, t_param));
        assert!(!is_generic_mapped_type_db(&interner, TypeId::STRING));
        let cond = make_conditional(&interner);
        assert!(!is_generic_mapped_type_db(&interner, cond));
    }
// TSZ_INLINE_TEST_END 4dd35d1a270650e9c0cf560b44eb2127a66f652bd90b38c9ec5b3807e47123f0

// TSZ_INLINE_TEST_BEGIN 2e3da0996b7bf35eb6cb752f5a1e71071833579594f2ed79fb4a99e2af00a21c 670 type_parameter_with_conditional_constraint
    #[test]
    fn type_parameter_with_conditional_constraint() {
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let tp = interner.type_param(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: Some(cond),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        assert!(type_parameter_has_conditional_constraint_db(&interner, tp));
    }
// TSZ_INLINE_TEST_END 2e3da0996b7bf35eb6cb752f5a1e71071833579594f2ed79fb4a99e2af00a21c

// TSZ_INLINE_TEST_BEGIN 55189323ab006a653c2e33a825cdcab007d68d400c21c7ac767a0d012330e364 684 type_parameter_with_conditional_via_projection_constraint
    #[test]
    fn type_parameter_with_conditional_via_projection_constraint() {
        // T extends Foo<Cond> — constraint is Application, args contain conditional.
        let interner = TypeInterner::new();
        let cond = make_conditional(&interner);
        let base = interner.lazy(DefId(1));
        let app = interner.application(base, vec![cond]);
        let tp = interner.type_param(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: Some(app),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        assert!(type_parameter_has_conditional_constraint_db(&interner, tp));
    }
// TSZ_INLINE_TEST_END 55189323ab006a653c2e33a825cdcab007d68d400c21c7ac767a0d012330e364

// TSZ_INLINE_TEST_BEGIN a391d89f73bc42db5fbd8acef158dda7ea445ec05e85e57171ed53e95614fb85 701 type_parameter_without_constraint
    #[test]
    fn type_parameter_without_constraint() {
        let interner = TypeInterner::new();
        let tp = make_type_param(&interner, "T");
        assert!(!type_parameter_has_conditional_constraint_db(&interner, tp));
        assert!(!type_parameter_has_mapped_constraint_db(&interner, tp));
    }
// TSZ_INLINE_TEST_END a391d89f73bc42db5fbd8acef158dda7ea445ec05e85e57171ed53e95614fb85

// TSZ_INLINE_TEST_BEGIN 72235653590f06877d633268937b289d177728917af6c3e8a9b0563a90c5752f 709 type_parameter_with_mapped_constraint
    #[test]
    fn type_parameter_with_mapped_constraint() {
        let interner = TypeInterner::new();
        let u_param = make_type_param(&interner, "U");
        let keyof_u = interner.keyof(u_param);
        let mapped = interner.mapped(MappedType {
            type_param: TypeParamInfo {
                name: interner.intern_string("K"),
                constraint: Some(keyof_u),
                default: None,
                is_const: false,
                origin: crate::types::TypeParamOrigin::User,
            },
            constraint: keyof_u,
            name_type: None,
            template: TypeId::NUMBER,
            readonly_modifier: None,
            optional_modifier: None,
        });
        let tp = interner.type_param(TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: Some(mapped),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        assert!(type_parameter_has_mapped_constraint_db(&interner, tp));
    }
// TSZ_INLINE_TEST_END 72235653590f06877d633268937b289d177728917af6c3e8a9b0563a90c5752f
