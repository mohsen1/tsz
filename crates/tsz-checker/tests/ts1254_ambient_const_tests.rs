//! Tests for TS1254: ambient const initializer validation.
//! Boolean literals (true/false) should be accepted as valid ambient const initializers.

use tsz_checker::context::CheckerOptions;

fn get_codes(source: &str) -> Vec<u32> {
    tsz_checker::test_utils::check_source(source, "test.d.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn ts1254_not_emitted_for_true_literal() {
    let codes = get_codes("export declare const x = true;");
    assert!(
        !codes.contains(&1254),
        "TS1254 should NOT fire for `true` literal in ambient const, got: {codes:?}"
    );
}

#[test]
fn ts1254_not_emitted_for_false_literal() {
    let codes = get_codes("export declare const x = false;");
    assert!(
        !codes.contains(&1254),
        "TS1254 should NOT fire for `false` literal in ambient const, got: {codes:?}"
    );
}

#[test]
fn ts1254_not_emitted_for_string_literal() {
    let codes = get_codes(r#"export declare const x = "hello";"#);
    assert!(
        !codes.contains(&1254),
        "TS1254 should NOT fire for string literal in ambient const, got: {codes:?}"
    );
}

#[test]
fn ts1254_not_emitted_for_numeric_literal() {
    let codes = get_codes("export declare const x = 42;");
    assert!(
        !codes.contains(&1254),
        "TS1254 should NOT fire for numeric literal in ambient const, got: {codes:?}"
    );
}
