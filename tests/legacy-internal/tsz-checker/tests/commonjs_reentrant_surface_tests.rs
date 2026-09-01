//! Property writes on a function-typed `module.exports` are checked consistently.
//!
//! While a file's JS export surface is being computed, `resolve_js_export_surface`
//! hands re-entrant callers a placeholder surface that reports no
//! `module.exports = X`. Consumers then synthesised a namespace containing every
//! `module.exports.<name>` in the file, and a property write typed inside that
//! window resolved against a namespace that *had* the member — losing its
//! missing-property diagnostic — while a sibling write typed after the window
//! resolved against the real export type and reported.
//!
//! The window is entered only for RHSs that need the checker: a function
//! expression re-enters, a numeric literal short-circuits. That is why
//! `module.exports.f = function () {}` was silent while
//! `module.exports.g = 1` reported, in one file, against one receiver.
//!
//! tsc reports both (verified against the pinned tsc 7.0.2).

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

const MISSING_PROPERTY: u32 = 2339;

fn missing_property_count(source: &str) -> usize {
    js_codes(source)
        .into_iter()
        .filter(|c| *c == MISSING_PROPERTY)
        .count()
}

#[test]
fn function_and_value_writes_are_both_reported() {
    let source = concat!(
        "module.exports = function () { }\n",
        "module.exports.f = function (a) { };\n",
        "module.exports.g = 1;\n",
    );
    assert_eq!(missing_property_count(source), 2);
}

/// The function-expression write alone — the case that used to be silent.
#[test]
fn function_expression_write_is_reported() {
    let source = "module.exports = function () { }\nmodule.exports.f = function (a) { };\n";
    assert!(js_codes(source).contains(&MISSING_PROPERTY));
}

/// Renamed export and helper: the rule is structural.
#[test]
fn function_expression_write_is_reported_renamed() {
    let source = "module.exports = function () { }\nmodule.exports.handler = function (q) { };\n";
    assert!(js_codes(source).contains(&MISSING_PROPERTY));
}

/// An arrow RHS also re-enters and must report.
#[test]
fn arrow_write_is_reported() {
    let source = "module.exports = function () { }\nmodule.exports.f = (a) => a;\n";
    assert!(js_codes(source).contains(&MISSING_PROPERTY));
}

// --- Files without a whole-module export are unaffected. ---

/// Plain named exports with no `module.exports = X` keep working: the members
/// are the module's exports, so reading them is fine.
#[test]
fn named_exports_without_whole_module_export_are_unaffected() {
    let source = concat!(
        "exports.a = function () { };\n",
        "exports.b = 1;\n",
        "exports.a;\n",
        "exports.b;\n",
    );
    assert_eq!(missing_property_count(source), 0);
}

/// A whole-module object-literal export still exposes its own members.
#[test]
fn object_literal_whole_module_export_keeps_its_members() {
    let source = "module.exports = { a: 1 };\nmodule.exports.a;\n";
    assert_eq!(missing_property_count(source), 0);
}
