use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_compiled_lib_files};
use tsz_common::common::{ModuleKind, ScriptTarget};

fn check_with_promise_lib(source: &str) -> Vec<u32> {
    let lib_files = load_compiled_lib_files(&["lib.es5.d.ts", "lib.es2015.promise.d.ts"]);
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

fn check_js_with_promise_lib(source: &str) -> Vec<u32> {
    let lib_files = load_compiled_lib_files(&["lib.es5.d.ts", "lib.es2015.promise.d.ts"]);
    check_source_with_libs(
        source,
        "test.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
        &lib_files,
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn async_return_completeness_does_not_trust_module_local_promise_spelling() {
    let codes = check_with_promise_lib(
        r#"
export {};

interface Promise<T> {
  shadow: T;
}

async function localPromise(): Promise<number> {
}
"#,
    );

    assert!(
        codes.contains(&1064),
        "Expected TS1064 for module-local Promise annotation, got: {codes:?}"
    );
    assert!(
        codes.contains(&2355),
        "Expected TS2355 because module-local Promise must not suppress return completeness by spelling, got: {codes:?}"
    );
}

#[test]
fn async_return_type_alias_to_promise_is_accepted() {
    // `type P = Promise<number>` referenced as a bare async return annotation.
    // tsc resolves the alias before its `isReferenceToType(globalPromise)` check
    // and accepts it; tsz previously raised a false-positive TS1064 because the
    // alias body evaluates to a flattened object that loses the Promise identity.
    let codes = check_with_promise_lib(
        r#"
type P = Promise<number>;
async function f(): P { return 1; }
"#,
    );
    assert!(
        !codes.contains(&1064),
        "a type alias resolving to Promise<T> is a valid async return type; got {codes:?}"
    );
}

#[test]
fn async_return_type_alias_chain_to_promise_is_accepted() {
    // A chain of non-generic aliases must be chased to the underlying Promise.
    let codes = check_with_promise_lib(
        r#"
type Aaa = Promise<string>;
type Bbb = Aaa;
type Ccc = Bbb;
async function g(): Ccc { return "x"; }
"#,
    );
    assert!(
        !codes.contains(&1064),
        "a chain of aliases resolving to Promise<T> is a valid async return type; got {codes:?}"
    );
}

#[test]
fn async_return_type_alias_accepted_across_function_positions() {
    // The alias must be accepted for methods, static methods, accessors-free
    // object-literal methods, and arrow functions, not only free functions.
    // Binder names are varied to guard against name-based shortcuts.
    let codes = check_with_promise_lib(
        r#"
type Fut = Promise<void>;
const arrow = async (): Fut => {};
class Holder {
    async instanceMethod(): Fut {}
    static async staticMethod(): Fut {}
}
const literal = { async objMethod(): Fut {} };
"#,
    );
    assert!(
        !codes.contains(&1064),
        "an alias to Promise must be accepted in every async function position; got {codes:?}"
    );
}

#[test]
fn async_return_generic_type_alias_application_is_accepted() {
    // Guard the already-working Application form (`type Fut<T> = Promise<T>`
    // used as `Fut<number>`) so the fix keeps both alias shapes in agreement.
    let codes = check_with_promise_lib(
        r#"
type Fut<T> = Promise<T>;
type NumFut = Fut<number>;
async function h(): NumFut { return 1; }
"#,
    );
    assert!(
        !codes.contains(&1064),
        "a generic alias application resolving to Promise<T> is a valid async return type; got {codes:?}"
    );
}

#[test]
fn async_return_type_alias_to_non_promise_still_reports() {
    // A renamed alias whose body is a primitive must still report TS1064.
    let codes = check_with_promise_lib(
        r#"
type Zebra = number;
async function f(): Zebra { return 1; }
"#,
    );
    assert!(
        codes.contains(&1064),
        "an alias to a non-Promise type must still report TS1064; got {codes:?}"
    );
}

#[test]
fn async_return_type_alias_to_promiselike_still_reports() {
    // `PromiseLike` is not the global `Promise`; an alias to it must still
    // report TS1064, matching tsc's strict `isReferenceToType` check.
    let codes = check_with_promise_lib(
        r#"
type Thenable = PromiseLike<number>;
async function f(): Thenable { return 1; }
"#,
    );
    assert!(
        codes.contains(&1064),
        "an alias to PromiseLike must still report TS1064; got {codes:?}"
    );
}

#[test]
fn async_return_type_alias_to_promise_union_still_reports() {
    // A union of Promises is not a reference to the global Promise; tsc reports
    // TS1064, so the alias-chase must not over-accept unions.
    let codes = check_with_promise_lib(
        r#"
type Either = Promise<number> | Promise<string>;
async function f(): Either { return 1; }
"#,
    );
    assert!(
        codes.contains(&1064),
        "an alias to a union of Promises must still report TS1064; got {codes:?}"
    );
}

#[test]
fn jsdoc_async_return_ignores_typedef_body_promise_mentions() {
    let codes = check_js_with_promise_lib(
        r#"
/** @typedef {Promise} Box */

/** @type {function(): Promise<number>} */
const f = async function() {
    return 1;
};
"#,
    );

    assert!(
        !codes.contains(&1064),
        "JSDoc typedef bodies that mention Promise must not shadow the global Promise return protocol; got {codes:?}"
    );
}
