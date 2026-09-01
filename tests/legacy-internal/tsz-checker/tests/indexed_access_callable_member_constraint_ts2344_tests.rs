//! Regression tests for TS2344 on a generic/deferred indexed access whose
//! member is callable, used where a callable signature constraint is required
//! (`Parameters<T[K]>`, `ReturnType<T[K]>`, …).
//!
//! Structural rule: indexed access is covariant in the object, so for any
//! `T <: C` the access `T[K]` is a subtype of `C[K]`. When the apparent
//! (base-constraint) type `C[K]` carries a call/construct signature, `T[K]` is
//! callable too and must satisfy a `(...args: any) => any` constraint — even
//! when `C` is an interface (`Lazy`), a hybrid call-signature-plus-properties
//! interface, or the object is a deferred conditional that still mentions an
//! `infer` variable. Conversely, when `C[K]` is not provably callable (a plain
//! non-callable member, or a generic key that distributes to a non-callable
//! value) tsz must still emit TS2344, matching `tsc`.
//!
//! Witnesses: zustand `ReduxDevtoolsExtension['connect']` (issue #14164,
//! Repro A) and the reduced `T['connect']` generic form.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

/// Type-check `source` with the default lib (so `Parameters` / `ReturnType`
/// resolve) under strict mode, returning the emitted diagnostic codes.
fn check(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .iter()
    .map(|d| d.code)
    .collect()
}

macro_rules! assert_code {
    ($codes:ident, $code:literal, $msg:literal) => {
        assert!($codes.contains(&$code), concat!($msg, " Got: {:?}"), $codes)
    };
}

macro_rules! assert_no_code {
    ($codes:ident, $code:literal, $msg:literal) => {
        assert!(
            !$codes.contains(&$code),
            concat!($msg, " Got: {:?}"),
            $codes
        )
    };
}

// ---------------------------------------------------------------------------
// 1. Generic `T[K]` whose constraint member is callable satisfies a callable
//    signature constraint (positive — no TS2344).
// ---------------------------------------------------------------------------

#[test]
fn generic_type_param_indexed_callable_member_satisfies_parameters_no_ts2344() {
    let codes = check(
        r#"
interface Hybrid {
    (config?: { type?: string }): unknown;
    connect: (preConfig: { type?: string }) => { send: (a: unknown) => void };
}
function probe<T extends Hybrid>(): Parameters<T['connect']>[0] {
    return null as any;
}
"#,
    );
    assert_no_code!(
        codes,
        2344,
        "`T['connect']` where T extends a hybrid interface must satisfy `(...args)=>any`."
    );
}

#[test]
fn generic_type_param_indexed_callable_member_renamed_satisfies_return_type_no_ts2344() {
    // Renamed binders (Svc/run/Q) prove the fix is structural, not name-driven.
    let codes = check(
        r#"
interface Svc { run: (n: number) => string }
function probe<Q extends Svc>(): ReturnType<Q['run']> {
    return 'x' as any;
}
"#,
    );
    assert_no_code!(
        codes,
        2344,
        "renamed `Q['run']` callable member must satisfy a callable constraint."
    );
}

// ---------------------------------------------------------------------------
// 2. Deferred conditional object whose resolved member is callable
//    (zustand Repro A — `(Win extends {x?: infer T} ? T : ...)['connect']`).
// ---------------------------------------------------------------------------

#[test]
fn conditional_infer_object_indexed_callable_member_satisfies_parameters_no_ts2344() {
    let codes = check(
        r#"
interface ReduxDevtoolsExtension {
    (config?: { type?: string }): unknown;
    connect: (preConfig: { type?: string }) => { send: (a: unknown) => void };
}
interface Win { ext?: ReduxDevtoolsExtension }
type Config = Parameters<
    (Win extends { ext?: infer T } ? T : { connect: (param: any) => unknown })['connect']
>[0];
const c: Config = { type: 'x' };
export { c };
"#,
    );
    assert_no_code!(
        codes,
        2344,
        "indexing a conditional that resolves to a hybrid interface must keep its callable member."
    );
}

#[test]
fn conditional_infer_object_indexed_callable_member_renamed_no_ts2344() {
    let codes = check(
        r#"
interface Plugin {
    (cfg?: unknown): void;
    open: (name: string) => number;
}
interface Host { slot?: Plugin }
type Opened = Parameters<
    (Host extends { slot?: infer P } ? P : { open: (k: any) => unknown })['open']
>[0];
const o: Opened = 'name';
export { o };
"#,
    );
    assert_no_code!(
        codes,
        2344,
        "renamed conditional-resolved hybrid interface member must satisfy a callable constraint."
    );
}

// ---------------------------------------------------------------------------
// 3. Wrapper / alias nesting around the constraint (positive — no TS2344).
// ---------------------------------------------------------------------------

#[test]
fn wrapper_aliased_constraint_indexed_callable_member_no_ts2344() {
    let codes = check(
        r#"
interface Box { fn: (s: string) => number }
type Wrap<T> = T;
function probe<B extends Wrap<Box>>(x: Parameters<B['fn']>[0]): string {
    return x;
}
"#,
    );
    assert_no_code!(
        codes,
        2344,
        "a wrapper-aliased constraint must still expose its callable member through `B['fn']`."
    );
}

// ---------------------------------------------------------------------------
// 4. Negative: a non-callable member still emits TS2344 (parity with tsc).
// ---------------------------------------------------------------------------

#[test]
fn non_callable_indexed_member_still_emits_ts2344() {
    let codes = check(
        r#"
interface Data { value: number }
type Bad = Parameters<Data['value']>;
export type { Bad };
"#,
    );
    assert_code!(
        codes,
        2344,
        "indexing a non-callable member (`Data['value']` is number) must still emit TS2344."
    );
}

#[test]
fn non_callable_indexed_member_renamed_still_emits_ts2344() {
    let codes = check(
        r#"
interface Record1 { count: bigint }
type Bad = ReturnType<Record1['count']>;
export type { Bad };
"#,
    );
    assert_code!(
        codes,
        2344,
        "renamed non-callable member must still emit TS2344."
    );
}

#[test]
fn generic_key_into_mixed_object_not_provably_callable_emits_ts2344() {
    // `M[K]` distributes to `((n:number)=>void) | string`, which is not
    // callable as a union — tsc emits TS2344, so tsz must too.
    let codes = check(
        r#"
interface Mix { a: (n: number) => void; b: string }
function neg<M extends Mix, K extends keyof M>(x: Parameters<M[K]>) { return x }
"#,
    );
    assert_code!(
        codes,
        2344,
        "a generic key into a mixed callable/non-callable object is not provably callable."
    );
}
