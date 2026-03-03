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
