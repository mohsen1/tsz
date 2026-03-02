//! Tests for JSDoc @readonly tag support.
//!
//! Verifies that @readonly annotations on class properties cause TS2540
//! when assigned to outside the constructor, matching tsc behavior.

use crate::test_utils::check_js_source_diagnostics;

/// @readonly on class property → TS2540 when assigned outside constructor
#[test]
fn test_jsdoc_readonly_property_assignment_emits_ts2540() {
    let source = r#"
class Foo {
    /** @readonly */
    y = 2
}
var f = new Foo()
f.y = 12
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2540 = diagnostics.iter().filter(|d| d.code == 2540).count();
    assert!(
        ts2540 >= 1,
        "Expected TS2540 for assignment to @readonly property, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @readonly property can be assigned in constructor → no error
#[test]
fn test_jsdoc_readonly_property_assignment_in_constructor_ok() {
    let source = r#"
class Foo {
    /** @readonly */
    y = 2
    constructor() {
        this.y = 3
    }
}
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2540 = diagnostics.iter().filter(|d| d.code == 2540).count();
    assert_eq!(
        ts2540,
        0,
        "Expected no TS2540 for constructor assignment to @readonly property, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Property without @readonly → no TS2540
#[test]
fn test_non_readonly_property_assignment_no_ts2540() {
    let source = r#"
class Foo {
    /** Just a comment */
    y = 2
}
var f = new Foo()
f.y = 12
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2540 = diagnostics.iter().filter(|d| d.code == 2540).count();
    assert_eq!(
        ts2540,
        0,
        "Expected no TS2540 for non-readonly property assignment, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
