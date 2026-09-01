//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/type_analysis/computed_helpers.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN e1a0b84f90e71ede6ac3ac6c73d284cfdafc76fe8a931f8c35e347036fd8ffb0 1210 literal_preserved_when_target_is_deferred_conditional_with_literal_branch
    /// Assigning a string literal to a deferred conditional whose false branch
    /// is that exact literal must NOT widen the source to `string`. Matches
    /// tsc's `isLiteralOfContextualType` recursing through conditional types.
    ///
    /// ```ts
    /// type Foo<T> = T extends true ? string : "a";
    /// function test<T>(x: Foo<T>) {
    ///   x = "a"; // ok — both branches accept "a"
    /// }
    /// ```
    #[test]
    fn literal_preserved_when_target_is_deferred_conditional_with_literal_branch() {
        let codes = check_source_codes(
            r#"type Foo<T> = T extends true ? string : "a";
               function test<T>(x: Foo<T>) {
                 x = "a";
               }"#,
        );
        assert!(
            !codes.contains(&2322),
            "Should not emit TS2322 when assigning matching literal to deferred conditional: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END e1a0b84f90e71ede6ac3ac6c73d284cfdafc76fe8a931f8c35e347036fd8ffb0

// TSZ_INLINE_TEST_BEGIN 2f4455b0b596597e23f28877f942f551ab607012882ad7b1ad58e39e54ccbfb8 1225 deferred_conditional_still_errors_on_widened_source
    /// Sanity check: assigning a non-matching `string` value still errors.
    #[test]
    fn deferred_conditional_still_errors_on_widened_source() {
        let codes = check_source_codes(
            r#"type Foo<T> = T extends true ? "b" : "a";
               function test<T>(x: Foo<T>, s: string) {
                 x = s;
               }"#,
        );
        assert!(
            codes.contains(&2322),
            "Should emit TS2322 when assigning a `string` value to a literal-only deferred conditional: {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 2f4455b0b596597e23f28877f942f551ab607012882ad7b1ad58e39e54ccbfb8
