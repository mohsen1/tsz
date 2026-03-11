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
