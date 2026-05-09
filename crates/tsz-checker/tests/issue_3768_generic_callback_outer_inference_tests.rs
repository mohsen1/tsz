//! Regression tests for [issue #3768]: generic callbacks must be instantiated
//! from the outer call's contextual inference, not compared against their own
//! still-generic type parameter.
//!
//! Both repros from the issue exited 0 on tsc 6.0.3 but emitted spurious
//! TS2345 on tsz at the time the issue was filed. They now pass on main; these
//! tests lock in the fix so it cannot silently regress.
//!
//! [issue #3768]: https://github.com/mohsen1/tsz/issues/3768

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
}

#[test]
fn generic_identity_callback_instantiates_from_outer_string_argument() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function map<T, U>(x: T, f: (s: T) => U): U;
declare function identity<V>(y: V): V;

var s = map("", identity);
"#,
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2345),
        "Did not expect TS2345 — `identity` must be instantiated as `(s: string) => string` from outer `T = string`. Got: {diagnostics:?}"
    );
}

#[test]
fn generic_callback_in_object_property_instantiates_from_outer_rest_args() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function wrap<X>(x: X): { x: X };
declare function call<A extends unknown[], T>(x: { x: (...args: A) => T }, ...args: A): T;

const leak = call(wrap(<T>(x: T) => x), 1);
"#,
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2345),
        "Did not expect TS2345 — the wrapped generic `<T>(x: T) => x` must be instantiated as `(x: number) => number` from outer `A = [number]`. Got: {diagnostics:?}"
    );
}
