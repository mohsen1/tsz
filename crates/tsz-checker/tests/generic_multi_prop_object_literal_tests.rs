use tsz_checker::test_utils::check_source_diagnostics;

/// When a generic interface has multiple type parameters and an object literal
/// is assigned with all-wrong types, tsc reports errors on EVERY mismatching
/// property — not just the last one iterated. This covers the class of bugs
/// where an early-exit after the first error silenced subsequent ones.
#[test]
fn test_generic_multi_prop_all_errors_reported() {
    let source = r#"
interface Foo<K, V extends K = K> {
    first: K
    second: V
}
const d: Foo<string, string> = { first: 1, second: 2 }
"#;
    let diagnostics: Vec<_> = check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .collect();
    assert_eq!(
        diagnostics.len(),
        2,
        "Expected 2 TS2322 errors (one for first:1, one for second:2), got: {diagnostics:#?}"
    );
}

/// Renamed type parameters: same rule applies regardless of parameter name.
#[test]
fn test_generic_multi_prop_renamed_params_all_errors_reported() {
    let source = r#"
interface Pair<A, B extends A = A> {
    left: A
    right: B
}
const p: Pair<string, string> = { left: 1, right: 2 }
"#;
    let diagnostics: Vec<_> = check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .collect();
    assert_eq!(
        diagnostics.len(),
        2,
        "Expected 2 TS2322 errors (one for left:1, one for right:2), got: {diagnostics:#?}"
    );
}

/// Three-property variant: all three mismatches must be reported.
#[test]
fn test_generic_three_prop_all_errors_reported() {
    let source = r#"
interface Triple<X> {
    a: X
    b: X
    c: X
}
const t: Triple<string> = { a: 1, b: 2, c: 3 }
"#;
    let diagnostics: Vec<_> = check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .collect();
    assert_eq!(
        diagnostics.len(),
        3,
        "Expected 3 TS2322 errors, got: {diagnostics:#?}"
    );
}

/// Single-property generic: still exactly one error.
#[test]
fn test_generic_single_prop_one_error() {
    let source = r#"
interface Bar<T> {
    x: T
}
const b: Bar<string> = { x: 1 }
"#;
    let diagnostics: Vec<_> = check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .collect();
    assert_eq!(
        diagnostics.len(),
        1,
        "Expected 1 TS2322 error for single-prop generic, got: {diagnostics:#?}"
    );
}

/// Non-generic interface: baseline must remain at 2 errors.
#[test]
fn test_non_generic_multi_prop_all_errors_reported() {
    let source = r#"
interface Baz {
    first: string
    second: string
}
const c: Baz = { first: 1, second: 2 }
"#;
    let diagnostics: Vec<_> = check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .collect();
    assert_eq!(
        diagnostics.len(),
        2,
        "Expected 2 TS2322 errors for non-generic multi-prop, got: {diagnostics:#?}"
    );
}
