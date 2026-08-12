//! Tests for `find_jsdoc_type_spelling_suggestion`'s meaning-filtered
//! candidate search.
//!
//! Structural rule: when a JSDoc type-position name reference (`@param`,
//! `@type`, `@template`, `@return`, ...) fails to resolve, `tsc` reports
//! plain TS2304 ("Cannot find name") with no spelling suggestion unless a
//! same-meaning (TYPE) candidate exists in scope. tsz previously ran an
//! extra fallback pass with the meaning filter dropped entirely whenever
//! the meaning-filtered search found nothing, so a same-file VALUE-only
//! symbol (a `const`/parameter of the same short name, differing only by
//! case) or a VALUE-only global lib symbol (`CSS`, `Intl`) got offered as
//! a "did you mean?" TS2552 suggestion — a diagnostic `tsc` never emits for
//! these fixtures. The fallback pass (`crates/tsz-checker/src/error_reporter/
//! suggestions.rs`, `find_jsdoc_type_spelling_suggestion`) is removed;
//! genuine TYPE-meaning suggestions (`@typedef`/class name typos in TYPE
//! position) still work through the existing meaning-filtered search.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn check_js(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        check_js: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
fn constructor_template_out_of_scope_in_prototype_method_reports_plain_ts2304() {
    // `conformance/salsa/constructorFunctionMethodTypeParameters.ts`: the
    // constructor's own `@template {string} T` does not extend into a
    // prototype method's JSDoc scope. `tsc` reports plain TS2304 for the
    // out-of-scope `T` reference; tsz previously suggested the same-file
    // value parameter `t` (case-only match) via the unfiltered fallback.
    let source = r#"
/**
 * @template {string} T
 * @param {T} t
 */
function Cls(t) {
    this.t = t;
}

/**
 * @template {string} V
 * @param {T} t
 * @param {V} v
 * @return {V}
 */
Cls.prototype.topLevelComment = function (t, v) {
    return v;
};

var c = new Cls('a');
"#;
    let diags = check_js(source);
    let t_diags: Vec<&(u32, String)> = diags.iter().filter(|(_, m)| m.contains("'T'")).collect();
    assert!(
        t_diags.iter().all(|(c, _)| *c == 2304),
        "expected plain TS2304 for out-of-scope 'T', got: {t_diags:?}",
    );
    assert!(
        t_diags.iter().all(|(_, m)| !m.contains("Did you mean")),
        "expected no spelling suggestion for out-of-scope 'T', got: {t_diags:?}",
    );
}

#[test]
fn unresolved_jsdoc_type_does_not_suggest_lib_global_by_cross_meaning() {
    // `class` and `int` are not TYPE-meaning names, but `CSS`/`Intl` lib
    // globals happen to be short, same-case-insensitive-adjacent names.
    // `tsc` reports plain TS2304 with no suggestion for both.
    let source = r#"
/**
 * @type {class}
 */
var x;
/**
 * @param {int} y
 */
function f(y) {}
"#;
    let diags = check_js(source);
    let relevant: Vec<&(u32, String)> = diags
        .iter()
        .filter(|(_, m)| m.contains("'class'") || m.contains("'int'"))
        .collect();
    assert!(
        relevant.iter().all(|(c, _)| *c == 2304),
        "expected plain TS2304, got: {relevant:?}",
    );
    assert!(
        relevant.iter().all(|(_, m)| !m.contains("Did you mean")),
        "expected no cross-meaning spelling suggestion, got: {relevant:?}",
    );
}

#[test]
fn unresolved_jsdoc_return_type_still_reports_ts2304() {
    // Adjacent negative control: a plain unresolved JSDoc `@return` type
    // name (no case-only same-file collision) must still report TS2304.
    let source = r#"
/**
 * @return {Nonexistent}
 */
function f() {
    return 1;
}
"#;
    let diags = check_js(source);
    assert!(
        diags
            .iter()
            .any(|(c, m)| *c == 2304 && m.contains("'Nonexistent'")),
        "expected TS2304 for unresolved return type, got: {diags:?}",
    );
}

#[test]
fn typedef_type_typo_still_suggests_same_meaning_candidate() {
    // Positive control: a real TYPE-meaning candidate (a `@typedef`) must
    // still be offered as a TS2552 suggestion — only the meaningless
    // cross-meaning fallback pass is removed, not the meaning-filtered search.
    let source = r#"
class Foo {}
/**
 * @type {Fooo}
 */
var w;
"#;
    let diags = check_js(source);
    assert!(
        diags
            .iter()
            .any(|(c, m)| *c == 2552 && m.contains("'Fooo'") && m.contains("'Foo'")),
        "expected TS2552 suggesting the same-meaning class name, got: {diags:?}",
    );
}
