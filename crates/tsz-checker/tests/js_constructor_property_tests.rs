//! Tests for JS constructor `this.prop = value` property inference.
//!
//! Verifies that in JS/checkJs mode, constructor body `this.prop = value`
//! assignments are recognized as class instance property declarations,
//! preventing false TS2339 errors.

mod js_constructor_property_support;

use std::sync::Arc;

use js_constructor_property_support::*;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::context::LibContext;
use tsz_checker::test_utils::load_compiled_lib_files;

fn check_ts(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions::default();

    let mut parser =
        tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::construction::TypeInterner::new();
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
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

fn load_es5_lib_for_test() -> Vec<Arc<LibFile>> {
    load_compiled_lib_files(&["lib.es5.d.ts"])
}

fn load_es5_and_dom_lib_for_test() -> Vec<Arc<LibFile>> {
    load_compiled_lib_files(&["lib.es5.d.ts", "lib.dom.d.ts"])
}

fn check_js_with_es5_lib(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    check_js_with_lib_files(source, options, load_es5_lib_for_test())
}

fn check_js_with_es5_and_dom_lib(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    let lib_files = load_es5_and_dom_lib_for_test();
    assert_eq!(
        lib_files.len(),
        2,
        "expected ES5 + DOM libs for JS constructor property tests; checked stripped assets, full assets, and TypeScript/lib"
    );
    check_js_with_lib_files(source, options, lib_files)
}

fn check_js_with_lib_files(
    source: &str,
    options: CheckerOptions,
    lib_files: Vec<Arc<LibFile>>,
) -> Vec<(u32, String)> {
    let mut parser =
        tsz_parser::parser::ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    if lib_files.is_empty() {
        binder.bind_source_file(parser.get_arena(), root);
    } else {
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    }

    let types = tsz_solver::construction::TypeInterner::new();
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.js".to_string(),
        options,
    );

    if lib_files.is_empty() {
        checker.ctx.set_lib_contexts(Vec::new());
    } else {
        let lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn checked_js_prototype_optional_parent_method_call_suppresses_ts2531() {
    let source = r#"
Element.prototype.remove ??= function () {
  this.parentNode?.removeChild(this);
};

/**
 * @this Node
 */
Element.prototype.remove ??= function () {
  this.parentNode?.removeChild(this);
};
"#;
    let diagnostics = check_js_with_es5_and_dom_lib(
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    assert_eq!(
        count_code(&diagnostics, 2531),
        0,
        "expected optional parentNode method calls to suppress TS2531, got: {diagnostics:?}"
    );
}

#[test]
fn checked_js_prototype_plain_parent_method_call_reports_ts2531() {
    let source = r#"
Element.prototype.remove = function () {
  this.parentNode.removeChild(this);
};
"#;
    let diagnostics = check_js_with_es5_and_dom_lib(
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    assert_eq!(
        count_code(&diagnostics, 2531),
        1,
        "expected non-optional parentNode method call to report TS2531 once, got: {diagnostics:?}"
    );
}

#[test]
fn checked_js_constructor_nullable_array_property_reports_possibly_null_on_method_read() {
    let source = r#"
function Installer () {
    this.twices = []
    this.twices = null
}
Installer.prototype.second = function () {
    this.twices.push(1)
    if (this.twices != null) {
        this.twices.push('hi')
    }
}
"#;
    let diagnostics = check_js_with_es5_lib(
        source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..Default::default()
        },
    );

    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| { *code == 2531 && message == "Object is possibly 'null'." }),
        "expected TS2531 on unchecked this.twices.push(), got: {diagnostics:#?}"
    );
    assert_eq!(
        count_code(&diagnostics, 2531),
        1,
        "expected the null check to narrow the second this.twices.push(), got: {diagnostics:#?}"
    );
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

#[test]
fn test_js_constructor_nullable_array_method_call_reports_ts2531() {
    let source = r#"
function Installer() {
    this.twices = [];
    this.twices = null;
}
Installer.prototype.second = function () {
    this.twices.push(1);
    if (this.twices != null) {
        this.twices.push("hi");
    }
}
"#;
    let diagnostics = check_js_with_options(
        source,
        CheckerOptions {
            check_js: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    assert_eq!(
        count_code(&diagnostics, 2531),
        1,
        "Expected nullable JS constructor property method call to report TS2531 exactly once, got: {diagnostics:?}"
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
fn test_js_plain_function_this_read_reports_ts2339() {
    let source = r#"
function toString() {
    this.yadda;
    this.someValue = "";
}
"#;
    let diagnostics = check_js(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, msg)| *code == 2339 && msg.contains("'yadda'"))
        .collect();
    assert!(
        !ts2339.is_empty(),
        "Expected TS2339 for unknown `this.yadda` in JS function, got: {diagnostics:?}"
    );
}

/// Conformance lock: TS2339 for `this.<inexistent>` inside a JS function
/// must still fire even when the function's name shadows or merges with a
/// lib ambient declaration (e.g. `function toString` shares the name with
/// numerous `toString()` overloads in `lib.dom.d.ts`).
///
/// Before the synthesizer fix, `synthesize_js_constructor_instance_type`
/// would resolve the function symbol's `value_declaration` to one of the
/// body-less ambient lib declarations and short-circuit (returning `None`
/// because `func.body.is_none()`). That left `this` untyped (`TypeId::ANY`)
/// inside the JS function body, so `this.yadda` was untyped and no TS2339
/// fired. Now: when called with a function-declaration / function-expression
/// node directly, the synthesizer reads the body from that node, bypassing
/// merged-symbol drift.
///
/// Mirrors `compiler/inexistentPropertyInsideToStringType.ts`.
#[test]
fn test_js_plain_function_this_read_reports_ts2339_with_lib_name_shadow() {
    // Use a symbol name that exists in lib defs (e.g., `toString` lives on
    // Object.prototype / Function.prototype / many DOM types). Without a
    // direct-function fallback, merged-symbol resolution would steer the
    // synthesizer toward a body-less ambient lib declaration.
    let source = r#"
function toString() {
    this.yadda;
    this.someValue = "";
}
"#;
    let diagnostics = check_js(source);
    let yadda_ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, msg)| *code == 2339 && msg.contains("'yadda'"))
        .collect();
    assert!(
        !yadda_ts2339.is_empty(),
        "Expected TS2339 for unknown `this.yadda` even when function name shadows a lib symbol, got: {diagnostics:?}"
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

#[test]
fn test_js_self_defaulting_expando_constructor_is_constructable() {
    let source = r#"
var test = {};
test.K = test.K ||
    function () {};
test.K.prototype = {
    add() {}
};

new test.K().add;
"#;

    let diagnostics = check_js(source);
    assert!(
        diagnostics
            .iter()
            .all(|(code, _)| !matches!(*code, 2351 | 7009 | 2339)),
        "Expected self-defaulting expando constructor to stay constructable with prototype members, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts_expando_reads_remain_any_typed() {
    let source = r#"
function fn() {}
fn.answer = 1;

let text: string = fn.answer;
"#;

    let diagnostics = check_ts(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();

    assert_eq!(
        ts2322.len(),
        0,
        "Expected TypeScript expando reads to stay any-typed, got: {diagnostics:?}"
    );
}

#[test]
fn test_js_object_expando_element_access_literal_keys_infer_nested_shape() {
    let source = r#"
const foo = {};
foo["baz"] = {};
foo["baz"]["blah"] = 3;
"#;

    let diagnostics = check_js(source);
    let ts7053: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7053)
        .collect();

    assert!(
        ts7053.is_empty(),
        "Expected string-literal element-access expando writes to avoid TS7053, got: {diagnostics:?}"
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

#[test]
fn test_js_self_alias_this_prop_no_implicit_any_regression() {
    let source = r#"
class C {
    constructor() {
        var self = this;
        self.x = 1;
        self.m = function() {
            console.log(self.x);
        };
    }
    mreal() {
        var self = this;
        self.y = 2;
    }
}
var c = new C();
c.x;
c.y;
c.m();
"#;
    let diagnostics = check_js_with_options(
        source,
        CheckerOptions {
            check_js: true,
            no_implicit_any: true,
            strict_null_checks: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        0,
        "Expected no TS2339 for self-alias class members under noImplicitAny, got: {diagnostics:?}"
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

#[test]
fn test_jsdoc_constructor_without_assignments_is_constructable_and_checks_this_reads() {
    let source = r#"
/**
 * @constructor
 */
function Actual() {
    return this.missing;
}

new Actual();
"#;
    let diagnostics = check_js(source);
    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| *code == 2339 && message.contains("'missing'")),
        "Expected TS2339 for missing @constructor instance property, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 7009),
        "Expected @constructor function to avoid TS7009, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2683),
        "Expected @constructor function to suppress TS2683, got: {diagnostics:?}"
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
fn test_plain_function_self_alias_prototype_method_preserves_member_types() {
    let source = r#"
function Foonly() {
    var self = this
    self.x = 1
    self.m = function() {
        console.log(self.x)
    }
}
Foonly.prototype.mreal = function() {
    var self = this
    self.y = 2
}
const foo = new Foonly()
/** @type {string} */
var sx = foo.x;
/** @type {string} */
var sy = foo.y;
foo.m()
"#;
    let diagnostics = check_js(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for plain-function self-alias constructor/prototype members, got: {diagnostics:?}"
    );
    assert!(
        ts2322.len() >= 2,
        "Expected typed plain-function self-alias members to reject assignment to string, got: {diagnostics:?}"
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
        ts2339.iter().any(|(_, msg)| msg.contains("doesNotExist")),
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

#[test]
fn test_plain_function_prototype_object_literal_methods_do_not_recurse() {
    let source = r#"
function A() {
    this.x = 1;
}
A.prototype = {
    /** @param {number} n */
    m(n) {
        return n + this.x;
    }
};
var a = new A();
a.m(1);
a.m("nope");
"#;

    let diagnostics = check_js(source);
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2339 | 2345 | 7009))
        .collect();

    assert_eq!(
        relevant.iter().filter(|(code, _)| *code == 2345).count(),
        1,
        "Expected prototype object literal method JSDoc to stay intact, got: {diagnostics:?}"
    );
    assert!(
        relevant.iter().all(|(code, _)| *code == 2345),
        "Expected no crash-regression fallback diagnostics from prototype object literal methods, got: {diagnostics:?}"
    );
}

#[test]
fn test_jsdoc_chained_prototype_and_static_function_assignments_preserve_member_types() {
    let source = r#"
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
"#;
    let diagnostics = check_js(source);
    let z_missing = diagnostics
        .iter()
        .filter(|(code, message)| {
            *code == 2339 && message.contains("Property 'z' does not exist on type 'A'")
        })
        .count();
    assert_eq!(
        z_missing, 0,
        "chained prototype method assignments should expose both y and z on instances; got: {diagnostics:?}"
    );
    let string_to_number = diagnostics
        .iter()
        .filter(|(code, message)| {
            *code == 2345
                && message.contains(
                    "Argument of type 'string' is not assignable to parameter of type 'number'",
                )
        })
        .count();
    assert_eq!(
        string_to_number, 2,
        "expected both annotated prototype method calls to reject string arguments; got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2339 && message.contains("Property 'x' does not exist on type 'typeof A'")
        }),
        "static chained assignment function body should bind this to typeof A; got: {diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, message)| {
            *code == 2339 && message.contains("Property 'x' does not exist on type 'g'")
        }),
        "static chained assignment must not bind this to the function value itself; got: {diagnostics:?}"
    );
}
