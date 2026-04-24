use crate::diagnostics::diagnostic_codes;

fn diagnostics_for_source(source: &str) -> Vec<u32> {
    crate::test_utils::check_source_codes(source)
}

fn diagnostics_with_spans(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    crate::test_utils::check_source_diagnostics(source)
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

#[test]
fn block_body_arrow_return_type_mismatch_anchors_return_statement() {
    let source = "const f = <T>(x: T): T => { return null; };";
    let diagnostics = diagnostics_with_spans(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .expect("expected TS2322");
    let return_start = source.find("return").expect("expected return keyword") as u32;
    assert_eq!(
        diag.start, return_start,
        "TS2322 for block-body arrow return mismatch should anchor at `return`: {diag:?}"
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

/// When a parameter has a binding pattern and an initializer, tsc uses the
/// binding pattern's implied type (`[any, any, any]`) as the contextual type
/// for the initializer. That preserves the tuple shape of the initializer
/// (`[undefined, null, undefined]`) instead of widening to an array
/// (`(null | undefined)[]`).
///
/// Mirrors tsc's behavior at
/// `TypeScript/src/compiler/checker.ts :: getContextualTypeForInitializerExpression`
/// where a binding-pattern declaration's pattern type is used as contextual.
#[test]
fn destructuring_param_initializer_preserves_tuple_shape() {
    // When the tuple is preserved, calling with arguments that violate the
    // per-position element types surfaces TS2322 errors referencing the
    // element types (`undefined`, `null`).  If instead the param were
    // inferred as an array `(null | undefined)[]`, the error message would
    // mention the union and/or an array target.
    let source = "function b6([a, z, y] = [undefined, null, undefined]) { }
                  b6([\"string\", 1, 2]);";
    let diags = diagnostics_with_spans(source);

    // We must see a TS2322 with target `null` (only present when the
    // per-position tuple element is preserved — position 1 = null).
    let has_null_target = diags
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .any(|d| d.message_text.contains("'null'"));
    assert!(
        has_null_target,
        "expected TS2322 mentioning target 'null' (tuple element 1), diags: {diags:#?}"
    );

    // And we must NOT mention an array target like `undefined[]` or
    // `(null | undefined)[]` which would indicate the initializer was
    // widened to an array type.
    let mentions_array_target = diags.iter().any(|d| {
        d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
            && (d.message_text.contains("undefined[]")
                || d.message_text.contains("null | undefined)[]"))
    });
    assert!(
        !mentions_array_target,
        "TS2322 should not mention array target — tuple shape should be preserved, diags: {diags:#?}"
    );
}

#[test]
fn async_generic_return_call_does_not_get_promise_union_context() {
    let diags = async_diagnostics(
        "class Api<D = {}> {
            async post<T = D>() { return this.request<T>(); }
            async request<D>(): Promise<D> { throw new Error(); }
         }
         declare const api: Api;
         interface Obj { x: number }
         async function fn<T>(): Promise<T extends object ? { [K in keyof T]: Obj } : Obj> {
             return api.post();
         }",
    );
    assert!(
        !diags.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "async generic return call should not be over-constrained by PromiseLike contextual unions: {diags:?}"
    );
}
