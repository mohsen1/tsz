//! Regression tests for [issue #7653]: inference from a parameterless lambda
//! must seed type parameters consumed by a sibling context-sensitive callback.
//!
//! Structural rule: a function expression with no parameters, no explicit
//! return-type annotation, and an expression body is context-sensitive only
//! when its body is itself context-sensitive (per tsc's `isContextSensitive`
//! / `hasContextSensitiveReturnExpression`). Literal kinds and plain calls
//! are not context-sensitive on their own.
//!
//! When the rule above misclassifies `() => 'hi'` as sensitive, Round 1
//! generic inference cannot use it to seed `T`. The sibling lambda
//! `n => n.length` then has `n: unknown` and the post-generic
//! uninferred-callback recheck emits a false TS18046.
//!
//! Two cooperating fixes pin this rule end-to-end:
//!
//! 1. `function_body_needs_contextual_return_type` (non-block branch) now
//!    calls `is_contextually_sensitive` instead of the broader
//!    `expression_needs_contextual_return_type`, matching tsc.
//!
//! 2. `argument_provides_type_param_evidence` now evaluates the sibling
//!    callback's expected param type before reading its contextual
//!    signature, so an `Application` like `Make<T>` resolves to its `(): T`
//!    call shape and is correctly seen as providing evidence for `T`.
//!
//! [issue #7653]: https://github.com/mohsen1/tsz/issues/7653

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
}

fn assert_no_ts18046(diagnostics: &[(u32, String)], context: &str) {
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 18046),
        "Did not expect TS18046 in {context}. Got: {diagnostics:?}"
    );
}

#[test]
fn parameterless_string_literal_lambda_seeds_t_for_sibling_callback() {
    // Exact fixture from `inferenceFromParameterlessLambda.ts`.
    let diagnostics = compile_and_get_diagnostics(
        r#"
// @target: es2015
function foo<T>(o: Take<T>, i: Make<T>) { }
interface Make<T> { (): T; }
interface Take<T> { (n: T): void; }
foo(n => n.length, () => 'hi');
"#,
    );
    assert_no_ts18046(
        &diagnostics,
        "the canonical inferenceFromParameterlessLambda fixture",
    );
}

#[test]
fn parameterless_lambda_seeds_t_with_renamed_type_parameter() {
    // Same shape with `U` instead of `T` — proves the rule is structural, not
    // keyed on a specific identifier (per CLAUDE.md §25/§26).
    let diagnostics = compile_and_get_diagnostics(
        r#"
function foo<U>(o: (n: U) => void, i: () => U) { }
foo(n => n.length, () => 'hi');
"#,
    );
    assert_no_ts18046(&diagnostics, "renamed type parameter `U`");
}

#[test]
fn parameterless_lambda_seeds_t_when_listed_before_callback_param() {
    // Swap the argument order so the parameterless lambda is encountered
    // first. The rule must still hold regardless of position.
    let diagnostics = compile_and_get_diagnostics(
        r#"
function foo<T>(i: () => T, o: (n: T) => void) { }
foo(() => 'hi', n => n.length);
"#,
    );
    assert_no_ts18046(&diagnostics, "parameterless lambda in first position");
}

#[test]
fn parameterless_lambda_with_non_sensitive_return_kinds_seeds_t() {
    // Each case asserts the same rule (`() => <non-sensitive>` seeds `T` for
    // the sibling callback) across the distinct return-body shapes that
    // `expression_needs_contextual_return_type` used to wrongly flag as
    // context-sensitive: literals of every primitive kind, plain object and
    // array literals, and non-sensitive call expressions.
    let cases: &[(&str, &str, &str)] = &[
        // (label, sibling callback body, parameterless lambda body)
        ("numeric literal", "n.toFixed(2)", "42"),
        ("boolean literal", "n ? 1 : 0", "true"),
        ("template literal", "n.length", "`hi`"),
        ("bigint literal", "n.toString()", "1n"),
        ("object literal", "n.x", "({ x: 1 })"),
        ("plain call expression", "n.length", "getString()"),
    ];
    for (label, callback_body, return_body) in cases {
        let source = format!(
            "declare function getString(): string;\n\
             function foo<T>(o: (n: T) => void, i: () => T) {{ }}\n\
             foo(n => {callback_body}, () => {return_body});\n"
        );
        let diagnostics = compile_and_get_diagnostics(&source);
        assert_no_ts18046(&diagnostics, label);
    }

    // Array literal needs `T[]` rather than `T` so the literal `['a', 'b']`
    // is a valid sibling for the array-accessing callback.
    let diagnostics = compile_and_get_diagnostics(
        r#"
function foo<T>(o: (n: T[]) => void, i: () => T[]) { }
foo(n => n[0].length, () => ['a', 'b']);
"#,
    );
    assert_no_ts18046(&diagnostics, "array literal return body");
}

#[test]
fn nested_context_sensitive_return_is_still_sensitive() {
    // A returned arrow with an unannotated parameter is itself
    // context-sensitive but still provides evidence for `T` via its own
    // return-position literal, so the outer call must accept the sibling
    // callback without TS18046.
    let diagnostics = compile_and_get_diagnostics(
        r#"
function foo<T>(o: (n: T) => void, i: () => (x: any) => T) { }
foo(n => n.length, () => (x) => 'hi');
"#,
    );
    assert_no_ts18046(&diagnostics, "nested-arrow context-sensitive return body");
}

#[test]
fn parameterless_lambda_through_aliased_interface_seeds_t() {
    // `Make<T>` is an `Application`; resolving its `(): T` contextual
    // signature requires evaluating the param type. The
    // `argument_provides_type_param_evidence` fix is what makes this case
    // work — without `evaluate_type_with_env` / `evaluate_application_type`
    // fallbacks, the sibling `()=>'hi'` is wrongly treated as providing no
    // evidence for `T`.
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Make<T> = { (): T };
type Take<T> = { (n: T): void };
function foo<T>(o: Take<T>, i: Make<T>) { }
foo(n => n.length, () => 'hi');
"#,
    );
    assert_no_ts18046(&diagnostics, "aliased `Make<T>` interface");
}

#[test]
fn block_body_with_literal_return_is_not_context_sensitive() {
    // Regression guard for the block-body path that was already correct.
    // `() => { return 'hi'; }` must continue to seed T just like
    // `() => 'hi'` now does.
    let diagnostics = compile_and_get_diagnostics(
        r#"
function foo<T>(o: (n: T) => void, i: () => T) { }
foo(n => n.length, () => { return 'hi'; });
"#,
    );
    assert_no_ts18046(
        &diagnostics,
        "block-bodied parameterless lambda with literal return",
    );
}

#[test]
fn block_body_returning_context_sensitive_arrow_is_still_sensitive() {
    // Regression guard: a block-bodied function that returns a
    // context-sensitive arrow must stay sensitive so the inner arrow
    // receives its contextual parameter type in Round 2.
    let diagnostics = compile_and_get_diagnostics(
        r#"
function map<T, U>(arr: T[], f: (t: T) => U): U[] { return [] as any; }
map([1, 2, 3], (t) => {
    return (x: number) => t + x;
});
"#,
    );
    // The fix should NOT regress this — `t` must remain typed as `number`,
    // not `unknown`, inside the block-bodied wrapper.
    assert_no_ts18046(
        &diagnostics,
        "block-bodied wrapper returning context-sensitive arrow",
    );
}

#[test]
fn callback_with_unannotated_param_still_emits_when_t_truly_uninferred() {
    // Negative case: when nothing in the call provides evidence for `T`,
    // the sibling callback `(t: T) => void` does NOT help (its parameter
    // position is contravariant). The post-generic recheck SHOULD still
    // fire so `T` is replaced with its constraint/default and the body
    // sees the right diagnostic. This locks in that we have not made the
    // evidence check too permissive.
    let diagnostics = compile_and_get_diagnostics(
        r#"
function foo<T>(o: (n: T) => void, i: (t: T) => void) { }
foo(n => n.length, t => { });
"#,
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 18046),
        "Expected TS18046 for genuinely uninferred T (both args are callbacks; \
         neither return position contains T). Got: {diagnostics:?}"
    );
}
