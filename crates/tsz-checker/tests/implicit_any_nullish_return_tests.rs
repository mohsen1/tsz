use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn diagnostics_for(
    source: &str,
    strict_null_checks: bool,
) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_implicit_any: true,
            strict_null_checks,
            use_unknown_in_catch_variables: false,
            ..CheckerOptions::default()
        },
    )
}

#[test]
fn nullish_only_returns_report_ts7010_without_strict_null_checks() {
    let diagnostics = diagnostics_for(
        r#"
function f() {
  return null;
}

function g() {
  return undefined;
}

function h(flag: boolean) {
  if (flag) return null;
  return undefined;
}
"#,
        false,
    );

    let ts7010 = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 7010)
        .collect::<Vec<_>>();
    assert_eq!(
        ts7010.len(),
        3,
        "expected TS7010 for f, g, and h; got diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn nullish_only_returns_do_not_report_ts7010_with_strict_null_checks() {
    let diagnostics = diagnostics_for(
        r#"
function f() {
  return null;
}

function g() {
  return undefined;
}

function h(flag: boolean) {
  if (flag) return null;
  return undefined;
}
"#,
        true,
    );

    assert!(
        diagnostics.iter().all(|diagnostic| diagnostic.code != 7010),
        "did not expect TS7010 with strictNullChecks enabled; got diagnostics: {diagnostics:#?}"
    );
}
