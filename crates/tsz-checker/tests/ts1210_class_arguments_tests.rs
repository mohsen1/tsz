//! Tests for TS1210 in class strict-mode JS contexts.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_js_source_codes_with_options, check_source_code_messages};

#[test]
fn class_method_local_arguments_emits_ts1210_not_ts1213() {
    let source = r#"
class A {
  m() {
    const arguments = this.arguments;
    return arguments;
  }
  get arguments() { return {}; }
}
"#;
    let codes = check_js_source_codes_with_options(
        source,
        "a.js",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        codes.contains(&1210),
        "expected TS1210 for class-local `arguments`, got: {codes:?}"
    );
    assert!(
        !codes.contains(&1213),
        "did not expect TS1213 for class-local `arguments`, got: {codes:?}"
    );
}

#[test]
fn regular_function_local_arguments_does_not_emit_ts1210() {
    let source = r#"
function f() {
  const arguments = 1;
  return arguments;
}
"#;
    let codes = check_js_source_codes_with_options(
        source,
        "a.js",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !codes.contains(&1210),
        "did not expect TS1210 outside class body, got: {codes:?}"
    );
}

#[test]
fn static_block_local_eval_and_arguments_do_not_emit_ts1210() {
    let source = r#"
class C {
  static {
    let eval = 1;
    let arguments = 1;
  }
}
"#;
    let diags = check_source_code_messages(source);
    let ts1210: Vec<_> = diags.iter().filter(|(code, _)| *code == 1210).collect();
    assert!(
        ts1210.is_empty(),
        "did not expect TS1210 for static-block local `eval`/`arguments`, got: {diags:#?}"
    );
}
