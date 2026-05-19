use crate::test_utils::check_source_strict;

#[test]
fn partial_record_indexed_access_accepts_template_value() {
    let diagnostics = check_source_strict(
        r#"
type Partial<T> = { [P in keyof T]?: T[P] };
type Record<K extends keyof any, V> = { [P in K]: V };

function first<T, K extends keyof T>() {
    let x: Partial<Record<keyof T, string>>[K] = "hello";
}

function renamed<U, X extends keyof U>() {
    let value: Partial<Record<keyof U, number>>[X] = 1;
}
"#,
    );

    assert!(
        !diagnostics.iter().any(|diag| diag.code == 2322),
        "mapped indexed access should accept values assignable to its template, got: {diagnostics:?}"
    );
}

#[test]
fn mapped_indexed_access_with_generic_key_remains_callable() {
    let diagnostics = check_source_strict(
        r#"
class Form<T> {
    private childFormFactories!: { [K in keyof T]: (v: T[K]) => Form<T[K]> };

    public set<K extends keyof T>(prop: K, value: T[K]) {
        this.childFormFactories[prop](value);
    }
}

class Renamed<U> {
    private handlers!: { [X in keyof U]: (value: U[X]) => U[X] };

    public run<X extends keyof U>(key: X, value: U[X]) {
        return this.handlers[key](value);
    }
}
"#,
    );

    assert!(
        !diagnostics.iter().any(|diag| diag.code == 2349),
        "mapped indexed access with a matching generic key should remain callable, got: {diagnostics:?}"
    );
}

#[test]
fn nested_generic_indexed_access_write_accepts_same_surface() {
    let diagnostics = check_source_strict(
        r#"
function assignTo<T, K1 extends keyof T, K2 extends keyof T[K1]>(
    object: T,
    key1: K1,
    key2: K2,
) {
    return (value: T[K1][K2]) => object[key1][key2] = value;
}

function renamed<U, A extends keyof U, B extends keyof U[A]>(
    item: U,
    first: A,
    second: B,
) {
    return (next: U[A][B]) => item[first][second] = next;
}
"#,
    );

    assert!(
        !diagnostics
            .iter()
            .any(|diag| diag.code == 2322 || diag.code == 2719),
        "equivalent nested indexed-access write surfaces should not report self-mismatches, got: {diagnostics:?}"
    );
}

#[test]
fn distinct_generic_indexed_access_surfaces_still_report_mismatch() {
    let diagnostics = check_source_strict(
        r#"
function mismatch<T, U, K extends keyof T, L extends keyof U>(
    source: T[K],
    target: U[L],
) {
    target = source;
}
"#,
    );

    assert!(
        diagnostics.iter().any(|diag| diag.code == 2322),
        "distinct generic indexed-access surfaces should still report TS2322, got: {diagnostics:?}"
    );
}
