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

/// Same as `diagnostics_for_js`, but with `noImplicitAny` on. tsc's
/// `typeFromPropertyAssignment*` salsa fixtures pin the prototype-literal
/// write behavior only under `@strict: false`; the diagnostic these display
/// tests exercise only fires once `noImplicitAny` is on (#17226 gap 2).
fn diagnostics_for_js_no_implicit_any(source: &str) -> Vec<(u32, String)> {
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
    let diags = diagnostics_for_js_no_implicit_any(
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
    let diags = diagnostics_for_js_no_implicit_any(
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
#[test]
fn ts2339_renamed_prototype_methods_use_literal_shape() {
    let diags = diagnostics_for_js_no_implicit_any(
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
// A non-empty prototype literal closes the prototype for EVERY function —
// constructor evidence is irrelevant; `noImplicitAny` is the only gate.
// =========================================================================
//
// Oracle-verified (tsconfig-sentinel method, typescript@7.0.2, both
// `noImplicitAny` configs): `X.prototype = { ... }` with a non-empty literal
// closes the prototype's shape for a plain `function F() {}` exactly the same
// as for a real `isJSConstructor` owner (`@constructor` JSDoc tag, or a body
// with `this.x = ...` assignments). A later `X.prototype.y = ...` write to an
// undeclared member is `TS2339` when `noImplicitAny` is on, and silently
// accepted (the JS open-container leniency) when it is off, for BOTH owner
// kinds. There is no owner-kind distinction in tsc here at all.
//
// The corpus salsa fixtures this file used to cite as evidence for a
// plain-function exemption (`typeFromPropertyAssignment11`/`13`) are pinned
// `// @strict: false` — they only ever exercise the `noImplicitAny`-off
// silence, never the `-on` firing, so they were never evidence for an
// owner-kind distinction; both configs were re-verified directly against the
// pinned oracle for this correction (#17226 gap 2).

fn ts2339_names(diags: &[(u32, String)]) -> Vec<String> {
    diags
        .iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, message)| message.clone())
        .collect()
}

const PLAIN_OWNER_SOURCES: [&str; 2] = [
    "function I() {}\nI.prototype = { m() {} };\nI.prototype.j = 2;\n",
    "var I = function() {};\nI.prototype = { m() {} };\nI.prototype.j = 2;\n",
];

const CONSTRUCTOR_OWNER_SOURCES: [&str; 2] = [
    "/** @constructor */\nvar M = function() {};\nM.prototype = { set: function() {} };\nM.prototype.addon = function () {};\n",
    "var M = function() { this._map = {}; };\nM.prototype = { set: function() {} };\nM.prototype.addon = function () {};\n",
];

#[test]
fn ts2339_prototype_write_is_silent_without_no_implicit_any() {
    for source in PLAIN_OWNER_SOURCES
        .iter()
        .chain(CONSTRUCTOR_OWNER_SOURCES.iter())
    {
        let diags = diagnostics_for_js(source);
        assert!(
            ts2339_names(&diags).is_empty(),
            "source={source:?} must not report TS2339 without noImplicitAny; got {:?}",
            ts2339_names(&diags)
        );
    }
}

#[test]
fn ts2339_prototype_write_is_reported_under_no_implicit_any() {
    for source in PLAIN_OWNER_SOURCES
        .iter()
        .chain(CONSTRUCTOR_OWNER_SOURCES.iter())
    {
        let diags = diagnostics_for_js_no_implicit_any(source);
        assert!(
            !ts2339_names(&diags).is_empty(),
            "source={source:?} must report TS2339 under noImplicitAny (owner-kind is irrelevant); got {:?}",
            ts2339_names(&diags)
        );
    }
}

#[test]
fn ts2339_prototype_write_no_implicit_any_is_name_independent() {
    for (ctor, prop) in [("I", "j"), ("Widget", "extra"), ("_a0", "_b1")] {
        let source = format!(
            "var {ctor} = function() {{}};\n{ctor}.prototype = {{ m() {{}} }};\n{ctor}.prototype.{prop} = 2;\n"
        );
        assert!(
            ts2339_names(&diagnostics_for_js_no_implicit_any(&source)).len() == 1,
            "ctor={ctor} prop={prop} must report exactly one TS2339 under noImplicitAny"
        );
        assert!(
            ts2339_names(&diagnostics_for_js(&source)).is_empty(),
            "ctor={ctor} prop={prop} must stay silent without noImplicitAny"
        );
    }
    let nested = "var O = {};\nO.Inner = function() {};\nO.Inner.prototype = { m() {} };\nO.Inner.prototype.j = 2;\n";
    assert_eq!(
        ts2339_names(&diagnostics_for_js_no_implicit_any(nested)).len(),
        1,
        "nested owner must report TS2339 under noImplicitAny"
    );
    assert!(
        ts2339_names(&diagnostics_for_js(nested)).is_empty(),
        "nested owner must stay silent without noImplicitAny"
    );
}

#[test]
fn ts2339_prototype_write_is_not_reported_when_the_literal_declares_it() {
    // Control: a write of a property the literal already declares is fine on
    // either owner kind, in both `noImplicitAny` configs.
    for owner in [
        "var M = function() { this._map = {}; };",
        "var M = function() {};",
    ] {
        let source = format!(
            "{owner}\nM.prototype = {{ set: function() {{}} }};\nM.prototype.set = function () {{}};\n"
        );
        assert!(
            ts2339_names(&diagnostics_for_js_no_implicit_any(&source)).is_empty(),
            "owner={owner} redeclaring `set` must not report under noImplicitAny; got {:?}",
            ts2339_names(&diagnostics_for_js_no_implicit_any(&source))
        );
        assert!(
            ts2339_names(&diagnostics_for_js(&source)).is_empty(),
            "owner={owner} redeclaring `set` must not report without noImplicitAny"
        );
    }
}

#[test]
fn ts2339_empty_prototype_literal_stays_open_under_no_implicit_any() {
    // An EMPTY prototype literal (`G.prototype = {}`) never closes the
    // prototype — every later write is a fresh expando declaration, even
    // under noImplicitAny (#17226's core emptiness rule, applied at the
    // prototype level).
    for owner in ["function G() {}", "var G = function() { this._x = 1; };"] {
        let source = format!("{owner}\nG.prototype = {{}};\nG.prototype.p = 1;\n");
        assert!(
            ts2339_names(&diagnostics_for_js_no_implicit_any(&source)).is_empty(),
            "owner={owner} empty prototype literal must stay open under noImplicitAny; got {:?}",
            ts2339_names(&diagnostics_for_js_no_implicit_any(&source))
        );
    }
}
