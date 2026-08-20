//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/type_queries/declaration_walks.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 9a62cd1ed761c9e726128d2301de5ae02c28854d316d692583a43599f76c0434 421 contains_mapped_type_resolves_lazy_through_callback
    #[test]
    fn contains_mapped_type_resolves_lazy_through_callback() {
        let interner = TypeInterner::new();
        let mapped = sample_mapped(&interner);
        let def = DefId(7);
        let lazy = interner.lazy(def);
        let array_of_lazy = interner.array(lazy);

        let mut resolve = |def_id: DefId| (def_id == def).then_some(mapped);
        assert!(contains_mapped_type_through_lazy(
            &interner,
            array_of_lazy,
            &mut resolve
        ));

        let mut unresolved = |_: DefId| None;
        assert!(!contains_mapped_type_through_lazy(
            &interner,
            array_of_lazy,
            &mut unresolved
        ));
        assert!(!contains_mapped_type_through_lazy(
            &interner,
            interner.array(TypeId::STRING),
            &mut unresolved
        ));
    }
// TSZ_INLINE_TEST_END 9a62cd1ed761c9e726128d2301de5ae02c28854d316d692583a43599f76c0434

// TSZ_INLINE_TEST_BEGIN f35b494753721dabe664d7ced4fb8b6fa2f8d4207f58ccddeec8f425765dabf4 449 contains_mapped_type_walks_application_args_and_unions
    #[test]
    fn contains_mapped_type_walks_application_args_and_unions() {
        let interner = TypeInterner::new();
        let mapped = sample_mapped(&interner);
        let base = interner.lazy(DefId(3));
        let app = interner.application(base, vec![mapped]);
        let union = interner.union(vec![TypeId::STRING, app]);

        let mut resolve = |_: DefId| None;
        assert!(contains_mapped_type_through_lazy(
            &interner,
            union,
            &mut resolve
        ));
    }
// TSZ_INLINE_TEST_END f35b494753721dabe664d7ced4fb8b6fa2f8d4207f58ccddeec8f425765dabf4

// TSZ_INLINE_TEST_BEGIN a44fa3630a8eaea13879a23aafe53924391fc07781096bdea4d176569f4e5c30 465 contains_mapped_type_respects_depth_fuel
    #[test]
    fn contains_mapped_type_respects_depth_fuel() {
        let interner = TypeInterner::new();
        let mut current = sample_mapped(&interner);
        for _ in 0..(DECLARATION_WALK_DEPTH_LIMIT + 2) {
            current = interner.array(current);
        }
        let mut resolve = |_: DefId| None;
        assert!(!contains_mapped_type_through_lazy(
            &interner,
            current,
            &mut resolve
        ));
    }
// TSZ_INLINE_TEST_END a44fa3630a8eaea13879a23aafe53924391fc07781096bdea4d176569f4e5c30

// TSZ_INLINE_TEST_BEGIN e241031557be5d4ad051a8ab31286f0cabe3fee381cc2e0e215ed4a395b18159 480 conditional_alias_application_detected_through_lazy_body
    #[test]
    fn conditional_alias_application_detected_through_lazy_body() {
        let interner = TypeInterner::new();
        let cond_body = interner.conditional(ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::STRING,
            true_type: TypeId::NUMBER,
            false_type: TypeId::BOOLEAN,
            is_distributive: false,
        });
        let def = DefId(11);
        let app = interner.application(interner.lazy(def), vec![TypeId::STRING]);
        let func = interner.function(FunctionShape::new(
            vec![param(&interner, "value", TypeId::STRING)],
            app,
        ));

        let mut resolve = |def_id: DefId| (def_id == def).then_some(cond_body);
        assert!(contains_conditional_alias_application_through_lazy(
            &interner,
            func,
            &mut resolve
        ));

        let mut non_conditional = |_: DefId| Some(TypeId::STRING);
        assert!(!contains_conditional_alias_application_through_lazy(
            &interner,
            func,
            &mut non_conditional
        ));
    }
// TSZ_INLINE_TEST_END e241031557be5d4ad051a8ab31286f0cabe3fee381cc2e0e215ed4a395b18159

// TSZ_INLINE_TEST_BEGIN e1e567200db78af37a5b0fb72d66cf2431091ff70f1772609fd79bbe66f517ba 512 conditional_alias_application_respects_depth_fuel
    #[test]
    fn conditional_alias_application_respects_depth_fuel() {
        let interner = TypeInterner::new();
        let cond_body = interner.conditional(ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::STRING,
            true_type: TypeId::NUMBER,
            false_type: TypeId::BOOLEAN,
            is_distributive: false,
        });
        let def = DefId(12);
        let mut current = interner.application(interner.lazy(def), vec![TypeId::STRING]);
        for _ in 0..(DECLARATION_WALK_DEPTH_LIMIT + 2) {
            current = interner.function(FunctionShape::new(vec![], current));
        }

        let mut resolve = |def_id: DefId| (def_id == def).then_some(cond_body);
        assert!(!contains_conditional_alias_application_through_lazy(
            &interner,
            current,
            &mut resolve
        ));
    }
// TSZ_INLINE_TEST_END e1e567200db78af37a5b0fb72d66cf2431091ff70f1772609fd79bbe66f517ba

// TSZ_INLINE_TEST_BEGIN 5985b1c1469f7078b152c2484b599aa4458a0c792f0c2cca00fda57f56b18d42 536 collect_application_base_defs_applies_policy
    #[test]
    fn collect_application_base_defs_applies_policy() {
        let interner = TypeInterner::new();
        let keep = DefId(21);
        let drop = DefId(22);
        let kept_app = interner.application(interner.lazy(keep), vec![TypeId::STRING]);
        let dropped_app = interner.application(interner.lazy(drop), vec![kept_app]);
        let union = interner.union(vec![dropped_app, TypeId::NUMBER]);

        let mut include = |def_id: DefId| def_id == keep;
        let defs = collect_lazy_application_base_defs_matching(&interner, union, &mut include);
        assert!(defs.contains(&keep));
        assert!(!defs.contains(&drop));
        assert_eq!(defs.len(), 1);
    }
// TSZ_INLINE_TEST_END 5985b1c1469f7078b152c2484b599aa4458a0c792f0c2cca00fda57f56b18d42

// TSZ_INLINE_TEST_BEGIN 9f6a605fa557a946562d48fb5422d84c115dc656cec87c4bd54f6115b6c041f4 552 collect_application_base_defs_respects_depth_fuel
    #[test]
    fn collect_application_base_defs_respects_depth_fuel() {
        let interner = TypeInterner::new();
        let keep = DefId(23);
        let wrapper = DefId(24);
        let mut current = interner.application(interner.lazy(keep), vec![TypeId::STRING]);
        for _ in 0..(DECLARATION_WALK_DEPTH_LIMIT + 2) {
            current = interner.application(interner.lazy(wrapper), vec![current]);
        }

        let mut include = |def_id: DefId| def_id == keep;
        let defs = collect_lazy_application_base_defs_matching(&interner, current, &mut include);
        assert!(
            defs.is_empty(),
            "depth fuel should keep deeply nested application bases out of the result: {defs:?}"
        );
    }
// TSZ_INLINE_TEST_END 9f6a605fa557a946562d48fb5422d84c115dc656cec87c4bd54f6115b6c041f4

// TSZ_INLINE_TEST_BEGIN 4315bf48fb619f5acb03e0df94b6455245c59b332d12c9cde54f55cec9473a8c 570 rebuild_reduces_applications_inside_function_shapes
    #[test]
    fn rebuild_reduces_applications_inside_function_shapes() {
        let interner = TypeInterner::new();
        let def = DefId(31);
        let app = interner.application(interner.lazy(def), vec![TypeId::STRING]);
        let func = interner.function(FunctionShape::new(
            vec![param(&interner, "value", app)],
            TypeId::VOID,
        ));

        let mut reduce = |ty: TypeId| (ty == app).then_some(TypeId::NUMBER);
        let mut evaluate = |ty: TypeId| ty;
        let rebuilt =
            rebuild_with_reduced_alias_applications(&interner, func, &mut reduce, &mut evaluate);
        assert_ne!(rebuilt, func);
        let expected = interner.function(FunctionShape::new(
            vec![param(&interner, "value", TypeId::NUMBER)],
            TypeId::VOID,
        ));
        assert_eq!(rebuilt, expected);

        let mut no_reduce = |_: TypeId| None;
        let unchanged =
            rebuild_with_reduced_alias_applications(&interner, func, &mut no_reduce, &mut evaluate);
        assert_eq!(unchanged, func);
    }
// TSZ_INLINE_TEST_END 4315bf48fb619f5acb03e0df94b6455245c59b332d12c9cde54f55cec9473a8c

// TSZ_INLINE_TEST_BEGIN ad41372cb0a843d78394979c6f4e5698cb16afe42635e80294f9e995eab5f39b 597 rebuild_evaluates_rebuilt_conditionals
    #[test]
    fn rebuild_evaluates_rebuilt_conditionals() {
        let interner = TypeInterner::new();
        let def = DefId(41);
        let app = interner.application(interner.lazy(def), vec![TypeId::STRING]);
        let cond = interner.conditional(ConditionalType {
            check_type: TypeId::STRING,
            extends_type: TypeId::STRING,
            true_type: app,
            false_type: TypeId::BOOLEAN,
            is_distributive: false,
        });

        let mut reduce = |ty: TypeId| (ty == app).then_some(TypeId::NUMBER);
        let mut evaluate = |_: TypeId| TypeId::NUMBER;
        let rebuilt =
            rebuild_with_reduced_alias_applications(&interner, cond, &mut reduce, &mut evaluate);
        assert_eq!(rebuilt, TypeId::NUMBER);
    }
// TSZ_INLINE_TEST_END ad41372cb0a843d78394979c6f4e5698cb16afe42635e80294f9e995eab5f39b

// TSZ_INLINE_TEST_BEGIN 50de8ed1384eaa72aee1b6d604f84e3b7ca0ceb492ad8c6245e5131e47ad5f6e 617 rebuild_reduced_alias_applications_respects_depth_fuel
    #[test]
    fn rebuild_reduced_alias_applications_respects_depth_fuel() {
        let interner = TypeInterner::new();
        let def = DefId(42);
        let app = interner.application(interner.lazy(def), vec![TypeId::STRING]);
        let mut current = app;
        for _ in 0..(DECLARATION_WALK_DEPTH_LIMIT + 2) {
            current = interner.function(FunctionShape::new(vec![], current));
        }

        let mut reduce = |ty: TypeId| (ty == app).then_some(TypeId::NUMBER);
        let mut evaluate = |ty: TypeId| ty;
        let rebuilt =
            rebuild_with_reduced_alias_applications(&interner, current, &mut reduce, &mut evaluate);
        assert_eq!(
            rebuilt, current,
            "depth fuel should preserve the existing opaque fallback"
        );
    }
// TSZ_INLINE_TEST_END 50de8ed1384eaa72aee1b6d604f84e3b7ca0ceb492ad8c6245e5131e47ad5f6e

// TSZ_INLINE_TEST_BEGIN 663130b296d77e3085f856ee812f85e1fce0922980cfdfa6be010d22c5e8ffbb 637 lazy_body_display_resolution_classifies_kinds
    #[test]
    fn lazy_body_display_resolution_classifies_kinds() {
        let interner = TypeInterner::new();
        let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
        assert!(lazy_body_resolves_for_declaration_display(&interner, union));
        assert!(lazy_body_resolves_for_declaration_display(
            &interner,
            interner.keyof(TypeId::STRING)
        ));
        assert!(lazy_body_resolves_for_declaration_display(
            &interner,
            TypeId::STRING
        ));
        assert!(lazy_body_resolves_for_declaration_display(
            &interner,
            interner.literal_string("draft")
        ));
        let object = interner.object(vec![]);
        assert!(!lazy_body_resolves_for_declaration_display(
            &interner, object
        ));
        let func = interner.function(FunctionShape::new(vec![], TypeId::VOID));
        assert!(!lazy_body_resolves_for_declaration_display(&interner, func));
    }
// TSZ_INLINE_TEST_END 663130b296d77e3085f856ee812f85e1fce0922980cfdfa6be010d22c5e8ffbb
