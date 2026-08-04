//! TS1058: a `return` statement inside an *annotated* async function/method
//! whose expression's type has a callable `then` member that does not resolve
//! to a valid promise shape.
//!
//! tsc's `checkReturnExpression` runs `checkAwaitedType(exprType, false, node,
//! The_return_type_of_an_async_function_must_either_be_a_valid_promise_or_must_not_contain_a_callable_then_member)`
//! whenever the enclosing function is async AND
//! `getReturnTypeFromAnnotation(container) != nil` — an inferred (unannotated)
//! return type has no independent annotation to validate the return expression
//! against, so the check does not run there (a distinct, unimplemented gap:
//! tsc still reports TS1058 for the unannotated case through a different path,
//! anchored at the function name rather than the return statement).
//!
//! The check tests the return *expression's* own type, independent of whether
//! the declared return type is itself `Promise<T>` — TS1064 (declared return
//! type is not `Promise<T>`) is a separate, earlier check on the annotation
//! node, and both can fire together on the same statement.
//!
//! Like `generator_yield_invalid_thenable_tests.rs` (TS1321, the `yield`
//! sibling of this check), every positive witness here uses the `this`-type
//! mismatch shape: tsz's `await_operand_invalid_thenable_this_type` — the same
//! predicate this fix reuses — only implements that sub-case of "invalid
//! thenable", not tsc's full "callable `then` with no extractable payload"
//! rule. Oracle-verified against `typescript@7.0.2`.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn strict_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

const BAD_THENABLE: &str = r#"
interface BadThenable<T> {
    then(this: { required: string }, onfulfilled?: ((value: T) => void) | null): void;
}
"#;

/// Baseline: a `this`-type-mismatched thenable returned from an annotated
/// async function draws TS1058 at the `return` statement, alongside TS1064 on
/// the declared return type.
#[test]
fn malformed_thenable_return_in_annotated_function_reports_ts1058() {
    let codes = strict_codes(&format!(
        r#"
export {{}};
{BAD_THENABLE}
declare const zzSource: BadThenable<string>;
async function f(): BadThenable<string> {{
  return zzSource;
}}
"#,
    ));
    assert!(
        codes.contains(&1058),
        "a return statement whose expression has an invalid thenable must report TS1058; got {codes:?}"
    );
}

/// Same shape on an async class method (`METHOD_DECLARATION`, not
/// `FUNCTION_DECLARATION`) — the enclosing-function lookup must resolve
/// methods, not just plain functions. Renamed binders throughout (a
/// differently-named interface, container, and method) to prove the check is
/// not keyed off any of `BadThenable`/`f`'s identifiers.
#[test]
fn malformed_thenable_return_in_annotated_method_reports_ts1058() {
    let codes = strict_codes(
        r#"
export {};
interface WeirdThenable<T> {
    then(this: { needed: string }, onfulfilled?: ((value: T) => void) | null): void;
}
declare const src: WeirdThenable<number>;
class Container {
    async m(): WeirdThenable<number> {
        return src;
    }
}
"#,
    );
    assert!(
        codes.contains(&1058),
        "an async method's invalid-thenable return must report TS1058; got {codes:?}"
    );
}

/// Same shape on an async arrow function with a block body (`ARROW_FUNCTION`).
#[test]
fn malformed_thenable_return_in_annotated_arrow_reports_ts1058() {
    let codes = strict_codes(
        r#"
export {};
interface BogusThenable<T> {
    then(this: { must: string }, onfulfilled?: ((value: T) => void) | null): void;
}
declare const bogusSource: BogusThenable<boolean>;
const arrowFn = async (): BogusThenable<boolean> => {
    return bogusSource;
};
"#,
    );
    assert!(
        codes.contains(&1058),
        "an async arrow function's invalid-thenable return must report TS1058; got {codes:?}"
    );
}

/// Negative: a real `Promise<T>` return draws neither TS1058 nor TS1064.
#[test]
fn valid_promise_return_does_not_report_ts1058() {
    let codes = strict_codes(
        r#"
export {};
async function ok(): Promise<string> {
    return "x";
}
"#,
    );
    assert!(
        !codes.contains(&1058),
        "a valid Promise<T> return must not report TS1058; got {codes:?}"
    );
    assert!(
        !codes.contains(&1064),
        "a valid Promise<T> return must not report TS1064; got {codes:?}"
    );
}

/// Negative/fallback: a returned value with no invalid-thenable shape at all
/// is not this check's concern — an ordinary type mismatch (TS2322) is
/// reported instead, not TS1058.
#[test]
fn non_thenable_mismatch_does_not_report_ts1058() {
    let codes = strict_codes(
        r#"
export {};
async function g(): Promise<string> {
    return 5 as unknown as { x: number };
}
"#,
    );
    assert!(
        !codes.contains(&1058),
        "a plain (non-thenable) type mismatch must not report TS1058; got {codes:?}"
    );
    assert!(
        codes.contains(&2322),
        "the plain type mismatch itself must still report TS2322; got {codes:?}"
    );
}

/// Fallback: an *unannotated* async function is out of this fix's scope (tsc
/// answers TS1058 there too, through inferred-return-type checking anchored at
/// the function name, not the return statement) — the annotated-only gate must
/// not accidentally fire here.
#[test]
fn unannotated_async_function_does_not_report_ts1058_at_return_statement() {
    let codes = strict_codes(&format!(
        r#"
export {{}};
{BAD_THENABLE}
declare const zzSource: BadThenable<string>;
async function noAnnotation() {{
  return zzSource;
}}
"#,
    ));
    assert!(
        !codes.contains(&1058),
        "the annotated-only TS1058 gate must not fire for an unannotated async function; got {codes:?}"
    );
}
