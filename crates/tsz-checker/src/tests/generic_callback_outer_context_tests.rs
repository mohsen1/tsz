//! Regression tests for issue #3768 — generic callbacks must be
//! instantiated from the outer call context.
//!
//! When a generic function value flows into a contextual function-typed
//! parameter, the outer call's inferred type arguments must be propagated
//! into the inner callback signature before relation checks. Otherwise
//! the inner callback's still-generic type parameter is compared against
//! the concrete argument and produces a false TS2345.
//!
//! Both repros below are tsc-clean. They were reported failing and are kept
//! here so that
//! the inference path from the outer context through the callback type
//! parameter cannot regress.
use crate::test_utils::check_source_diagnostics;

/// Repro 1 from issue #3768. `map("", identity)` should infer `T = ""`
/// from the first argument and instantiate `identity<V>` with `V = ""`
/// when matching the `(s: T) => U` parameter shape; `tsz` previously
/// kept `identity`'s `V` and emitted TS2345 on the first argument.
#[test]
fn generic_identity_callback_instantiates_from_outer_string_arg() {
    let diags = check_source_diagnostics(
        r#"
declare function map<T, U>(x: T, f: (s: T) => U): U;
declare function identity<V>(y: V): V;

var s = map("", identity);
"#,
    );

    let unexpected: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2345 || d.code == 2322)
        .collect();
    assert!(
        unexpected.is_empty(),
        "Expected no TS2345/TS2322 — generic callback should be instantiated from outer call inference, got: {:?}",
        unexpected
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Same shape as repro 1 but with a numeric outer argument and a
/// distinct callback type parameter name. Locks in that the fix is
/// structural rather than tied to specific identifier spellings.
#[test]
fn generic_identity_callback_instantiates_from_outer_number_arg() {
    let diags = check_source_diagnostics(
        r#"
declare function pipe<A, B>(value: A, step: (input: A) => B): B;
declare function pass<W>(z: W): W;

var n = pipe(42, pass);
"#,
    );

    let unexpected: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2345 || d.code == 2322)
        .collect();
    assert!(
        unexpected.is_empty(),
        "Expected no TS2345/TS2322 with renamed type parameters, got: {:?}",
        unexpected
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Repro 2 from issue #3768. The inner generic arrow `<T>(x: T) => x`
/// is wrapped, then passed to `call`'s `{ x: (...args: A) => T }`
/// contextual shape. The outer `T` must be inferred (here from the
/// trailing `1` argument plus `A = [number]`) without leaking the inner
/// arrow's still-generic `T` into the relation check.
#[test]
fn nested_generic_arrow_wrapped_into_rest_callback_outer_context() {
    let diags = check_source_diagnostics(
        r#"
declare function wrap<X>(x: X): { x: X };
declare function call<A extends any[], T>(x: { x: (...args: A) => T }, ...args: A): T;

const leak = call(wrap(<T>(x: T) => x), 1);
"#,
    );

    let unexpected: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2345 || d.code == 2322)
        .collect();
    assert!(
        unexpected.is_empty(),
        "Expected no TS2345/TS2322 — wrapped generic arrow must be instantiated from outer call inference, got: {:?}",
        unexpected
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
