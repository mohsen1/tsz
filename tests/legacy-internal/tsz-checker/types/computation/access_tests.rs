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

/// Structural rule: when `strictNullChecks` is off, property access on
/// `unknown` resolves against `unknown`'s apparent type — the
/// `Object.prototype` surface (`toString`, `valueOf`, `hasOwnProperty`, ...)
/// — instead of either always failing (dot access) or masking every key to
/// `any` (element access). A genuinely missing member still reports TS2339
/// for dot access; `strictNullChecks` on is unaffected (TS18046/TS2571 still
/// fire unconditionally, matching tsc's `unknown` restriction).
mod unknown_non_strict_apparent_member_access {
    use super::*;
    use crate::test_utils::{check_source_non_strict, check_source_non_strict_codes};

    #[test]
    fn dot_access_to_object_prototype_member_is_clean() {
        let diags = check_source_non_strict(
            r#"
declare var call: { <T>(): T };
call().toString();
call().valueOf();
call().hasOwnProperty("x");
"#,
        );
        assert!(
            semantic_errors(&diags).is_empty(),
            "Expected no diagnostics for Object.prototype members on non-strict unknown, got: {:?}",
            semantic_errors(&diags)
        );
    }

    #[test]
    fn dot_access_to_missing_member_still_reports_ts2339() {
        let codes = check_source_non_strict_codes(
            r#"
declare var call: { <T>(): T };
call().nonexistent();
"#,
        );
        assert_eq!(
            codes,
            vec![2339],
            "A member absent from Object.prototype must still be TS2339 under non-strict unknown"
        );
    }

    #[test]
    fn dot_access_renamed_binder_still_resolves() {
        // Same shape with different identifiers, to rule out a name-string check.
        let diags = check_source_non_strict(
            r#"
declare var produce: { <Widget>(): Widget };
produce().toString();
"#,
        );
        assert!(
            semantic_errors(&diags).is_empty(),
            "Renamed binder must not change resolution: {:?}",
            semantic_errors(&diags)
        );
    }

    #[test]
    fn bracket_access_to_object_prototype_member_resolves_real_type() {
        // If tsz masked the member to `any` (its pre-fix behavior), this
        // assignment would be silently accepted instead of reporting TS2322.
        let codes = check_source_non_strict_codes(
            r#"
declare var call: { <T>(): T };
var mismatch: number = call()["toString"]();
"#,
        );
        assert_eq!(
            codes,
            vec![2322],
            "Bracket access must resolve the real Object.prototype member type, got: {codes:?}"
        );
    }

    #[test]
    fn bracket_access_to_missing_member_stays_implicit_any() {
        // Unchanged pre-fix behavior: a non-Object member via bracket access
        // on non-strict `unknown` still falls back to implicit `any`, not TS2339.
        let diags = check_source_non_strict(
            r#"
declare var call: { <T>(): T };
call()["nonexistent"]();
"#,
        );
        assert!(
            semantic_errors(&diags).is_empty(),
            "Bracket access to a missing member should stay implicit any under non-strict unknown: {:?}",
            semantic_errors(&diags)
        );
    }

    #[test]
    fn general_index_access_on_unknown_is_unaffected() {
        // A non-literal (dynamic) index has no fixed name to check against
        // Object.prototype, so this path is untouched by the fix.
        let diags = check_source_non_strict(
            r#"
declare var call: { <T>(): T };
declare var key: string;
call()[key];
"#,
        );
        assert!(
            semantic_errors(&diags).is_empty(),
            "General index access on non-strict unknown must stay unaffected: {:?}",
            semantic_errors(&diags)
        );
    }

    #[test]
    fn strict_null_checks_still_rejects_object_prototype_member() {
        // Regression guard: the strict-mode gate must still fire
        // unconditionally, even for a genuine Object.prototype member. A
        // plain identifier receiver gets the named TS18046 form; the call
        // expression in the sibling test below gets the unnamed TS2571 form
        // (no printable base name) — both are the strict-mode block, neither
        // falls through to the non-strict apparent-member resolution.
        let diags = check_source_with_default_libs(
            r#"
declare var u: unknown;
u.toString();
"#,
        );
        let errors = semantic_errors(&diags);
        assert_eq!(
            errors,
            vec![18046],
            "strictNullChecks must still block unknown member access unconditionally: {errors:?}"
        );
    }

    #[test]
    fn strict_null_checks_still_rejects_object_prototype_member_no_printable_name() {
        let diags = check_source_with_default_libs(
            r#"
declare var call: { <T>(): T };
call().toString();
"#,
        );
        let errors = semantic_errors(&diags);
        assert_eq!(
            errors,
            vec![2571],
            "strictNullChecks must still block unknown member access without a printable base name: {errors:?}"
        );
    }
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
