//! Issue #3133: a JSDoc `@typedef` in a global-script JS file (no
//! imports/exports) whose name collides with a lib global must emit
//! TS2300 "Duplicate identifier" at the typedef site, mirroring tsc.
//!
//! Repro from the issue: `// @ts-check` JS file with
//! `/** @typedef {string} Object */` is expected to surface
//! `TS2300: Duplicate identifier 'Object'.` because the lib defines the
//! global `Object` type/value.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

fn check_js_with_lib(source: &str, file_name: &str) -> Vec<Diagnostic> {
    let lib_files = tsz_checker::test_utils::load_compiled_lib_files(&["lib.es5.d.ts"]);
    assert!(
        !lib_files.is_empty(),
        "Expected to find lib.es5.d.ts for the test"
    );
    tsz_checker::test_utils::check_source_with_libs(
        source,
        file_name,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..Default::default()
        },
        &lib_files,
    )
}

/// `@typedef Object` in a global-script JS file collides with the lib
/// `Object` global → TS2300 must fire on the typedef name.
#[test]
fn jsdoc_typedef_object_in_global_script_js_emits_ts2300() {
    let source = "// @ts-check\n\n/** @typedef {string} Object */\n";
    let diags = check_js_with_lib(source, "lib_collide.js");

    let object_dups: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| d.code == 2300 && d.message_text.contains("'Object'"))
        .collect();
    assert!(
        !object_dups.is_empty(),
        "expected TS2300 for `@typedef Object` colliding with lib global, got: {diags:?}"
    );
}

/// Same shape but with a non-lib name (`MyAlias`) must NOT emit TS2300 —
/// guards against the fix over-triggering on every typedef.
#[test]
fn jsdoc_typedef_non_lib_name_does_not_emit_ts2300() {
    let source = "// @ts-check\n\n/** @typedef {string} MyAlias */\n";
    let diags = check_js_with_lib(source, "no_collide.js");

    let dups: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == 2300).collect();
    assert!(
        dups.is_empty(),
        "expected NO TS2300 for non-lib-colliding typedef, got: {dups:?}"
    );
}

/// A different lib global (`Promise`) shadowed by a typedef should also
/// emit TS2300 — verifies the rule is structural, not name-specific.
#[test]
fn jsdoc_typedef_promise_in_global_script_js_emits_ts2300() {
    let source = "// @ts-check\n\n/** @typedef {string} Promise */\n";
    let diags = check_js_with_lib(source, "promise_collide.js");

    let promise_dups: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| d.code == 2300 && d.message_text.contains("'Promise'"))
        .collect();
    assert!(
        !promise_dups.is_empty(),
        "expected TS2300 for `@typedef Promise` colliding with lib global, got: {diags:?}"
    );
}
