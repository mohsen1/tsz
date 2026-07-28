//! `exports` / `module` used as a JSDoc type in a CommonJS file.
//!
//! Those names are the module object — a value — in a file that assigns to
//! them, so `tsc` reports TS2749 "refers to a value, but is being used as a
//! type here". Without any export assignment they are not values either, and
//! tsc reports a cannot-find-name variant instead; tsz leaves that case alone.
//!
//! Verified against the pinned tsc 7.0.2 (`--allowJs --checkJs`):
//!
//! ```text
//! module.exports = {}   /** @type {exports} */ var x   -> TS2749
//! module.exports = {}   /** @type {module}  */ var x   -> TS2749
//! ```

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn js_codes(source: &str) -> Vec<u32> {
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

const VALUE_USED_AS_TYPE: u32 = 2749;

#[test]
fn exports_as_a_type_reports_when_the_file_exports() {
    let source = "module.exports = {}\n/**\n * @type {exports}\n */\nvar x\n";
    assert!(js_codes(source).contains(&VALUE_USED_AS_TYPE));
}

#[test]
fn module_as_a_type_reports_when_the_file_exports() {
    let source = "module.exports = {}\n/**\n * @type {module}\n */\nvar x\n";
    assert!(js_codes(source).contains(&VALUE_USED_AS_TYPE));
}

/// A named export assignment counts as well as a whole-module one.
#[test]
fn named_export_assignment_also_makes_exports_a_value() {
    let source = "exports.a = 1\n/**\n * @type {exports}\n */\nvar x\n";
    assert!(js_codes(source).contains(&VALUE_USED_AS_TYPE));
}

/// Without an export assignment the names are not values; this path is left to
/// the existing cannot-find-name handling and must not report TS2749.
#[test]
fn exports_without_an_export_assignment_is_not_value_used_as_type() {
    let source = "/**\n * @type {exports}\n */\nvar x\n";
    assert!(!js_codes(source).contains(&VALUE_USED_AS_TYPE));
}

/// An ordinary local value used as a type keeps reporting — the new branch must
/// not shadow the general predicate.
#[test]
fn local_value_used_as_a_type_still_reports() {
    let source = "var v = 1\n/**\n * @type {v}\n */\nvar y\n";
    assert!(js_codes(source).contains(&VALUE_USED_AS_TYPE));
}

/// A real type is still accepted in the same file shape.
#[test]
fn a_genuine_type_is_still_accepted() {
    let source = "module.exports = {}\n/**\n * @typedef {number} Num\n */\n/**\n * @type {Num}\n */\nvar x\n";
    assert!(!js_codes(source).contains(&VALUE_USED_AS_TYPE));
}
