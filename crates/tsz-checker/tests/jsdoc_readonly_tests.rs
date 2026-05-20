//! Tests for JSDoc @readonly tag and @extends/@augments validation.
//!
//! Verifies that @readonly annotations on class properties cause TS2540
//! when assigned to outside the constructor, and that @extends/@augments
//! on non-class declarations emits TS8022.

use crate::context::CheckerOptions;
use crate::test_utils::{check_js_source_diagnostics, check_source, diagnostic_codes};

fn check_strict_js_source_diagnostics(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_source(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            strict: true,
            no_implicit_this: true,
            no_implicit_override: true,
            ..CheckerOptions::default()
        },
    )
}

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
        diagnostic_codes(&diagnostics)
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
        diagnostic_codes(&diagnostics)
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
        diagnostic_codes(&diagnostics)
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
        diagnostic_codes(&diagnostics)
    );
}

/// @extends separated from its class by another JSDoc block → TS8022.
/// tsc treats the interposed `/** @constructor */` block as interrupting
/// the @extends tag's attachment, making it orphaned. Confirmed via
/// conformance/jsdoc/extendsTag2.ts fingerprint (code 8022, file="").
#[test]
fn test_jsdoc_extends_interposed_jsdoc_emits_ts8022() {
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
        "Expected TS8022 for @extends separated from class by interposed JSDoc, got: {:?}",
        diagnostic_codes(&diagnostics)
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
        diagnostic_codes(&diagnostics)
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
        diagnostic_codes(&diagnostics)
    );
}

#[test]
fn invalid_typedef_prefix_does_not_declare_jsdoc_alias() {
    let source = r#"
/**
 * @typedefx {{ n: number }} Foo
 */

/** @type {Foo} */
const value = { n: 1 };

value.n.toFixed();
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let codes = diagnostic_codes(&diagnostics);
    assert!(
        codes.contains(&2304),
        "Expected unresolved Foo when @typedefx is ignored, got: {codes:?}",
    );
    assert!(
        !codes.contains(&8021),
        "Expected no malformed @typedef diagnostic for @typedefx, got: {codes:?}",
    );
}

#[test]
fn invalid_import_prefix_does_not_create_jsdoc_alias() {
    let source = r#"
/**
 * @importx { Foo } from "./types"
 */

/** @type {Foo} */
const value = { n: 1 };

value.n.toFixed();
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let codes = diagnostic_codes(&diagnostics);
    assert!(
        codes.contains(&2304),
        "Expected unresolved Foo when @importx is ignored, got: {codes:?}",
    );
    assert!(
        !codes.contains(&1109),
        "Expected no malformed @import diagnostic for @importx, got: {codes:?}",
    );
}

#[test]
fn invalid_this_prefix_does_not_suppress_implicit_this() {
    let source = r#"
/**
 * @thisx {{ n: number }}
 */
function f() {
  this.n.toFixed();
}

f;
"#;
    let diagnostics = check_strict_js_source_diagnostics(source);
    let codes = diagnostic_codes(&diagnostics);
    assert!(
        codes.contains(&2683),
        "Expected TS2683 when @thisx is ignored, got: {codes:?}",
    );
}

#[test]
fn invalid_override_prefix_does_not_satisfy_no_implicit_override() {
    let source = r#"
class Base {
  m() {}
}

class Derived extends Base {
  /** @overridex */
  m() {}
}

Derived;
"#;
    let diagnostics = check_strict_js_source_diagnostics(source);
    let codes = diagnostic_codes(&diagnostics);
    assert!(
        codes.contains(&4119),
        "Expected TS4119 when @overridex is ignored, got: {codes:?}",
    );
}

#[test]
fn invalid_template_prefix_on_constructor_does_not_emit_ts1092() {
    let source = r#"
// @ts-check
class C {
  /** @templateFoo */
  constructor() {}
}
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let codes = diagnostic_codes(&diagnostics);
    assert!(
        !codes.contains(&1092),
        "Expected no TS1092 for @templateFoo, got: {codes:?}",
    );
}

#[test]
fn jsdoc_template_on_constructor_still_emits_ts1092() {
    let source = r#"
// @ts-check
class C {
  /** @template T */
  constructor() {}
}
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let codes = diagnostic_codes(&diagnostics);
    assert!(
        codes.contains(&1092),
        "Expected TS1092 for a real constructor @template tag, got: {codes:?}",
    );
}

#[test]
fn jsdoc_return_after_constructor_typedef_still_emits_ts1093() {
    let source = r#"
// @ts-check
class C {
  /**
   * @typedef {number} N
   * @return {string}
   */
  constructor() {}
}
"#;
    let diagnostics = check_strict_js_source_diagnostics(source);
    let codes = diagnostic_codes(&diagnostics);
    assert!(
        codes.contains(&1093),
        "Expected TS1093 for constructor @return after @typedef, got: {codes:?}",
    );
}

#[test]
fn jsdoc_callback_return_on_constructor_does_not_emit_ts1093() {
    let source = r#"
// @ts-check
class C {
  /**
   * @callback Getter
   * @returns {string}
   */
  constructor() {}
}
"#;
    let diagnostics = check_strict_js_source_diagnostics(source);
    let codes = diagnostic_codes(&diagnostics);
    assert!(
        !codes.contains(&1093),
        "Expected nested callback @returns not to emit TS1093, got: {codes:?}",
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
        diagnostic_codes(&diagnostics)
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
        diagnostic_codes(&diagnostics)
    );
}

#[test]
fn test_jsdoc_multiple_typedefs_missing_second_type_emits_ts8021() {
    let source = r#"
// @ts-check
/**
 * @typedef {object} A
 * @typedef B
 */
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts8021 = diagnostics.iter().filter(|d| d.code == 8021).count();
    assert_eq!(
        ts8021, 1,
        "Expected exactly one TS8021 for the second @typedef, got: {diagnostics:?}"
    );
}

#[test]
fn test_jsdoc_type_tags_are_counted_per_typedef() {
    let source = r#"
// @ts-check
/**
 * @typedef A
 * @type {string}
 * @typedef B
 * @type {number}
 */
"#;
    let diagnostics = check_js_source_diagnostics(source);
    assert!(
        !diagnostics.iter().any(|d| d.code == 8033 || d.code == 8021),
        "Expected one @type per @typedef to be valid, got: {diagnostics:?}"
    );
}

#[test]
fn test_jsdoc_duplicate_type_tags_within_one_typedef_emits_ts8033() {
    let source = r#"
// @ts-check
/**
 * @typedef A
 * @type {string}
 * @type {number}
 */
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts8033 = diagnostics.iter().filter(|d| d.code == 8033).count();
    assert_eq!(
        ts8033, 1,
        "Expected duplicate @type tags in one @typedef to emit TS8033, got: {diagnostics:?}"
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
        diagnostic_codes(&diagnostics)
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
        diagnostic_codes(&diagnostics)
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

#[test]
fn test_jsdoc_param_postfix_nullable_recovery_and_rest_order() {
    let source = r#"
/**
 * @param {number![]} x
 * @param {!number[]} y
 * @param {(number[])!} z
 * @param {number?[]} a
 * @param {?number[]} b
 * @param {(number[])?} c
 * @param {...?number} e
 * @param {...number?} f
 * @param {...number!?} g
 * @param {...number?!} h
 * @param {...number[]} i
 * @param {...number![]?} j
 * @param {...number?[]!} k
 * @param {number extends number ? true : false} l
 * @param {[number, number?]} m
 */
function f(x, y, z, a, b, c, e, f, g, h, i, j, k, l, m) {
}
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let count = |code| diagnostics.iter().filter(|d| d.code == code).count();

    assert_eq!(
        count(1005),
        3,
        "Expected three TS1005 diagnostics for unparenthesized postfix nullable JSDoc arrays, got: {diagnostics:?}"
    );
    assert_eq!(
        count(1014),
        5,
        "Expected five TS1014 diagnostics for non-final JSDoc rest params, got: {diagnostics:?}"
    );
    assert_eq!(
        count(8024),
        3,
        "Expected three TS8024 diagnostics for malformed @param tags recovering with an empty name, got: {diagnostics:?}"
    );
    assert_eq!(
        count(7006),
        3,
        "Expected malformed JSDoc params a/h/k not to suppress TS7006, got: {diagnostics:?}"
    );
}
