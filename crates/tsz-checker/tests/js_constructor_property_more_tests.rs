//! Additional tests for JS constructor property and prototype inference.

mod js_constructor_property_support;

use js_constructor_property_support::*;
use tsz_checker::context::CheckerOptions;

#[test]
fn test_js_prototype_object_function_properties_keep_constructor_this_and_ts7006() {
    let source = r#"
function Color(obj) {
    this.example = true;
}
Color.prototype = {
    negate: function () { return this; },
    lighten: function (ratio) { return this; },
    darken: function (ratio) { return this; },
    saturate: function (ratio) { return this; },
    desaturate: function (ratio) { return this; },
    whiten: function (ratio) { return this; },
    blacken: function (ratio) { return this; },
    greyscale: function () { return this; },
    clearer: function (ratio) { return this; },
    toJSON: function () { return this.rgb(); },
};
"#;

    let diagnostics = check_js(source);
    let ts7006: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7006)
        .collect();
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();

    assert_eq!(
        ts7006.len(),
        8,
        "Expected TS7006 for obj plus every unannotated prototype-function ratio parameter, got: {diagnostics:?}"
    );
    assert!(
        ts7006
            .iter()
            .any(|(_, message)| message.contains("Parameter 'obj' implicitly has an 'any' type.")),
        "Expected TS7006 for the constructor parameter, got: {diagnostics:?}"
    );
    assert_eq!(
        ts7006
            .iter()
            .filter(|(_, message)| {
                message.contains("Parameter 'ratio' implicitly has an 'any' type.")
            })
            .count(),
        7,
        "Expected TS7006 for each unannotated prototype-function ratio parameter, got: {diagnostics:?}"
    );
    assert_eq!(
        ts2339.len(),
        1,
        "Expected a single missing-member error for this.rgb(), got: {diagnostics:?}"
    );
    assert!(
        ts2339[0]
            .1
            .contains("Property 'rgb' does not exist on type 'Color'."),
        "Expected the prototype-function receiver to display as Color, got: {diagnostics:?}"
    );
}

#[test]
fn test_plain_function_prototype_object_literal_private_methods_report_without_crashing() {
    let source = r#"
function A() {}
A.prototype = {
    #x: 1,
    #m() {},
    get #p() { return ""; }
};
"#;

    let diagnostics = check_js(source);

    assert!(
        diagnostics.is_empty(),
        "Expected checker-only pass to avoid recursive crashes on illegal private prototype members, got: {diagnostics:?}"
    );
}

/// Generic constructor function with @template: instance properties get instantiated types
#[test]
fn test_generic_constructor_function_template_instantiation() {
    let source = r#"
/**
 * @param {T} t
 * @template T
 */
function Zet(t) {
    /** @type {T} */
    this.u
    this.t = t
}
var z = new Zet(1)
z.t = 2
z.u = false
"#;
    let diagnostics = check_js(source);
    // z.u = false should produce TS2322: boolean not assignable to number
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly 1 TS2322 for 'z.u = false', got: {diagnostics:?}"
    );
    assert!(
        ts2322[0].1.contains("boolean"),
        "Expected error about boolean, got: {}",
        ts2322[0].1
    );
}

/// Generic constructor: z.t = 2 should not error (number assignable to number)
#[test]
fn test_generic_constructor_function_template_compatible_assignment() {
    let source = r#"
/**
 * @param {T} t
 * @template T
 */
function Zet(t) {
    this.t = t
}
var z = new Zet(1)
z.t = 2
"#;
    let diagnostics = check_js(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        0,
        "Expected no TS2322 for compatible assignment z.t = 2, got: {diagnostics:?}"
    );
}

#[test]
fn test_jsdoc_extends_type_args_specialize_inherited_constructor() {
    let source = r#"
/**
 * @template T
 * @param {T} flavour
 */
function Soup(flavour) {
    this.flavour = flavour
}

/** @extends {Soup<{ claim: "ignorant" | "malicious" }>} */
class Chowder extends Soup {
}

var chowder = new Chowder({ claim: "ignorant" });
chowder.flavour.claim
var errorNoArgs = new Chowder();
var errorArgType = new Chowder(0);
"#;

    let diagnostics = check_js(source);
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2339),
        "Expected JSDoc @extends type arguments to specialize inherited instance properties, got: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| { *code == 2554 && message.contains("Expected 1 arguments") }),
        "Expected inherited constructor arity to require Soup's parameter, got: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| { *code == 2345 && message.contains("not assignable") }),
        "Expected inherited constructor parameter type to use JSDoc @extends argument, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_class_method_jsdoc_params_check_against_constructor_prototype_method() {
    let source = r#"
/**
 * @constructor
 * @param {number} numberOxen
 */
function Wagon(numberOxen) {
    this.numberOxen = numberOxen
}
/** @param {*[]=} supplies */
Wagon.prototype.load = function (supplies) {
}
class Sql extends Wagon {
    /**
     * @param {string[]} files
     * @param {"csv" | "json"} format
     */
    load(files, format) {
    }
}
"#;

    let diagnostics = check_js(source);
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| { *code == 2416 && message.contains("Property 'load'") }),
        "Expected TS2416 for incompatible JSDoc-typed method override, got: {diagnostics:?}"
    );
}

#[test]
fn test_generic_constructor_function_template_self_alias_instantiation() {
    let source = r#"
/**
 * @param {T} t
 * @template T
 */
function Zet(t) {
    var self = this;
    self.t = t;
    /** @type {T} */
    self.u
}
var z = new Zet(1)
z.t = 2
z.u = false
"#;
    let diagnostics = check_js(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected generic self-alias constructor properties to stay visible, got: {diagnostics:?}"
    );
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly 1 TS2322 for 'z.u = false' through self-alias generic constructor, got: {diagnostics:?}"
    );
}

#[test]
fn test_generic_constructor_prototype_object_methods_allow_new_this_props() {
    let source = r#"
/**
 * @class
 * @template T
 * @param {T} t
 */
function Cp(t) {
    /** @type {this} */
    this.dit = this
    this.y = t
    /** @return {this} */
    this.m3 = () => this
}

Cp.prototype = {
    /** @return {this} */
    m4() {
        this.z = this.y; return this
    }
}

/**
 * @class
 * @template T
 * @param {T} t
 */
function Cpp(t) {
    this.y = t
}
/** @return {this} */
Cpp.prototype.m2 = function () {
    this.z = this.y; return this
}

var cp = new Cp(1)
var cpp = new Cpp(2)
cp.dit

/** @type {Cpp<number>} */
var cppn = cpp.m2()

/** @type {Cp<number>} */
var cpn = cp.m3()
/** @type {Cp<number>} */
var cpn = cp.m4()
"#;

    let diagnostics = check_js(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    let ts2403: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2403)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected generic constructor prototype methods to allow new `this` properties, got: {diagnostics:?}"
    );
    assert!(
        ts2403.is_empty(),
        "Expected JSDoc generic constructor instance types to stay stable across prototype methods, got: {diagnostics:?}"
    );
}

#[test]
fn test_generic_constructor_prototype_object_methods_keep_existing_member_checks() {
    let source = r#"
/**
 * @class
 * @template T
 * @param {T} t
 */
function Cp(t) {
    this.x = 1
    this.y = t
}

Cp.prototype = {
    m4() {
        this.x = "oops"
        this.z = this.y
        return this
    }
}
"#;

    let diagnostics = check_js(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, message)| {
            *code == 2322 && message.contains("string") && message.contains("number")
        })
        .collect();

    assert!(
        ts2339.is_empty(),
        "Expected prototype object literal expando writes to avoid TS2339, got: {diagnostics:?}"
    );
    assert_eq!(
        ts2322.len(),
        1,
        "Expected exactly one TS2322 for writing string into numeric `this.x`, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_class_can_extend_js_constructor_function() {
    let source = r#"
/**
 * @constructor
 * @param {number} numberOxen
 */
function Wagon(numberOxen) {
    this.numberOxen = numberOxen;
}
/** @param {*[]=} supplies */
Wagon.prototype.load = function (supplies) {};
class Sql extends Wagon {
    constructor() {
        super();
        this.foonly = 12;
    }
    /** @param {Array.<string>} files @param {"csv" | "json" | "xmlolololol"} format */
    load(files, format) {}
}
"#;
    let diagnostics = check_js(source);
    let ts2507: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2507)
        .collect();
    assert_eq!(
        ts2507.len(),
        0,
        "Expected JS class extends JS constructor function to avoid TS2507, got: {diagnostics:?}"
    );
}

#[test]
fn test_plain_js_function_constructor_is_constructable_and_types_this_properties() {
    let source = r#"
function A() {
    this.unknown = null;
    this.empty = [];
}
var a = new A();
a.unknown = 1;
a.empty;
"#;
    let diagnostics = check_js(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2683 | 7009 | 2339))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected plain JS function constructors to avoid TS2322/TS2683/TS7009/TS2339 on instance properties, got: {diagnostics:?}"
    );
}

#[test]
fn test_plain_js_function_constructor_initializers_widen_like_js() {
    let source = r#"
function A() {
    this.unknown = null;
    this.unknowable = undefined;
    this.empty = [];
}
var a = new A();
a.unknown = 1;
a.unknowable = "ok";
a.empty;
"#;
    let diagnostics = check_js(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2683 | 7009 | 2339))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected JS constructor null/undefined/[] initializers to widen for instance properties, got: {diagnostics:?}"
    );
}

#[test]
fn test_plain_js_function_constructor_void_zero_initializer_does_not_declare_property() {
    let source = r#"
exports.j = 1;
exports.k = void 0;
var o = {};
o.x = 1;
o.y = void 0;
o.x + o.y;

function C() {
    this.p = 1;
    this.q = void 0;
}
var c = new C();
c.p + c.q;
"#;
    let diagnostics = check_js(source);
    let q_missing: Vec<_> = diagnostics
        .iter()
        .filter(|(code, msg)| {
            *code == 2339 && msg.contains("Property 'q' does not exist on type 'C'.")
        })
        .collect();
    assert_eq!(
        q_missing.len(),
        2,
        "Expected TS2339 for both void-zero constructor assignment and later property access, got: {diagnostics:?}"
    );
}

#[test]
fn test_plain_js_function_constructor_provisional_initializers_emit_ts7008_in_check_js() {
    let source = r#"
function A() {
    this.unknown = null;
    this.unknowable = undefined;
    this.empty = [];
}
"#;
    let diagnostics = check_js_with_options(
        source,
        CheckerOptions {
            check_js: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    let ts7008_messages: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7008)
        .map(|(_, msg)| msg.as_str())
        .collect();
    assert!(
        ts7008_messages
            .iter()
            .any(|msg| msg.contains("Member 'unknown' implicitly has an 'any' type.")),
        "Expected TS7008 for JS null-initialized constructor property, got: {diagnostics:?}"
    );
    assert!(
        ts7008_messages
            .iter()
            .any(|msg| msg.contains("Member 'unknowable' implicitly has an 'any' type.")),
        "Expected TS7008 for JS undefined-initialized constructor property, got: {diagnostics:?}"
    );
    assert!(
        ts7008_messages
            .iter()
            .any(|msg| msg.contains("Member 'empty' implicitly has an 'any[]' type.")),
        "Expected TS7008 for JS empty-array constructor property, got: {diagnostics:?}"
    );
}

#[test]
fn test_plain_js_function_constructor_implicit_any_properties_keep_any_write_surface() {
    let source = r#"
function A() {
    this.unknown = null;
    this.unknowable = undefined;
    this.empty = [];
}
var a = new A();
a.unknown = 1;
a.unknown = true;
a.unknown = {};
a.unknown = "hi";
a.unknowable = 1;
a.unknowable = true;
a.unknowable = {};
a.unknowable = "hi";
a.empty.push(1);
a.empty.push(true);
a.empty.push({});
a.empty.push("hi");
"#;
    let diagnostics = check_js_with_options(
        source,
        CheckerOptions {
            check_js: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        0,
        "Expected JS implicit-any constructor properties to accept later writes, got: {diagnostics:?}"
    );
}

#[test]
fn test_checked_js_undefined_var_initializer_keeps_any_assignment_target() {
    let source = r#"
var u = undefined;
u = undefined;
u = 1;
u = true;
u = {};
u = "ok";
"#;
    let diagnostics = check_js_with_options(
        source,
        CheckerOptions {
            check_js: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        0,
        "Expected checked-JS undefined-initialized var writes to use any target, got: {diagnostics:?}"
    );
}

#[test]
fn test_plain_js_function_constructor_provisional_writes_merge_like_salsa() {
    let source = r#"
function Installer () {
    this.arg = 0;
    this.unknown = null;
    this.twice = undefined;
    this.twice = 'hi';
    this.twices = [];
    this.twices = null;
}
Installer.prototype.first = function () {
    this.arg = 'hi';
    this.unknown = 'hi';
    this.newProperty = 1;
    this.twice = undefined;
    this.twice = 'hi';
}
Installer.prototype.second = function () {
    this.arg = false;
    this.unknown = false;
    this.newProperty = false;
    this.twice = null;
    this.twice = false;
    this.twices.push(1);
    if (this.twices != null) {
        this.twices.push('hi');
    }
}
"#;
    let diagnostics = check_js_with_options(
        source,
        CheckerOptions {
            check_js: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    let codes: Vec<u32> = diagnostics.iter().map(|(code, _)| *code).collect();
    assert_eq!(
        codes.iter().filter(|&&code| code == 2322).count(),
        5,
        "Expected closed constructor properties to report the same assignment mismatches as tsc, got: {diagnostics:?}"
    );
    assert_eq!(
        codes.iter().filter(|&&code| code == 2531).count(),
        1,
        "Expected unchecked nullable constructor property access to report TS2531 once, got: {diagnostics:?}"
    );
    let ts7008_messages: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7008)
        .map(|(_, msg)| msg.as_str())
        .collect();
    assert!(
        ts7008_messages
            .iter()
            .any(|msg| msg.contains("Member 'twices' implicitly has an 'any[]' type.")),
        "Expected a constructor-origin TS7008 for twices, got: {diagnostics:?}"
    );
    assert!(
        ts7008_messages
            .iter()
            .all(|msg| !msg.contains("Member 'unknown' implicitly has an 'any' type.")),
        "Expected prototype writes to suppress the stale constructor TS7008 for unknown, got: {diagnostics:?}"
    );
    assert!(
        ts7008_messages
            .iter()
            .all(|msg| !msg.contains("Member 'twice' implicitly has an 'any' type.")),
        "Expected no prototype-method TS7008 duplication for twice, got: {diagnostics:?}"
    );
    let push_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(_, msg)| msg.contains("Property 'push' does not exist on type 'any[]'."))
        .collect();
    // The no-lib harness still lacks Array.prototype.push, but null narrowing
    // suppresses the narrowed-branch access error.
    assert_eq!(
        push_errors.len(),
        1,
        "Expected only the un-narrowed push access to fail in the no-lib harness, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_function_constructor_with_factory_guard_is_constructable() {
    let source = r#"
/** @param {number} x */
function A(x) {
    if (!(this instanceof A)) {
        return new A(x);
    }
    this.x = x;
}
var j = new A(2);
j.x;
"#;
    let diagnostics = check_js(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2683 | 7009 | 2339))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected JS constructor with factory guard to avoid TS2683/TS7009/TS2339, got: {diagnostics:?}"
    );
}

#[test]
fn test_variable_assigned_js_constructor_with_prototype_object_types_this_members() {
    let source = r#"
/** @constructor */
var Multimap = function() {
    this._map = {};
    this._map;
};
Multimap.prototype = {
    /** @param {number} n */
    set: function(n) {
        this._map;
    },
    get() {
        this._map;
    }
};
var mm = new Multimap();
mm._map;
mm.set(1);
mm.get();
"#;
    let diagnostics = check_js(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2683 | 7009 | 2339 | 7006))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected variable-assigned JS constructor with prototype object to avoid TS2683/TS7009/TS2339/TS7006, got: {diagnostics:?}"
    );
}

#[test]
fn test_jsdoc_constructor_tag_on_object_literal_method_keeps_object_literal_this_closed() {
    let source = r#"
const obj = {
    /** @constructor */
    Foo() { this.bar = "bar"; }
};
(new obj.Foo()).bar;
"#;
    let diagnostics = check_js_with_options(
        source,
        CheckerOptions {
            check_js: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );
    let ts2339_messages: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .map(|(_, msg)| msg.as_str())
        .collect();
    assert!(
        ts2339_messages
            .iter()
            .any(|msg| msg.contains("Property 'bar' does not exist on type '{ Foo(): void; }'.")),
        "Expected TS2339 on object-literal-owned `this.bar` inside JSDoc constructor-tagged method, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 7009),
        "Expected TS7009 for `new obj.Foo()` in object literal JSDoc constructor case, got: {diagnostics:?}"
    );
}

// === Computed property (element access) tests ===

/// this[symbolKey] = value in JS class constructor → no false TS2322
#[test]
fn test_js_constructor_element_access_symbol_key_no_false_error() {
    let source = r#"
const _sym = Symbol("_sym");
class MyClass {
    constructor() {
        this[_sym] = "ok";
    }
    method() {
        this[_sym] = "yep";
        const x = this[_sym];
    }
}
"#;
    let diagnostics = check_js(source);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322 || *code == 7053)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no TS2322/TS7053 for Symbol-keyed constructor property, got: {errors:?}"
    );
}

/// this[stringKey] = value in JS class constructor → no false TS7053
#[test]
fn test_js_constructor_element_access_string_key_no_false_error() {
    let source = r#"
const _key = "my-key";
class MyClass {
    constructor() {
        this[_key] = "ok";
    }
    method() {
        this[_key] = "yep";
        const x = this[_key];
    }
}
"#;
    let diagnostics = check_js(source);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7053 || *code == 2322)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no TS7053/TS2322 for string-keyed constructor property, got: {errors:?}"
    );
}

/// Non-literal computed keys on `this[...]` in JS should still report TS7053.
#[test]
fn test_js_constructor_element_access_computed_key_reports_ts7053() {
    let source = r#"
class MyClass {
    constructor() {
        this["a" + "b"] = 0;
    }
}
"#;
    let diagnostics = check_js(source);
    let ts7053: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7053)
        .collect();
    assert!(
        !ts7053.is_empty(),
        "Expected TS7053 for non-literal computed element assignment on `this`, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_top_level_this_property_assignment_declares_single_hop_properties() {
    let source = r#"
this.x = {};
this.x.y = {};

/** @constructor */
function F() {
  this.a = {};
  this.a.b = {};
}

const f = new F();
f.a;
"#;

    let diagnostics = check_js(source);

    assert_eq!(
        count_code(&diagnostics, 2339),
        2,
        "Expected only chained `this.prop.prop` writes to emit TS2339, got: {diagnostics:?}"
    );
}

#[test]
fn test_jsdoc_this_direct_write_checks_explicit_receiver_shape() {
    let source = r#"
/**
 * @this {{ ready: boolean }}
 */
function mark() {
  this.ready = true;
  this.missing = 1;
}

mark.call({ ready: false });
"#;

    let diagnostics = check_js(source);

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2339
                && message == "Property 'missing' does not exist on type '{ ready: boolean; }'."
        }),
        "Expected TS2339 for `this.missing` against the explicit @this receiver shape, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_chained_this_element_assignment_reports_ts7053() {
    let source = r#"
this["y"] = {};
this["y"]["z"] = {};

/** @constructor */
function F() {
  this["b"] = {};
  this["b"]["c"] = {};
}
"#;

    let diagnostics = check_js(source);

    assert_eq!(
        count_code(&diagnostics, 7053),
        2,
        "Expected chained `this[...]...[...]` writes to emit TS7053, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_top_level_this_computed_property_assignment_reports_ts7053() {
    let source = r#"
this["a" + "b"] = 0;
"#;

    let diagnostics = check_js(source);

    assert!(
        count_code(&diagnostics, 7053) > 0,
        "Expected top-level computed `this[...]` assignment to emit TS7053, got: {diagnostics:?}"
    );
}

/// self[symbolKey] = value (this alias) in JS class constructor → no false error
#[test]
fn test_js_constructor_element_access_self_alias_no_false_error() {
    let source = r#"
const _sym = Symbol("_sym");
class MyClass {
    constructor() {
        var self = this;
        self[_sym] = "ok";
    }
    method() {
        var self = this;
        self[_sym] = "yep";
        const x = self[_sym];
    }
}
"#;
    let diagnostics = check_js(source);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322 || *code == 7053)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no TS2322/TS7053 for self-alias element access, got: {errors:?}"
    );
}

/// Prototype element-access symbol-keyed property should emit TS7053.
/// TSC treats `Ctor.prototype[sym] = val` as "currently unsupported" late-bound
/// assignment declarations and does NOT expose the property on the instance type.
#[test]
fn test_plain_function_constructor_prototype_symbol_key_emits_ts7053() {
    let source = r#"
const _sym = Symbol("_sym");
function Ctor() {}
Ctor.prototype[_sym] = "ok";
const inst = new Ctor();
inst[_sym];
"#;
    let diagnostics = check_js(source);
    let ts7053: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7053)
        .collect();
    assert_eq!(
        ts7053.len(),
        1,
        "Expected TS7053 for prototype symbol-keyed element-access, got: {diagnostics:?}"
    );
}

/// Plain function constructor: this.prop in prototype method should be accessible but nullable
#[test]
fn test_plain_function_constructor_prototype_this_prop_has_undefined() {
    let source = r#"
function Baz() {
    this.x = 1;
}
Baz.prototype.m = function() {
    this.y = 12;
};
var bz = new Baz();
bz.y = undefined;
"#;
    let diagnostics = check_js(source);
    // bz.y = undefined should NOT error (y is number | undefined from prototype method)
    let ts2322_for_y: Vec<_> = diagnostics
        .iter()
        .filter(|(code, msg)| *code == 2322 && msg.contains("undefined"))
        .collect();
    assert_eq!(
        ts2322_for_y.len(),
        0,
        "Expected no TS2322 for assigning undefined to prototype-method property, got: {diagnostics:?}"
    );
}

/// Prototype element-access expandos (F.prototype[sym] = val) should NOT suppress TS7053.
/// TSC treats these as "currently unsupported" late-bound assignment declarations.
#[test]
fn test_prototype_element_access_expando_emits_ts7053() {
    // Test 1: string key via const variable
    let source_str = r#"
const _str = "my-fake-sym";
function F() {}
F.prototype[_str] = "ok";
const inst = new F();
const _y = inst[_str];
"#;
    let diag_str = check_js(source_str);
    let ts7053_str: Vec<_> = diag_str.iter().filter(|(c, _)| *c == 7053).collect();
    assert_eq!(
        ts7053_str.len(),
        1,
        "Expected TS7053 for prototype string-keyed element-access expando read, got: {diag_str:?}"
    );

    // Test 2: symbol key
    let source_sym = r#"
const _sym = Symbol();
function F() {}
F.prototype[_sym] = "ok";
const inst = new F();
const _z = inst[_sym];
"#;
    let diag_sym = check_js(source_sym);
    let ts7053_sym: Vec<_> = diag_sym.iter().filter(|(c, _)| *c == 7053).collect();
    assert_eq!(
        ts7053_sym.len(),
        1,
        "Expected TS7053 for prototype symbol-keyed element-access expando read, got: {diag_sym:?}"
    );
}

/// Arrow functions inside JS prototype methods should inherit the instance `this` type.
#[test]
fn test_js_prototype_method_arrow_inherits_instance_this_type() {
    let source = r#"
function Installer() {
    this.args = 0;
}
Installer.prototype.loadArgMetadata = function(next) {
    (args) => {
        this.args = "hi";
        this.newProperty = 1;
    };
}
"#;
    let diagnostics = check_js(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, msg)| *code == 2322 && msg.contains("string") && msg.contains("number"))
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected prototype-method arrow to inherit instance this and report TS2322, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_prototype_method_reports_implicit_any_for_own_params() {
    let source = r#"
function Installer() {
    this.args = 0;
}
Installer.prototype.loadArgMetadata = function(next) {
    (args) => {
        this.args = "hi";
    };
}
"#;
    let diagnostics = check_js(source);
    let ts7006_next: Vec<_> = diagnostics
        .iter()
        .filter(|(code, msg)| {
            *code == 7006 && msg.contains("Parameter 'next' implicitly has an 'any' type.")
        })
        .collect();
    assert_eq!(
        ts7006_next.len(),
        1,
        "Expected bare JS prototype method parameter to report TS7006, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_prototype_method_arrow_adds_instance_properties() {
    let source = r#"
function Installer() {
    this.args = 0;
}
Installer.prototype.loadArgMetadata = function(next) {
    (args) => {
        this.newProperty = 1;
    };
}
var i = new Installer();
i.newProperty = i.args;
"#;
    let diagnostics = check_js(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2339 | 18048 | 7009))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected prototype-method arrows to contribute instance properties, got: {diagnostics:?}"
    );
}

fn ts_codes(diagnostics: &[(u32, String)], code: u32) -> Vec<&str> {
    diagnostics
        .iter()
        .filter(|(c, _)| *c == code)
        .map(|(_, m)| m.as_str())
        .collect()
}

// Issue #9774: a JS this-property initialized from an implicit-any parameter
// borrows that `any`; tsc reports only the parameter's TS7006 and does not
// additionally flag the member with TS7008. Only fresh widening initializers
// (missing / null / undefined / empty-array) carry a member-level implicit-any
// obligation.

#[test]
fn test_this_property_from_implicit_any_param_no_ts7008() {
    let source = r#"
function Animal(name) {
    this.name = name;
}
"#;
    let diagnostics = check_js(source);
    assert_eq!(
        ts_codes(&diagnostics, 7006).len(),
        1,
        "Expected the implicit-any parameter to report TS7006 once, got: {diagnostics:?}"
    );
    assert!(
        ts_codes(&diagnostics, 7008).is_empty(),
        "Expected no redundant TS7008 on a member borrowing an implicit-any param, got: {diagnostics:?}"
    );
}

#[test]
fn test_this_property_from_implicit_any_param_renamed_no_ts7008() {
    // Same rule, different identifier spellings — the fix must not be keyed on names.
    let source = r#"
function Widget(label) {
    this.title = label;
}
"#;
    let diagnostics = check_js(source);
    assert_eq!(
        ts_codes(&diagnostics, 7006).len(),
        1,
        "Expected one TS7006 regardless of identifier names, got: {diagnostics:?}"
    );
    assert!(
        ts_codes(&diagnostics, 7008).is_empty(),
        "Expected no TS7008 regardless of identifier names, got: {diagnostics:?}"
    );
}

#[test]
fn test_this_property_from_typed_param_no_errors() {
    // Negative control: an annotated param removes both TS7006 and TS7008.
    let source = r#"
/** @param {string} name */
function Animal(name) {
    this.name = name;
}
/** @param {string} label */
function Widget(label) {
    this.title = label;
}
"#;
    let diagnostics = check_js(source);
    assert!(
        ts_codes(&diagnostics, 7006).is_empty() && ts_codes(&diagnostics, 7008).is_empty(),
        "Expected typed params to clear both TS7006 and TS7008, got: {diagnostics:?}"
    );
}

#[test]
fn test_multiple_this_properties_from_same_any_param_single_ts7006_no_ts7008() {
    let source = r#"
function Point(value) {
    this.x = value;
    this.y = value;
    this.z = value;
}
"#;
    let diagnostics = check_js(source);
    assert_eq!(
        ts_codes(&diagnostics, 7006).len(),
        1,
        "Expected a single TS7006 for the shared implicit-any param, got: {diagnostics:?}"
    );
    assert!(
        ts_codes(&diagnostics, 7008).is_empty(),
        "Expected no TS7008 for any of the members borrowing the param, got: {diagnostics:?}"
    );
}

#[test]
fn test_fresh_widening_initializers_still_emit_ts7008_alongside_borrowed_any() {
    // The borrowed-any suppression must not leak into the fresh-widening cases:
    // null / undefined / empty-array initializers still owe a TS7008, while a
    // sibling member borrowing an implicit-any param does not.
    let source = r#"
function Mixed(param) {
    this.borrowed = param;
    this.nulled = null;
    this.undef = undefined;
    this.arr = [];
}
"#;
    let diagnostics = check_js_with_options(
        source,
        CheckerOptions {
            check_js: true,
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    let ts7008 = ts_codes(&diagnostics, 7008);
    assert!(
        ts7008.iter().any(|m| m.contains("Member 'nulled'"))
            && ts7008.iter().any(|m| m.contains("Member 'undef'"))
            && ts7008.iter().any(|m| m.contains("Member 'arr'")),
        "Expected fresh widening members to still report TS7008, got: {diagnostics:?}"
    );
    assert!(
        ts7008.iter().all(|m| !m.contains("Member 'borrowed'")),
        "Expected the member borrowing an implicit-any param to not report TS7008, got: {diagnostics:?}"
    );
}
