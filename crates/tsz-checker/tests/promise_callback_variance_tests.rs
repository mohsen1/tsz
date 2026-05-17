//! Variance regression tests for the Promise<T>-style callback pattern.
//!
//! These tests pin tsc-equivalent assignability behaviour for generic
//! interfaces whose only mention of `T` is inside a non-method callback that
//! is itself a parameter of a method. They specifically guard the variance
//! fix that distinguishes:
//!
//!   * `interface C<T> { m(x: T): void; }` — bivariant (method bivariance)
//!   * `interface C<T> { m(cb: (x: T) => void): void; }` — covariant
//!   * `interface C<T> { m(x: T, cb: (x: T) => void): void; }` — covariant
//!     (the strict callback occurrence pins the variance)
//!   * `interface Wrap<T> { container: C<T>; }` — inherits bivariance from
//!     the wrapped C<T>, so both directions remain assignable
//!
//! The first three cases are validated by the
//! `promisesWithConstraints` conformance test; the fourth is a regression
//! guard for the `inside_unreliable_application` shield introduced in the
//! variance computation.

use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::{
    check_source_code_messages as diagnostics, check_source_with_libs_code_messages, load_lib_files,
};

fn collect_codes(source: &str) -> Vec<u32> {
    diagnostics(source)
        .into_iter()
        .map(|(code, _)| code)
        .filter(|code| *code != 2318) // ignore "Cannot find global type" noise
        .collect()
}

fn collect_relevant_diagnostics(source: &str) -> Vec<(u32, String)> {
    diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect()
}

fn collect_relevant_diagnostics_with_libs(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let lib_files = load_lib_files(&[
        "es5.d.ts",
        "es2015.d.ts",
        "es2015.core.d.ts",
        "es2015.collection.d.ts",
        "es2015.iterable.d.ts",
        "es2015.generator.d.ts",
        "es2015.promise.d.ts",
        "es2015.proxy.d.ts",
        "es2015.reflect.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
    ]);
    check_source_with_libs_code_messages(source, "test.ts", options, &lib_files)
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect()
}

#[test]
fn promise_like_callback_pattern_rejects_mismatched_args() {
    // T appears only inside a non-method callback nested inside a method.
    // Variance must be COVARIANT, not bivariant.
    let codes = collect_codes(
        r#"
interface MyPromise<T> {
    then<U>(cb: (x: T) => MyPromise<U>): MyPromise<U>;
}
interface Foo { x: any; }
interface Bar { x: any; y: any; }

declare var a: MyPromise<Foo>;
declare var b: MyPromise<Bar>;
a = b; // ok: Bar <: Foo
b = a; // ERROR: Foo not <: Bar (missing y)
"#,
    );
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for `b = a` (MyPromise<Foo> -> MyPromise<Bar>), got {codes:?}"
    );
}

#[test]
fn promise_constraints_conformance_keeps_ts2322_over_complexity() {
    let diags = collect_relevant_diagnostics_with_libs(
        r#"
interface Promise<T> {
    then<U>(cb: (x: T) => Promise<U>): Promise<U>;
}

interface CPromise<T extends { x: any; }> {
    then<U extends { x: any; }>(cb: (x: T) => Promise<U>): Promise<U>;
}

interface Foo { x: any; }
interface Bar { x: any; y: any; }

var a: Promise<Foo>;
declare var b: Promise<Bar>;
a = b;
b = a;

var a2: CPromise<Foo>;
declare var b2: CPromise<Bar>;
a2 = b2;
b2 = a2;
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let codes = diags.iter().map(|(code, _)| *code).collect::<Vec<_>>();
    assert!(
        !codes.contains(&2859),
        "promisesWithConstraints should not mask TS2322 with TS2859, got {diags:#?}"
    );
    assert!(
        codes.iter().filter(|code| **code == 2322).count() == 2,
        "promisesWithConstraints should retain TS2322 diagnostics, got {diags:#?}"
    );
}

#[test]
fn mixed_method_and_callback_variance_pins_covariant() {
    // The direct-method-param T occurrence is bivariant; the nested
    // callback occurrence is strictly covariant. The strict signal must win.
    let codes = collect_codes(
        r#"
interface C<T> { m(x: T, cb: (x: T) => void): void; }
interface Foo { x: any; }
interface Bar { x: any; y: any; }

declare var a: C<Foo>;
declare var b: C<Bar>;
a = b; // ok
b = a; // ERROR: callback occurrence pins variance to covariant
"#,
    );
    assert!(
        codes.contains(&2322),
        "Expected TS2322 — callback occurrence should pin variance to covariant, got {codes:?}"
    );
}

#[test]
fn pure_method_param_variance_stays_bivariant() {
    // Regression guard: T solely as a direct method parameter must remain
    // bivariant (REJECTION_UNRELIABLE) so structural method bivariance can
    // still allow both assignment directions.
    let codes = collect_codes(
        r#"
interface C<T> { m(x: T): void; }
interface Foo { x: any; }
interface Bar { x: any; y: any; }

declare var a: C<Foo>;
declare var b: C<Bar>;
a = b;
b = a;
"#,
    );
    assert!(
        !codes.contains(&2322),
        "Pure method-param C<T> should be bivariant — both assignments must succeed, got {codes:?}"
    );
}

#[test]
fn wrapper_of_bivariant_generic_stays_bivariant() {
    // `Wrap<T>` inherits unreliability from the bivariant `C<T>` it wraps.
    // The `inside_unreliable_application` shield must keep
    // `REJECTION_UNRELIABLE` set for `Wrap<T>` so structural bivariance can
    // accept both directions.
    let codes = collect_codes(
        r#"
interface C<T> { m(x: T): void; }
interface Wrap<T> { container: C<T>; }
interface Foo { x: any; }
interface Bar { x: any; y: any; }

declare var a: Wrap<Foo>;
declare var b: Wrap<Bar>;
a = b;
b = a;
"#,
    );
    assert!(
        !codes.contains(&2322),
        "Wrap<T> wrapping a bivariant C<T> must stay bivariant, got {codes:?}"
    );
}

#[test]
fn indexed_access_bivariance_hack_stays_bivariant() {
    // Regression guard for the React-style event handler pattern:
    // extracting a method through indexed access strips the method shape, so
    // variance collection must leave the method parameter independent and let
    // structural function bivariance handle assignment.
    let codes = collect_codes(
        r#"
type EventHandler<E> = { bivarianceHack(event: E): void }["bivarianceHack"];
interface Foo { x: any; }
interface Bar { x: any; y: any; }

declare var fooHandler: EventHandler<Foo>;
declare var barHandler: EventHandler<Bar>;
fooHandler = barHandler;
barHandler = fooHandler;
"#,
    );
    assert!(
        !codes.contains(&2322),
        "indexed-access bivarianceHack handlers should be bivariant, got {codes:?}"
    );
}

#[test]
fn wrapper_of_indexed_access_bivariance_hack_stays_bivariant() {
    // The wrapper path exercises variance propagation through the extracted
    // callable alias instead of only direct assignment of the alias itself.
    let codes = collect_codes(
        r#"
type EventHandler<E> = { bivarianceHack(event: E): void }["bivarianceHack"];
interface Props<T> { onEvent: EventHandler<T>; }
interface Foo { x: any; }
interface Bar { x: any; y: any; }

declare var fooProps: Props<Foo>;
declare var barProps: Props<Bar>;
fooProps = barProps;
barProps = fooProps;
"#,
    );
    assert!(
        !codes.contains(&2322),
        "wrappers around indexed-access bivarianceHack handlers should stay bivariant, got {codes:?}"
    );
}

#[test]
fn callback_alias_application_is_checked_as_callable_parameter() {
    let diags = collect_relevant_diagnostics(
        r#"
type Fn<T> = (x: T) => T;

declare let source: (cb: Fn<string>) => void;
declare let target: (cb: Fn<number>) => void;
target = source;
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "callback alias applications should produce the outer TS2322 wrapper, got {diags:#?}"
    );
    assert!(
        diags.iter().all(|(code, _)| *code != 2328),
        "evaluated callback alias applications must not leak top-level TS2328, got {diags:#?}"
    );
}

#[test]
fn callback_interface_application_is_checked_as_callable_parameter() {
    let diags = collect_relevant_diagnostics(
        r#"
interface Fn<T> {
    (x: T): T;
}

declare let source: (cb: Fn<string>) => void;
declare let target: (cb: Fn<number>) => void;
target = source;
"#,
    );
    assert!(
        diags.iter().any(|(code, message)| {
            *code == 2322
                && message.contains("(cb: Fn<string>) => void")
                && message.contains("(cb: Fn<number>) => void")
        }),
        "diagnostic should preserve declared callable interface applications on the outer wrapper, got {diags:#?}"
    );
    assert!(
        diags.iter().all(|(code, _)| *code != 2328),
        "evaluated callback interface applications must not leak top-level TS2328, got {diags:#?}"
    );
}
