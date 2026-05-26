//! Regression tests for inline conditional return types with `infer` patterns.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source;

fn check_strict(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..Default::default()
        },
    )
    .into_iter()
    .filter(|diagnostic| diagnostic.code != 2318)
    .collect()
}

fn assert_ts2322_mentions(
    diagnostics: &[Diagnostic],
    expected_source: &str,
    expected_target: &str,
) {
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code == 2322
                && diagnostic.message_text.contains(expected_source)
                && diagnostic.message_text.contains(expected_target)
        }),
        "expected TS2322 mentioning {expected_source:?} and {expected_target:?}, got: {diagnostics:#?}"
    );
}

#[test]
fn generic_identity_return_preserves_inferred_source_control() {
    let diagnostics = check_strict(
        r#"
declare function identity<Input>(value: Input): Input;

const result = identity("hello");
const bad: number = result;
"#,
    );

    assert_ts2322_mentions(&diagnostics, "\"hello\"", "number");
}

#[test]
fn inline_generic_ref_infer_false_branch_preserves_inferred_source() {
    let diagnostics = check_strict(
        r#"
interface Box<Value> { val: Value; }

declare function unwrap<Input>(value: Input): Input extends Box<infer Inner> ? Inner : Input;

const result = unwrap("hello");
const bad: number = result;
"#,
    );

    assert_ts2322_mentions(&diagnostics, "string", "number");
}

#[test]
fn inline_generic_ref_infer_true_branch_substitutes_inferred_source() {
    let diagnostics = check_strict(
        r#"
interface Box<Value> { val: Value; }

declare function unwrap<Input>(value: Input): Input extends Box<infer Inner> ? Inner : Input;

const result = unwrap({ val: 1 });
const bad: string = result;
"#,
    );

    assert_ts2322_mentions(&diagnostics, "number", "string");
}
