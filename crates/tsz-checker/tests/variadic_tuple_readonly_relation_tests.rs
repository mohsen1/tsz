use tsz_checker::test_utils::check_source_strict;

#[test]
fn readonly_constrained_type_param_rejects_mutable_spread_tuple_assignment() {
    let diags = check_source_strict(
        r#"
function f<T extends readonly unknown[]>(t: T, m: [...T]) {
    m = t;
}
"#,
    );

    let ts2322 = diags
        .iter()
        .find(|diag| diag.code == 2322)
        .unwrap_or_else(|| {
            panic!("expected TS2322 for assigning readonly-constrained T to mutable [...T], got {diags:?}")
        });
    assert!(
        ts2322.message_text.contains("type '[...T]'"),
        "expected target display to preserve mutable variadic tuple wrapper, got {ts2322:?}"
    );
}

#[test]
fn variadic_tuple_assignment_keeps_type_param_source_display() {
    let diags = check_source_strict(
        r#"
function f<T extends string[], U extends T>(t: T, u: [...U]) {
    u = t;
}
"#,
    );

    let ts2322 = diags
        .iter()
        .find(|diag| diag.code == 2322)
        .unwrap_or_else(|| panic!("expected TS2322 for T assigned to [...U], got {diags:?}"));
    assert!(
        ts2322.message_text.contains("Type 'T' is not assignable"),
        "expected source display to preserve type parameter T, got {ts2322:?}"
    );
}
