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

/// A different iteration variable name — `one`/`two`/`three` instead of the
/// idiomatic `set`/`get` — must produce the same structural display: the
/// rule is about the receiver shape, not about specific identifier names.
///
/// `C` has to be a JS *constructor* for the prototype to be closed at all;
/// `this._m = {}` supplies the symbol members that make it one. A plain
/// `function C() {}` owner is an open prototype and correctly reports
/// nothing — see `ts2339_plain_function_prototype_write_is_not_reported`.
#[test]
fn ts2339_renamed_prototype_methods_use_literal_shape() {
    let diags = diagnostics_for_js(
        r#"
function C() { this._m = {}; }
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

// =========================================================================
// Only a JS *constructor*'s prototype is closed by its object literal
// =========================================================================
//
// tsc's `isJSConstructor`: the owner function carries a `@constructor`
// (`@class`) JSDoc tag, or its symbol has members — which for a JS function
// means the body performs `this.x = ...` assignments. For such an owner,
// `X.prototype = { ... }` establishes the complete prototype and a later
// `X.prototype.y = ...` writing an undeclared property is TS2339. For a plain
// function the write is an ordinary prototype-property declaration that merges
// with the literal, and reporting it is a false positive
// (`typeFromPropertyAssignment11`/`13` expect no diagnostics at all).

fn ts2339_names(source: &str) -> Vec<String> {
    diagnostics_for_js(source)
        .into_iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, message)| message)
        .collect()
}

#[test]
fn ts2339_plain_function_prototype_write_is_not_reported() {
    // Empty-bodied function owner: open prototype, the write declares `j`.
    for owner in ["function I() {}", "var I = function() {};"] {
        let source = format!("{owner}\nI.prototype = {{ m() {{}} }};\nI.prototype.j = 2;\n");
        assert!(
            ts2339_names(&source).is_empty(),
            "owner={owner} must not report TS2339; got {:?}",
            ts2339_names(&source)
        );
    }
}

#[test]
fn ts2339_plain_function_prototype_write_is_name_independent() {
    // Same shape under renamed binders and a nested owner.
    for (ctor, prop) in [("I", "j"), ("Widget", "extra"), ("_a0", "_b1")] {
        let source = format!(
            "var {ctor} = function() {{}};\n{ctor}.prototype = {{ m() {{}} }};\n{ctor}.prototype.{prop} = 2;\n"
        );
        assert!(
            ts2339_names(&source).is_empty(),
            "ctor={ctor} prop={prop} must not report TS2339"
        );
    }
    let nested = "var O = {};\nO.Inner = function() {};\nO.Inner.prototype = { m() {} };\nO.Inner.prototype.j = 2;\n";
    assert!(
        ts2339_names(nested).is_empty(),
        "nested owner must not report TS2339"
    );
}

#[test]
fn ts2339_js_constructor_prototype_write_is_still_reported() {
    // Both disjuncts of `isJSConstructor` keep the prototype closed.
    let via_tag = "/** @constructor */\nvar M = function() {};\nM.prototype = { set: function() {} };\nM.prototype.addon = function () {};\n";
    let via_members = "var M = function() { this._map = {}; };\nM.prototype = { set: function() {} };\nM.prototype.addon = function () {};\n";
    for (label, source) in [
        ("@constructor tag", via_tag),
        ("this.x members", via_members),
    ] {
        let messages = ts2339_names(source);
        assert!(
            messages.iter().any(|m| m.contains("'addon'")),
            "{label}: a JS constructor's prototype stays closed; got {messages:?}"
        );
    }
}

#[test]
fn ts2339_prototype_write_is_not_reported_when_the_literal_declares_it() {
    // Control: a write of a property the literal already declares is fine on
    // either owner kind.
    for owner in [
        "var M = function() { this._map = {}; };",
        "var M = function() {};",
    ] {
        let source = format!(
            "{owner}\nM.prototype = {{ set: function() {{}} }};\nM.prototype.set = function () {{}};\n"
        );
        assert!(
            ts2339_names(&source).is_empty(),
            "owner={owner} redeclaring `set` must not report; got {:?}",
            ts2339_names(&source)
        );
    }
}

// =========================================================================
// A plain function's prototype closes too, but only under `noImplicitAny`
// =========================================================================
//
// Oracle-verified (`typescript@7.0.2`, tsconfig-sentinel method — see #17226's
// own scope-correction: bare CLI-arg invocation without a tsconfig behaves
// like `noImplicitAny` is always on and cannot distinguish these cases).
// Unlike a JS *constructor* (closed unconditionally, see
// `ts2339_js_constructor_prototype_write_is_still_reported` above), a plain
// function's non-empty `X.prototype = {...}` literal only closes the shape
// when `noImplicitAny` is on; under noImplicitAny false the write stays a
// silent, ordinary prototype-property declaration (matching
// `ts2339_plain_function_prototype_write_is_not_reported` above, which pins
// the noImplicitAny-false side of this same matrix).

fn ts2339_names_no_implicit_any(source: &str) -> Vec<String> {
    tsz_checker::test_utils::check_source(
        source,
        "test.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|d| d.code == 2339)
    .map(|d| d.message_text)
    .collect()
}

#[test]
fn ts2339_plain_function_prototype_write_is_reported_under_no_implicit_any() {
    for (owner_name, owner_decl, prop) in [
        ("I", "function I() {}", "j"),
        ("Widget", "var Widget = function() {};", "extra"),
    ] {
        let source = format!(
            "{owner_decl}\n{owner_name}.prototype = {{ m() {{}} }};\n{owner_name}.prototype.{prop} = 2;\n"
        );
        let messages = ts2339_names_no_implicit_any(&source);
        assert!(
            messages.iter().any(|m| m.contains(&format!("'{prop}'"))),
            "owner={owner_name} must report TS2339 for `{prop}` under noImplicitAny; got {messages:?}"
        );
    }
}

#[test]
fn ts2339_plain_function_prototype_write_stays_silent_with_empty_literal_under_no_implicit_any() {
    // The emptiness rule, not noImplicitAny alone, gates closing: an empty
    // `X.prototype = {}` keeps the prototype open even under noImplicitAny.
    let source = "function I() {}\nI.prototype = {};\nI.prototype.j = 2;\nI.prototype.j;\n";
    assert!(
        ts2339_names_no_implicit_any(source).is_empty(),
        "empty-literal prototype must stay open under noImplicitAny; got {:?}",
        ts2339_names_no_implicit_any(source)
    );
}
