//! CommonJS named exports are typed from every assignment, not the prior ones.
//!
//! `tsc` gives `exports.x` a declaration-level type: every assignment in the
//! module contributes regardless of position. So a call written before the
//! assignment is fine, and an early `= undefined` does not make the property
//! possibly-undefined once a real value is assigned later.
//!
//! Verified against the pinned tsc 7.0.2 (`--allowJs --checkJs --strict`):
//!
//! ```text
//! exports.f = undefined; exports.f(); … exports.f = a;  -> nothing
//! exports.f(); … exports.f = a;                          -> nothing
//! exports.f = undefined; exports.f();                    -> TS2722 (only contributor)
//! ```

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn js_codes(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

const POSSIBLY_UNDEFINED_CALL: u32 = 2722;

// --- A later assignment supplies the type. ---

/// Witness `moduleExportDuplicateAlias`: the `undefined` assignment does not
/// make the export possibly-undefined once a function is assigned later.
#[test]
fn undefined_then_function_is_not_possibly_undefined() {
    let source = concat!(
        "exports.apply = undefined;\n",
        "function a() { }\n",
        "exports.apply()\n",
        "exports.apply = a;\n",
        "exports.apply()\n",
    );
    assert!(!js_codes(source).contains(&POSSIBLY_UNDEFINED_CALL));
}

/// The same with a renamed export and helper, so the rule is structural.
#[test]
fn undefined_then_function_is_not_possibly_undefined_renamed() {
    let source = concat!(
        "exports.handler = undefined;\n",
        "function h() { }\n",
        "exports.handler()\n",
        "exports.handler = h;\n",
    );
    assert!(!js_codes(source).contains(&POSSIBLY_UNDEFINED_CALL));
}

#[test]
fn call_before_any_assignment_is_accepted() {
    let source = "exports.f()\nfunction a() { }\nexports.f = a;\n";
    assert!(!js_codes(source).contains(&POSSIBLY_UNDEFINED_CALL));
}

// --- `undefined` as the only contributor still reports. ---

/// Nothing else is assigned, so the declared type really is `undefined` and tsc
/// reports here too.
#[test]
fn undefined_as_sole_assignment_still_reports() {
    let source = "exports.f = undefined;\nexports.f()\n";
    assert!(js_codes(source).contains(&POSSIBLY_UNDEFINED_CALL));
}

// --- Ordinary shapes stay silent. ---

#[test]
fn assignment_before_use_is_silent() {
    let source = "function a() { }\nexports.f = a;\nexports.f()\n";
    assert!(!js_codes(source).contains(&POSSIBLY_UNDEFINED_CALL));
}

/// A genuinely non-callable export still reports its own error rather than being
/// silenced by the declaration-level lookup.
#[test]
fn non_callable_export_still_reports() {
    let source = "exports.n = 1;\nexports.n()\n";
    let codes = js_codes(source);
    assert!(codes.contains(&2349) || codes.contains(&2348));
}
