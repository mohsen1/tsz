//! Method bodies of an object literal outside a class are checked.
//!
//! `skip_body_check` in `function_type.rs` is keyed on the node kind alone. Its
//! comment justifies skipping method bodies during type-environment building
//! because `check_class_member` re-checks them later with the class context
//! established. An object-literal method is also a `METHOD_DECLARATION` and gets
//! no such second pass, so its body was never checked — every diagnostic inside
//! `X.prototype = { m() { … } }` was dropped.
//!
//! Verified against the pinned tsc 7.0.2: it reports inside all of these.

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

const NOT_ASSIGNABLE: u32 = 2322;

/// The regressing shape: a whole-prototype object-literal assignment.
#[test]
fn prototype_object_literal_method_body_is_checked() {
    let source = concat!(
        "function M() { };\n",
        "M.prototype = {\n",
        "    m() {\n",
        "        /** @type {string} */\n",
        "        var s = 1;\n",
        "        return s;\n",
        "    }\n",
        "};\n",
        "M;\n",
    );
    assert!(js_codes(source).contains(&NOT_ASSIGNABLE));
}

/// A renamed constructor and method: the rule is structural.
#[test]
fn prototype_object_literal_method_body_is_checked_renamed() {
    let source = concat!(
        "function Widget() { };\n",
        "Widget.prototype = {\n",
        "    render() {\n",
        "        /** @type {number} */\n",
        "        var n = 'x';\n",
        "        return n;\n",
        "    }\n",
        "};\n",
        "Widget;\n",
    );
    assert!(js_codes(source).contains(&NOT_ASSIGNABLE));
}

/// An object literal assigned to a plain variable already worked; keep it.
#[test]
fn plain_object_literal_method_body_is_still_checked() {
    let source = concat!(
        "var o = {\n",
        "    m() {\n",
        "        /** @type {string} */\n",
        "        var s = 1;\n",
        "        return s;\n",
        "    }\n",
        "};\n",
        "o;\n",
    );
    assert!(js_codes(source).contains(&NOT_ASSIGNABLE));
}

/// A function expression assigned to a prototype member already worked too.
#[test]
fn prototype_member_function_expression_is_still_checked() {
    let source = concat!(
        "function M() { };\n",
        "M.prototype.m = function () {\n",
        "    /** @type {string} */\n",
        "    var s = 1;\n",
        "    return s;\n",
        "};\n",
        "M;\n",
    );
    assert!(js_codes(source).contains(&NOT_ASSIGNABLE));
}

/// A class method still reports through its own later pass — the narrowed skip
/// must not disturb the class path.
#[test]
fn class_method_body_is_still_checked() {
    let source = concat!(
        "class K {\n",
        "    m() {\n",
        "        /** @type {string} */\n",
        "        var s = 1;\n",
        "        return s;\n",
        "    }\n",
        "}\n",
        "new K();\n",
    );
    assert!(js_codes(source).contains(&NOT_ASSIGNABLE));
}
