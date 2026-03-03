//! Tests for JSDoc @private/@protected/@public accessibility tag enforcement.
//!
//! Verifies that JSDoc accessibility tags on class members in JS files
//! trigger TS2341 (private) and TS2445 (protected) when accessed externally
//! or from subclasses, matching tsc behavior.

use crate::test_utils::check_js_source_diagnostics;

/// @private on class property → TS2341 when accessed externally
#[test]
fn test_jsdoc_private_property_emits_ts2341_on_external_access() {
    let source = r#"
class A {
    /** @private */
    priv = 4;
}
new A().priv
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2341 = diagnostics.iter().filter(|d| d.code == 2341).count();
    assert!(
        ts2341 >= 1,
        "Expected TS2341 for external access to @private property, got codes: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @protected on class property → TS2445 when accessed externally
#[test]
fn test_jsdoc_protected_property_emits_ts2445_on_external_access() {
    let source = r#"
class A {
    /** @protected */
    prot = 5;
}
new A().prot
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2445 = diagnostics.iter().filter(|d| d.code == 2445).count();
    assert!(
        ts2445 >= 1,
        "Expected TS2445 for external access to @protected property, got codes: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @public on class property → no error on external access
#[test]
fn test_jsdoc_public_property_no_error_on_external_access() {
    let source = r#"
class A {
    /** @public */
    pub = 6;
}
new A().pub
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let access_errors = diagnostics
        .iter()
        .filter(|d| d.code == 2341 || d.code == 2445)
        .count();
    assert_eq!(
        access_errors,
        0,
        "Expected no access errors for @public property, got codes: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @private on constructor this.x assignment → TS2341 on external access
#[test]
fn test_jsdoc_private_ctor_this_assignment_emits_ts2341() {
    let source = r#"
class C {
    constructor() {
        /** @private */
        this.priv2 = 1;
    }
}
new C().priv2
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2341 = diagnostics.iter().filter(|d| d.code == 2341).count();
    assert!(
        ts2341 >= 1,
        "Expected TS2341 for external access to @private ctor property, got codes: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @protected property accessible in subclass via this → no TS2445
#[test]
fn test_jsdoc_protected_property_accessible_in_subclass() {
    let source = r#"
class A {
    /** @protected */
    prot = 5;
}
class B extends A {
    m() {
        this.prot
    }
}
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2445 = diagnostics.iter().filter(|d| d.code == 2445).count();
    assert_eq!(
        ts2445,
        0,
        "Expected no TS2445 for subclass accessing @protected property, got codes: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @private property NOT accessible in subclass → TS2341
#[test]
fn test_jsdoc_private_property_not_accessible_in_subclass() {
    let source = r#"
class A {
    /** @private */
    priv = 4;
}
class B extends A {
    m() {
        this.priv
    }
}
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2341 = diagnostics.iter().filter(|d| d.code == 2341).count();
    assert!(
        ts2341 >= 1,
        "Expected TS2341 for subclass accessing @private property, got codes: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
