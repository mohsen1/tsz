//! `Awaited<X>` must fold the same way whether it appears directly or behind
//! an intermediate alias. Regression coverage for issue #5824.

use std::sync::{Arc, OnceLock};
use tsz_binder::lib_loader::LibFile;
use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{check_source_codes, check_source_with_libs, load_default_lib_files};

/// Codes from a check run against the full default lib bundle, so global types
/// like `Awaited`, `Promise`, and `PromiseLike` resolve (the bare
/// `check_source_codes` harness binds no libs).
fn lib_codes(source: &str) -> Vec<u32> {
    static LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    let libs = LIBS.get_or_init(load_default_lib_files);
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), libs)
        .iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn awaited_union_with_promise_folds_to_value() {
    let source = r#"
function checkB<T>(x: Awaited<T | Promise<T>>): T {
    return x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "Awaited<T | Promise<T>> must fold to T; got: {codes:?}"
    );
}

#[test]
fn awaited_union_with_promise_through_alias_folds_to_value() {
    let source = r#"
type _A<T> = Awaited<T | Promise<T>>;
function checkA<T>(x: _A<T>): T {
    return x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "Awaited<T | Promise<T>> wrapped in an alias must still fold to T; got: {codes:?}"
    );
}

#[test]
fn awaited_through_inner_union_alias_folds_to_value() {
    let source = r#"
type _U<T> = T | Promise<T>;
function checkU<T>(x: Awaited<_U<T>>): T {
    return x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "Awaited<X> must evaluate X (an alias to a value-or-promise union) before folding; \
         got: {codes:?}"
    );
}

#[test]
fn awaited_promise_through_alias_folds_to_value() {
    let source = r#"
type _AP<T> = Awaited<Promise<T>>;
function checkAP<T>(x: _AP<T>): T {
    return x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "Awaited<Promise<T>> wrapped in an alias must still fold to T; got: {codes:?}"
    );
}

#[test]
fn awaited_triple_union_folds_to_value() {
    let source = r#"
type _Triple<T> = Awaited<T | Promise<T> | PromiseLike<T>>;
function checkTriple<T>(x: _Triple<T>): T {
    return x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "Awaited over a value | Promise | PromiseLike union must fold to T; got: {codes:?}"
    );
}

#[test]
fn await_distributes_over_renamed_type_parameter_via_alias() {
    // Same shape as the issue repro: the type-parameter name (`Value` vs `T`)
    // must not matter — the fold is structural.
    let source = r#"
async function process<Value>(input: () => Value | Promise<Value>): Promise<Value> {
    const out: Value = await input();
    return out;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "await over `Value | Promise<Value>` must produce Value regardless of the parameter \
         name; got: {codes:?}"
    );
}

#[test]
fn awaited_nested_promise_through_alias_folds_to_value() {
    let source = r#"
type _Nested<T> = Awaited<Promise<T | Promise<T>>>;
function checkNested<T>(x: _Nested<T>): T {
    return x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "Awaited<Promise<T | Promise<T>>> must recursively unwrap to T; got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Cyclic thenables must report TS2589, not fold to a finite value.
//
// `Awaited<X>` over a thenable whose `then` callback value transitively yields
// the thenable again is infinitely deep — tsc reports TS2589. The fold's cycle
// detection must span the *mutual* case (`A -> B -> A`), not only the direct
// self-cycle (`A -> A`), and it must be structural: the interface names are
// irrelevant. Regression coverage for issue #14141 (removal of the
// `align_awaited_type_instantiation_diagnostics` conformance-fixture rewrite;
// the `awaitedTypeStrictNull` fixture now matches tsc natively).

#[test]
fn direct_self_referential_thenable_reports_ts2589() {
    let source = r#"
interface SelfThenable { then(cb: (value: SelfThenable) => void): void; }
type Unwrapped = Awaited<SelfThenable>;
declare const _u: Unwrapped;
"#;
    let codes = lib_codes(source);
    assert!(
        codes.contains(&2589),
        "a directly self-referential thenable must report TS2589; got: {codes:?}"
    );
}

#[test]
fn mutually_recursive_thenable_two_cycle_reports_ts2589() {
    // Ping -> Pong -> Ping. Deliberately not named `BadPromise1`/`BadPromise2`
    // (the old rewrite keyed on those literals) to prove the fix is structural.
    let source = r#"
interface Ping { then(cb: (value: Pong) => void): void; }
interface Pong { then(cb: (value: Ping) => void): void; }
type Unwrapped = Awaited<Ping>;
declare const _u: Unwrapped;
"#;
    let codes = lib_codes(source);
    assert!(
        codes.contains(&2589),
        "a mutually recursive thenable (2-cycle) must report TS2589; got: {codes:?}"
    );
}

#[test]
fn mutually_recursive_thenable_three_cycle_reports_ts2589() {
    let source = r#"
interface Alpha { then(cb: (value: Beta) => void): void; }
interface Beta { then(cb: (value: Gamma) => void): void; }
interface Gamma { then(cb: (value: Alpha) => void): void; }
type Unwrapped = Awaited<Alpha>;
declare const _u: Unwrapped;
"#;
    let codes = lib_codes(source);
    assert!(
        codes.contains(&2589),
        "a mutually recursive thenable (3-cycle) must report TS2589; got: {codes:?}"
    );
}

#[test]
fn renamed_binders_mutual_cycle_reports_ts2589() {
    // Same shape, arbitrary names — the fold must not depend on any identifier.
    let source = r#"
interface Zeta { then(cb: (value: Omega) => void): void; }
interface Omega { then(cb: (value: Zeta) => void): void; }
type Resolved = Awaited<Zeta>;
declare const _r: Resolved;
"#;
    let codes = lib_codes(source);
    assert!(
        codes.contains(&2589),
        "a mutually recursive thenable must report TS2589 regardless of binder names; got: {codes:?}"
    );
}

#[test]
fn finite_nested_promise_does_not_report_ts2589() {
    // A genuinely finite unwrap chain must never be mistaken for a cycle.
    let source = r#"
type A = Awaited<Promise<Promise<number>>>;
declare const a: A;
const n: number = a;
"#;
    let codes = lib_codes(source);
    assert!(
        !codes.contains(&2589),
        "a finite nested promise must not report TS2589; got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Awaited<Promise<Promise<number>>> must fold to number; got: {codes:?}"
    );
}

#[test]
fn finite_structural_thenable_does_not_report_ts2589() {
    let source = r#"
type B = Awaited<{ then(cb: (value: number, other: {}) => void): void }>;
declare const b: B;
const n: number = b;
"#;
    let codes = lib_codes(source);
    assert!(
        !codes.contains(&2589),
        "a finite (non-cyclic) structural thenable must not report TS2589; got: {codes:?}"
    );
}

#[test]
fn awaited_keeps_non_thenable_unchanged() {
    // Non-thenables must pass through Awaited untouched — the fold is not
    // allowed to discard the value.
    let source = r#"
function checkString(x: Awaited<string>): string {
    return x;
}
function checkNumber(x: Awaited<number | undefined>): number | undefined {
    return x;
}
function checkNull(x: Awaited<null>): null {
    return x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "Awaited<NonThenable> must equal NonThenable; got: {codes:?}"
    );
}
