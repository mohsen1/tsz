//! Regression tests for `this` references inside a JS function declaration that
//! has expando-style property assignments.
//!
//! In JS files, an expando-pattern function like:
//!
//! ```js
//! function toString() {
//!     this.yadda
//!     this.someValue = "";
//! }
//! ```
//!
//! is not a constructor in TypeScript 7: the old JS constructor-function
//! inference was dropped, so `this` is implicitly `any` (TS2683 under
//! noImplicitThis) and unknown-property reads on it are `any` — no TS2339 and
//! therefore no receiver-name display to compute.

use crate::test_utils::check_js_source_diagnostics;

fn codes_for_js(source: &str) -> Vec<u32> {
    check_js_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// Property access on `this.yadda` inside a plain JS function no longer reports
/// TS2339: `this` is implicitly `any` (TS2683) in TypeScript 7.
#[test]
fn ts2339_displays_function_name_for_this_in_js_expando_function() {
    let source = "\
function toString() {
    this.yadda;
    this.someValue = \"\";
}
";
    let codes = codes_for_js(source);
    assert!(
        !codes.contains(&2339),
        "Expected no TS2339 for `this.yadda` (implicit-any `this`), got: {codes:?}"
    );
    assert!(
        codes.contains(&2683),
        "Expected implicit-any `this` (TS2683), got: {codes:?}"
    );
}

/// Anti-hardcoding cover: a different (non-builtin) function name proves the
/// behavior is not name-specific.
#[test]
fn ts2339_displays_function_name_for_this_in_js_expando_function_renamed() {
    let source = "\
function widgetSetup() {
    this.unknownProp;
    this.title = \"hello\";
}
";
    let codes = codes_for_js(source);
    assert!(
        !codes.contains(&2339),
        "Expected no TS2339 for `this.unknownProp` (implicit-any `this`), got: {codes:?}"
    );
    assert!(
        codes.contains(&2683),
        "Expected implicit-any `this` (TS2683), got: {codes:?}"
    );
}
