//! Regression tests for TS2339 receiver display when `this` references inside
//! a JS function declaration that has expando-style property assignments.
//!
//! In JS files, an expando-pattern function like:
//!
//! ```js
//! function toString() {
//!     this.yadda            // <-- TS2339
//!     this.someValue = "";
//! }
//! ```
//!
//! creates `this`-typed properties. tsc's TS2339 message displays the
//! receiver as the function's name (`'toString'`), not as the inferred
//! expando object shape (`'{ someValue: string; }'`). Source:
//! `compiler/inexistentPropertyInsideToStringType.ts`. See also
//! `crates/tsz-checker/src/error_reporter/properties.rs` —
//! `js_constructor_receiver_display_for_node` now falls back to the
//! enclosing function's name when no prototype-owner expression is found.

use crate::test_utils::check_js_source_diagnostics;

fn ts2339_messages_for_js(source: &str) -> Vec<String> {
    check_js_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2339)
        .map(|d| d.message_text)
        .collect()
}

/// Property access on `this.yadda` inside a JS expando-pattern function
/// reports the function's name as the receiver type.
#[test]
fn ts2339_displays_function_name_for_this_in_js_expando_function() {
    let source = "\
function toString() {
    this.yadda;
    this.someValue = \"\";
}
";
    let messages = ts2339_messages_for_js(source);
    assert_eq!(messages.len(), 1, "Expected one TS2339, got: {messages:?}");
    let msg = &messages[0];
    assert!(
        msg.contains("'toString'"),
        "TS2339 message must name the function as the receiver type. Got: {msg:?}"
    );
    assert!(
        !msg.contains("someValue"),
        "TS2339 message must not expose the inferred expando shape. Got: {msg:?}"
    );
}

/// Anti-hardcoding cover: same shape with a different (non-builtin) function
/// name proves the helper isn't matching one specific name.
#[test]
fn ts2339_displays_function_name_for_this_in_js_expando_function_renamed() {
    let source = "\
function widgetSetup() {
    this.unknownProp;
    this.title = \"hello\";
}
";
    let messages = ts2339_messages_for_js(source);
    assert_eq!(messages.len(), 1, "Expected one TS2339, got: {messages:?}");
    let msg = &messages[0];
    assert!(
        msg.contains("'widgetSetup'"),
        "Renamed variant: TS2339 must use the new function name as the receiver. Got: {msg:?}"
    );
}
