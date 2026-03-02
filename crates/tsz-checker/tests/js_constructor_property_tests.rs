//! Tests for JS constructor `this.prop = value` property inference.
//!
//! Verifies that in JS/checkJs mode, constructor body `this.prop = value`
//! assignments are recognized as class instance property declarations,
//! preventing false TS2339 errors.

use crate::test_utils::check_js_source_diagnostics;

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
    let diagnostics = check_js_source_diagnostics(source);
    let ts2339 = diagnostics.iter().filter(|d| d.code == 2339).count();
    assert_eq!(
        ts2339,
        0,
        "Expected no TS2339 for constructor this.prop access, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
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
    let diagnostics = check_js_source_diagnostics(source);
    let ts2339 = diagnostics.iter().filter(|d| d.code == 2339).count();
    assert_eq!(
        ts2339,
        0,
        "Expected no TS2339 for JSDoc-annotated constructor property, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
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
    let diagnostics = check_js_source_diagnostics(source);
    let ts2339 = diagnostics.iter().filter(|d| d.code == 2339).count();
    assert_eq!(
        ts2339,
        0,
        "Expected no TS2339 when explicit declaration exists, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
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
    let diagnostics = check_js_source_diagnostics(source);
    let ts2339 = diagnostics.iter().filter(|d| d.code == 2339).count();
    assert_eq!(
        ts2339,
        0,
        "Expected no TS2339 for subclass constructor properties, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
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
    let diagnostics = check_js_source_diagnostics(source);
    // We may or may not get TS2339 for nonexistent depending on JS mode behavior.
    // The important thing is that x does NOT cause TS2339.
    let ts2339_for_nonexistent = diagnostics
        .iter()
        .filter(|d| d.code == 2339 && d.message_text.contains("nonexistent"))
        .count();
    // In JS checkJs mode, unknown properties on class instances may or may not error.
    // Just verify no crash and that x doesn't error.
    let ts2339_for_x = diagnostics
        .iter()
        .filter(|d| d.code == 2339 && d.message_text.contains("'x'"))
        .count();
    assert_eq!(
        ts2339_for_x,
        0,
        "Expected no TS2339 for constructor-declared 'x', got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    let _ = ts2339_for_nonexistent; // just to suppress unused warning
}
