//! Regression coverage for issue #9757.
//!
//! Structural rule: when `Array.prototype.reduce` resolves the overload with
//! an explicit initial accumulator value, the callback accumulator parameter is
//! contextually typed as the widened accumulator type, not as the initializer's
//! literal type.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_compiled_lib_files};

fn check_with_es5(source: &str) -> Vec<(u32, String)> {
    let libs = load_compiled_lib_files(&["lib.es5.d.ts"]);
    check_source_with_libs_code_messages(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .filter(|(code, _)| *code != 2318)
    .collect()
}

#[test]
fn reduce_accumulator_parameter_uses_widened_initial_value() {
    let diagnostics = check_with_es5(
        r#"
declare const values: number[];
values.reduce((acc, cur) => {
  const probe: string = acc;
  return acc + cur;
}, 0);
"#,
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "expected one TS2322 probing the accumulator type, got: {diagnostics:?}"
    );
    assert!(
        ts2322[0]
            .1
            .contains("Type 'number' is not assignable to type 'string'"),
        "accumulator should be contextually typed as widened number, got: {ts2322:?}"
    );
}

#[test]
fn reduce_right_accumulator_parameter_uses_widened_initial_value() {
    let diagnostics = check_with_es5(
        r#"
declare const values: number[];
values.reduceRight((acc, cur) => {
  const probe: string = acc;
  return acc + cur;
}, 0);
"#,
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "expected one TS2322 probing the reduceRight accumulator type, got: {diagnostics:?}"
    );
    assert!(
        ts2322[0]
            .1
            .contains("Type 'number' is not assignable to type 'string'"),
        "reduceRight accumulator should be widened to number, got: {ts2322:?}"
    );
}
