//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/visitors/visitor_predicates/content.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 51c0a201af304aa7cd19486ca2e7756593f4e1cccedf616bff953b5b8e8e3775 921 free_decl_origins_scope_nested_generic_binders_but_keep_outer_captures
    #[test]
    fn free_decl_origins_scope_nested_generic_binders_but_keep_outer_captures() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("capture.js");
        let outer_info = TypeParamInfo {
            origin: TypeParamOrigin::DeclScoped { file, node: 10 },
            ..TypeParamInfo::simple(interner.intern_string("OuterValue"))
        };
        let inner_info = TypeParamInfo {
            origin: TypeParamOrigin::DeclScoped { file, node: 20 },
            ..TypeParamInfo::simple(interner.intern_string("InnerValue"))
        };
        let outer = interner.type_param(outer_info);
        let inner = interner.type_param(inner_info);
        let nested_generic = interner.function(FunctionShape {
            type_params: vec![inner_info],
            ..FunctionShape::new(vec![ParamInfo::unnamed(inner)], outer)
        });
        let object = interner.object(vec![PropertyInfo::new(
            interner.intern_string("transform"),
            nested_generic,
        )]);

        let origins = free_decl_scoped_type_parameter_origins_in(&interner, [object]);
        assert_eq!(origins.len(), 1);
        assert!(origins.contains(&(outer_info.origin, outer_info.name)));
        assert!(!origins.contains(&(inner_info.origin, inner_info.name)));
    }
// TSZ_INLINE_TEST_END 51c0a201af304aa7cd19486ca2e7756593f4e1cccedf616bff953b5b8e8e3775

// TSZ_INLINE_TEST_BEGIN 8c8e8fdd68ea3690db04df929d9c32588966c5e8133182c7a98fdb8cd1c80b90 950 free_decl_origins_deduplicate_reminted_ids_and_ignore_legacy_user_params
    #[test]
    fn free_decl_origins_deduplicate_reminted_ids_and_ignore_legacy_user_params() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("remint.js");
        let name = interner.intern_string("Value");
        let origin = TypeParamOrigin::DeclScoped { file, node: 30 };
        let first = interner.type_param(TypeParamInfo {
            origin,
            ..TypeParamInfo::simple(name)
        });
        let second = interner.type_param(TypeParamInfo {
            constraint: Some(TypeId::STRING),
            origin,
            ..TypeParamInfo::simple(name)
        });
        let legacy = interner.type_param(TypeParamInfo::simple(name));
        assert_ne!(first, second, "the witness must use separately minted ids");

        let origins = free_decl_scoped_type_parameter_origins_in(
            &interner,
            [interner.tuple(vec![
                TupleElement::fixed(first),
                TupleElement::fixed(second),
                TupleElement::fixed(legacy),
            ])],
        );
        assert_eq!(origins, FxHashSet::from_iter([(origin, name)]));
    }
// TSZ_INLINE_TEST_END 8c8e8fdd68ea3690db04df929d9c32588966c5e8133182c7a98fdb8cd1c80b90

// TSZ_INLINE_TEST_BEGIN b1db280e615ac4e297e09236abb8f6f128d20e2a87949ab9344e54ca45341bc9 979 predicate_worklist_visit_state_names_intrinsic_entered_and_revisit
    #[test]
    fn predicate_worklist_visit_state_names_intrinsic_entered_and_revisit() {
        let interner = TypeInterner::new();
        let type_id = interner.object(vec![]);
        let mut visited = FxHashSet::default();

        assert_eq!(
            PredicateWorklistVisitState::enter(TypeId::ANY, &mut visited),
            PredicateWorklistVisitState::IgnoredIntrinsic
        );
        assert!(visited.is_empty());
        assert_eq!(
            PredicateWorklistVisitState::enter(type_id, &mut visited),
            PredicateWorklistVisitState::Entered
        );
        assert_eq!(
            PredicateWorklistVisitState::enter(type_id, &mut visited),
            PredicateWorklistVisitState::AlreadyVisited
        );
    }
// TSZ_INLINE_TEST_END b1db280e615ac4e297e09236abb8f6f128d20e2a87949ab9344e54ca45341bc9

// TSZ_INLINE_TEST_BEGIN 36f116e00c1090baba0869dc82cb66cbbe93e0fc2b4b54534dc6872406439ff7 1000 contains_type_by_id_visit_state_names_entered_and_revisit
    #[test]
    fn contains_type_by_id_visit_state_names_entered_and_revisit() {
        let interner = TypeInterner::new();
        let type_id = interner.object(vec![]);
        let mut visited = FxHashSet::default();

        assert_eq!(
            ContainsTypeByIdVisitState::enter(type_id, &mut visited),
            ContainsTypeByIdVisitState::Entered
        );
        assert_eq!(
            ContainsTypeByIdVisitState::enter(type_id, &mut visited),
            ContainsTypeByIdVisitState::AlreadyVisited
        );
    }
// TSZ_INLINE_TEST_END 36f116e00c1090baba0869dc82cb66cbbe93e0fc2b4b54534dc6872406439ff7

// TSZ_INLINE_TEST_BEGIN 97d0088eb7c1500dd10828c0e67a0f5b5ff1df03c976e31933dfbe72783e0431 1016 contains_type_by_id_handles_shared_child_once
    #[test]
    fn contains_type_by_id_handles_shared_child_once() {
        let interner = TypeInterner::new();
        let child = interner.object(vec![]);
        let root = interner.tuple(vec![TupleElement::fixed(child), TupleElement::fixed(child)]);

        assert!(contains_type_by_id(&interner, root, child));
        assert!(!contains_type_by_id(&interner, root, TypeId::STRING));
    }
// TSZ_INLINE_TEST_END 97d0088eb7c1500dd10828c0e67a0f5b5ff1df03c976e31933dfbe72783e0431

// TSZ_INLINE_TEST_BEGIN 52a65fd1f15a5e2f4f9a5a068aa1369036725c1e98ebadbf9257cbdfaeeb7144 1026 type_parameter_binder_predicate_uses_scoped_identity_with_legacy_fallback
    #[test]
    fn type_parameter_binder_predicate_uses_scoped_identity_with_legacy_fallback() {
        let interner = TypeInterner::new();
        let file = interner.intern_string("binder-predicate.ts");
        let name = interner.intern_string("U");
        let owned = TypeParamInfo {
            name,
            constraint: None,
            default: None,
            is_const: false,
            origin: TypeParamOrigin::DeclScoped { file, node: 1 },
        };
        let foreign = TypeParamInfo {
            origin: TypeParamOrigin::DeclScoped { file, node: 2 },
            ..owned
        };

        assert!(contains_type_parameter_binder(
            &interner,
            interner.fresh_type_param(owned),
            owned,
        ));
        assert!(!contains_type_parameter_binder(
            &interner,
            interner.fresh_type_param(foreign),
            owned,
        ));

        let unstamped = TypeParamInfo::simple(name);
        assert!(contains_type_parameter_binder(
            &interner,
            interner.fresh_type_param(unstamped),
            TypeParamInfo::simple(name),
        ));
    }
// TSZ_INLINE_TEST_END 52a65fd1f15a5e2f4f9a5a068aa1369036725c1e98ebadbf9257cbdfaeeb7144

// TSZ_INLINE_TEST_BEGIN 2e532163a53ca4768a978f66defa826646027d9f8f8396a729817eb769ec1e90 1062 predicate_checker_memo_entry_counts_are_observable
    #[test]
    fn predicate_checker_memo_entry_counts_are_observable() {
        let interner = TypeInterner::new();
        let t_name = interner.intern_string("T");
        let u_name = interner.intern_string("U");
        let t_param = interner.type_param(TypeParamInfo::simple(t_name));
        let u_infer = interner.infer(TypeParamInfo::simple(u_name));
        let wrapper = interner.readonly_type(t_param);

        let mut contains_checker =
            DeepContainsChecker::new(&interner, ChildPolicy::CONTENT_PREDICATE, |key| {
                matches!(key, TypeData::TypeParameter(_))
            });
        assert!(contains_checker.check(wrapper));
        assert!(contains_checker.memo_entries() > 0);

        assert!(contains_free_type_parameters(&interner, wrapper));
        assert!(contains_free_infer_types(&interner, u_infer));
        assert!(!contains_free_infer_types(&interner, wrapper));
    }
// TSZ_INLINE_TEST_END 2e532163a53ca4768a978f66defa826646027d9f8f8396a729817eb769ec1e90

// TSZ_INLINE_TEST_BEGIN c144ede03cbac9cd5053df7f2bc632f6d0c2d6a53e3159fb1716dc90ec162515 1086 free_infer_policy_skips_type_param_constraints
    /// `contains_free_infer_types` must not treat structural `infer` patterns
    /// inside a `TypeParameter`'s constraint as live inference variables, while
    /// the generic deep walk does descend into constraints.
    #[test]
    fn free_infer_policy_skips_type_param_constraints() {
        let interner = TypeInterner::new();
        let v_name = interner.intern_string("V");
        let t_name = interner.intern_string("T");
        let infer_v = interner.infer(TypeParamInfo::simple(v_name));
        let constrained = interner.type_param(TypeParamInfo {
            constraint: Some(infer_v),
            ..TypeParamInfo::simple(t_name)
        });
        let wrapper = interner.readonly_type(constrained);

        assert!(!contains_free_infer_types(&interner, wrapper));
        assert!(contains_infer_types(&interner, wrapper));
    }
// TSZ_INLINE_TEST_END c144ede03cbac9cd5053df7f2bc632f6d0c2d6a53e3159fb1716dc90ec162515

// TSZ_INLINE_TEST_BEGIN 04a29c63a76392ee4fa14877554c3d3bbbc8087af67e1faf3368fded1437d38c 1109 free_infer_policy_skips_conditional_bound_infer
    /// `contains_free_infer_types` must not treat an `infer` declared inside a
    /// conditional's `extends` clause as a live inference variable: it is bound
    /// by that conditional and is part of a stable deferred type (e.g. the
    /// declared return type of a method). Counting it made `Box<string>`
    /// (whose method `m` returns `U extends Promise<infer V> ? …`) look like it
    /// held a transient inference placeholder, suppressing real `TS2322`/`TS2345`
    /// diagnostics. A bare/root `infer` stays free.
    #[test]
    fn free_infer_policy_skips_conditional_bound_infer() {
        use crate::types::ConditionalType;
        let interner = TypeInterner::new();
        let v_name = interner.intern_string("V");
        let t_name = interner.intern_string("T");
        let infer_v = interner.infer(TypeParamInfo::simple(v_name));
        let t_param = interner.type_param(TypeParamInfo::simple(t_name));

        // `T extends infer V ? 1 : 2` — `infer V` is bound by the conditional.
        let cond = interner.conditional(ConditionalType {
            check_type: t_param,
            extends_type: infer_v,
            true_type: TypeId::NUMBER,
            false_type: TypeId::NUMBER,
            is_distributive: false,
        });
        let wrapper = interner.readonly_type(cond);

        assert!(
            !contains_free_infer_types(&interner, wrapper),
            "an `infer` bound by a conditional must not count as a free infer"
        );
        // The generic deep walk still sees the structural `infer` node.
        assert!(contains_infer_types(&interner, wrapper));
        // A bare `infer` is still free.
        assert!(contains_free_infer_types(&interner, infer_v));
    }
// TSZ_INLINE_TEST_END 04a29c63a76392ee4fa14877554c3d3bbbc8087af67e1faf3368fded1437d38c

// TSZ_INLINE_TEST_BEGIN 40fbeb23dd1be5f1d5823de2bdb9da8595d16416d0c312780274d3fdea3a631c 1141 free_type_param_policy_skips_generic_signature_bodies
    /// Free-type-parameter checks skip the bodies of generic signatures (their
    /// parameters are bound), but still see free parameters in non-generic
    /// signature bodies.
    #[test]
    fn free_type_param_policy_skips_generic_signature_bodies() {
        let interner = TypeInterner::new();
        let t_name = interner.intern_string("T");
        let t_param = interner.type_param(TypeParamInfo::simple(t_name));

        let generic_fn = interner.function(crate::types::FunctionShape {
            type_params: vec![TypeParamInfo::simple(t_name)],
            ..crate::types::FunctionShape::new(vec![], t_param)
        });
        assert!(!contains_free_type_parameters(&interner, generic_fn));
        assert!(contains_type_parameters(&interner, generic_fn));

        let plain_fn = interner.function(crate::types::FunctionShape::new(vec![], t_param));
        assert!(contains_free_type_parameters(&interner, plain_fn));
    }
// TSZ_INLINE_TEST_END 40fbeb23dd1be5f1d5823de2bdb9da8595d16416d0c312780274d3fdea3a631c

// TSZ_INLINE_TEST_BEGIN c969bc2222d9767e1e35215b4b993def5792427123fe8f778d47f2ad9172e857 1172 free_infer_policy_skips_conditional_bound_infer_in_signature_bodies
    /// An `infer V` declared inside a conditional's `extends` clause is a
    /// definitional binder scoped to that conditional — never a live transient
    /// inference placeholder — regardless of whether the enclosing signature is
    /// generic. The `FREE_INFER` policy encodes this with `deferred_operations:
    /// false`: the walk stops at a deferred conditional/mapped/indexed/`keyof`
    /// node, so an `infer` reachable only through such an operand is not
    /// reported. Reporting it would wrongly route
    /// `should_suppress_assignability_diagnostic` into suppressing a real
    /// `TS2322`/`TS2345` (issue #14784).
    ///
    /// This mirrors the sibling `free_infer_policy_skips_conditional_bound_infer`
    /// (which wraps the same conditional in a `readonly` type): the container —
    /// generic signature, non-generic signature, or `readonly` — does not change
    /// the answer, because the conditional itself is the binder.
    #[test]
    fn free_infer_policy_skips_conditional_bound_infer_in_signature_bodies() {
        let interner = TypeInterner::new();
        let u_name = interner.intern_string("U");
        let v_name = interner.intern_string("V");
        let u_param = interner.type_param(TypeParamInfo::simple(u_name));
        let infer_v = interner.infer(TypeParamInfo::simple(v_name));
        // `U extends infer V ? U : U` — a deferred conditional whose `extends`
        // declares `infer V`.
        let cond = interner.conditional(crate::types::ConditionalType {
            check_type: u_param,
            extends_type: infer_v,
            true_type: u_param,
            false_type: u_param,
            is_distributive: false,
        });

        // A *generic* signature binds both its own `U` and the conditional's
        // `infer V`; the free-infer walk treats the whole body as bound.
        let generic_fn = interner.function(crate::types::FunctionShape {
            type_params: vec![TypeParamInfo::simple(u_name)],
            ..crate::types::FunctionShape::new(vec![], cond)
        });
        assert!(!contains_free_infer_types(&interner, generic_fn));
        // The structural `infer` is still observable to the un-scoped walk.
        assert!(contains_infer_types(&interner, generic_fn));

        // A *non-generic* signature body carrying the same conditional is ALSO
        // not free-infer-bearing: the `infer V` is bound by the conditional, not
        // by any enclosing signature. Classifying it as free would reintroduce
        // the #14784 false negative (a stable deferred conditional type is not a
        // live inference session, so its assignability diagnostics must stand).
        let plain_fn = interner.function(crate::types::FunctionShape::new(vec![], cond));
        assert!(!contains_free_infer_types(&interner, plain_fn));
        // The un-scoped walk still sees the structural `infer`.
        assert!(contains_infer_types(&interner, plain_fn));
    }
// TSZ_INLINE_TEST_END c969bc2222d9767e1e35215b4b993def5792427123fe8f778d47f2ad9172e857

// TSZ_INLINE_TEST_BEGIN 5cc5493fc680aabb925d0ea9e1744f690add7ca47fa32035bae36f75150369ee 1216 free_infer_policy_skips_generic_signature_bodies
    /// Isolates `skip_generic_signature_bodies` from `deferred_operations`: a
    /// *genuinely free* `infer` placed as a bare (non-deferred) return type is a
    /// live placeholder and IS observed on a non-generic signature, but a
    /// generic signature binds its whole body and hides it. This is the only
    /// child position where `skip_generic_signature_bodies` acts independently —
    /// `deferred_operations: false` alone does not gate a non-deferred child.
    #[test]
    fn free_infer_policy_skips_generic_signature_bodies() {
        let interner = TypeInterner::new();
        let u_name = interner.intern_string("U");
        let v_name = interner.intern_string("V");
        let infer_v = interner.infer(TypeParamInfo::simple(v_name));

        // Non-generic signature returning a bare structural `infer` → the free
        // `infer` is reachable as a direct (non-deferred) child and is reported.
        let plain_fn = interner.function(crate::types::FunctionShape::new(vec![], infer_v));
        assert!(contains_free_infer_types(&interner, plain_fn));

        // A generic signature binds its whole body, so the same bare `infer`
        // return is treated as bound — the case only
        // `skip_generic_signature_bodies` covers.
        let generic_fn = interner.function(crate::types::FunctionShape {
            type_params: vec![TypeParamInfo::simple(u_name)],
            ..crate::types::FunctionShape::new(vec![], infer_v)
        });
        assert!(!contains_free_infer_types(&interner, generic_fn));
        // The un-scoped walk still sees the structural `infer` in both.
        assert!(contains_infer_types(&interner, generic_fn));
    }
// TSZ_INLINE_TEST_END 5cc5493fc680aabb925d0ea9e1744f690add7ca47fa32035bae36f75150369ee
