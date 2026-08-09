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
    let diags = diagnostics_for_js(
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

    // The static chained assignment `A.s = A.t = function g(m) { ... this.x }`
    // binds `this` to `typeof A`, so `this.x` reports TS2339 against `typeof A`.
    assert!(
        diags.iter().any(|(code, message)| {
            *code == 2339 && message == "Property 'x' does not exist on type 'typeof A'."
        }),
        "static chained assignment should type `this` as typeof A; got: {diags:?}"
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
    let diags = diagnostics_for_js(
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

/// `tsc` 7.0.2 no longer synthesizes a `this` type for a plain JS "constructor"
/// function from its `this.prop = value` assignments (the `isJSConstructor`
/// inference was dropped in TS7), so `this` in the body is untyped and
/// `this.x = false` is never cross-checked against the `C.prototype.x` JSDoc
/// declaration. For
/// `function C() { this.x = false; } /** @type {number} */ C.prototype.x; new C().x;`
/// the checked-in oracle
/// (`conformance/jsdoc/jsdocPrototypePropertyAccessWithType.ts` in
/// `scripts/conformance/tsc-cache-full.json`) reports `[TS2683, TS7009]` and
/// **no TS2322** — the corpus `.errors.txt` (TS2322) is a stale pre-TS7
/// baseline. #17040 removed the stale cross-check on the primary path and
/// rewrote its sibling test
/// (`jsdoc_typed_prototype_does_not_cross_check_constructor_this_assignment`);
/// this mirror in the display suite was missed (#17048). tsz's in-process
/// check surfaces the TS2683 companion (`this` implicitly `any`) here.
#[test]
fn jsdoc_prototype_property_access_decl_does_not_cross_check_constructor_assignment() {
    let diags = diagnostics_for_js(
        r#"
function C() { this.x = false; }
/** @type {number} */
C.prototype.x;
new C().x;
"#,
    );
    assert!(
        !diags.iter().any(|(c, _)| *c == 2322),
        "tsc 7.0.2 does not cross-check a constructor's `this.x` assignment against the \
         `C.prototype.x` JSDoc type when `this` is untyped (oracle: [2683, 7009], no 2322); \
         got: {diags:?}"
    );
    assert!(
        diags.iter().any(|(c, _)| *c == 2683),
        "expected the companion TS2683 ('this' implicitly has type 'any') that evidences the \
         untyped `this`; got: {diags:?}"
    );
}
