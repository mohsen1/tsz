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
