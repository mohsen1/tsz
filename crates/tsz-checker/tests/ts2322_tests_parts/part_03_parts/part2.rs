#[test]
fn test_destructured_parameter_in_const_fn_is_not_treated_as_const() {
    // Regression: `is_const_symbol` walked past PARAMETER/ARROW_FUNCTION
    // boundaries to the enclosing `const fn = …` VARIABLE_DECLARATION,
    // wrongly classifying the parameter as const. This caused
    // `analyze_loop_fixed_point` to skip the iteration and emit stale
    // narrowed types for parameters reassigned inside loops.
    //
    // The walk must terminate when it encounters PARAMETER, FUNCTION_*,
    // CLASS_*, or SOURCE_FILE — those are scope boundaries past which the
    // symbol is no longer the variable being declared.
    let source = r#"
const fn = ({ x }: { x: number | string }) => {
    while (Math.random() < 0.5) {
        x = "next";
    }
    const y: number | string = x;
    return y;
};
"#;
    let diagnostics = get_all_diagnostics(source);
    let ts2322 = diagnostic_count(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert_eq!(
        ts2322, 0,
        "Destructured parameter in const-fn must not be skipped by fixed-point iteration; \
         got TS2322 diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_ts2322_too_many_parameters_emits_chained_target_signature_elaboration() {
    // When a function-typed source has more required parameters than the target
    // accepts, tsc emits TS2322 with a chained sub-message:
    //
    //   error TS2322: Type '...' is not assignable to type '...'.
    //     Target signature provides too few arguments. Expected N or more, but got M.
    //
    // The chained message has its own diagnostic code (TS2849), but is rendered
    // as related-information on the parent TS2322 so the final output matches
    // tsc's `messageText` chain. Without the elaboration the user only sees the
    // top-level "Type X is not assignable to Y" message, which is harder to
    // diagnose for callback / mapped-type contextual mismatches.
    let source = r#"
        type Selector<S, R> = (state: S) => R;
        const f: Selector<string, number> = (state: string, props: string) => 1;
    "#;

    let diags = diagnostics_for_source(source);
    let mismatch = diagnostics_with_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("expected TS2322, got: {diags:#?}"));

    assert!(
        mismatch
            .related_information
            .iter()
            .any(|r| r.code == diagnostic_codes::TARGET_SIGNATURE_PROVIDES_TOO_FEW_ARGUMENTS_EXPECTED_OR_MORE_BUT_GOT
                && r.message_text.contains("Target signature provides too few arguments")
                && r.message_text.contains("Expected 2 or more, but got 1")),
        "expected chained TS2849 'Target signature provides too few arguments' \
         elaboration with counts (2,1); got: {:#?}",
        mismatch.related_information
    );
}

#[test]
fn test_reverse_mapped_contextual_target_display_uses_inferred_application_args() {
    let source = r#"
        type Selector<S, R> = (state: S) => R;

        declare function createStructuredSelector<S, T>(
            selectors: {[K in keyof T]: Selector<S, T[K]>},
        ): Selector<S, T>;

        const editable = () => ({});

        const mapStateToProps = createStructuredSelector({
            editable: (state: any, props: any) => editable(),
        });
    "#;

    let diags = diagnostics_for_source(source);
    let mismatch = diagnostics_with_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("expected TS2322, got: {diags:#?}"));

    assert!(
        mismatch.message_text.contains("Selector<unknown, {}>"),
        "expected contextual target display to use inferred application args; got: {mismatch:#?}"
    );
    assert!(
        !mismatch
            .message_text
            .contains("Selector<S, T[\"editable\"]>"),
        "target display should not expose unresolved reverse-mapped type parameters; got: {mismatch:#?}"
    );
}

#[test]
fn test_reverse_mapped_contextual_target_display_is_structural_for_renamed_params() {
    let source = r#"
        type PickResult<Store, Result> = (store: Store) => Result;

        declare function buildSelectors<Store, Shape>(
            selectors: {[Key in keyof Shape]: PickResult<Store, Shape[Key]>},
        ): PickResult<Store, Shape>;

        const getTitle = () => "title";

        const selectors = buildSelectors({
            title: (store: any, extra: any) => getTitle(),
        });
    "#;

    let diags = diagnostics_for_source(source);
    let mismatch = diagnostics_with_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("expected TS2322, got: {diags:#?}"));

    assert!(
        mismatch
            .message_text
            .contains("PickResult<unknown, string>"),
        "expected contextual target display to use inferred application args; got: {mismatch:#?}"
    );
    assert!(
        !mismatch.message_text.contains("Shape[\"title\"]"),
        "target display should not expose unresolved reverse-mapped indexed access; got: {mismatch:#?}"
    );
}

#[test]
fn test_ts_nocheck_in_string_literal_does_not_suppress_ts2322() {
    // A string literal containing "@ts-nocheck" must NOT suppress checking.
    // Only a `// @ts-nocheck` or `/* @ts-nocheck */` comment in the leading
    // trivia of the file suppresses diagnostics.
    let source = r#"const marker = "@ts-nocheck";
const n: number = "not a number";
"#;
    let diags = compile_with_options(source, "test.ts", CheckerOptions::default());
    assert!(
        has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "@ts-nocheck inside a string literal must not suppress TS2322; got: {diags:?}"
    );
}

#[test]
fn test_ts_nocheck_in_real_comment_suppresses_checking() {
    // Sanity check: a genuine `// @ts-nocheck` leading comment should still
    // suppress diagnostics (the pre-existing behaviour must be preserved).
    let source = r#"// @ts-nocheck
const n: number = "not a number";
"#;
    let diags = compile_with_options(source, "test.ts", CheckerOptions::default());
    assert!(
        !has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "// @ts-nocheck in leading comment should suppress TS2322; got: {diags:?}"
    );
}

#[test]
fn test_ts_nocheck_after_code_does_not_suppress_ts2322() {
    // A `// @ts-nocheck` comment that appears *after* real code is not
    // a leading-trivia directive and must not suppress subsequent errors.
    let source = r#"const marker = 1;
// @ts-nocheck
const n: number = "not a number";
"#;
    let diags = compile_with_options(source, "test.ts", CheckerOptions::default());
    assert!(
        has_diagnostic_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "@ts-nocheck after real code must not suppress TS2322; got: {diags:?}"
    );
}
