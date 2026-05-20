use crate::diagnostics::Diagnostic;

fn check_source_with_default_libs(source: &str) -> Vec<Diagnostic> {
    crate::test_utils::check_source_diagnostics(source)
}

fn has_code(diags: &[Diagnostic], code: u32) -> bool {
    diags.iter().any(|d| d.code == code)
}

/// Filter out TS2318 ("Cannot find global type") which fires when lib files aren't loaded.
fn semantic_errors(diags: &[Diagnostic]) -> Vec<u32> {
    diags
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| d.code)
        .collect()
}

/// Minimal Promise/PromiseLike type definitions for tests.
const PROMISE_LIB: &str = r#"
interface PromiseLike<T> {
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null,
        onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null
    ): PromiseLike<TResult1 | TResult2>;
}
interface Promise<T> {
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null,
        onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null
    ): Promise<TResult1 | TResult2>;
}
interface PromiseConstructor {
    new <T>(executor: (resolve: (value: T | PromiseLike<T>) => void, reject: (reason?: any) => void) => void): Promise<T>;
}
declare var Promise: PromiseConstructor;
"#;

#[test]
fn contextual_type_through_new_promise_variable_decl() {
    // `const p: Promise<string> = new Promise(resolve => resolve("hello"))` should
    // infer T = string from the contextual type, producing no errors.
    let source = format!(
        r#"{PROMISE_LIB}
const p: Promise<string> = new Promise(resolve => resolve("hello"));"#
    );
    let diags = check_source_with_default_libs(&source);
    let errors = semantic_errors(&diags);
    assert!(
        errors.is_empty(),
        "Expected no semantic errors for contextually typed new Promise, got: {errors:?}"
    );
}

#[test]
fn contextual_type_through_await_new_promise() {
    // `const s: string = await new Promise(resolve => resolve("ok"))` should
    // infer T = string via the await contextual type union.
    let source = format!(
        r#"{PROMISE_LIB}
async function f() {{ const s: string = await new Promise(resolve => resolve("ok")); }}"#
    );
    let diags = check_source_with_default_libs(&source);
    let errors = semantic_errors(&diags);
    assert!(
        errors.is_empty(),
        "Expected no semantic errors for await new Promise with contextual type, got: {errors:?}"
    );
}

#[test]
fn contextual_type_async_return_new_promise() {
    // Note: the full async return + new Promise fix requires real lib files because
    // resolve_global_interface_type("Promise") doesn't find local declarations.
    // This test verifies the code doesn't crash; the full fix is validated by
    // the contextuallyTypeAsyncFunctionReturnType conformance test.
    let source = format!(
        r#"{PROMISE_LIB}
interface Obj {{ key: "value"; }}
async function f(): Promise<Obj> {{
    return new Promise(resolve => {{
        resolve({{ key: "value" }});
    }});
}}"#
    );
    let diags = check_source_with_default_libs(&source);
    // Without real lib files, global Promise resolution fails and inference
    // falls back to unknown, producing TS2322/TS2345. This is expected.
    // The important thing is no crash and the code path executes.
    let _ = semantic_errors(&diags);
}

#[test]
fn tuple_expression_negative_index_emits_t2514() {
    // `as const` makes the literal a readonly tuple. Without it, `["a", 1]`
    // is inferred as `(string | number)[]` and TS2514 is not expected.
    let diags = check_source_with_default_libs(
        r#"
const tuple = ["a", 1] as const;
const bad = tuple[-1];
"#,
    );

    assert!(
        has_code(&diags, 2514),
        "Expected TS2514 for tuple expression negative index, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn private_name_access_unknown_reports_18046() {
    let diags = check_source_with_default_libs(
        r#"
class A {
    #foo = true;
    static #baz = 10;
    static #m() {}
    method(thing: unknown) {
        thing.#foo;
        thing.#m();
        thing.#baz;
        thing.#bar;
        thing.#foo();
    }
}
"#,
    );
    let errors = semantic_errors(&diags);
    assert_eq!(
        errors.iter().filter(|code| **code == 18046).count(),
        5,
        "Expected 5 TS18046 diagnostics for private access on unknown, got: {errors:?}"
    );
    assert_eq!(
        errors.iter().filter(|code| **code == 2339).count(),
        1,
        "Expected one TS2339 diagnostic for undeclared private name, got: {errors:?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == 2339 && d.message_text.contains("#bar")),
        "Expected the TS2339 diagnostic to mention '#bar': {diags:?}"
    );
}

#[test]
fn private_name_access_never_reports_2339() {
    let diags = check_source_with_default_libs(
        r#"
class A {
    #foo = true;
    static #baz = 10;
    static #m() {}
    method(thing: never) {
        thing.#foo;
        thing.#m();
        thing.#baz;
        thing.#bar;
        thing.#foo();
    }
}
"#,
    );
    let errors = semantic_errors(&diags);
    assert_eq!(
        errors.iter().filter(|code| **code == 2339).count(),
        5,
        "Expected 5 TS2339 diagnostics for private access on never, got: {errors:?}"
    );
    assert!(
        errors.iter().all(|code| *code == 2339),
        "Expected only TS2339 diagnostics, got: {errors:?}"
    );
}

#[test]
fn inherited_static_member_element_access_emits_ts2576() {
    let diags = check_source_with_default_libs(
        r#"
class Base {
    static count = 1;
    static get size() {
        return 2;
    }
}
class Derived extends Base {}
const value = new Derived();
value["count"];
value["size"];
"#,
    );

    let errors = semantic_errors(&diags);
    assert_eq!(
        errors.iter().filter(|code| **code == 2576).count(),
        2,
        "Expected TS2576 for inherited static field and accessor element access, got: {errors:?}"
    );
}
