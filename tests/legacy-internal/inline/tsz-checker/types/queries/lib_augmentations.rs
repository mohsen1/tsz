//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/types/queries/lib_augmentations.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 255f50b9a473b3634bd2fdd08f0a9e7cabb6167f44fc1a4ece25bfef659d1cde 427 selected_same_name_lazy_identity_does_not_rewrite_or_bump_generation
    #[test]
    fn selected_same_name_lazy_identity_does_not_rewrite_or_bump_generation() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            &arena,
            &binder,
            &types,
            "identity.ts".to_string(),
            CheckerOptions::default(),
        );
        let name = types.intern_string("AliasToken");
        let canonical = checker
            .ctx
            .definition_store
            .register(DefinitionInfo::type_alias(name, Vec::new(), TypeId::STRING));
        let sibling = checker
            .ctx
            .definition_store
            .register(DefinitionInfo::type_alias(name, Vec::new(), TypeId::NUMBER));

        let sibling_ref = types.lazy(sibling);
        let generation = checker.ctx.definition_store.generation();
        checker.register_finalized_lib_body_for_def("AliasToken", sibling_ref, Some(sibling));

        assert_eq!(checker.ctx.definition_store.generation(), generation);
        assert_eq!(
            checker.ctx.definition_store.get_body(canonical),
            Some(TypeId::STRING),
            "a same-name sibling wrapper is its own public identity, not a body for the first def",
        );
        assert_eq!(
            checker.ctx.definition_store.get_body(sibling),
            Some(TypeId::NUMBER),
            "finalization must preserve the sibling's already-published structural body",
        );
    }
// TSZ_INLINE_TEST_END 255f50b9a473b3634bd2fdd08f0a9e7cabb6167f44fc1a4ece25bfef659d1cde

// TSZ_INLINE_TEST_BEGIN 43d506c2c5240eee00c5bb609e42ba4cdba90179c21ca57747468d8b4c32c946 466 selected_same_name_structural_body_still_finalizes_canonical_definition
    #[test]
    fn selected_same_name_structural_body_still_finalizes_canonical_definition() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            &arena,
            &binder,
            &types,
            "structural.ts".to_string(),
            CheckerOptions::default(),
        );
        let name = types.intern_string("AliasToken");
        let canonical = checker
            .ctx
            .definition_store
            .register(DefinitionInfo::type_alias(name, Vec::new(), TypeId::STRING));
        let sibling = checker
            .ctx
            .definition_store
            .register(DefinitionInfo::type_alias(name, Vec::new(), TypeId::NUMBER));

        checker.register_finalized_lib_body_for_def("AliasToken", TypeId::BOOLEAN, Some(sibling));

        assert_eq!(
            checker.ctx.definition_store.get_body(canonical),
            Some(TypeId::BOOLEAN),
            "structural lib bodies must publish through the stable canonical name entry",
        );
        assert_eq!(
            checker.ctx.definition_store.get_body(sibling),
            Some(TypeId::NUMBER),
            "a worker-local selected sibling must not become the structural finalization target",
        );
    }
// TSZ_INLINE_TEST_END 43d506c2c5240eee00c5bb609e42ba4cdba90179c21ca57747468d8b4c32c946

// TSZ_INLINE_TEST_BEGIN 385c5a9bbf71d4aa81555ce2a79f9d43f7b5dbb9e1e9b2c994a50446d1b5c8e5 502 same_basename_distinct_lazy_target_remains_a_real_alias_chain
    #[test]
    fn same_basename_distinct_lazy_target_remains_a_real_alias_chain() {
        let arena = NodeArena::default();
        let binder = BinderState::new();
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            &arena,
            &binder,
            &types,
            "same-basename-chain.ts".to_string(),
            CheckerOptions::default(),
        );
        let name = types.intern_string("AliasToken");
        let source = checker
            .ctx
            .definition_store
            .register(DefinitionInfo::type_alias(name, Vec::new(), TypeId::STRING));
        let target = checker
            .ctx
            .definition_store
            .register(DefinitionInfo::type_alias(name, Vec::new(), TypeId::NUMBER));
        let target_ref = types.lazy(target);

        checker.register_finalized_lib_body_for_def("AliasToken", target_ref, Some(source));

        assert_eq!(
            checker.ctx.definition_store.get_body(source),
            Some(target_ref),
            "a same-basename but distinct lazy target is a valid alias chain",
        );
    }
// TSZ_INLINE_TEST_END 385c5a9bbf71d4aa81555ce2a79f9d43f7b5dbb9e1e9b2c994a50446d1b5c8e5
