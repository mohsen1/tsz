//! Tests for TS2314 ("Generic type 'X' requires N type argument(s)") emitted for
//! bare generic types in JSDoc `@param` and `@return`/`@returns` tags when
//! `noImplicitAny` is enabled.
//!
//! tsc emits TS2314 for `@param {Array} x` and `@return {Promise}` when
//! strict/noImplicitAny is active, because those bare names require a type
//! argument. The fix lives in `check_jsdoc_typedef_base_types` (jsdoc/diagnostics.rs)
//! which now scans all JSDoc comments for bare @param/@return type names and
//! checks them against global lib symbols for required type arguments.

use rustc_hash::FxHashSet;
use std::path::Path;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_lib_files_for_test() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_roots = [
        manifest_dir.join("../../crates/tsz-core/src/lib-assets"),
        manifest_dir.join("../../crates/tsz-core/src/lib-assets-stripped"),
        manifest_dir.join("../../TypeScript/src/lib"),
    ];
    let lib_names = [
        "es5.d.ts",
        "es2015.d.ts",
        "es2015.core.d.ts",
        "es2015.collection.d.ts",
        "es2015.iterable.d.ts",
        "es2015.generator.d.ts",
        "es2015.promise.d.ts",
        "es2015.proxy.d.ts",
        "es2015.reflect.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
        "dom.d.ts",
        "dom.generated.d.ts",
        "dom.iterable.d.ts",
        "esnext.d.ts",
    ];

    let mut lib_files = Vec::new();
    let mut seen_files = FxHashSet::default();
    for file_name in lib_names {
        for root in &lib_roots {
            let lib_path = root.join(file_name);
            if lib_path.exists()
                && let Ok(content) = std::fs::read_to_string(&lib_path)
            {
                if !seen_files.insert(file_name.to_string()) {
                    break;
                }
                let lib_file = LibFile::from_source(file_name.to_string(), content);
                lib_files.push(Arc::new(lib_file));
                break;
            }
        }
    }

    lib_files
}

fn check_js_with_libs(source: &str, options: CheckerOptions) -> Vec<u32> {
    let file_name = "test.js";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let lib_files = load_lib_files_for_test();

    let mut binder = BinderState::new();
    if lib_files.is_empty() {
        binder.bind_source_file(parser.get_arena(), root);
    } else {
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    }

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    if !lib_files.is_empty() {
        let lib_contexts: Vec<tsz_checker::context::LibContext> = lib_files
            .iter()
            .map(|lib| tsz_checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }

    checker.check_source_file(root);
    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

fn check_js_no_implicit_any(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        check_js: true,
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    check_js_with_libs(source, options)
}

fn check_js_strict(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        check_js: true,
        strict: true,
        strict_null_checks: true,
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    check_js_with_libs(source, options)
}

fn check_js_permissive(source: &str) -> Vec<u32> {
    // Deliberately no lib files: tests that my guard (no_implicit_any check)
    // prevents TS2314 from firing even when Array/Promise are in the symbol table.
    // Pre-existing paths emit TS2314 with lib+noImplicitAny via other channels;
    // here we only care that check_jsdoc_typedef_base_types doesn't fire.
    let options = CheckerOptions {
        check_js: true,
        ..CheckerOptions::default()
    };
    use tsz_checker::test_utils::check_source;
    check_source(source, "test.js", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

// ---------------------------------------------------------------------------
// @param {Array} — TS2314 when noImplicitAny
// ---------------------------------------------------------------------------

#[test]
fn param_bare_array_emits_ts2314_when_no_implicit_any() {
    let codes = check_js_no_implicit_any(
        r#"
/**
 * @param {Array} arr
 * @return {Array}
 */
function f(arr) { return arr; }
"#,
    );
    assert!(
        codes.contains(&2314),
        "expected TS2314 for @param {{Array}}, got: {codes:?}"
    );
}

#[test]
fn return_bare_array_emits_ts2314_when_no_implicit_any() {
    let codes = check_js_no_implicit_any(
        r#"
/**
 * @param {Array} arr
 * @return {Array}
 */
function f(arr) { return arr; }
"#,
    );
    // Both @param and @return should each produce TS2314
    let count = codes.iter().filter(|&&c| c == 2314).count();
    assert!(
        count >= 2,
        "expected at least 2 TS2314 (one for @param, one for @return), got {count} in {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// @param {Promise} — TS2314 when noImplicitAny
// ---------------------------------------------------------------------------

#[test]
fn param_bare_promise_emits_ts2314_when_no_implicit_any() {
    let codes = check_js_no_implicit_any(
        r#"
/**
 * @param {Promise} pr
 * @returns {Promise}
 */
function f(pr) { return pr; }
"#,
    );
    assert!(
        codes.contains(&2314),
        "expected TS2314 for @param {{Promise}}, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// No TS2314 without noImplicitAny (bare Array/Promise = any in JSDoc)
// ---------------------------------------------------------------------------

#[test]
fn no_ts2314_without_no_implicit_any() {
    let codes = check_js_permissive(
        r#"
/**
 * @param {Array} arr
 * @return {Array}
 */
function f(arr) { return arr; }
"#,
    );
    assert!(
        !codes.contains(&2314),
        "should NOT emit TS2314 without noImplicitAny, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// @param {Array<number>} — already parameterized, no TS2314
// ---------------------------------------------------------------------------

#[test]
fn parameterized_array_no_ts2314() {
    let codes = check_js_no_implicit_any(
        r#"
/**
 * @param {Array<number>} arr
 * @return {Array<string>}
 */
function f(arr) { return []; }
"#,
    );
    assert!(
        !codes.contains(&2314),
        "should not emit TS2314 for already-parameterized Array<T>, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// strict mode mirrors the jsdocArrayObjectPromiseNoImplicitAny.ts scenario
// ---------------------------------------------------------------------------

#[test]
fn strict_emits_ts2314_for_array_and_promise_in_function_jsdoc() {
    let codes = check_js_strict(
        r#"
/**
 * @param {Array} arr
 * @return {Array}
 */
function returnNotAnyArray(arr) { return arr; }

/**
 * @param {Promise} pr
 * @return {Promise}
 */
function returnNotAnyPromise(pr) { return pr; }
"#,
    );
    let count_2314 = codes.iter().filter(|&&c| c == 2314).count();
    assert!(
        count_2314 >= 4,
        "expected at least 4 TS2314 (2 Array + 2 Promise), got {count_2314} in {codes:?}"
    );
}
