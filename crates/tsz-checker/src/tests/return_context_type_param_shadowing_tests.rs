//! Return-context inference when an enclosing function's type parameter
//! shares its name with the callee's type parameter.
//!
//! Structural rule: `tsc` identifies type parameters by symbol, never by
//! name. When an `async` function `execute<T>(): Promise<T>` returns a
//! generic call `provide<T>(..): Promise<T>`, the contextual return type
//! (`T | PromiseLike<T> | Promise<T>`) mentions the *enclosing* `T`, which is
//! a distinct entity from the callee's `T`. The name-keyed blocking in the
//! return-context substitution used to refuse the legitimate binding
//! (callee `T` := enclosing `T`) and later bind callee `T` to a lib
//! `Promise.then` signature parameter instead, producing
//! `Type 'Promise<TResult2>' is not assignable to type 'T'`
//! (kysely.ts 707/717/751/776, tracker #10663).

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn diagnostic_summaries(source: &str) -> Vec<String> {
    // These fixtures reference `Promise`/`PromiseLike`, so the check must run
    // with the standard lib loaded. Filter TS2318 missing-default-lib noise so
    // the assertions see only the semantic diagnostics.
    let lib_files = load_default_lib_files();
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &lib_files)
        .into_iter()
        .filter(|diagnostic| diagnostic.code != 2318)
        .map(|diagnostic| format!("TS{}: {}", diagnostic.code, diagnostic.message_text))
        .collect()
}

#[test]
fn async_return_of_generic_call_with_shadowed_type_param_is_clean() {
    let diags = diagnostic_summaries(
        r#"
/// <reference lib="es2015.promise" />
declare function provide<T>(p: Promise<T>): Promise<T>;

async function execute<T>(callback: () => Promise<T>): Promise<T> {
    return provide(callback());
}
"#,
    );
    assert!(
        diags.is_empty(),
        "same-named callee/enclosing type params must not block return-context inference; got {diags:?}"
    );
}

#[test]
fn async_return_of_generic_call_with_shadowed_param_async_callback_is_clean() {
    // The kysely.ts witness shape: a generic method whose async-arrow
    // consumer is contextually typed from the callee's parameter. The leaked
    // `TResult2` binding used to contaminate the arrow's contextual return
    // type, producing `Type 'T' is not assignable to type 'TResult2'`.
    let diags = diagnostic_summaries(
        r#"
/// <reference lib="es2015.promise" />
interface Runner {
    withConnection<T>(consumer: (connection: string) => Promise<T>): Promise<T>;
}

class Builder {
    declare readonly runner: Runner;

    async run<T>(callback: (db: number) => Promise<T>): Promise<T> {
        return this.runner.withConnection(async (connection) => {
            return await callback(connection.length);
        });
    }
}
"#,
    );
    assert!(
        diags.is_empty(),
        "async-arrow consumer with shadowed type param must infer through the contextual Promise; got {diags:?}"
    );
}

#[test]
fn async_return_of_generic_call_with_renamed_type_params_is_clean() {
    // Renamed-binder adjacents: the fix must not depend on the parameters
    // actually colliding.
    let diags = diagnostic_summaries(
        r#"
/// <reference lib="es2015.promise" />
declare function provideA<TInner>(p: Promise<TInner>): Promise<TInner>;
declare function provideB<T>(p: Promise<T>): Promise<T>;

async function executeA<TOuter>(callback: () => Promise<TOuter>): Promise<TOuter> {
    return provideA(callback());
}

async function executeB<TOuter>(callback: () => Promise<TOuter>): Promise<TOuter> {
    return provideB(callback());
}

async function executeC<TResult1>(callback: () => Promise<TResult1>): Promise<TResult1> {
    return provideA(callback());
}
"#,
    );
    assert!(
        diags.is_empty(),
        "renamed type-param adjacents must stay clean; got {diags:?}"
    );
}

#[test]
fn sync_return_of_generic_call_with_shadowed_type_param_is_clean() {
    // Non-async fallback: the contextual type is the plain `Promise<T>`
    // annotation rather than the async awaited union.
    let diags = diagnostic_summaries(
        r#"
/// <reference lib="es2015.promise" />
declare function provide<T>(p: Promise<T>): Promise<T>;

function execute<T>(callback: () => Promise<T>): Promise<T> {
    return provide(callback());
}
"#,
    );
    assert!(
        diags.is_empty(),
        "non-async shadowed type params must stay clean; got {diags:?}"
    );
}

#[test]
fn async_return_of_mismatched_generic_call_still_reports_ts2322() {
    // Negative case: an actually-wrong return must still fail with tsc's
    // diagnostic (`number` is not the enclosing `T`).
    let diags = diagnostic_summaries(
        r#"
/// <reference lib="es2015.promise" />
declare function provide<T>(p: Promise<T>): Promise<T>;

async function bad<T>(callback: () => Promise<T>): Promise<T> {
    return provide(Promise.resolve(123));
}
"#,
    );
    assert_eq!(
        diags.len(),
        1,
        "concrete mismatch must still be reported exactly once; got {diags:?}"
    );
    assert!(
        diags[0].starts_with("TS2322:")
            && diags[0].contains("'number'")
            && diags[0].contains("'T'"),
        "expected TS2322 number-vs-T; got {diags:?}"
    );
}
