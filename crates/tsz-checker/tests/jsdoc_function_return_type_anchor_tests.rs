//! Regression tests for TS2355/TS2366 when the return type is provided through
//! a JSDoc `@type` annotation in JS files.
//!
//! These were written against the Closure `@type {function(): T}` spelling,
//! which TypeScript 7 rejects outright: it reports TS1005 and gives the
//! annotation no type, so no TS2355 follows. The oracle for the mirrored
//! corpus test `conformance/jsdoc/jsdocFunction_missingReturn.ts` expects
//! exactly TS1005 and TS8030 — and no TS2355. The first test now pins that.
//!
//! The second test is about something else that survives the change: a
//! function must be associated with its *own* leading JSDoc comment, not an
//! unrelated earlier one. It uses the arrow spelling so it keeps testing
//! association rather than the retired Closure path.

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
fn closure_jsdoc_type_reports_ts1005_and_no_ts2355() {
    // Mirrors `conformance/jsdoc/jsdocFunction_missingReturn.ts`, whose oracle
    // is exactly TS1005 + TS8030. TypeScript 7 does not accept the Closure
    // `function(): T` form, so the annotation yields no return type and the
    // missing-return diagnostic never arises.
    let source = "/** @type {function(): number} */\nfunction f() {}\n";

    let diagnostics = check_source(source, "a.js", options_js_strict());

    assert!(
        diagnostics.iter().any(|d| d.code == 1005),
        "expected TS1005 for the Closure function type, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2355),
        "TypeScript 7 gives the Closure form no type, so no TS2355 follows, got: {diagnostics:#?}"
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
    let source = "/** @type {() => number} */\nvar prior = 1;\n/** @type {() => number} */\nfunction f() {}\n";

    let diagnostics = check_source(source, "a.js", options_js_strict());

    let ts2355: Vec<_> = diagnostics.iter().filter(|d| d.code == 2355).collect();
    assert_eq!(
        ts2355.len(),
        1,
        "expected exactly one TS2355 (for f), got: {diagnostics:#?}"
    );

    // The point is association, not the exact anchor token: the diagnostic must
    // belong to `f`, so it has to sit at or after f's own leading comment —
    // never back at `prior`'s unrelated one.
    let f_comment_pos = source.rfind("/** @type").expect("f's own leading comment") as u32;
    let diag = ts2355[0];
    assert!(
        diag.start >= f_comment_pos,
        "TS2355 must be associated with f (at or after {f_comment_pos}), not with the \
         earlier unrelated annotation; got start={}",
        diag.start,
    );
}

#[test]
fn ts2322_expression_arrow_jsdoc_cast_return_anchors_on_outer_cast() {
    // Mirrors TypeScript/tests/cases/compiler/arrowExpressionBodyJSDoc.ts.
    // For concise JS arrows with `@returns {T}`, tsc reports the return
    // mismatch at the outer JSDoc cast expression, not the inner object literal
    // or nested cast.
    let source = r#"
/**
 * @template T
 * @param {T|undefined} value value or not
 * @returns {T} result value
 */
const foo1 = value => /** @type {string} */({ ...value });

/**
 * @template T
 * @param {T|undefined} value value or not
 * @returns {T} result value
 */
const foo2 = value => /** @type {string} */(/** @type {T} */({ ...value }));
"#;

    let diagnostics = check_source(source, "a.js", options_js_strict());
    let ts2322_starts: Vec<u32> = diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.start)
        .collect();

    let foo1_cast = source.find("*/({").expect("foo1 cast") + "*/".len();
    let foo2_line = source.find("const foo2").expect("foo2 line");
    let foo2_cast = foo2_line + source[foo2_line..].find("*/(").expect("foo2 cast") + "*/".len();

    assert!(
        ts2322_starts.contains(&(foo1_cast as u32)),
        "foo1 TS2322 should anchor at outer JSDoc cast paren offset {foo1_cast}, got: {ts2322_starts:?}"
    );
    assert!(
        ts2322_starts.contains(&(foo2_cast as u32)),
        "foo2 TS2322 should anchor at outer JSDoc cast paren offset {foo2_cast}, got: {ts2322_starts:?}"
    );
}
