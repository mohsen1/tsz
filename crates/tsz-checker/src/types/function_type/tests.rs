use crate::diagnostics::diagnostic_codes;

fn diagnostics_for_source(source: &str) -> Vec<u32> {
    crate::test_utils::check_source_codes(source)
}

#[test]
fn expression_body_arrow_with_return_annotation_reports_type_mismatch() {
    let diagnostics = diagnostics_for_source("const f = (): number => \"str\";");
    let target = diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE;

    assert!(
        diagnostics.contains(&target),
        "expected TS2322, got diagnostics: {diagnostics:?}"
    );
}

/// Minimal Promise definition so async tests can resolve Promise<T>.
const PROMISE_DEF: &str = "interface Promise<T> { then<U>(cb: (val: T) => U): Promise<U>; }";

fn async_diagnostics(body: &str) -> Vec<u32> {
    diagnostics_for_source(&format!("{PROMISE_DEF}\n{body}"))
}

#[test]
fn async_arrow_expression_body_promise_return_no_false_error() {
    let diags = async_diagnostics("const f = async (): Promise<number> => 42;");
    assert!(
        !diags.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "should not emit TS2322 for async arrow expression body, got: {diags:?}"
    );
}

#[test]
fn async_arrow_block_body_promise_return_no_false_error() {
    let diags = async_diagnostics("const f = async (): Promise<number> => { return 42; };");
    assert!(
        !diags.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "should not emit TS2322 for async arrow block body, got: {diags:?}"
    );
}

#[test]
fn async_function_expression_promise_return_no_false_error() {
    let diags = async_diagnostics("const f = async function(): Promise<number> { return 42; };");
    assert!(
        !diags.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "should not emit TS2322 for async function expression, got: {diags:?}"
    );
}

#[test]
fn async_arrow_generic_promise_return_no_false_error() {
    let diags = async_diagnostics("const f = async <T>(x: T): Promise<T> => x;");
    assert!(
        !diags.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "should not emit TS2322 for async generic arrow, got: {diags:?}"
    );
}

#[test]
fn async_inferred_return_unwraps_promise() {
    let diags = async_diagnostics(
        "declare function load(): Promise<boolean>;
         const cb: () => Promise<boolean> = async () => load();",
    );
    assert!(
        !diags.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "async returning Promise<T> should infer Promise<T>, not Promise<Promise<T>>: {diags:?}"
    );
}

#[test]
fn async_inferred_return_unwraps_promise_then_chain() {
    let diags = async_diagnostics(
        "declare function load(): Promise<boolean>;
         const cb: () => Promise<boolean> = async () => load().then(m => m);",
    );
    assert!(
        !diags.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "async returning .then() chain should infer correct Promise type: {diags:?}"
    );
}

#[test]
fn async_inferred_return_non_promise_wraps_once() {
    let diags = async_diagnostics("const cb: () => Promise<number> = async () => 42;");
    assert!(
        !diags.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "async returning non-Promise should wrap once: {diags:?}"
    );
}

#[test]
fn async_inferred_return_union_with_promise() {
    let diags = async_diagnostics(
        "declare function load(): Promise<boolean>;
         type LoadCallback = () => Promise<boolean> | string;
         const cb: LoadCallback = async () => load();",
    );
    assert!(
        !diags.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "async returning Promise in union context should not double-wrap: {diags:?}"
    );
}
