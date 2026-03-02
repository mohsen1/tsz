//! Tests for TS1210 in class strict-mode JS contexts.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostic_codes_for_js(source: &str) -> Vec<u32> {
    let mut parser = ParserState::new("a.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "a.js".to_string(),
        CheckerOptions {
            strict: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    );

    checker.check_source_file(root);
    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

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
    let codes = diagnostic_codes_for_js(source);
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
    let codes = diagnostic_codes_for_js(source);
    assert!(
        !codes.contains(&1210),
        "did not expect TS1210 outside class body, got: {codes:?}"
    );
}
