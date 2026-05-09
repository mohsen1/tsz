//! TS2339 receiver display for `Foo.prototype.X = ...` after a literal
//! prototype assignment.
//!
//! Regression for `typeFromPrototypeAssignment2.ts`: when a JS function's
//! prototype is assigned an object literal (`Foo.prototype = { a, b }`) and a
//! later statement writes a property not declared in that literal
//! (`Foo.prototype.c = ...`), tsc emits TS2339 with the literal's structural
//! shape as the receiver display: `{ a: () => void; b(): void; }`. Following
//! a `display_alias` to the constructor's `prototype` symbol — which can be
//! recorded incidentally by the type system, especially for nested
//! constructors inside an IIFE — produces a misleading "type 'prototype'"
//! display that does not match tsc.

use tsz_checker::context::CheckerOptions;

fn diagnostics_for_js(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(
        source,
        "test.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_implicit_any: false,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn assert_prototype_addon_message_is_structural(diags: &[(u32, String)]) {
    let ts2339_addon: Vec<_> = diags
        .iter()
        .filter(|(c, m)| *c == 2339 && m.contains("'addon'"))
        .collect();
    assert!(
        !ts2339_addon.is_empty(),
        "expected TS2339 for `addon`; got: {diags:?}"
    );
    for (_, msg) in &ts2339_addon {
        assert!(
            msg.contains("set: () => void") && msg.contains("get(): void"),
            "TS2339 receiver must be the prototype literal's structural shape; got: {msg:?}",
        );
        assert!(
            !msg.contains("type 'prototype'"),
            "TS2339 receiver must not display as the constructor's prototype symbol; got: {msg:?}",
        );
    }
}

/// Top-level salsa form: `var X = function() {}; X.prototype = {...}; X.prototype.Y = ...`.
/// The receiver of the TS2339 must be the literal's shape, never `'prototype'`.
#[test]
fn ts2339_top_level_prototype_property_assignment_uses_literal_shape() {
    let diags = diagnostics_for_js(
        r#"
/** @constructor */
var Multimap = function() {};

Multimap.prototype = {
    set: function() {},
    get() {}
};

Multimap.prototype.addon = function () {};
"#,
    );
    assert_prototype_addon_message_is_structural(&diags);
}

/// IIFE-wrapped salsa form: same shape, nested inside `(function () { ... })`.
/// Earlier code paths only located the prototype owner via `file_locals`, so
/// the constructor inside an IIFE was invisible and the literal type ended up
/// displaying as `'prototype'` via a `display_alias` redirect. Resolving the
/// owner through normal scope lookup keeps this case structural too.
#[test]
fn ts2339_nested_iife_prototype_property_assignment_uses_literal_shape() {
    let diags = diagnostics_for_js(
        r#"
(function container() {
    /** @constructor */
    var Multimap = function() {};

    Multimap.prototype = {
        set: function() {},
        get() {}
    };

    Multimap.prototype.addon = function () {};
});
"#,
    );
    assert_prototype_addon_message_is_structural(&diags);
}

/// A different iteration variable name — `K`/`P`/`X` instead of the
/// idiomatic `set`/`get` — must produce the same structural display: the
/// rule is about the receiver shape, not about specific identifier names.
#[test]
fn ts2339_renamed_prototype_methods_use_literal_shape() {
    let diags = diagnostics_for_js(
        r#"
function C() {}
C.prototype = {
    one: function() {},
    two() {}
};
C.prototype.three = function () {};
"#,
    );
    let ts2339_three: Vec<_> = diags
        .iter()
        .filter(|(c, m)| *c == 2339 && m.contains("'three'"))
        .collect();
    assert!(
        !ts2339_three.is_empty(),
        "expected TS2339 for `three`; got: {diags:?}"
    );
    for (_, msg) in &ts2339_three {
        assert!(
            msg.contains("one: () => void") && msg.contains("two(): void"),
            "TS2339 receiver must be the renamed literal's structural shape; got: {msg:?}",
        );
        assert!(
            !msg.contains("type 'prototype'"),
            "TS2339 must not display as 'prototype'; got: {msg:?}",
        );
    }
}
