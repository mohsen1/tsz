//! Tests for JSDoc @readonly tag and @extends/@augments validation.
//!
//! Verifies that @readonly annotations on class properties cause TS2540
//! when assigned to outside the constructor, and that @extends/@augments
//! on non-class declarations emits TS8022.

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

/// @augments on a function declaration → TS8022
#[test]
fn test_jsdoc_augments_on_function_emits_ts8022() {
    let source = r#"
class A {}
/** @augments A */
function b() {}
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts8022 = diagnostics.iter().filter(|d| d.code == 8022).count();
    assert!(
        ts8022 >= 1,
        "Expected TS8022 for @augments on function, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Dangling @extends between two classes → TS8022
#[test]
fn test_jsdoc_dangling_extends_between_classes_emits_ts8022() {
    // The @extends comment is NOT the leading comment of class B
    // (because @constructor is), so it's dangling
    let source = r#"
class A {
    constructor() {}
}

/** @extends {A} */

/** @constructor */
class B extends A {
    constructor() { super(); }
}
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts8022 = diagnostics.iter().filter(|d| d.code == 8022).count();
    assert!(
        ts8022 >= 1,
        "Expected TS8022 for dangling @extends, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Dangling @extends at end of file → TS8022
#[test]
fn test_jsdoc_dangling_extends_at_eof_emits_ts8022() {
    let source = r#"
class A {
    constructor() {}
}

/** @extends {A} */
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts8022 = diagnostics.iter().filter(|d| d.code == 8022).count();
    assert!(
        ts8022 >= 1,
        "Expected TS8022 for dangling @extends at EOF, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @typedef without type or @property → TS8021
#[test]
fn test_jsdoc_typedef_missing_type_emits_ts8021() {
    let source = r#"
/** @typedef T */
const t = 0;
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts8021 = diagnostics.iter().filter(|d| d.code == 8021).count();
    assert!(
        ts8021 >= 1,
        "Expected TS8021 for @typedef without type, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @typedef with type → no TS8021
#[test]
fn test_jsdoc_typedef_with_type_no_ts8021() {
    let source = r#"
/** @typedef {Object} Foo */
const t = 0;
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts8021 = diagnostics.iter().filter(|d| d.code == 8021).count();
    assert_eq!(
        ts8021,
        0,
        "Expected no TS8021 for @typedef with type, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @typedef with @property → no TS8021
#[test]
fn test_jsdoc_typedef_with_property_no_ts8021() {
    let source = r#"
/**
 * @typedef Person
 * @property {string} name
 */
/** @type Person */
const person = { name: "" };
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts8021 = diagnostics.iter().filter(|d| d.code == 8021).count();
    assert_eq!(
        ts8021,
        0,
        "Expected no TS8021 for @typedef with @property, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @extends on a class declaration → no TS8022
#[test]
fn test_jsdoc_extends_on_class_no_ts8022() {
    let source = r#"
class A {}
/** @extends {A} */
class B extends A {}
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts8022 = diagnostics.iter().filter(|d| d.code == 8022).count();
    assert_eq!(
        ts8022,
        0,
        "Expected no TS8022 for @extends on class, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `@property {string} #id` in JSDoc should emit TS1003.
#[test]
fn test_jsdoc_property_private_name_emits_ts1003() {
    let source = r#"
/**
 * @typedef A
 * @type {object}
 * @property {string} #id
 */
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts1003 = diagnostics.iter().filter(|d| d.code == 1003).count();
    assert!(
        ts1003 >= 1,
        "Expected TS1003 for JSDoc private-name property, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn test_jsdoc_param_star_wrapping_emits_ts1003_and_empty_name_ts8024() {
    let source = r#"
/**
 * @param *
 * {number} x Arg x.
 * @param {number}
 * * y Arg y.
 * @param {number} * z
 * Arg z.
 */
function bad(x, y, z) {
}
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts1003 = diagnostics.iter().filter(|d| d.code == 1003).count();
    let ts8024 = diagnostics.iter().filter(|d| d.code == 8024).count();
    let ts7006 = diagnostics.iter().filter(|d| d.code == 7006).count();

    assert_eq!(
        ts1003, 3,
        "Expected three TS1003 diagnostics, got: {diagnostics:?}"
    );
    assert_eq!(
        ts8024, 2,
        "Expected two TS8024 diagnostics for empty malformed @param names, got: {diagnostics:?}"
    );
    assert_eq!(
        ts7006, 3,
        "Expected TS7006 on all three parameters after malformed JSDoc recovery, got: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .filter(|d| d.code == 8024)
            .all(|d| !d.message_text.contains("name '*'")),
        "Did not expect TS8024 to preserve '*' as a JSDoc param name, got: {diagnostics:?}"
    );
}
