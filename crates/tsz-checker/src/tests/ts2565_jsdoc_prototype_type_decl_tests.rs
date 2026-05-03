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
