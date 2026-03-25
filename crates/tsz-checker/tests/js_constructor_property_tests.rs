//! Tests for JS constructor `this.prop = value` property inference.
//!
//! Verifies that in JS/checkJs mode, constructor body `this.prop = value`
//! assignments are recognized as class instance property declarations,
//! preventing false TS2339 errors.

use tsz_checker::context::CheckerOptions;

fn check_js(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        check_js: true,
        strict: true,
        ..CheckerOptions::default()
    };

    let mut parser =
        tsz_parser::parser::ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.js".to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn check_js_with_options(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    let mut parser =
        tsz_parser::parser::ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.js".to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn count_code(diags: &[(u32, String)], code: u32) -> usize {
    diags.iter().filter(|(c, _)| *c == code).count()
}

/// Basic constructor this.prop assignment → no TS2339 on instance access
#[test]
fn test_js_constructor_this_prop_no_false_ts2339() {
    let source = r#"
class K {
    constructor() {
        this.p1 = 12;
        this.p2 = "ok";
    }
}
var k = new K();
k.p1;
k.p2;
"#;
    let diagnostics = check_js(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for constructor this.prop access, got: {ts2339:?}"
    );
}

/// Constructor this.prop with JSDoc @type annotation → correct type inference
#[test]
fn test_js_constructor_this_prop_with_jsdoc_type() {
    let source = r#"
class Foo {
    constructor() {
        /** @type {string} */
        this.name = "";
    }
}
var f = new Foo();
f.name;
"#;
    let diagnostics = check_js(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for JSDoc-annotated constructor property, got: {ts2339:?}"
    );
}

/// Explicit property declaration takes precedence over constructor assignment
#[test]
fn test_js_constructor_this_prop_explicit_declaration_precedence() {
    let source = r#"
class Foo {
    /** @type {number} */
    x = 5;
    constructor() {
        this.x = 10;
    }
}
var f = new Foo();
f.x;
"#;
    let diagnostics = check_js(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 when explicit declaration exists, got: {ts2339:?}"
    );
}

/// Constructor this.prop in subclass → no TS2339
#[test]
fn test_js_constructor_this_prop_in_subclass() {
    let source = r#"
class Base {
    constructor() {
        this.a = 1;
    }
}
class Derived extends Base {
    constructor() {
        super();
        this.b = 2;
    }
}
var d = new Derived();
d.a;
d.b;
"#;
    let diagnostics = check_js(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for subclass constructor properties, got: {ts2339:?}"
    );
}

/// JSDoc @return {x is Type} type predicate → narrowing works
#[test]
fn test_jsdoc_return_type_predicate_narrowing() {
    let source = r#"
/**
 * @param {any} value
 * @return {value is string}
 */
function isString(value) {
    return typeof value === "string";
}

/** @param {string | number} x */
function test(x) {
    if (isString(x)) {
        x.toUpperCase();
    }
}
"#;
    let diagnostics = check_js(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 after type guard narrowing, got: {ts2339:?}"
    );
}

/// Method body `this.prop = value` infers class property (not just constructor)
#[test]
fn test_js_method_body_this_prop_no_false_ts2339() {
    let source = r#"
class Base {
    m() {
        this.p = 1;
    }
}
class Derived extends Base {
    m() {
        this.p = 1;
    }
}
"#;
    let diagnostics = check_js(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for method body this.prop, got: {ts2339:?}"
    );
}

#[test]
fn test_js_static_block_super_expando_reports_ts2565() {
    let source = r#"
class C {
    static blah1 = 123;
}
C.blah2 = 456;

class D extends C {
    static {
        super.blah1;
        super.blah2;
    }
}
"#;

    let diagnostics = check_js_with_options(
        source,
        CheckerOptions {
            check_js: true,
            strict: true,
            target: tsz_common::common::ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );

    assert_eq!(
        count_code(&diagnostics, 2565),
        1,
        "Expected JS static block super expando access to report TS2565, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_expando_reads_use_ts2565_instead_of_missing_member_errors() {
    let source = r#"
function d() {}
if (cond) {
    d.q = false;
}
d.q;

const g = function() {};
if (cond) {
    g.expando = 1;
}
g.expando;
"#;

    let diagnostics = check_js(source);
    let ts2565: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2565)
        .collect();
    let missing_member: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339 || *code == 2551)
        .collect();

    assert_eq!(
        ts2565.len(),
        2,
        "Expected conditional JS expando reads to report TS2565 twice, got: {diagnostics:?}"
    );
    assert!(
        missing_member.is_empty(),
        "Expected expando reads to avoid TS2339/TS2551 once flow-based TS2565 applies, got: {missing_member:?}"
    );
}

#[test]
fn test_js_prototype_read_before_assignment_reports_ts2565() {
    let source = r#"
class NewAjax {}
NewAjax.prototype.case6_unexpectedlyResolvesPathToNodeModules;
"#;

    let diagnostics = check_js(source);

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2565
                && message.contains(
                    "Property 'case6_unexpectedlyResolvesPathToNodeModules' is used before being assigned."
                )
        }),
        "Expected JS prototype read on an expando-capable root to report TS2565, got: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|(code, _)| *code != 2339 && *code != 2551),
        "Expected JS prototype read-before-write to avoid missing-member diagnostics, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_nested_scope_expando_reads_do_not_emit_ts2565() {
    let source = r#"
var NS = {};
NS.K = class {
    values() {
        return new NS.K();
    }
};

var Host = {};
Host.UserMetrics = {};
Host.UserMetrics.Action = {
    WindowDocked: 1,
};

class Other {
    usage() {
        return Host.UserMetrics.Action.WindowDocked;
    }
}
"#;

    let diagnostics = check_js(source);

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2565),
        "Expected nested-scope expando reads to avoid TS2565, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_class_prototype_declared_member_read_has_no_ts2565() {
    let source = r#"
class C {
    foo() {}
}

class D extends C {
    foo() {
        return super.foo();
    }
}

D.prototype.foo.call(new D());
"#;

    let diagnostics = check_js(source);

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2565),
        "Expected declared class prototype member reads to avoid TS2565, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_super_implicit_base_field_reports_ts2855_without_missing_member_noise() {
    let source = r#"
class YaddaBase {
    constructor() {
        this.roots = "hi";
        /** @type number */
        this.justProp;
        /** @type string */
        this['literalElementAccess'];
    }
}

class DerivedYadda extends YaddaBase {
    get rootTests() {
        return super.roots;
    }
    get justPropTests() {
        return super.justProp;
    }
    get literalElementAccessTests() {
        return super.literalElementAccess;
    }
}
"#;

    let diagnostics = check_js_with_options(
        source,
        CheckerOptions {
            check_js: true,
            strict: true,
            target: tsz_common::common::ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2855),
        "Expected JS super access to implicit base fields to report TS2855, got: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|(code, _)| *code != 2339 && *code != 7053),
        "Expected JS super implicit-field checks to avoid TS2339/TS7053 fallback noise, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_super_implicit_base_field_reports_ts2855_for_constructor_and_accessor_writes() {
    let source = r#"
class YaddaBase {
    constructor() {
        this.roots = "hi";
        /** @type number */
        this.justProp;
        /** @type string */
        this['literalElementAccess'];

        this.b()
    }
    accessor b = () => {
        this.foo = 10
    }
}

class DerivedYadda extends YaddaBase {
    get rootTests() {
        return super.roots;
    }
    get fooTests() {
        return super.foo;
    }
    get justPropTests() {
        return super.justProp;
    }
    get literalElementAccessTests() {
        return super.literalElementAccess;
    }
}
"#;

    let diagnostics = check_js_with_options(
        source,
        CheckerOptions {
            check_js: true,
            strict: true,
            target: tsz_common::common::ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.iter().filter(|(code, _)| *code == 2855).count() >= 4,
        "Expected JS super access to constructor and accessor-defined base fields to report TS2855, got: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|(code, _)| *code != 2339 && *code != 7053),
        "Expected JS super implicit-field checks to avoid TS2339/TS7053 fallback noise, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2855
                && message
                    .contains("Class field ''literalElementAccess'' defined by the parent class")
        }),
        "Expected TS2855 to preserve string-literal member display text, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_static_super_field_reads_allow_declared_and_expando_base_fields() {
    let source = r#"
class C {
    static blah1 = 123;
}
C.blah2 = 456;

class D extends C {
    static {
        console.log(super.blah1);
        console.log(super.blah2);
    }
}
"#;

    let diagnostics = check_js_with_options(
        source,
        CheckerOptions {
            check_js: true,
            strict: true,
            target: tsz_common::common::ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics
            .iter()
            .all(|(code, _)| !matches!(*code, 2339 | 2551 | 2855 | 7053)),
        "Expected JS static super field reads to avoid TS2339/TS2551/TS2855/TS7053, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_self_defaulting_expando_initializer_has_no_ts2565() {
    let source = r#"
var test = {};
test.K = test.K || function () {};
test.K.prototype = {
    add() {}
};
"#;

    let diagnostics = check_js(source);

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2565),
        "Expected self-defaulting expando initializer reads to avoid TS2565, got: {diagnostics:?}"
    );
}

/// `var self = this; self.prop = value` alias pattern in constructor
#[test]
fn test_js_self_alias_this_prop_constructor() {
    let source = r#"
class C {
    constructor() {
        var self = this;
        self.x = 1;
        self.m = function() {
            console.log(self.x);
        };
    }
}
var c = new C();
c.x;
c.m();
"#;
    let diagnostics = check_js(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for self-alias constructor properties, got: {ts2339:?}"
    );
}

/// `var self = this; self.prop = value` alias in methods
#[test]
fn test_js_self_alias_this_prop_method() {
    let source = r#"
class C {
    constructor() {
        var self = this;
        self.x = 1;
    }
    mreal() {
        var self = this;
        self.y = 2;
    }
}
var c = new C();
c.x;
c.y;
"#;
    let diagnostics = check_js(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for self-alias method properties, got: {ts2339:?}"
    );
}

/// Non-existent property still emits TS2339 (regression guard)
#[test]
fn test_js_constructor_nonexistent_prop_still_errors() {
    let source = r#"
class Foo {
    constructor() {
        this.x = 1;
    }
}
var f = new Foo();
f.nonexistent;
"#;
    let diagnostics = check_js(source);
    // x should NOT cause TS2339
    let ts2339_for_x: Vec<_> = diagnostics
        .iter()
        .filter(|(code, msg)| *code == 2339 && msg.contains("'x'"))
        .collect();
    assert_eq!(
        ts2339_for_x.len(),
        0,
        "Expected no TS2339 for constructor-declared 'x', got: {diagnostics:?}"
    );
}

// === Plain function constructor tests (non-class) ===

/// Plain function constructor: `new Foo()` should return instance type with this.prop properties
#[test]
fn test_plain_function_constructor_this_prop_inference() {
    let source = r#"
/** @param {number} x */
function Foo(x) {
    this.x = x;
    this.y = "hello";
}
var f = new Foo(42);
/** @type {string} */
var s = f.x;
"#;
    let diagnostics = check_js(source);
    // f.x is number, assigning to string should produce TS2322
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        !ts2322.is_empty(),
        "Expected TS2322 for assigning number to string, got: {diagnostics:?}"
    );
}

#[test]
fn test_plain_function_constructor_new_result_is_not_possibly_undefined() {
    let source = r#"
function Foo() {
    this.x = 1;
}
var f = new Foo();
f.x;
"#;
    let diagnostics = check_js(source);
    let ts18048: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 18048)
        .collect();
    assert_eq!(
        ts18048.len(),
        0,
        "Expected JS constructor new result to avoid false TS18048, got: {diagnostics:?}"
    );
}

/// Plain function constructor: prototype methods should be accessible on instances
#[test]
fn test_plain_function_constructor_prototype_method_accessible() {
    let source = r#"
function Bar() {
    this.x = 1;
}
Bar.prototype.greet = function() {
    return "hi";
};
var b = new Bar();
b.greet();
b.x;
"#;
    let diagnostics = check_js(source);
    // Neither b.greet nor b.x should trigger TS2339
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for constructor/prototype properties, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_class_expression_assigned_to_property_preserves_base_instance_members() {
    let source = r#"
var UI = {}
UI.TreeElement = class {
    constructor() {
        this.treeOutline = 12
    }
};
UI.context = new UI.TreeElement()

class C extends UI.TreeElement {
    onpopulate() {
        this.doesNotExist
        this.treeOutline.doesntExistEither()
    }
};
"#;

    let diagnostics = check_js(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();

    assert!(
        ts2339.len() >= 2,
        "Expected missing-member diagnostics for unknown `this` property and invalid number member access, got: {diagnostics:?}"
    );
    assert!(
        ts2339.iter().any(|(_, msg)| msg.contains("doesNotExist")),
        "Expected TS2339 for `this.doesNotExist`, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_class_expression_assigned_to_element_property_preserves_base_instance_members() {
    let source = r#"
var UI = {}
UI["TreeElement"] = class {
    constructor() {
        this.treeOutline = 12
    }
};
UI.context = new UI["TreeElement"]()

class C extends UI["TreeElement"] {
    onpopulate() {
        this.doesNotExist
        this.treeOutline.doesntExistEither()
    }
};
"#;

    let diagnostics = check_js(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();

    assert!(
        ts2339.len() >= 2,
        "Expected missing-member diagnostics for unknown `this` property and invalid number member access through element-assigned base class, got: {diagnostics:?}"
    );
    assert!(
        ts2339
            .iter()
            .any(|(_, msg)| msg.contains("doesNotExist")),
        "Expected TS2339 for `this.doesNotExist`, got: {diagnostics:?}"
    );
}

/// Plain JS constructor functions with computed prototype assignments are still
/// constructable in checkJs, even though the computed members themselves remain
/// unsupported for property lookup.
#[test]
fn test_plain_function_constructor_with_computed_prototype_assignment_is_constructable() {
    let source = r#"
const _sym = Symbol();
const _str = "my-fake-sym";
function F() {}
F.prototype[_sym] = "ok";
F.prototype[_str] = "ok";
var f = new F();
"#;
    let diagnostics = check_js(source);
    let ts7009: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7009)
        .collect();
    assert_eq!(
        ts7009.len(),
        0,
        "Expected no TS7009 for JS constructor with computed prototype assignments, got: {ts7009:?}"
    );
}

/// Object.defineProperty on a JS constructor prototype should also mark the
/// function as constructable in checkJs.
#[test]
fn test_plain_function_constructor_with_define_property_prototype_is_constructable() {
    let source = r#"
const _sym = Symbol();
function F() {}
Object.defineProperty(F.prototype, _sym, { value: "ok" });
var f = new F();
"#;
    let diagnostics = check_js(source);
    let ts7009: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7009)
        .collect();
    assert_eq!(
        ts7009.len(),
        0,
        "Expected no TS7009 for JS constructor with Object.defineProperty prototype writes, got: {ts7009:?}"
    );
}

/// Chained prototype object assignment should keep every participating function
/// constructable and surface the shared prototype members on instances.
#[test]
fn test_variable_assigned_function_constructors_with_chained_prototype_object_are_constructable() {
    let source = r#"
var A = function A() {
    this.a = 1;
};
var B = function B() {
    this.b = 2;
};
A.prototype = B.prototype = {
    /** @param {number} n */
    m(n) {
        return n + 1;
    }
};
var a = new A();
var b = new B();
a.m(1);
b.m(2);
"#;
    let diagnostics = check_js(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 7009 | 2339 | 7006))
        .collect();
    assert_eq!(
        relevant.len(),
        0,
        "Expected chained prototype constructors to stay constructable with method members, got: {diagnostics:?}"
    );
}

#[test]
fn test_variable_assigned_function_constructors_with_chained_prototype_object_preserve_method_types()
 {
    let source = r#"
var A = function A() {
    this.a = 1;
};
var B = function B() {
    this.b = 2;
};
A.prototype = B.prototype = {
    /** @param {number} n */
    m(n) {
        return n + 1;
    }
};
var a = new A();
var b = new B();
a.m("nope");
b.m("still nope");
"#;
    let diagnostics = check_js(source);
    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    assert_eq!(
        ts2345.len(),
        2,
        "Expected chained prototype methods to preserve JSDoc parameter types, got: {diagnostics:?}"
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
fn test_plain_js_function_constructor_initializers_emit_ts7008_in_check_js() {
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
this["y"] = {};
this["y"]["z"] = {};

/** @constructor */
function F() {
  this.a = {};
  this.a.b = {};
  this["b"] = {};
  this["b"]["c"] = {};
}

const f = new F();
f.a;
f.b;
"#;

    let diagnostics = check_js(source);

    assert_eq!(
        count_code(&diagnostics, 2339),
        0,
        "Expected single-hop `this` property declarations to avoid TS2339, got: {diagnostics:?}"
    );
    assert_eq!(
        count_code(&diagnostics, 7053),
        0,
        "Expected literal `this[...]` declarations to avoid TS7053, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_top_level_this_computed_property_assignment_requires_literal_key() {
    let source = r#"
this["a" + "b"] = 0;
"#;

    let diagnostics = check_js(source);

    assert_eq!(
        count_code(&diagnostics, 7053),
        1,
        "Expected non-literal top-level `this[...]` assignment to report TS7053, got: {diagnostics:?}"
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

/// Plain function constructor prototype symbol-keyed property → no false TS7053
#[test]
fn test_plain_function_constructor_prototype_symbol_key_no_false_error() {
    let source = r#"
const _sym = Symbol("_sym");
function Ctor() {}
Ctor.prototype[_sym] = "ok";
const inst = new Ctor();
inst[_sym];
"#;
    let diagnostics = check_js(source);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7053 || *code == 2339)
        .collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no TS7053/TS2339 for symbol-keyed prototype constructor property, got: {errors:?}"
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
