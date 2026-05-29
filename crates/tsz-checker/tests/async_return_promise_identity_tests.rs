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
