//! Focused coverage for flow guard narrowing routed through query boundaries.

use crate::test_utils::check_source_strict_codes as check_strict;

#[test]
fn assertion_type_predicate_narrows_after_call() {
    let codes = check_strict(
        r#"
declare function assertString(value: unknown): asserts value is string;

function use(value: unknown) {
    assertString(value);
    const text: string = value;
    text.toUpperCase();
}
"#,
    );

    assert!(
        !codes.contains(&2322) && !codes.contains(&2339),
        "expected assertion predicate narrowing to make value string, got codes: {codes:?}"
    );
}

#[test]
fn instanceof_condition_narrows_both_branches() {
    let codes = check_strict(
        r#"
class Box {
    value = 1;
}

function use(value: Box | string) {
    if (value instanceof Box) {
        value.value;
    } else {
        value.toUpperCase();
    }
}
"#,
    );

    assert!(
        !codes.contains(&2339),
        "expected instanceof narrowing in both branches, got codes: {codes:?}"
    );
}
