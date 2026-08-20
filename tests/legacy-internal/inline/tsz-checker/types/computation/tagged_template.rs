//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/types/computation/tagged_template.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN dc69626722e57acd04380cca8386e88a44d6917ac184bde936faca5c5678a8cc 639 direct_pass_widens_round1_literal_before_seeding_sensitive_arg_context
    #[test]
    fn direct_pass_widens_round1_literal_before_seeding_sensitive_arg_context() {
        assert_no_ts2345(&format!(
            "{SINGLE_ARITY_TAG}\nvar a = tempFun`${{ x => x }}  ${{ 10 }}`;"
        ));
    }
// TSZ_INLINE_TEST_END dc69626722e57acd04380cca8386e88a44d6917ac184bde936faca5c5678a8cc

// TSZ_INLINE_TEST_BEGIN fde152702fb74fd980faba27d260d0d5d9ac18886345b803b67c70c00956b519 646 literal_widening_is_not_numeric_specific
    #[test]
    fn literal_widening_is_not_numeric_specific() {
        // The fix reuses the general-purpose `widen_round2_contextual_substitution`
        // helper, so a string-literal candidate must widen exactly like a
        // numeric one — this is not a numeric-literal special case.
        assert_no_ts2345(&format!(
            r#"{SINGLE_ARITY_TAG}
var s = tempFun`${{ x => x }} ${{ "s" }}`;"#
        ));
    }
// TSZ_INLINE_TEST_END fde152702fb74fd980faba27d260d0d5d9ac18886345b803b67c70c00956b519

// TSZ_INLINE_TEST_BEGIN 7699cf658141ad9f2f69f2d8158031124e467400417d8f1ddb5fcd30b8e8f3fd 657 parenthesized_arrow_variants_are_unaffected
    #[test]
    fn parenthesized_arrow_variants_are_unaffected() {
        assert_no_ts2345(&format!(
            "{SINGLE_ARITY_TAG}\nvar b = tempFun`${{ (x => x) }}  ${{ 10 }}`;"
        ));
        assert_no_ts2345(&format!(
            "{SINGLE_ARITY_TAG}\nvar c = tempFun`${{ ((x => x)) }} ${{ 10 }}`;"
        ));
    }
// TSZ_INLINE_TEST_END 7699cf658141ad9f2f69f2d8158031124e467400417d8f1ddb5fcd30b8e8f3fd

// TSZ_INLINE_TEST_BEGIN ae1ff5ccddbf23bcceae78b0bfd5d1027483b68562c060e0ba89b7801969ef86 667 renamed_binder_and_type_param_are_unaffected
    #[test]
    fn renamed_binder_and_type_param_are_unaffected() {
        assert_no_ts2345(
            "
function stamp<Value>(strs: TemplateStringsArray, project: (received: Value) => Value, seed: Value): Value {
    return project(seed);
}
var out = stamp`${ received => received }  ${ 10 }`;
",
        );
    }
// TSZ_INLINE_TEST_END ae1ff5ccddbf23bcceae78b0bfd5d1027483b68562c060e0ba89b7801969ef86

// TSZ_INLINE_TEST_BEGIN b99812f643e63673660d680b9b3997ca19d20edfa06b2ca138ce9648a5c30c91 679 overload_with_two_callback_params_widens_both
    #[test]
    fn overload_with_two_callback_params_widens_both() {
        // Second overload: two `(x: T) => T` callbacks before the literal.
        const TWO_CALLBACK_TAG: &str = "
function tempFun<T>(tempStrs: TemplateStringsArray, g: (x: T) => T, x: T): T;
function tempFun<T>(tempStrs: TemplateStringsArray, g: (x: T) => T, h: (y: T) => T, x: T): T;
function tempFun<T>(tempStrs: TemplateStringsArray, g: (x: T) => T, x: T): T {
    return g(x);
}
";
        assert_no_ts2345(&format!(
            "{TWO_CALLBACK_TAG}\nvar d = tempFun`${{ x => x }} ${{ x => x }} ${{ 10 }}`;"
        ));
        assert_no_ts2345(&format!(
            "{TWO_CALLBACK_TAG}\nvar e = tempFun`${{ x => x }} ${{ (x => x) }} ${{ 10 }}`;"
        ));
        assert_no_ts2345(&format!(
            "{TWO_CALLBACK_TAG}\nvar f = tempFun`${{ x => x }} ${{ ((x => x)) }} ${{ 10 }}`;"
        ));
        assert_no_ts2345(&format!(
            "{TWO_CALLBACK_TAG}\nvar g = tempFun`${{ (x => x) }} ${{ (((x => x))) }} ${{ 10 }}`;"
        ));
    }
// TSZ_INLINE_TEST_END b99812f643e63673660d680b9b3997ca19d20edfa06b2ca138ce9648a5c30c91

// TSZ_INLINE_TEST_BEGIN b5460cc96c1a74ebc98119f87eb2628aeec223f12dbea86de2867c15f15360f4 703 nullish_literal_positional_argument_is_unaffected
    #[test]
    fn nullish_literal_positional_argument_is_unaffected() {
        const TWO_CALLBACK_TAG: &str = "
function tempFun<T>(tempStrs: TemplateStringsArray, g: (x: T) => T, x: T): T;
function tempFun<T>(tempStrs: TemplateStringsArray, g: (x: T) => T, h: (y: T) => T, x: T): T;
function tempFun<T>(tempStrs: TemplateStringsArray, g: (x: T) => T, x: T): T {
    return g(x);
}
";
        assert_no_ts2345(&format!(
            "{TWO_CALLBACK_TAG}\nvar h = tempFun`${{ (x => x) }} ${{ (((x => x))) }} ${{ undefined }}`;"
        ));
    }
// TSZ_INLINE_TEST_END b5460cc96c1a74ebc98119f87eb2628aeec223f12dbea86de2867c15f15360f4

// TSZ_INLINE_TEST_BEGIN 9364b2127a0855b7c29176796f92fa0445a9f94323d95e63b4ae54e9f87109a7 717 genuine_body_mismatch_still_reports_after_widening
    #[test]
    fn genuine_body_mismatch_still_reports_after_widening() {
        // Negative control: the fix must widen the *contextual parameter type*
        // fed to the callback, not silence real errors inside its body. Once
        // `x` is correctly widened to `number`, `.length` on it is a genuine
        // TS2339, proving the widened type actually reached the callback.
        let diags = check_source_diagnostics(&format!(
            "{SINGLE_ARITY_TAG}\nvar neg = tempFun`${{ x => x.length }} ${{ 10 }}`;"
        ));
        let ts2339: Vec<_> = diags.iter().filter(|d| d.code == 2339).collect();
        assert_eq!(
            ts2339.len(),
            1,
            "expected exactly one TS2339 from `.length` on the widened `number` \
             parameter, got: {ts2339:?} (all diagnostics: {diags:?})"
        );
    }
// TSZ_INLINE_TEST_END 9364b2127a0855b7c29176796f92fa0445a9f94323d95e63b4ae54e9f87109a7
