//! Regression tests for TS2565 ('used before being assigned') suppression on
//! JSDoc-typed prototype property declarations.
//!
//! `function C() {}; /** @type {T} */ C.prototype.x;` is a function-as-
//! constructor pattern where the bare prototype reference declares the
//! property's type via JSDoc. tsc treats this as a declaration, not an
//! "used before assigned" read.
//!
//! For ES `class C {}` declarations the same prototype attachment is still a
//! genuine "used before assigned" because the prototype shape is the class
//! instance type — tsc emits TS2565 there.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn diag_codes_js(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn diagnostics_js(source: &str) -> Vec<tsz_common::diagnostics::Diagnostic> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
}

#[test]
fn ts2565_suppressed_for_jsdoc_typed_prototype_on_function_constructor() {
    let codes = diag_codes_js(
        r#"
function C() { this.x = false; };
/** @type {number} */
C.prototype.x;
new C().x;
"#,
    );
    assert!(
        !codes.contains(&2565),
        "TS2565 must NOT fire for JSDoc-typed prototype on function-as-constructor. Got: {codes:?}"
    );
}

#[test]
fn ts2565_still_fires_for_jsdoc_typed_prototype_on_class() {
    let codes = diag_codes_js(
        r#"
class K {
    method() {}
}
/** @type {(x: number) => void} */
K.prototype.late;
"#,
    );
    assert!(
        codes.contains(&2565),
        "TS2565 must STILL fire for JSDoc-typed prototype on ES `class` (the prototype shape is the class instance type, late attachment is genuinely 'used before assigned'). Got: {codes:?}"
    );
}

/// Anti-hardcoding cover: the rule is structural (function vs class), not
/// based on identifier names — works with arbitrary names.
#[test]
fn ts2565_suppression_works_with_renamed_constructor() {
    let codes = diag_codes_js(
        r#"
function MyThing() { this.value = 0; };
/** @type {string} */
MyThing.prototype.label;
"#,
    );
    assert!(
        !codes.contains(&2565),
        "TS2565 suppression must work for any function-constructor name. Got: {codes:?}"
    );
}

#[test]
fn jsdoc_typed_prototype_checks_constructor_this_assignment() {
    let source =
        "function C() { this.x = false; };\n/** @type {number} */\nC.prototype.x;\nnew C().x;\n";
    let diagnostics = diagnostics_js(source);
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected one TS2322 for constructor `this.x` conflicting with prototype JSDoc; got: {diagnostics:?}"
    );
    assert_eq!(
        ts2322[0].message_text,
        "Type 'boolean' is not assignable to type 'number'."
    );
    assert_eq!(
        ts2322[0].start as usize,
        source
            .find("this.x")
            .expect("test source should contain this.x"),
        "TS2322 should be anchored on the constructor assignment target"
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == 2565),
        "the JSDoc prototype declaration should still suppress TS2565; got: {diagnostics:?}"
    );
}

#[test]
fn matching_jsdoc_typed_prototype_constructor_assignment_has_no_ts2322() {
    let codes = diag_codes_js(
        r#"
function C() { this.x = 1; };
/** @type {number} */
C.prototype.x;
"#,
    );
    assert!(
        !codes.contains(&2322),
        "matching constructor assignment and prototype JSDoc type must not emit TS2322; got: {codes:?}"
    );
}
