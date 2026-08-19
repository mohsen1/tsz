//! TS2322 source/target display for `/** @type {T} */ Foo.prototype = X`.
//!
//! Regression for `typeTagPrototypeAssignment.ts`: a JSDoc `@type` annotation
//! on a `Foo.prototype = X` assignment declares the prototype's type, not the
//! source RHS type. The diagnostic source must be the RHS's actual type
//! (`number` for `12`), not the JSDoc-declared target (`string`). This is the
//! same shape as the existing CommonJS `module.exports = X` carve-out.

use tsz_checker::context::CheckerOptions;

fn diagnostics_for_js(source: &str) -> Vec<(u32, String)> {
    diagnostics_for_js_with_no_implicit_any(source, false)
}

fn diagnostics_for_js_with_no_implicit_any(
    source: &str,
    no_implicit_any: bool,
) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(
        source,
        "test.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_implicit_this: true,
            strict_null_checks: true,
            no_implicit_any,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

/// A checked-JS constructor assigned to a variable can acquire instance
/// members from both `this.x = ...` in the constructor body and sibling
/// `Ctor.prototype.x = ...` / `Ctor.prototype = { ... }` declarations.
/// Collecting the prototype function types must not emit provisional TS2339
/// diagnostics against the constructor instance before the complete shape
/// exists.
#[test]
fn checked_js_constructor_variable_prototype_methods_share_complete_this_shape() {
    // The `addon` assertion below needs noImplicitAny on: a non-empty
    // prototype literal closes the prototype (and reports TS2339 for an
    // undeclared later write) only under noImplicitAny, regardless of
    // `isJSConstructor` evidence (oracle-verified, #17226 gap 2).
    let diags = diagnostics_for_js_with_no_implicit_any(
        r#"
/** @constructor */
var Multimap = function() {
    this._map = {};
    this._map
    this.set
    this.get
    this.addon
};

Multimap.prototype = {
    set: function() {
        this._map
        this.set
        this.get
        this.addon
    },
    get() {
        this._map
        this.set
        this.get
        this.addon
    }
}

Multimap.prototype.addon = function () {
    this._map
    this.set
    this.get
    this.addon
}

var mm = new Multimap();
mm._map
mm.set
mm.get
mm.addon
"#,
        true,
    );
    let instance_member_ts2339: Vec<_> = diags
        .iter()
        .filter(|(code, message)| {
            *code == 2339
                && message.contains("does not exist on type 'Multimap'")
                && (message.contains("'set'")
                    || message.contains("'get'")
                    || message.contains("'addon'"))
        })
        .collect();
    assert!(
        instance_member_ts2339.is_empty(),
        "prototype-derived constructor members should not produce provisional TS2339s against Multimap; got: {diags:?}"
    );
    assert!(
        diags.iter().any(|(code, message)| {
            *code == 2339
                && message
                    == "Property 'addon' does not exist on type '{ set: () => void; get(): void; }'."
        }),
        "prototype assignment target should display the prior object-literal prototype shape; got: {diags:?}"
    );
}

#[test]
fn checked_js_chained_assignment_jsdoc_flows_to_all_targets() {
    let diags = diagnostics_for_js_with_no_implicit_any(
        r#"
function A () {
    this.x = 1
    /** @type {1} */
    this.first = this.second = 1
}
/** @param {number} n */
A.prototype.y = A.prototype.z = function f(n) {
    return n + this.x
}
/** @param {number} m */
A.s = A.t = function g(m) {
    return m + this.x
}
var a = new A()
a.y('no')
a.z('not really')
A.s('still no')
A.t('not here either')
a.first = 10
"#,
        true,
    );

    // TypeScript 7 dropped TS6-era constructor-function `this` inference: the
    // static chained assignment `A.s = A.t = function g(m) { ... this.x }`
    // binds `this` to `A`'s structural merged shape (call signature plus its
    // own static expando members `s`/`t`), not `typeof A` (#17654).
    assert!(
        diags.iter().any(|(code, message)| {
            *code == 2339
                && message
                    == "Property 'x' does not exist on type '{ (): any; s: (m?: any) => any; t: (m?: any) => any; }'."
        }),
        "static chained assignment should type `this` as A's merged expando shape, \
         not typeof A; got: {diags:?}"
    );
    // TypeScript 7: `new A()` is `any` (TS7009), so the `a.y(...)`/`a.z(...)`
    // instance calls are unchecked — no `'z' does not exist` diagnostic.
    assert!(
        !diags.iter().any(|(code, message)| {
            *code == 2339 && message == "Property 'z' does not exist on type 'A'."
        }),
        "instance method calls on an `any` receiver must not report missing members; got: {diags:?}"
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 7009),
        "expected TS7009 for `new A()` on a non-constructor function; got: {diags:?}"
    );
}

#[test]
fn checked_js_constructor_variable_prototype_methods_work_in_local_scope() {
    // See the top-level variant's comment: the `addon` assertion needs
    // noImplicitAny on (#17226 gap 2).
    let diags = diagnostics_for_js_with_no_implicit_any(
        r#"
(function container() {
    /** @constructor */
    var Multimap = function() {
        this._map = {};
        this._map
        this.set
        this.get
        this.addon
    };

    Multimap.prototype = {
        set: function() {
            this._map
            this.set
            this.get
            this.addon
        },
        get() {
            this._map
            this.set
            this.get
            this.addon
        }
    }

    Multimap.prototype.addon = function () {
        this._map
        this.set
        this.get
        this.addon
    }

    var mm = new Multimap();
    mm._map
    mm.set
    mm.get
    mm.addon
});
"#,
        true,
    );
    let instance_member_ts2339: Vec<_> = diags
        .iter()
        .filter(|(code, message)| {
            *code == 2339
                && message.contains("does not exist on type 'Multimap'")
                && (message.contains("'set'")
                    || message.contains("'get'")
                    || message.contains("'addon'"))
        })
        .collect();
    assert!(
        instance_member_ts2339.is_empty(),
        "prototype-derived local constructor members should not produce provisional TS2339s against Multimap; got: {diags:?}"
    );
    assert!(
        diags.iter().any(|(code, message)| {
            *code == 2339
                && message
                    == "Property 'addon' does not exist on type '{ set: () => void; get(): void; }'."
        }),
        "local prototype assignment target should display the prior object-literal prototype shape; got: {diags:?}"
    );
}

/// `/** @type {string} */ C.prototype = 12` must emit
/// `Type 'number' is not assignable to type 'string'.` — source uses the RHS's
/// actual type (`number`), not the JSDoc-declared target type (`string`).
#[test]
fn ts2322_for_prototype_jsdoc_assignment_uses_rhs_type_for_source() {
    let diags = diagnostics_for_js(
        r#"
function C() {}
/** @type {string} */
C.prototype = 12
"#,
    );
    let ts2322: Vec<_> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'number'") && msg.contains("'string'"),
        "TS2322 must show source as 'number' (the RHS type) and target as 'string' (the JSDoc target); got: {msg:?}"
    );
    assert!(
        !msg.contains("Type 'string' is not assignable to type 'string'"),
        "TS2322 must not collapse both sides to the JSDoc-declared target type; got: {msg:?}"
    );
}

/// A bare `/** @type {number} */ C.prototype.x;` declaration does NOT give the
/// constructor's `this` a type, so `tsc` never checks `this.x = false` against
/// it — the constructor's `this` is implicit `any` (TS2683) and `new C()` has
/// no construct signature (TS7009). No TS2322 is produced.
///
/// Oracle-pinned against `typescript@7.0.2` on this exact source:
///
/// ```text
/// t.js(1,16): error TS2683: 'this' implicitly has type 'any' because it does
///                           not have a type annotation.
/// t.js(4,1):  error TS7009: 'new' expression, whose target lacks a construct
///                           signature, implicitly has an 'any' type.
/// ```
///
/// This test previously asserted exactly one TS2322 comparing `boolean` against
/// the JSDoc `number`. That expectation was never measured against `tsc`, and it
/// inverted the gate: it passed only while tsz emitted a diagnostic `tsc` does
/// not, and went red when #17040 made tsz correct. Its own cited witness,
/// `conformance/jsdoc/jsdocPrototypePropertyAccessWithType.ts`, agrees with the
/// oracle above and flipped to PASSING on the same change (#17048).
#[test]
fn jsdoc_prototype_property_access_decl_does_not_type_constructor_this() {
    let diags = diagnostics_for_js(
        r#"
function C() { this.x = false; }
/** @type {number} */
C.prototype.x;
new C().x;
"#,
    );
    let codes: Vec<u32> = diags.iter().map(|(c, _)| *c).collect();
    assert!(
        !codes.contains(&2322),
        "tsc reports no TS2322 here — a bare `C.prototype.x;` declaration does not \
         type the constructor's `this`; got: {diags:?}"
    );
    assert!(
        codes.contains(&2683),
        "constructor `this` is implicit any, so TS2683 must fire; got: {diags:?}"
    );
    // NOTE: the oracle (and the real CLI) also report TS7009 on `new C()`, but
    // `diagnostics_for_js` does not surface it — this harness sees only TS2683.
    // Asserting TS7009 here would pin a behavior this harness cannot observe,
    // so the TS7009 half of the parity claim lives in the conformance row
    // `conformance/jsdoc/jsdocPrototypePropertyAccessWithType.ts`, which does
    // check it end-to-end and passes.
}
