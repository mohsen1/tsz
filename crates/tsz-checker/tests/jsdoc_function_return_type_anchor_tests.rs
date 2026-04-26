//! Regression tests for TS2355/TS2366 anchor positions when the return type
//! is provided through a JSDoc `@type {function(): T}` annotation in JS files.
//!
//! tsc anchors the diagnostic on the JSDoc return-type token (e.g. `number`
//! within `@type {function(): number}`) rather than on the function name.
//! These tests pin that anchor so we don't regress.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn options_js_strict() -> CheckerOptions {
    CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        ..CheckerOptions::default()
    }
}

#[test]
fn ts2355_anchors_on_jsdoc_function_return_type_for_function_declaration() {
    // Test mirrors `conformance/jsdoc/jsdocFunction_missingReturn.ts`. tsc
    // points TS2355 at `number` within the JSDoc `function(): number` type
    // expression. Anchor must be on that token, not on the function name.
    let source = "/** @type {function(): number} */\nfunction f() {}\n";

    let diagnostics = check_source(source, "a.js", options_js_strict());

    let ts2355: Vec<_> = diagnostics.iter().filter(|d| d.code == 2355).collect();
    assert_eq!(
        ts2355.len(),
        1,
        "expected exactly one TS2355 for missing return value, got: {diagnostics:#?}"
    );

    let number_pos = source.find("number").expect("number in JSDoc") as u32;
    let diag = ts2355[0];
    assert_eq!(
        (diag.start, diag.length),
        (number_pos, "number".len() as u32),
        "TS2355 should anchor on the JSDoc return type 'number', got start={} length={} (expected start={} length={})",
        diag.start,
        diag.length,
        number_pos,
        "number".len()
    );
}

#[test]
fn ts2355_falls_back_to_name_when_no_jsdoc_return_type() {
    // Without a JSDoc `@type {function(): T}` annotation, TS2355 should not
    // fire at all because there's no declared return type to enforce. This
    // test guards against accidentally widening the JSDoc anchor path so it
    // fires for plain JS functions.
    let source = "function f() {}\n";

    let diagnostics = check_source(source, "a.js", options_js_strict());

    let ts2355 = diagnostics.iter().filter(|d| d.code == 2355).count();
    assert_eq!(
        ts2355, 0,
        "no JSDoc return type means no TS2355; got: {diagnostics:#?}"
    );
}

#[test]
fn ts2355_anchors_on_owner_jsdoc_after_unrelated_function_decl_above() {
    // Regression for PR #1431 followup: the parent-walk loop in
    // `jsdoc_function_return_type_span_for_function` (lookup.rs ~lines
    // 454-464) previously scanned ALL earlier comments (no early break) and
    // lacked the SOURCE_FILE/BLOCK container guard that
    // `try_jsdoc_with_ancestor_walk` (params.rs ~lines 697-732) uses.
    //
    // This test pins the canonical "JSDoc on a `function f()` declaration
    // located right after an unrelated earlier `@type {function(): T}`
    // annotation" anchor: the diagnostic for `f` must point at *f's own*
    // `number` token, not at the earlier unrelated `number` token. With the
    // buggy parent walk, when the immediate-leading-comment loop fails to
    // resolve via the function node directly, the parent walk would step
    // through the SOURCE_FILE container without the guard and find the
    // unrelated comment.
    let source = "/** @type {function(): number} */\nvar prior = 1;\n/** @type {function(): number} */\nfunction f() {}\n";

    let diagnostics = check_source(source, "a.js", options_js_strict());

    let ts2355: Vec<_> = diagnostics.iter().filter(|d| d.code == 2355).collect();
    assert_eq!(
        ts2355.len(),
        1,
        "expected exactly one TS2355 (for f), got: {diagnostics:#?}"
    );

    // Find the *second* `number` occurrence -- f's own `@type` token.
    let first_number_pos = source.find("number").expect("first number") as u32;
    let second_number_pos = source[first_number_pos as usize + 1..]
        .find("number")
        .expect("second number") as u32
        + first_number_pos
        + 1;

    let diag = ts2355[0];
    assert_eq!(
        (diag.start, diag.length),
        (second_number_pos, "number".len() as u32),
        "TS2355 must anchor on f's *own* @type return token at {second_number_pos}, \
         not at the earlier unrelated `number` at {first_number_pos}; got start={} length={}",
        diag.start,
        diag.length,
    );
}
