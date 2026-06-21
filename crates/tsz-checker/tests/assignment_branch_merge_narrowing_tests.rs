use tsz_checker::test_utils::check_source_strict_codes;

const TS2322: u32 = 2322;

fn codes(source: &str) -> Vec<u32> {
    check_source_strict_codes(source)
}

#[test]
fn branch_assignment_preserves_defined_local_after_intervening_read() {
    let diagnostics = codes(
        r#"
declare const arr: (number | undefined)[];
declare function maybeNumber(): number | undefined;

function f() {
    let value = maybeNumber();
    if (!value) {
        value = 5;
        arr.push(value);
    }
    const z: number = value;
}
"#,
    );

    assert!(
        !diagnostics.contains(&TS2322),
        "branch assignment should narrow value to number after the merge, got {diagnostics:?}"
    );
}

#[test]
fn branch_without_assignment_still_keeps_possibly_undefined() {
    let diagnostics = codes(
        r#"
declare function maybeNumber(): number | undefined;

function f() {
    let value = maybeNumber();
    if (!value) {
        value;
    }
    const z: number = value;
}
"#,
    );

    assert!(
        diagnostics.contains(&TS2322),
        "without a branch assignment the value can still be undefined, got {diagnostics:?}"
    );
}

#[test]
fn branch_array_literal_assignment_narrows_returned_let_binding() {
    // deepkit-type repro for issue #14219: an array-literal RHS reassignment in a
    // guard must kill the `undefined` member of the `let` binding so the inferred
    // return type is `string[]`, not `string[] | undefined`.
    let diagnostics = codes(
        r#"
declare function maybeLabels(): string[] | undefined;

function getLabels() {
    let value = maybeLabels();
    if (!value) {
        value = ["a"];
    }
    return value;
}

const n: number = getLabels().length;
"#,
    );

    assert!(
        !diagnostics.contains(&TS2322),
        "array-literal branch assignment should narrow value to string[], got {diagnostics:?}"
    );
}

#[test]
fn branch_object_literal_assignment_narrows_returned_let_binding() {
    // Object-literal sibling of #14219: the object-literal RHS must also narrow
    // away `undefined` through the flow fallback resolver.
    let diagnostics = codes(
        r#"
declare function maybeConfig(): { a: number } | undefined;

function getConfig() {
    let value = maybeConfig();
    if (!value) {
        value = { a: 1 };
    }
    return value;
}

const n: number = getConfig().a;
"#,
    );

    assert!(
        !diagnostics.contains(&TS2322),
        "object-literal branch assignment should narrow value to {{ a: number }}, got {diagnostics:?}"
    );
}
