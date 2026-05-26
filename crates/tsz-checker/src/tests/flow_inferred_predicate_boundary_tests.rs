//! Focused coverage for inferred predicate guard narrowing through flow
//! query-boundary helpers.

use crate::test_utils::check_source_strict_codes as check_strict;

#[test]
fn inferred_predicate_narrows_both_call_branches() {
    let codes = check_strict(
        r#"
const isText = (candidate: string | number) => typeof candidate === "string";

function use(value: string | number) {
    if (isText(value)) {
        const text: string = value;
        text.toUpperCase();
    } else {
        const count: number = value;
        count.toFixed();
    }
}
"#,
    );

    assert!(
        !codes.contains(&2322) && !codes.contains(&2339),
        "expected inferred predicate to narrow both branches, got codes: {codes:?}"
    );
}

#[test]
fn explicit_boolean_annotation_does_not_infer_predicate() {
    let codes = check_strict(
        r#"
const isText = (candidate: string | number): boolean => typeof candidate === "string";

function use(value: string | number) {
    if (isText(value)) {
        const text: string = value;
    }
}
"#,
    );

    assert!(
        codes.contains(&2322),
        "expected explicit boolean annotation to keep value wide, got codes: {codes:?}"
    );
}
