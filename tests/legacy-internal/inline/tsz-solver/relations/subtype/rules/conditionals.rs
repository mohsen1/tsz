//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/relations/subtype/rules/conditionals.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 831b16e00d7a5f605476b68a964121f9e8a51bb51594339950428685d996e28d 873 conditional_identity_fallback_inherits_cycle_and_depth_policies
    #[test]
    fn conditional_identity_fallback_inherits_cycle_and_depth_policies() {
        let interner = TypeInterner::new();
        let checker = SubtypeChecker::new(&interner)
            .with_assume_related_on_cycle(false)
            .with_assume_related_on_depth(false);

        let fallback = checker.conditional_identity_fallback_checker();

        assert!(!fallback.assume_related_on_cycle);
        assert!(!fallback.assume_related_on_depth);
    }
// TSZ_INLINE_TEST_END 831b16e00d7a5f605476b68a964121f9e8a51bb51594339950428685d996e28d
