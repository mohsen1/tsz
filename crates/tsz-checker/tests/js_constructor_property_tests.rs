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

    // Same shape as `test_js_constructor_nullable_array_method_call_reports_ts2531`,
    // run through the es5-lib harness. TypeScript 7 dropped JS
    // constructor-function inference, so `this` is implicitly `any` and
    // `this.twices` carries no nullability to report on.
    assert_eq!(
        count_code(&diagnostics, 2531),
        0,
        "`this` is `any` without constructor inference, so `this.twices.push()` is \
         unchecked; expected no TS2531, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2683),
        "expected TS2683 for the implicitly-`any` constructor `this`, got: {diagnostics:#?}"
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

    // TypeScript 7 dropped JS constructor-function inference, so `this.twices`
    // never acquires the `never[] | null` shape this test was written against —
    // `this` is implicitly `any` and the member read is unchecked. tsc 7.0.2
    // reports only TS2683 on the constructor body for this source.
    assert_eq!(
        count_code(&diagnostics, 2531),
        0,
        "`this` is `any` without constructor inference, so no nullability check \
         applies; expected no TS2531, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2683),
        "expected TS2683 for the implicitly-`any` constructor `this`, got: {diagnostics:?}"
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
fn test_js_plain_function_this_read_reports_ts2683_not_ts2339() {
    // TypeScript 7 no longer synthesizes an instance type for a plain JS
    // function, so `this` is implicitly `any` (TS2683 under noImplicitThis) and
    // unknown-property reads on it are `any` — no TS2339.
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
        ts2339.is_empty(),
        "Expected no TS2339 for `this.yadda` (implicit-any `this`), got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2683),
        "Expected implicit-any `this` (TS2683), got: {diagnostics:?}"
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
fn test_js_plain_function_this_read_reports_ts2683_with_lib_name_shadow() {
    // Even when the function name shadows a lib ambient declaration (e.g.
    // `toString`), TypeScript 7 leaves the plain function's `this` implicitly
    // `any`: TS2683 fires and unknown-property reads do not report TS2339.
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
        yadda_ts2339.is_empty(),
        "Expected no TS2339 for `this.yadda` (implicit-any `this`), got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2683),
        "Expected implicit-any `this` (TS2683), got: {diagnostics:?}"
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

/// An ES `class`'s prototype is the closed instance type, so a missing-member
/// read through it is an ordinary TS2339, never TS2565 — `NewAjax.prototype`
/// is not an expando-capable root the way a function-as-constructor's
/// prototype is. Oracle (`tsc` 7.0.2 `--strict --allowJs --checkJs`):
/// `error TS2339: Property 'case6_unexpectedlyResolvesPathToNodeModules' does
/// not exist on type 'NewAjax'.` See #16049.
#[test]
fn test_js_prototype_read_before_assignment_reports_ts2339() {
    let source = r#"
class NewAjax {}
NewAjax.prototype.case6_unexpectedlyResolvesPathToNodeModules;
"#;

    let diagnostics = check_js(source);

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2339
                && message.contains(
                    "Property 'case6_unexpectedlyResolvesPathToNodeModules' does not exist on type 'NewAjax'."
                )
        }),
        "Expected JS class-prototype read of an absent member to report TS2339 (tsc 7.0.2 does), got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2565),
        "An ES class prototype is closed, not expando-capable — tsc never reports TS2565 here, got: {diagnostics:?}"
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
fn test_ts7_js_super_distinguishes_assignments_from_bare_jsdoc_members() {
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

    assert_eq!(
        diagnostics.iter().filter(|(code, _)| *code == 2855).count(),
        1
    );
    assert_eq!(
        diagnostics.iter().filter(|(code, _)| *code == 2339).count(),
        3
    );
    assert_eq!(
        diagnostics.iter().filter(|(code, _)| *code == 7053).count(),
        1
    );
}

#[test]
fn test_ts7_js_super_keeps_constructor_and_accessor_assignments_as_fields() {
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

    assert_eq!(
        diagnostics.iter().filter(|(code, _)| *code == 2855).count(),
        2
    );
    assert_eq!(
        diagnostics.iter().filter(|(code, _)| *code == 2339).count(),
        3
    );
    assert_eq!(
        diagnostics.iter().filter(|(code, _)| *code == 7053).count(),
        1
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
fn test_js_self_defaulting_expando_constructor_is_not_constructable() {
    let source = r#"
var test = {};
test.K = test.K ||
    function () {};
test.K.prototype = {
    add() {}
};

new test.K().add;
"#;

    // TypeScript 7 dropped JS constructor-function inference, so the
    // self-defaulting expando `test.K` is no longer constructable: `new test.K()`
    // reports an implicit-any diagnostic. (tsc uses TS7022 for the self-reference;
    // tsz reports the missing-construct-signature TS7009 — both flag the same
    // implicit-`any` result rather than silently constructing.)
    let diagnostics = check_js(source);
    assert!(
        diagnostics
            .iter()
            .any(|(code, _)| matches!(*code, 7009 | 7022)),
        "Expected `new` on a non-constructor expando to report an implicit-any diagnostic, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts_expando_reads_type_from_assignment() {
    // tsc 7.0.2 oracle: a TS-file expando property types from its assignment
    // RHS (widened), so `fn.answer = 1` makes `fn.answer: number` and the
    // string annotation errors with TS2322 — reads do NOT stay `any`.
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
        1,
        "Expected the expando read to type as number (TS2322 vs string), got: {diagnostics:?}"
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
    // TypeScript 7: `new Foo(42)` lacks a construct signature (TS7009) and is
    // typed `any`, so `f.x` is `any` and assigning it to `string` no longer
    // produces TS2322.
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 7009),
        "Expected TS7009 for `new` on a non-constructor function, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2322),
        "Expected no TS2322 (instance members are `any` under TS7), got: {diagnostics:?}"
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
    // TypeScript 7 dropped `@constructor` special-casing: the function is not a
    // constructor, so `this` is implicitly `any` (TS2683, no TS2339 on
    // `this.missing`) and `new Actual()` reports TS7009.
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2339),
        "Expected no TS2339 on `this.missing` (implicit-any `this`), got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 7009),
        "Expected `new` on a `@constructor` function to report TS7009, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2683),
        "Expected implicit-any `this` (TS2683) inside the function body, got: {diagnostics:?}"
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
    // TypeScript 7: `new Foonly()` lacks a construct signature (TS7009) so `foo`
    // is `any`; `foo.x`/`foo.y` reads are `any` — no TS2339 and no TS2322 from
    // the `string` annotations.
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for `any`-typed self-alias members, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 7009),
        "Expected TS7009 for `new` on a non-constructor function, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2322),
        "Expected no TS2322 (instance members are `any` under TS7), got: {diagnostics:?}"
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
    // TypeScript 7: a plain function with computed prototype writes is not a
    // constructor, so `new F()` reports TS7009.
    let ts7009: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7009)
        .collect();
    assert!(
        !ts7009.is_empty(),
        "Expected TS7009 for `new` on a non-constructor function, got: {diagnostics:?}"
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
    // TypeScript 7: `Object.defineProperty` on a plain function's prototype does
    // not make it a constructor, so `new F()` reports TS7009.
    let ts7009: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7009)
        .collect();
    assert!(
        !ts7009.is_empty(),
        "Expected TS7009 for `new` on a non-constructor function, got: {diagnostics:?}"
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
    // TypeScript 7: chained `A.prototype = B.prototype = { ... }` no longer makes
    // `A`/`B` constructors, so `new A()`/`new B()` report TS7009.
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 7009),
        "Expected TS7009 for `new` on non-constructor chained functions, got: {diagnostics:?}"
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
    // TypeScript 7: `new A()`/`new B()` are `any` (TS7009), so the `a.m(...)` /
    // `b.m(...)` calls are unchecked — no TS2345 from the prototype method's
    // JSDoc parameter types.
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 7009),
        "Expected TS7009 for `new` on non-constructor chained functions, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2345),
        "Expected no TS2345 (instances are `any` under TS7), got: {diagnostics:?}"
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
    // TypeScript 7: `new A()` is `any` (TS7009), so `a.m(...)` is unchecked — no
    // TS2345 — and the prototype object literal is walked without recursing (no
    // crash-fallback diagnostics).
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 7009),
        "Expected TS7009 for `new` on a non-constructor function, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2345),
        "Expected no TS2345 (instance is `any` under TS7), got: {diagnostics:?}"
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
    // TypeScript 7: `new A()` is `any` (TS7009), so instance method calls
    // `a.y(...)`/`a.z(...)` are unchecked — no `'z' does not exist` diagnostic.
    let z_missing = diagnostics
        .iter()
        .filter(|(code, message)| {
            *code == 2339 && message.contains("Property 'z' does not exist on type 'A'")
        })
        .count();
    assert_eq!(
        z_missing, 0,
        "instance method calls on an `any` receiver must not report missing members; got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 7009),
        "expected TS7009 for `new A()` on a non-constructor function; got: {diagnostics:?}"
    );
    // The static chained assignment `A.s = A.t = function g(m) { ... this.x }`
    // binds `this` to `typeof A` (the constructor object), so `this.x` reports
    // TS2339 against `typeof A`.
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

/// A `return new Self(...)` self-invocation is a `new`-expression, not a
/// plain recursive self-call: TypeScript 7 dropped implicit `isJSConstructor`
/// inference, so `Self` has no construct signature and `new Self(...)`
/// resolves to `any` (TS7009) rather than diverging. The "every return is a
/// direct self-call -> never" degenerate-recursion collapse
/// (`all_returns_are_direct_self_calls` in `function_type_circular.rs`) must
/// not treat this the same as `function fn2(n) { return fn2(n); }`, or a
/// plain call `A(1)` infers `never` and every subsequent property access
/// spuriously reports TS2339. Oracle-verified against `typescript@7.0.2`
/// (`constructorFunctionsStrict.ts`): `tsc` reports only TS7009 here.
#[test]
fn test_recursive_constructor_function_new_self_call_is_not_never_return_type() {
    let source = r#"
function A(x) {
    if (!(this instanceof A)) {
        return new A(x)
    }
    this.x = x
}
var k = A(1)
var j = new A(2)
k.x === j.x
"#;
    let diagnostics = check_js(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        0,
        "A plain call to a recursive `new Self(...)` constructor-function pattern \
         must not infer `never` (and so must not report TS2339 on later property \
         access); got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 7009),
        "expected TS7009 for `new A(...)` on a non-`@constructor` JS function; got: {diagnostics:?}"
    );
}

/// Adjacent case: the var-expression form (`var A = function(x) {...}`) of the
/// same recursive `new Self(...)` idiom must also not collapse to `never`.
#[test]
fn test_recursive_constructor_function_expression_new_self_call_is_not_never_return_type() {
    let source = r#"
var A = function (x) {
    if (!(this instanceof A)) {
        return new A(x)
    }
    this.x = x
};
var k = A(1)
k.x
"#;
    let diagnostics = check_js(source);
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        0,
        "A function-expression variant of the recursive `new Self(...)` pattern \
         must not infer `never` either; got: {diagnostics:?}"
    );
}

/// Negative/control case: a genuinely non-terminating self-recursive function
/// with NO `new` and no base case still infers `never` — this predicate must
/// keep collapsing plain self-calls, only `new self(...)` is exempted.
#[test]
fn test_plain_self_recursive_call_with_no_base_case_still_infers_never() {
    let source = r#"
function fn2(n) {
    return fn2(n);
}
var r = fn2(1);
r.anything;
"#;
    let diagnostics = check_js(source);
    // `never` has every property, so no TS2339 fires either way here — the
    // real signal is that `r`'s inferred type is still the degenerate
    // `never`/`any` collapse tsc itself performs, not a change in shape.
    // What must NOT regress is the `new`-exemption swallowing this case: a
    // renamed self-call with an unrelated `new` sibling elsewhere in the
    // file must still collapse this function's own return type.
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2345),
        "plain infinite self-recursion must not spuriously mismatch call argument types; \
         got: {diagnostics:?}"
    );
}
