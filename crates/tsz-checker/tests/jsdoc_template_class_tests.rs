//! Tests for JSDoc @template tag support on JS class declarations.
//!
//! Verifies that @template type parameters on JS classes are recognized
//! and used for generic type checking, matching tsc behavior.

use crate::test_utils::check_js_source_diagnostics;

/// @template T on a class makes it generic — constructor infers T from argument.
/// Assigning incompatible generic instances should produce TS2322.
#[test]
fn test_jsdoc_template_class_type_mismatch() {
    let source = r#"
/** @template T */
class Foo {
    /** @param {T} x */
    constructor(x) {
        this.a = x;
    }
}
var f = new Foo(1);
var g = new Foo(false);
f.a = g.a;
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for assigning boolean to number via @template class, got codes: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @template T on a class — accessing this.prop should not produce TS2339.
/// The constructor's this.a = x should define property 'a' on the instance type.
#[test]
fn test_jsdoc_template_class_no_false_ts2339() {
    let source = r#"
/** @template T */
class Box {
    /** @param {T} val */
    constructor(val) {
        this.value = val;
    }
}
var b = new Box("hello");
b.value;
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2339 = diagnostics.iter().filter(|d| d.code == 2339).count();
    assert_eq!(
        ts2339,
        0,
        "Expected no TS2339 for property access on @template class instance, got codes: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @template T on a class — multiple type parameters should all be in scope.
#[test]
fn test_jsdoc_template_class_multiple_type_params() {
    let source = r#"
/**
 * @template K
 * @template V
 */
class Pair {
    /**
     * @param {K} key
     * @param {V} val
     */
    constructor(key, val) {
        this.key = key;
        this.val = val;
    }
}
var p = new Pair("name", 42);
p.key;
p.val;
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2339 = diagnostics.iter().filter(|d| d.code == 2339).count();
    assert_eq!(
        ts2339,
        0,
        "Expected no TS2339 for multi-param @template class, got codes: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Non-JS class with syntax type params should NOT be affected by JSDoc @template.
/// This test ensures we don't break TS files.
#[test]
fn test_ts_class_with_syntax_type_params_unaffected() {
    use crate::test_utils::check_source_diagnostics;
    let source = r#"
class Box<T> {
    value: T;
    constructor(val: T) {
        this.value = val;
    }
}
var b = new Box(1);
var c = new Box("hello");
b.value = c.value;
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for incompatible generic assignment in TS class, got codes: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
