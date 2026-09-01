//! Async context is a property of the immediately enclosing function.
//!
//! Structural rule: when a function body is entered, `tsc` decides whether the
//! nodes inside it are "in an async context" from that function's own
//! `getFunctionFlags(func) & FunctionFlags.Async` — never from how many `async`
//! functions enclose it. A plain function nested inside an `async` one is
//! therefore *not* in an async context: its return expressions keep their
//! `Promise` wrapper (`checkReturnStatement` only unwraps for
//! `FunctionFlags.Async`), and an `await` in its body is the TS1308 grammar
//! error.
//!
//! tsz used to track this as a monotonically increasing `async_depth` counter
//! that nested non-async bodies inherited, so both halves were wrong at once:
//! a false positive on the return (`Promise<T>` unwrapped to `T`, then reported
//! unassignable to the declared `Promise<T>` — issue #16053) and a false
//! negative on the `await`. Entering a body now *replaces* the flag with that
//! function's own asyncness (`CheckerContext::enter_function_async_context`).
//!
//! Every fixture below is pinned against
//! `tsc@7.0.2 --noEmit --strict --pretty false --target es2017`.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn diagnostic_summaries(source: &str) -> Vec<String> {
    // The `Promise` fixtures need the standard lib. TS2318 missing-default-lib
    // noise is filtered so the assertions see only semantic diagnostics.
    let lib_files = load_default_lib_files();
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &lib_files)
        .into_iter()
        .filter(|diagnostic| diagnostic.code != 2318)
        .map(|diagnostic| format!("TS{}: {}", diagnostic.code, diagnostic.message_text))
        .collect()
}

#[test]
fn sync_arrow_returning_promise_under_async_enclosing_function_is_clean() {
    // #16053's witness: the arrow is contextually typed from the callee's
    // `(connection: string) => Promise<T>` parameter, so its return statement
    // is checked against `Promise<number>`. The enclosing `run` is async; the
    // arrow is not. tsc: exit 0.
    let diags = diagnostic_summaries(
        r#"
/// <reference lib="es2015.promise" />
declare function withConnection<T>(consumer: (connection: string) => Promise<T>): Promise<T>;

async function run(): Promise<number> {
    return withConnection((connection) => {
        return Promise.resolve(connection.length);
    });
}
"#,
    );
    assert!(
        diags.is_empty(),
        "a non-async arrow inside an async function must not have its returned \
         Promise unwrapped; got {diags:?}"
    );
}

#[test]
fn sync_arrow_returning_promise_under_async_enclosing_function_renamed_binders_is_clean() {
    // Renamed-binder control: nothing in the rule may key off `T`,
    // `withConnection`, `connection`, or `run`.
    let diags = diagnostic_summaries(
        r#"
/// <reference lib="es2015.promise" />
declare function borrow<Elem>(take: (handle: string) => Promise<Elem>): Promise<Elem>;

async function drive(): Promise<boolean> {
    return borrow((handle) => {
        return Promise.resolve(handle.length > 0);
    });
}
"#,
    );
    assert!(
        diags.is_empty(),
        "renamed binders must behave identically to the #16053 witness; got {diags:?}"
    );
}

#[test]
fn nested_function_declaration_with_declared_promise_return_is_clean() {
    // The same defect without any contextual typing: a plain `function`
    // declaration with its own `Promise<string>` annotation, nested in an async
    // function. This travels the function-declaration body path rather than the
    // contextually-typed-arrow path, so it pins the second owner site.
    // tsc: exit 0.
    let diags = diagnostic_summaries(
        r#"
/// <reference lib="es2015.promise" />
async function outer(): Promise<number> {
    function inner(): Promise<string> {
        return Promise.resolve("x");
    }
    inner();
    return 1;
}
"#,
    );
    assert!(
        diags.is_empty(),
        "a nested non-async function declaration keeps its own Promise return; got {diags:?}"
    );
}

#[test]
fn nested_object_literal_method_with_declared_promise_return_is_clean() {
    // Method-body path (the third site that scopes the flag). tsc: exit 0.
    let diags = diagnostic_summaries(
        r#"
/// <reference lib="es2015.promise" />
async function outer(): Promise<number> {
    const obj = {
        m(): Promise<string> {
            return Promise.resolve("x");
        },
    };
    obj.m();
    return 1;
}
"#,
    );
    assert!(
        diags.is_empty(),
        "a nested non-async method keeps its own Promise return; got {diags:?}"
    );
}

#[test]
fn await_inside_nested_plain_function_reports_ts1308() {
    // The negative half of the same invariant: inheriting the outer async flag
    // also *suppressed* a real grammar error. tsc reports TS1308 here.
    let diags = diagnostic_summaries(
        r#"
/// <reference lib="es2015.promise" />
async function outer(): Promise<number> {
    function inner(): number {
        return await 1;
    }
    return inner();
}
"#,
    );
    assert!(
        diags.iter().any(|d| d.starts_with("TS1308:")),
        "`await` in a non-async function nested in an async one is TS1308; got {diags:?}"
    );
}

#[test]
fn await_inside_nested_plain_arrow_reports_ts1308() {
    // Same, through the arrow body path.
    //
    // The arrow deliberately uses a *block* body. A concise-bodied arrow
    // (`(): number => await 1`) misses the TS1308 check entirely in tsz, even
    // at top level with no `async` anywhere in the file, so it is a separate
    // pre-existing false negative rather than a witness for this invariant —
    // filed apart from this change rather than pinned here.
    let diags = diagnostic_summaries(
        r#"
/// <reference lib="es2015.promise" />
async function outer(): Promise<number> {
    const inner = (): number => {
        return await 1;
    };
    return inner();
}
"#,
    );
    assert!(
        diags.iter().any(|d| d.starts_with("TS1308:")),
        "`await` in a non-async arrow nested in an async one is TS1308; got {diags:?}"
    );
}

#[test]
fn async_arrow_under_async_enclosing_function_still_unwraps() {
    // Fallback control: the unwrap must still happen when the inner function IS
    // async. Scoping the flag must not turn the fix into a blanket disable.
    // tsc: exit 0.
    let diags = diagnostic_summaries(
        r#"
/// <reference lib="es2015.promise" />
declare function withConnection<T>(consumer: (connection: string) => Promise<T>): Promise<T>;

async function run(): Promise<number> {
    return withConnection(async (connection) => {
        return connection.length;
    });
}
"#,
    );
    assert!(
        diags.is_empty(),
        "an async arrow must still have its return auto-wrapped in Promise; got {diags:?}"
    );
}

#[test]
fn async_function_nested_inside_plain_function_inside_async_still_unwraps() {
    // Three levels: async -> plain -> async. The innermost body must recover
    // the async context that the middle body cleared, which a "clear once and
    // never restore" fix would break. tsc: exit 0.
    let diags = diagnostic_summaries(
        r#"
/// <reference lib="es2015.promise" />
async function outer(): Promise<number> {
    function middle() {
        async function inner(): Promise<string> {
            return "x";
        }
        return inner();
    }
    middle();
    return 1;
}
"#,
    );
    assert!(
        diags.is_empty(),
        "async context must be restored per body, not cleared globally; got {diags:?}"
    );
}
