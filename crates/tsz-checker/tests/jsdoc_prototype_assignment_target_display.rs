//! TS2322 source/target display for `/** @type {T} */ Foo.prototype = X`.
//!
//! Regression for `typeTagPrototypeAssignment.ts`: a JSDoc `@type` annotation
//! on a `Foo.prototype = X` assignment declares the prototype's type, not the
//! source RHS type. The diagnostic source must be the RHS's actual type
//! (`number` for `12`), not the JSDoc-declared target (`string`). This is the
//! same shape as the existing CommonJS `module.exports = X` carve-out.

use rustc_hash::FxHashSet;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostics_for_js(source: &str) -> Vec<(u32, String)> {
    diagnostics_for_js_with_no_implicit_any(source, false)
}

fn diagnostics_for_js_with_no_implicit_any(
    source: &str,
    no_implicit_any: bool,
) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        no_implicit_this: true,
        strict_null_checks: true,
        no_implicit_any,
        ..CheckerOptions::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.js".to_string(),
        options,
    );
    let _: FxHashSet<u32> = FxHashSet::default(); // keep import alive in case
    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
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

    assert!(
        diags.iter().any(|(code, message)| {
            *code == 2339 && message == "Property 'x' does not exist on type 'typeof A'."
        }),
        "static chained assignment should type `this` as typeof A; got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|(code, message)| {
            *code == 2339 && message == "Property 'z' does not exist on type 'A'."
        }),
        "prototype chained assignment should declare both prototype targets; got: {diags:?}"
    );
    assert!(
        diags
            .iter()
            .filter(|(code, message)| {
                *code == 2345
                    && message
                        == "Argument of type 'string' is not assignable to parameter of type 'number'."
            })
            .count()
            >= 2,
        "prototype chained callable targets should preserve the JSDoc parameter type; got: {diags:?}"
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
