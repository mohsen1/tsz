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
    let ts2683: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2683)
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
            .contains("Property 'rgb' does not exist on type '{ negate: () => any;"),
        "Expected the prototype-function receiver to display as the complete object literal, got: {diagnostics:?}"
    );
    assert_eq!(
        ts2683.len(),
        1,
        "TypeScript 7 reports the constructor body's implicit `this`, got: {diagnostics:?}"
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

    // Oracle-verified (typescript@7.0.2): each private-identifier object-
    // literal key reports its own TS18016 ("Private identifiers are not
    // allowed outside class bodies"), matching the general rule pinned by
    // `js_object_literal_private_identifier_ts18016_tests`. The original
    // `is_empty()` assertion only meant to guard against a recursive crash
    // (per the doc comment above) and over-constrained the result; tsz
    // already emits the correct diagnostics without crashing.
    assert_eq!(
        diagnostics.len(),
        3,
        "Expected one TS18016 per private-identifier key (no crash), got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|(code, _)| *code == 18016),
        "Expected only TS18016 diagnostics, got: {diagnostics:?}"
    );
}

/// TypeScript 7 dropped generic (`@template`) JS constructor-function inference:
/// the function is an ordinary function, so `this` is implicitly `any` and
/// `new Zet(1)` reports TS7009. The resulting `any` instance means `z.u = false`
/// no longer produces TS2322.
#[test]
fn test_generic_constructor_function_template_is_not_instantiated_under_ts7() {
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
    assert_eq!(
        count_code(&diagnostics, 2683),
        2,
        "Expected TS2683 for each `this` reference in the generic function, got: {diagnostics:?}"
    );
    assert_eq!(
        count_code(&diagnostics, 7009),
        1,
        "Expected TS7009 for `new Zet(1)` under TypeScript 7, got: {diagnostics:?}"
    );
    assert_eq!(
        count_code(&diagnostics, 2322),
        0,
        "Expected no TS2322 because the instance is `any`, got: {diagnostics:?}"
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
fn test_jsdoc_extends_js_constructor_function_reports_ts2507_under_ts7() {
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

    // TypeScript 7 dropped JS constructor-function inference, so `Soup` is a plain
    // (generic) function and not a constructor function type: `class Chowder
    // extends Soup` reports TS2507. With no instance type inherited, `chowder.flavour`
    // is TS2339 and every `new Chowder(arg)` is a TS2554 arity error, matching tsc 7.
    let diagnostics = check_js(source);
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2507),
        "Expected TS2507 for a class extending a plain JS function under TypeScript 7, got: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| { *code == 2339 && message.contains("flavour") }),
        "Expected TS7 to drop the inherited `flavour` property (TS2339), got: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| { *code == 2554 && message.contains("Expected 0 arguments") }),
        "Expected TS7's parameterless synthesized constructor arity (TS2554), got: {diagnostics:?}"
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

    // TypeScript 7 dropped JS constructor-function inference: `Wagon` is an
    // ordinary function, not a constructor, so `class Sql extends Wagon` reports
    // TS2507. Because the base is not a valid constructor, there is no base class
    // shape to override against, so the JSDoc-typed `load` method is not compared
    // to `Wagon.prototype.load` and no TS2416 override diagnostic is emitted.
    let diagnostics = check_js(source);
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2507),
        "Expected TS2507 for a class extending a plain JS function under TypeScript 7, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2416),
        "Expected no TS2416 override diagnostic once TS7 drops the JS constructor base, got: {diagnostics:?}"
    );
}

#[test]
fn test_generic_constructor_function_template_self_alias_is_not_instantiated_under_ts7() {
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
    // TypeScript 7: `var self = this` binds an implicitly-`any` `this` (TS2683),
    // `new Zet(1)` reports TS7009, and the `any` instance keeps `z.u = false`
    // free of TS2322 while the alias stays `any` (no TS2339).
    assert_eq!(
        count_code(&diagnostics, 2683),
        1,
        "Expected TS2683 for `var self = this`, got: {diagnostics:?}"
    );
    assert_eq!(
        count_code(&diagnostics, 7009),
        1,
        "Expected TS7009 for `new Zet(1)` under TypeScript 7, got: {diagnostics:?}"
    );
    assert_eq!(
        count_code(&diagnostics, 2322) + count_code(&diagnostics, 2339),
        0,
        "Expected no TS2322/TS2339 on the `any` self-alias instance, got: {diagnostics:?}"
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
    // TypeScript 7 dropped JS constructor-function inference, so `@class` /
    // `@template` no longer synthesize an instance type carrying the
    // constructor's `this.<prop>` writes. A new `this.<prop>` inside a method of
    // the literal assigned to `Cp.prototype` is therefore a missing member, which
    // this test previously asserted must be *allowed*.
    assert!(
        ts2339.iter().any(|(_, message)| message.contains("'y'")),
        "`this.y` is not a member once constructor inference is gone; expected \
         TS2339, got: {diagnostics:?}"
    );
    // tsc 7.0.2 names the object literal as the receiver —
    //     TS2339 Property 'y' does not exist on type '{ m4(): any; }'
    // tsz's pre-existing missing TS2526 recovery for the invalid JSDoc
    // `@return {this}` keeps the member return as `this`; this assertion owns
    // only the receiver provenance and must not accept `Cp` / `typeof Cp`.
    assert!(
        ts2339
            .iter()
            .all(|(_, message)| { message.contains("type '{ m4(): ") && !message.contains("Cp") }),
        "the prototype method receiver must be the object literal, not `Cp` or \
         `typeof Cp`; got: {ts2339:?}"
    );
    assert!(
        ts2403.is_empty(),
        "Expected no subsequent-variable-declaration conflicts, got: {diagnostics:?}"
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

    // TypeScript 7 dropped JS constructor-function inference: a plain
    // `@class`-tagged function is not a constructor type, so `this` in its body
    // has no instance type (TS2683) and the prototype-object method's `this`
    // refers to the object literal, not an instance. There is therefore no
    // numeric `this.x` to violate — tsc 7.0.2 emits no TS2322 here.
    //
    // Oracle witness (`tsc 7.0.2`, `strict`):
    // ```text
    // cp.js(7,5):  error TS2683: 'this' implicitly has type 'any' …
    // cp.js(8,5):  error TS2683: 'this' implicitly has type 'any' …
    // cp.js(13,14): error TS2339: Property 'x' does not exist on type '{ m4(): … }'.
    // ```
    let diagnostics = check_js(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, message)| {
            *code == 2322 && message.contains("string") && message.contains("number")
        })
        .collect();
    let ts2683: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2683)
        .collect();

    assert!(
        ts2322.is_empty(),
        "TS7 drops constructor-function inference: no numeric `this.x` member \
         check fires, got: {diagnostics:?}"
    );
    assert!(
        !ts2683.is_empty(),
        "TS7: `this` in a non-constructor JS function body is implicitly any \
         (TS2683), got: {diagnostics:?}"
    );
}

#[test]
fn test_js_class_cannot_extend_js_constructor_function_under_ts7() {
    // TypeScript 7 dropped JS constructor-function inference, so a plain JS
    // function is not a constructor function type: `class Sql extends Wagon`
    // reports TS2507 (matching tsc 7.0.2 and the existing ESM-import behavior).
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
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2507),
        "Expected TS2507 for a class extending a plain JS function under TypeScript 7, got: {diagnostics:?}"
    );
}

#[test]
fn test_plain_js_function_constructor_is_not_constructable_under_ts7() {
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
    // TypeScript 7 dropped JS constructor-function inference: `this` is implicitly
    // `any` (TS2683 per reference) and `new A()` lacks a construct signature
    // (TS7009). The resulting `any` instance means later member accesses do not error.
    assert_eq!(
        count_code(&diagnostics, 2683),
        2,
        "Expected TS2683 for each `this` reference in the plain JS function, got: {diagnostics:?}"
    );
    assert_eq!(
        count_code(&diagnostics, 7009),
        1,
        "Expected TS7009 for `new A()` under TypeScript 7, got: {diagnostics:?}"
    );
    let instance_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2339))
        .collect();
    assert!(
        instance_errors.is_empty(),
        "Expected the resulting `any` instance to avoid TS2322/TS2339, got: {diagnostics:?}"
    );
}

#[test]
fn test_plain_js_function_constructor_initializers_are_not_declared_under_ts7() {
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
    // TypeScript 7 no longer synthesizes instance members from `this.x =`
    // initializers: each `this` reference is implicitly `any` (TS2683) and
    // `new A()` reports TS7009, leaving the `any` instance clean of TS2322/TS2339.
    assert_eq!(
        count_code(&diagnostics, 2683),
        3,
        "Expected TS2683 for each `this` initializer in the plain JS function, got: {diagnostics:?}"
    );
    assert_eq!(
        count_code(&diagnostics, 7009),
        1,
        "Expected TS7009 for `new A()` under TypeScript 7, got: {diagnostics:?}"
    );
    let instance_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2339))
        .collect();
    assert!(
        instance_errors.is_empty(),
        "Expected the resulting `any` instance to avoid TS2322/TS2339, got: {diagnostics:?}"
    );
}

#[test]
fn test_plain_js_function_constructor_void_zero_initializer_is_not_constructable_under_ts7() {
    let source = r#"
function C() {
    this.p = 1;
    this.q = void 0;
}
var c = new C();
c.p + c.q;
"#;
    let diagnostics = check_js(source);
    // TypeScript 7: `this.p`/`this.q` in the plain function are implicitly `any`
    // (TS2683) and `new C()` reports TS7009, so no instance property `q` is
    // declared and the `any` instance keeps `c.p + c.q` free of TS2339.
    assert_eq!(
        count_code(&diagnostics, 2683),
        2,
        "Expected TS2683 for each `this` initializer, got: {diagnostics:?}"
    );
    assert_eq!(
        count_code(&diagnostics, 7009),
        1,
        "Expected TS7009 for `new C()` under TypeScript 7, got: {diagnostics:?}"
    );
    assert_eq!(
        count_code(&diagnostics, 2339),
        0,
        "Expected no TS2339 on the `any` instance, got: {diagnostics:?}"
    );
}

#[test]
fn test_plain_js_function_constructor_initializers_report_ts2683_not_ts7008_under_ts7() {
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
    // TypeScript 7 does not synthesize instance members, so there is no member to
    // carry an implicit-any obligation (no TS7008). Each `this` initializer is
    // implicitly `any` and reports TS2683 instead.
    assert_eq!(
        count_code(&diagnostics, 2683),
        3,
        "Expected TS2683 for each `this` initializer, got: {diagnostics:?}"
    );
    assert_eq!(
        count_code(&diagnostics, 7008),
        0,
        "Expected no TS7008 because no instance member is declared under TypeScript 7, got: {diagnostics:?}"
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
    // TypeScript 7 dropped JS constructor-function inference. `Installer` gains
    // no synthesized instance type, so every `this.<prop>` here is a member of an
    // implicitly-`any` receiver: the assignment mismatches (TS2322), the
    // nullability check (TS2531) and the implicit-member reports (TS7008) this
    // test was written against can no longer arise. Verified against the pinned
    // tsc 7.0.2, which reports TS2683 on each constructor-body `this` and
    // nothing else for this source.
    let codes: Vec<u32> = diagnostics.iter().map(|(code, _)| *code).collect();
    for stale in [2322u32, 2531, 7008] {
        assert_eq!(
            codes.iter().filter(|&&code| code == stale).count(),
            0,
            "TS{stale} depends on constructor-function inference, which TS7 removed; \
             got: {diagnostics:?}"
        );
    }
    assert!(
        codes.contains(&2683),
        "expected TS2683 for each implicitly-`any` constructor `this`, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_function_constructor_with_factory_guard_is_not_constructable_under_ts7() {
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
    // TypeScript 7: the factory guard does not rescue the plain function. Both
    // `this` references are implicitly `any` (TS2683) and both `new A(...)`
    // sites report TS7009; the `any` instance keeps `j.x` free of TS2339.
    assert_eq!(
        count_code(&diagnostics, 2683),
        2,
        "Expected TS2683 for each `this` reference, got: {diagnostics:?}"
    );
    assert_eq!(
        count_code(&diagnostics, 7009),
        2,
        "Expected TS7009 for each `new A(...)` site, got: {diagnostics:?}"
    );
    assert_eq!(
        count_code(&diagnostics, 2339),
        0,
        "Expected no TS2339 on the `any` instance, got: {diagnostics:?}"
    );
}

#[test]
fn test_variable_assigned_js_constructor_is_not_constructable_under_ts7() {
    // TypeScript 7: a `@constructor`-tagged function-expression variable is a
    // plain function. Its `this` references are implicitly `any` (TS2683) and
    // `new Multimap()` reports TS7009; the `any` instance keeps `mm._map` clean.
    let source = r#"
/** @constructor */
var Multimap = function() {
    this._map = {};
    this._map;
};
var mm = new Multimap();
mm._map;
"#;
    let diagnostics = check_js(source);
    assert_eq!(
        count_code(&diagnostics, 2683),
        2,
        "Expected TS2683 for each `this` reference, got: {diagnostics:?}"
    );
    assert_eq!(
        count_code(&diagnostics, 7009),
        1,
        "Expected TS7009 for `new Multimap()` under TypeScript 7, got: {diagnostics:?}"
    );
    assert_eq!(
        count_code(&diagnostics, 2339),
        0,
        "Expected no TS2339 on the `any` instance, got: {diagnostics:?}"
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

/// this[symbolKey] = value in a JS class constructor still reports TS7053:
/// a computed key (even a `const`-bound literal) never declares a property
/// on the class the way a plain `this.foo = ...` assignment does. Oracle-
/// confirmed (typescript@7.0.2, re-verified for #17203): tsc reports TS7053
/// at all three access sites. This test previously pinned zero diagnostics,
/// which was a stale expectation, not a regression.
#[test]
fn test_js_constructor_element_access_symbol_key_reports_ts7053() {
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
        3,
        "Expected TS7053 at all three Symbol-keyed access sites, got: {errors:?}"
    );
    assert!(
        errors.iter().all(|(code, _)| *code == 7053),
        "expected only TS7053, got: {errors:?}"
    );
}

/// this[stringKey] = value in a JS class constructor still reports TS7053,
/// for the same reason as the Symbol-keyed case above. Oracle-confirmed
/// (typescript@7.0.2, re-verified for #17203): tsc reports TS7053 at all
/// three access sites. This test previously pinned zero diagnostics, which
/// was a stale expectation, not a regression.
#[test]
fn test_js_constructor_element_access_string_key_reports_ts7053() {
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
        3,
        "Expected TS7053 at all three string-keyed access sites, got: {errors:?}"
    );
    assert!(
        errors.iter().all(|(code, _)| *code == 7053),
        "expected only TS7053, got: {errors:?}"
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
fn test_jsdoc_this_direct_read_checks_explicit_receiver_shape() {
    // Read/write parity: reading an unknown member of a JSDoc `@this`-typed
    // receiver must emit TS2339 just like writing one does. Previously the
    // read silently typed `any`.
    let source = r#"
/**
 * @this {{ ready: boolean }}
 */
function mark() {
  const x = this.missing;
}
"#;

    let diagnostics = check_js(source);

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2339
                && message == "Property 'missing' does not exist on type '{ ready: boolean; }'."
        }),
        "Expected TS2339 for reading `this.missing` against the explicit @this receiver shape, got: {diagnostics:?}"
    );
}

#[test]
fn test_jsdoc_this_bare_read_expression_checks_receiver_shape() {
    // Bare expression-statement read (no binding) must also be checked.
    let source = r#"
/**
 * @this {{ ready: boolean }}
 */
function mark() {
  this.missing;
}
"#;

    let diagnostics = check_js(source);

    assert_eq!(
        count_code(&diagnostics, 2339),
        1,
        "Expected exactly one TS2339 for bare `this.missing` read, got: {diagnostics:?}"
    );
}

#[test]
fn test_jsdoc_this_read_known_member_is_clean() {
    // Reading a member that DOES exist on the @this receiver must not error.
    let source = r#"
/**
 * @this {{ ready: boolean }}
 */
function mark() {
  const x = this.ready;
}
"#;

    let diagnostics = check_js(source);

    assert_eq!(
        count_code(&diagnostics, 2339),
        0,
        "Expected no TS2339 for reading the known `this.ready` member, got: {diagnostics:?}"
    );
}

#[test]
fn test_jsdoc_this_read_on_function_expression_checks_receiver_shape() {
    // The fix applies uniformly to function EXPRESSIONS, not just declarations.
    let source = r#"
/**
 * @this {{ ready: boolean }}
 */
const mark = function () {
  const x = this.missing;
};
"#;

    let diagnostics = check_js(source);

    assert_eq!(
        count_code(&diagnostics, 2339),
        1,
        "Expected TS2339 for `this.missing` read inside a @this-typed function expression, got: {diagnostics:?}"
    );
}

#[test]
fn test_jsdoc_this_read_is_structural_not_name_keyed() {
    // Anti-hardcoding: the miss was structural. Vary both the receiver-shape
    // member name and the accessed property name; the read must still error.
    let source = r#"
/**
 * @this {{ alpha: number }}
 */
function configure() {
  const v = this.omega;
}
"#;

    let diagnostics = check_js(source);

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2339
                && message == "Property 'omega' does not exist on type '{ alpha: number; }'."
        }),
        "Expected TS2339 for `this.omega` against `{{ alpha: number }}`, got: {diagnostics:?}"
    );
}

#[test]
fn test_jsdoc_this_read_write_parity_count() {
    // Both a read and a write of unknown members emit exactly one TS2339 each;
    // a known-member read/write stays clean.
    let source = r#"
/**
 * @this {{ ready: boolean }}
 */
function mark() {
  this.ready = true;
  const x = this.missingRead;
  this.missingWrite = 1;
}
"#;

    let diagnostics = check_js(source);

    assert_eq!(
        count_code(&diagnostics, 2339),
        2,
        "Expected exactly two TS2339 (one read, one write) for unknown members, got: {diagnostics:?}"
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

/// self[symbolKey] = value (this alias) in a JS class constructor still
/// reports TS7053, same reason and same oracle-confirmed correction as the
/// direct `this[...]` cases above (#17203): a computed key never declares a
/// property, alias or not. This test previously pinned zero diagnostics,
/// which was a stale expectation, not a regression.
#[test]
fn test_js_constructor_element_access_self_alias_reports_ts7053() {
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
        3,
        "Expected TS7053 at all three self-alias access sites, got: {errors:?}"
    );
    assert!(
        errors.iter().all(|(code, _)| *code == 7053),
        "expected only TS7053, got: {errors:?}"
    );
}

/// TypeScript 7: `Ctor` is a plain function, so `new Ctor()` reports TS7009 and
/// `inst` is `any` -- element access `inst[_sym]` on `any` does not report TS7053.
#[test]
fn test_plain_function_constructor_prototype_symbol_key_is_not_constructable_under_ts7() {
    let source = r#"
const _sym = Symbol("_sym");
function Ctor() {}
Ctor.prototype[_sym] = "ok";
const inst = new Ctor();
inst[_sym];
"#;
    let diagnostics = check_js(source);
    assert_eq!(
        count_code(&diagnostics, 7009),
        1,
        "Expected TS7009 for `new Ctor()` under TypeScript 7, got: {diagnostics:?}"
    );
    assert_eq!(
        count_code(&diagnostics, 7053),
        0,
        "Expected no TS7053 because `inst` is `any`, got: {diagnostics:?}"
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

/// TypeScript 7: `F` is a plain function, so `new F()` reports TS7009 and the
/// resulting `any` instance makes prototype element-access reads (`inst[key]`)
/// free of TS7053, for both string- and symbol-keyed expandos.
#[test]
fn test_prototype_element_access_expando_is_not_constructable_under_ts7() {
    // Test 1: string key via const variable
    let source_str = r#"
const _str = "my-fake-sym";
function F() {}
F.prototype[_str] = "ok";
const inst = new F();
const _y = inst[_str];
"#;
    let diag_str = check_js(source_str);
    assert_eq!(
        count_code(&diag_str, 7009),
        1,
        "Expected TS7009 for `new F()` under TypeScript 7, got: {diag_str:?}"
    );
    assert_eq!(
        count_code(&diag_str, 7053),
        0,
        "Expected no TS7053 on the `any` instance for a string-keyed expando read, got: {diag_str:?}"
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
    assert_eq!(
        count_code(&diag_sym, 7009),
        1,
        "Expected TS7009 for `new F()` under TypeScript 7, got: {diag_sym:?}"
    );
    assert_eq!(
        count_code(&diag_sym, 7053),
        0,
        "Expected no TS7053 on the `any` instance for a symbol-keyed expando read, got: {diag_sym:?}"
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
    // TypeScript 7 dropped JS constructor-function inference: `Installer` has no
    // synthesized instance type, so `this` in a prototype method — and in an
    // arrow nested inside it — is implicitly `any`. Assigning `"hi"` to
    // `this.args` is therefore unchecked. tsc 7.0.2 reports only TS2683 on the
    // constructor body and TS7006 for the untyped parameters; no TS2322.
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        ts2322.is_empty(),
        "`this` is `any` in a prototype-method arrow, so the write is unchecked; \
         expected no TS2322, got: {ts2322:?}"
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2683),
        "expected TS2683 for the implicitly-`any` constructor `this`, got: {diagnostics:?}"
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
fn test_js_prototype_method_arrow_does_not_add_instance_properties_under_ts7() {
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
    // TypeScript 7: the plain function is not a constructor, so `new Installer()`
    // reports TS7009 and no arrow-contributed instance properties exist. The
    // constructor `this` is implicitly `any` (TS2683) and the untyped
    // prototype-method parameters report TS7006.
    assert_eq!(
        count_code(&diagnostics, 7009),
        1,
        "Expected TS7009 for `new Installer()` under TypeScript 7, got: {diagnostics:?}"
    );
    assert_eq!(
        count_code(&diagnostics, 2683),
        1,
        "Expected TS2683 for the constructor `this`, got: {diagnostics:?}"
    );
    assert_eq!(
        count_code(&diagnostics, 7006),
        2,
        "Expected TS7006 for the untyped `next` and `args` parameters, got: {diagnostics:?}"
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
fn test_plain_js_function_this_initializers_report_ts2683_not_ts7008_under_ts7() {
    // TypeScript 7 no longer synthesizes instance members from `this.x =`
    // initializers, so none of null / undefined / empty-array / borrowed-param
    // owe a TS7008. The implicit-any parameter still reports TS7006 and each
    // `this` reference reports TS2683.
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
    assert_eq!(
        ts_codes(&diagnostics, 7006).len(),
        1,
        "Expected TS7006 for the implicit-any parameter, got: {diagnostics:?}"
    );
    assert_eq!(
        count_code(&diagnostics, 2683),
        4,
        "Expected TS2683 for each `this` initializer, got: {diagnostics:?}"
    );
    assert!(
        ts_codes(&diagnostics, 7008).is_empty(),
        "Expected no TS7008 because no instance member is declared under TypeScript 7, got: {diagnostics:?}"
    );
}
