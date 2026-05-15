//! Regression tests for issue #3768.
//!
//! When a generic function value (e.g. `identity<V>`) is passed as an
//! argument to another generic call (e.g. `map<T, U>(x: T, f: (s: T) => U)`),
//! tsc instantiates the callback's signature from the outer call's
//! inference context — `identity` becomes `(y: T) => T` and the call
//! type-checks. tsz must not compare a concrete argument against the
//! callback's still-generic type parameter, which would falsely emit
//! TS2345.
//!
//! Both repros from the issue must produce zero diagnostics. The tests
//! also vary type-parameter and parameter names so a future regression
//! cannot pass by hardcoding the original spelling
//! (.claude/CLAUDE.md §25 — anti-hardcoding directive).

use tsz_checker::test_utils::check_source_diagnostics;

fn assert_no_call_argument_errors(label: &str, source: &str) {
    let diags = check_source_diagnostics(source);
    let offending: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2345)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();
    assert!(
        offending.is_empty(),
        "{label}: expected no TS2322/TS2345 from outer-call inference, got: {offending:#?}"
    );
}

#[test]
fn issue_3768_repro1_identity_passed_to_map_string() {
    // From the bug report verbatim: `map("", identity)` must type-check.
    let source = r#"
declare function map<T, U>(x: T, f: (s: T) => U): U;
declare function identity<V>(y: V): V;

var s = map("", identity);
"#;
    assert_no_call_argument_errors("repro1: map(\"\", identity)", source);
}

#[test]
fn issue_3768_repro1_alternate_param_names_no_ts2345() {
    // Same shape with different type-parameter and parameter names. A fix
    // that hardcoded `T`/`U`/`V` or any specific identifier would only
    // work on the original spelling and break here.
    let source = r#"
declare function pipe<A, B>(value: A, fn: (input: A) => B): B;
declare function passthrough<R>(value: R): R;

var n: number = pipe(42, passthrough);
var o: { foo: number } = pipe({ foo: 1 }, passthrough);
"#;
    assert_no_call_argument_errors("repro1 alt-names: pipe(value, passthrough)", source);
}

#[test]
fn issue_3768_repro1_returns_string_with_dotted_callback() {
    // Member-access form of the callback — guards a fix that depends on
    // `identity` being a bare identifier expression.
    let source = r#"
declare function map<T, U>(x: T, f: (s: T) => U): U;
declare function identity<V>(y: V): V;
var holder = { fn: identity };

var s = map("", holder.fn);
"#;
    assert_no_call_argument_errors("repro1 dotted: map(\"\", holder.fn)", source);
}

#[test]
fn issue_3768_repro2_wrap_call_spread_args() {
    // From the bug report verbatim: `call(wrap(<T>(x: T) => x), 1)`.
    // `call` infers `A = [number]` from its rest spread arg, and the
    // generic arrow inside `wrap` must be instantiated with `T = number`
    // through the same outer-call substitution. Previously tsz emitted
    // TS2345 ("Argument of type 'number' is not assignable to parameter
    // of type 'T'.").
    let source = r#"
declare function wrap<X>(x: X): { x: X };
declare function call<T, A extends any[]>(x: { x: (...args: A) => T }, ...args: A): T;

const leak = call(wrap(<T>(x: T) => x), 1);
"#;
    assert_no_call_argument_errors("repro2: call(wrap(<T>(x: T) => x), 1)", source);
}

#[test]
fn issue_3768_repro2_alternate_param_names_no_ts2345() {
    // Same structural shape — different identifier spellings. A fix that
    // hardcodes `T`/`A`/`X` names breaks here.
    let source = r#"
declare function box<I>(i: I): { i: I };
declare function dispatch<R, P extends any[]>(
    target: { i: (...p: P) => R },
    ...p: P
): R;

const leaked: number = dispatch(box(<U>(u: U) => u), 1);
const leaked2: string = dispatch(box(<U>(u: U) => u), "hi");
"#;
    assert_no_call_argument_errors("repro2 alt-names: dispatch(box(<U>(u) => u), …)", source);
}

#[test]
fn issue_3768_repro2_zero_extra_args_no_ts2345() {
    // No spread args — `A` infers to `[]`. Generic callback still needs
    // to be instantiated against the empty parameter list; previously
    // any concrete-context check that required at least one inferred
    // type would skip instantiation here.
    let source = r#"
declare function wrap<X>(x: X): { x: X };
declare function call<T, A extends any[]>(x: { x: (...args: A) => T }, ...args: A): T;

const v = call(wrap(<T>() => 1));
"#;
    assert_no_call_argument_errors("repro2 no-args: call(wrap(<T>() => 1))", source);
}

#[test]
fn issue_3768_inline_generic_arrow_directly_assigned() {
    // Repro shape adjacent to the issue: passing a generic arrow
    // directly (no `wrap` indirection) into a generic call. tsc accepts
    // this; tsz must not emit TS2345 against the still-generic `T`.
    let source = r#"
declare function call<T, A extends any[]>(f: (...args: A) => T, ...args: A): T;

const r1 = call(<T>(x: T) => x, 1);
const r2 = call(<T>(x: T) => x, "");
"#;
    assert_no_call_argument_errors("inline arrow: call(<T>(x) => x, …)", source);
}
