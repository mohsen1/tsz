//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/checkers/generic_checker/mod.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 4bd3c2470d1f17e0f9e630e5721d49db5ec75c2c69f310bd05a93d6cc3394294 1667 narrowed_key_ignores_an_unreferenced_outer_scope_entry
    #[test]
    fn narrowed_key_ignores_an_unreferenced_outer_scope_entry() {
        let (mut checker, type_args) = checker_and_type_args("type X = Wrap<T>;", "Wrap");

        checker
            .ctx
            .type_parameter_scope
            .insert("T".into(), TypeId(1));
        let narrow_before = checker.type_reference_arg_validation_scope_key_for_args(&type_args);
        let full_before = checker.type_reference_arg_validation_scope_key();

        // Simulate one more level of nested generic-alias descent pushing an
        // unrelated type parameter into the ambient scope; `T`'s binding is
        // unchanged and `Wrap<T>`'s own argument never mentions `Extra`.
        checker
            .ctx
            .type_parameter_scope
            .insert("Extra".into(), TypeId(2));
        let narrow_after = checker.type_reference_arg_validation_scope_key_for_args(&type_args);
        let full_after = checker.type_reference_arg_validation_scope_key();

        assert_eq!(
            narrow_before, narrow_after,
            "an outer-scope parameter the reference's type arguments never \
             mention must not change the narrowed arg_validation cache key"
        );
        assert_ne!(
            full_before, full_after,
            "control: the full-scope key (pre-fix behavior) does change here, \
             confirming this scenario is the real memo-defeat case #15729 reports"
        );
    }
// TSZ_INLINE_TEST_END 4bd3c2470d1f17e0f9e630e5721d49db5ec75c2c69f310bd05a93d6cc3394294

// TSZ_INLINE_TEST_BEGIN 631668fb73affcb9901d32c1539fde29408112e1ce60e5c006277eeea5983eaa 1700 narrowed_key_still_changes_when_the_referenced_binding_changes
    #[test]
    fn narrowed_key_still_changes_when_the_referenced_binding_changes() {
        let (mut checker, type_args) = checker_and_type_args("type X = Wrap<T>;", "Wrap");

        checker
            .ctx
            .type_parameter_scope
            .insert("T".into(), TypeId(1));
        let before = checker.type_reference_arg_validation_scope_key_for_args(&type_args);

        checker
            .ctx
            .type_parameter_scope
            .insert("T".into(), TypeId(99));
        let after = checker.type_reference_arg_validation_scope_key_for_args(&type_args);

        assert_ne!(
            before, after,
            "the narrowed key must still change when a name the reference \
             actually uses resolves to a different type -- narrowing may only \
             drop irrelevant entries, never relevant ones"
        );
    }
// TSZ_INLINE_TEST_END 631668fb73affcb9901d32c1539fde29408112e1ce60e5c006277eeea5983eaa
