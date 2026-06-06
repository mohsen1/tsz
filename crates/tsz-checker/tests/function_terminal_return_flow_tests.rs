use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn diagnostic_codes(source: &str, no_implicit_returns: bool) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            no_implicit_returns,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn terminal_value_return_suppresses_no_implicit_returns() {
    let codes = diagnostic_codes(
        r#"
function renamedTerminal(flag: boolean) {
    if (flag) {
        return 1;
    }
    return 2;
}
"#,
        true,
    );

    assert!(
        !codes.contains(&7030),
        "terminal value return should prove the function does not fall through; got {codes:?}"
    );
}

#[test]
fn nonterminal_partial_return_still_reports_no_implicit_returns() {
    let codes = diagnostic_codes(
        r#"
function renamedPartial(flag: boolean) {
    if (flag) {
        return 1;
    }
    flag;
}
"#,
        true,
    );

    assert!(
        codes.contains(&7030),
        "partial return without a terminal return/throw must still run fallthrough analysis; got {codes:?}"
    );
}

#[test]
fn terminal_throw_satisfies_declared_number_return_completeness() {
    let codes = diagnostic_codes(
        r#"
function renamedThrow(): number {
    throw 1;
}
"#,
        false,
    );

    assert!(
        !codes.iter().any(|code| matches!(code, 2355 | 2366)),
        "terminal throw should not emit return-completeness diagnostics; got {codes:?}"
    );
}

#[test]
fn terminal_bare_return_preserves_unknown_return_completeness() {
    let codes = diagnostic_codes(
        r#"
function renamedUnknown(flag: boolean): unknown {
    if (flag) {
        return 1;
    }
    return;
}
"#,
        false,
    );

    assert!(
        !codes.contains(&2355),
        "terminal bare return should not look like an empty falling-through unknown body; got {codes:?}"
    );
}
