//! Tests for JSDoc @type tag on class properties, function declarations,
//! object literal properties, and braceless @type syntax.
//!
//! Verifies that @type annotations are used for type checking initializers
//! and that @type function types provide parameter types in JS files.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{
    HasDiagnosticCode, check_source, check_source_with_libs, diagnostic_codes,
    load_default_lib_files,
};

#[derive(Debug)]
struct Diag {
    code: u32,
    message_text: String,
}

impl HasDiagnosticCode for Diag {
    fn diagnostic_code(&self) -> u32 {
        self.code
    }
}

fn check_js_internal(source: &str, with_libs: bool) -> Vec<Diag> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        ..CheckerOptions::default()
    };
    let libs = if with_libs {
        load_default_lib_files()
    } else {
        Vec::new()
    };
    check_source_with_libs(source, "test.js", options, &libs)
        .into_iter()
        .map(|d| Diag {
            code: d.code,
            message_text: d.message_text,
        })
        .collect()
}

fn check_js(source: &str) -> Vec<Diag> {
    check_js_internal(source, false)
}

fn check_js_with_libs(source: &str) -> Vec<Diag> {
    check_js_internal(source, true)
}

#[test]
fn test_jsdoc_unknown_intrinsic_does_not_emit_ts2304() {
    let source = r#"
/** @type {unknown} */
let x;

/** @type {{ value: unknown, cb: function(unknown): unknown }} */
let y;

/** @type {Record<string, unknown>} */
let z;
"#;
    let diagnostics = check_js_with_libs(source);
    let ts2304_unknown: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2304 && diag.message_text.contains("'unknown'"))
        .collect();
    assert!(
        ts2304_unknown.is_empty(),
        "JSDoc unknown should resolve as an intrinsic, got: {diagnostics:#?}"
    );
}

fn check_js_with_exact_optional(source: &str) -> Vec<Diag> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        exact_optional_property_types: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
        .into_iter()
        .map(|d| Diag {
            code: d.code,
            message_text: d.message_text,
        })
        .collect()
}

/// @type {boolean} on class field with incompatible initializer → TS2322
#[test]
fn test_jsdoc_type_on_class_field_initializer_mismatch() {
    let source = r#"
class A {
    /** @type {boolean} */
    foo = 3
}
"#;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for number assigned to boolean @type field, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

#[test]
fn jsdoc_numeric_literal_type_accepts_exponent_syntax() {
    let source = r#"
// @ts-check
/** @type {1e3} */
let bad = 999;
"#;
    let diagnostics = check_js(source);

    assert!(
        diagnostics.iter().any(|d| {
            d.code == 2322 && d.message_text.contains("'999'") && d.message_text.contains("'1000'")
        }),
        "Expected TS2322 from JSDoc numeric literal type 1e3, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.as_str()))
            .collect::<Vec<_>>()
    );
}

/// @type {string} on class field with compatible initializer → no error
#[test]
fn test_jsdoc_type_on_class_field_compatible_initializer() {
    let source = r#"
class A {
    /** @type {string} */
    foo = "hello"
}
"#;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322,
        0,
        "Expected no TS2322 for string assigned to string @type field, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// function(string): void Closure syntax is parsed correctly
#[test]
fn test_jsdoc_function_closure_syntax_contextual_typing() {
    let source = r#"
/** @type {function(string): void} */
var f = function(value) {
    value = 1
}
"#;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for number assigned to string parameter from @type function, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// Broad @type {Function} should not suppress implicit-any on function expressions.
#[test]
fn test_jsdoc_type_function_object_does_not_contextually_type_params() {
    let source = r#"
/** @type {Function} */
const x = (a) => a + 1;
x(1);
"#;
    let diagnostics = check_js(source);
    let ts7006 = diagnostics.iter().filter(|d| d.code == 7006).count();
    assert!(
        ts7006 >= 1,
        "Expected TS7006 for broad @type {{Function}} on function expression, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

#[test]
fn test_jsdoc_typedef_before_paren_does_not_suppress_implicit_any() {
    let source = r#"
/** @typedef {object} Alias */
({ fn: x => x });
"#;
    let diagnostics = check_js(source);
    let ts7006 = diagnostics.iter().filter(|d| d.code == 7006).count();
    assert!(
        ts7006 >= 1,
        "Expected TS7006 for @typedef before parenthesized expression, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

#[test]
fn test_inline_jsdoc_typedef_does_not_type_parameter() {
    let source = r#"
function f(/** @typedef {string} Alias */ value) {
    return value;
}

f(1);
"#;
    let diagnostics = check_js(source);
    let ts7006 = diagnostics.iter().filter(|d| d.code == 7006).count();
    assert!(
        ts7006 >= 1,
        "Expected TS7006 for parameter with inline @typedef, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// JSDoc `@type {Object<K, V>}` is a record-shaped indexed type; it must not
/// emit TS2315 ("Type 'Object' is not generic") in JS files even though the
/// lib `interface Object` declaration has no type parameters.
#[test]
fn test_jsdoc_object_record_does_not_emit_ts2315() {
    let source = r#"
/** @type {Object<string, number>} */
const tagCounts = {};
tagCounts["x"] = 1;
"#;
    let diagnostics = check_js(source);
    let ts2315 = diagnostics.iter().filter(|d| d.code == 2315).count();
    assert_eq!(
        ts2315,
        0,
        "Object<K, V> must not emit TS2315 in JS, got codes: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

#[test]
fn test_jsdoc_dot_generic_array_resolves_base_name() {
    let source = r#"
/**
 * @param {Array.<string>} files
 */
function load(files) {
    files.push(1);
}
"#;
    let diagnostics = check_js_with_libs(source);
    let unresolved_array: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2304 && d.message_text.contains("Array.<string>"))
        .collect();
    assert!(
        unresolved_array.is_empty(),
        "JSDoc Array.<T> should resolve through Array, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2345),
        "Expected Array.<string> to type the parameter and reject push(1), got: {diagnostics:?}"
    );
}

#[test]
fn test_jsdoc_object_record_preserves_nested_value_type() {
    let source = r#"
// @ts-check

/** @type {Object.<string, Object.<string, number>>} */
const table = {
  row: {
    count: "wrong",
  },
};
"#;
    let diagnostics = check_js(source);
    let codes = diagnostic_codes(&diagnostics);
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for nested Object value type, got: {codes:?}"
    );
}

/// Broad @type {function} should not suppress implicit-any on function expressions.
#[test]
fn test_jsdoc_type_lowercase_function_does_not_contextually_type_params() {
    let source = r#"
/** @type {function} */
const y = (a) => a + 1;
y(1);
"#;
    let diagnostics = check_js(source);
    let ts7006 = diagnostics.iter().filter(|d| d.code == 7006).count();
    assert!(
        ts7006 >= 1,
        "Expected TS7006 for broad @type {{function}} on function expression, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// @type function type on function declaration provides parameter types
#[test]
fn test_jsdoc_type_on_function_declaration_provides_param_types() {
    let source = r#"
/** @type {(s: string) => void} */
function g(s) {
    s = 1
}
"#;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for number assigned to string parameter from @type on function decl, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

// =============================================================================
// JSDoc @type on object literal properties
// =============================================================================

/// @type {string|undefined} on object property uses declared type, not initializer
#[test]
fn test_jsdoc_type_on_object_property_overrides_initializer_type() {
    let source = r##"
var obj = {
    /** @type {string|undefined} */
    foo: undefined
};
obj.foo = 'hello';
"##;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322,
        0,
        "Expected no TS2322 when assigning string to @type {{string|undefined}} property, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// @type {string|undefined} on object property: incompatible initializer → TS2322
#[test]
fn test_jsdoc_type_on_object_property_checks_initializer() {
    let source = r#"
var obj = {
    /** @type {string|undefined} */
    bar: 42
};
"#;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for number initializer on @type {{string|undefined}} property, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// Concrete function @type on object-literal function properties should still contextually type parameters.
#[test]
fn test_jsdoc_type_on_object_function_property_provides_callable_context() {
    let source = r##"
const obj = {
    /** @type {function(number): number} */
    method1: (n1) => {
        n1 = "42";
        return 1;
    },
};
"##;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for string assigned to number parameter under @type {{function(number): number}}, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// @type {"a"} literal on object property: literal value is compatible
#[test]
fn test_jsdoc_type_literal_on_object_property_preserves_literal() {
    let source = r##"
var obj = {
    /** @type {"a"} */
    a: "a"
};
"##;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322,
        0,
        "Expected no TS2322 for literal \"a\" assigned to @type {{\"a\"}} property, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

// =============================================================================
// Braceless @type support
// =============================================================================

/// Braceless @type string on variable declaration
#[test]
fn test_braceless_jsdoc_type_simple_type() {
    let source = r#"
/** @type string */
var x = 42;
"#;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for number assigned to braceless @type string, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// Braceless @type with compatible value → no error
#[test]
fn test_braceless_jsdoc_type_compatible() {
    let source = r#"
/** @type number */
var x = 42;
"#;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322,
        0,
        "Expected no TS2322 for number assigned to braceless @type number, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// Braceless JSDoc intersections should contextually re-check object literal initializers.
#[test]
fn test_braceless_jsdoc_intersection_object_initializer_reports_ts2322() {
    let source = r#"
/** @type ({ type: 'foo' } | { type: 'bar' }) & { prop: number } */
const obj = { type: "other", prop: 10 };
"#;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for incompatible discriminant under braceless JSDoc intersection, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// Broad Function/function tags should still report TS7006 in the full mixed JSDoc file.
#[test]
fn test_jsdoc_type_tag_broad_function_full_file_regression() {
    let source = r##"
// @ts-check
/** @type {String} */
var S = "hello world";

/** @type {number} */
var n = 10;

/** @type {*} */
var anyT = 2;
anyT = "hello";

/** @type {?} */
var anyT1 = 2;
anyT1 = "hi";

/** @type {Function} */
const x = (a) => a + 1;
x(1);

/** @type {function} */
const y = (a) => a + 1;
y(1);

/** @type {function (number)} */
const x1 = (a) => a + 1;
x1(0);

/** @type {function (number): number} */
const x2 = (a) => a + 1;
x2(0);

/**
 * @type {object}
 */
var props = {};

/**
 * @type {Object}
 */
var props = {};
"##;
    let diagnostics = check_js_with_libs(source);
    let ts7006 = diagnostics.iter().filter(|d| d.code == 7006).count();
    let ts2403 = diagnostics.iter().filter(|d| d.code == 2403).count();
    assert!(
        ts7006 >= 2,
        "Expected two TS7006 diagnostics in the mixed JSDoc file, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
    assert!(
        ts2403 >= 1,
        "Expected TS2403 for @type {{object}} vs @type {{Object}} redeclaration, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

#[test]
fn test_jsdoc_object_and_object_interface_redeclaration_emit_ts2403() {
    let source = r#"
// @ts-check
/** @type {object} */
var props = {};

/** @type {Object} */
var props = {};
"#;
    let diagnostics = check_js_with_libs(source);
    let ts2403 = diagnostics.iter().filter(|d| d.code == 2403).count();
    assert!(
        ts2403 >= 1,
        "Expected TS2403 for @type {{object}} vs @type {{Object}} redeclaration, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

#[test]
fn test_jsdoc_type_predicate_cast_emits_ts1228() {
    let source = r#"
// @ts-check
/** @type {number | string} */
var value;
if (/** @type {value is string} */ (value === undefined)) {
}
"#;
    let diagnostics = check_js(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 1228),
        "Expected TS1228 for a type predicate in a JSDoc @type cast, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

#[test]
fn test_jsdoc_any_cast_string_concat_redeclaration_no_ts2403() {
    let source = r#"
// @ts-check
/** @type {string} */
var s;
var s = "" + /** @type {*} */ (4);
"#;
    let diagnostics = check_js(source);
    assert!(
        !diagnostics.iter().any(|d| d.code == 2403),
        "Did not expect TS2403 when string concatenation with a JSDoc-any cast redeclares a string var, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

#[test]
fn test_js_constructor_instance_assignment_source_uses_constructor_name() {
    let source = r#"
// @ts-check
class SomeBase {
    constructor() {
        this.p = 42;
    }
}
class SomeDerived extends SomeBase {
    constructor() {
        super();
        this.x = 42;
    }
}
function SomeFakeClass() {
    /** @type {string|number} */
    this.p = "bar";
}
var someBase = new SomeBase();
var someDerived = new SomeDerived();
var someFakeClass = new SomeFakeClass();
someFakeClass = someBase;
someFakeClass = someDerived;
someBase = someFakeClass;
"#;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| {
            panic!(
                "Expected TS2322 for assigning a checked-JS constructor instance to SomeBase, got: {:?}",
                diagnostic_codes(&diagnostics)
            )
        });
    assert!(
        ts2322
            .message_text
            .contains("Type 'SomeFakeClass' is not assignable to type 'SomeBase'."),
        "TS2322 should use the constructor instance source name, got: {}",
        ts2322.message_text
    );
}

/// JS function declarations annotated with a generic `@type {<T>(...) => T}`
/// must inherit the contextual type parameters — JS syntax has no way to
/// declare `<T>` on a function declaration, so without this inheritance the
/// call site sees a free type parameter and inference fails with TS2345.
/// Regression test for typeTagOnFunctionReferencesGeneric conformance test.
#[test]
fn test_jsdoc_type_tag_generic_on_function_declaration_inherits_type_params() {
    let source = r#"
/**
 * @typedef {<T>(m: T) => T} IFn
 */

/** @type {IFn} */
function inJs(l) {
    return l;
}
inJs(1);
"#;
    let diagnostics = check_js(source);
    let ts2345 = diagnostics.iter().filter(|d| d.code == 2345).count();
    assert_eq!(
        ts2345,
        0,
        "Expected no TS2345 for call on generic JSDoc @type function declaration, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// Arrow function reference case — already worked before the fix, keep as
/// parity guard so regressions show up paired with the declaration case.
#[test]
fn test_jsdoc_type_tag_generic_on_arrow_function_no_ts2345() {
    let source = r#"
/**
 * @typedef {<T>(m: T) => T} IFn
 */

/** @type {IFn} */
const inJsArrow = (j) => {
    return j;
};
inJsArrow(2);
"#;
    let diagnostics = check_js(source);
    let ts2345 = diagnostics.iter().filter(|d| d.code == 2345).count();
    assert_eq!(
        ts2345,
        0,
        "Expected no TS2345 for call on generic JSDoc @type arrow, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

#[test]
fn test_jsdoc_type_tag_arrow_generic_constraint_accepts_tab_whitespace() {
    let source = "\
// @ts-check

/** @type {<T extends\tstring>(value: T) => T} */
const echo = (value) => value;

echo(123);
";
    let diagnostics = check_js(source);
    let ts2345 = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .unwrap_or_else(|| {
            panic!(
                "Expected TS2345 for number argument against tab-whitespace JSDoc generic constraint, got: {:?}",
                diagnostic_codes(&diagnostics)
            )
        });
    assert!(
        ts2345
            .message_text
            .contains("Argument of type 'number' is not assignable to parameter of type 'string'."),
        "TS2345 should report the string constraint, got: {}",
        ts2345.message_text
    );
}

/// Inline generic JSDoc `@type` (no typedef alias) on a function declaration
/// must also inherit the contextual `<T>`.
#[test]
fn test_jsdoc_type_tag_inline_generic_signature_on_function_declaration() {
    let source = r#"
/** @type {<T>(param?: T) => T | undefined} */
function typed(param) {
    return param;
}
typed(1);
typed("x");
"#;
    let diagnostics = check_js(source);
    let ts2345 = diagnostics.iter().filter(|d| d.code == 2345).count();
    assert_eq!(
        ts2345,
        0,
        "Expected no TS2345 for call on inline generic JSDoc @type function, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// Under `exactOptionalPropertyTypes`, a JSDoc `@property {T} [name]` declares
/// `name?: T` (not `name?: T | undefined`). Assigning `name: undefined` must
/// emit TS2375 — the same diagnostic produced for the equivalent inline form
/// `@typedef {{ name?: T }}`.
///
/// Before the fix, the imperative `@typedef {object}` + `@property` path
/// unconditionally widened the property type to `T | undefined` whenever
/// `strictNullChecks` was on, swallowing the TS2375 because `value: undefined`
/// matched the widened property type.
#[test]
fn test_jsdoc_property_optional_under_exact_optional_emits_ts2375() {
    let source = r#"
/**
 * @typedef {object} A
 * @property {number} [value]
 */

/** @type {A} */
const a = { value: undefined };
"#;
    let diagnostics = check_js_with_exact_optional(source);
    let ts2375 = diagnostics.iter().filter(|d| d.code == 2375).count();
    assert_eq!(
        ts2375,
        1,
        "Expected TS2375 for `value: undefined` under exactOptionalPropertyTypes \
         when @property uses `[value]` optional syntax, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
}

/// JSDoc `@typedef {object} A` should preserve the alias name `A` in TS2375
/// diagnostic messages — tsc reports `Type '...' is not assignable to type
/// 'A' with 'exactOptionalPropertyTypes: true'.`, not the expanded body
/// shape `'{ value?: number; }'`. The fix attaches a display-alias
/// (`body_type → lazy(def_for_A)`) so
/// `format_exact_optional_target_type_for_message` can recover the
/// authoritative def name.
#[test]
fn test_jsdoc_typedef_object_alias_name_preserved_in_ts2375() {
    let source = r#"
/**
 * @typedef {object} MyAlias
 * @property {number} [value]
 */

/** @type {MyAlias} */
const a = { value: undefined };
"#;
    // Use the same harness as the existing TS2375 test but capture the
    // message text so we can assert on the displayed target type.
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        exact_optional_property_types: true,
        ..CheckerOptions::default()
    };
    let diagnostics = check_source(source, "test.js", options);

    let ts2375: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2375).collect();
    assert_eq!(
        ts2375.len(),
        1,
        "Expected exactly one TS2375 for the typedef-aliased target, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
    let msg = &ts2375[0].message_text;
    assert!(
        msg.contains("'MyAlias'"),
        "TS2375 target display must include the typedef name `'MyAlias'`, got: {msg}"
    );
    assert!(
        !msg.contains("'{ value?: number; }'"),
        "TS2375 target display must NOT expand to the body shape `'{{ value?: number; }}'`, got: {msg}"
    );
}

/// Same alias-preservation rule with the inline `@typedef {{ ... }}` form
/// — pins that the fix is structural (any `@typedef` registers the
/// display-alias, not just the imperative `{object}` + `@property` form).
#[test]
fn test_jsdoc_typedef_inline_alias_name_preserved_in_ts2375() {
    let source = r#"
/**
 * @typedef {{ flag?: string }} OtherAlias
 */

/** @type {OtherAlias} */
const b = { flag: undefined };
"#;
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        exact_optional_property_types: true,
        ..CheckerOptions::default()
    };
    let diagnostics = check_source(source, "test.js", options);

    let ts2375: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.code == 2375).collect();
    assert_eq!(
        ts2375.len(),
        1,
        "Expected exactly one TS2375 for the inline-typedef alias target, got: {:?}",
        diagnostic_codes(&diagnostics)
    );
    let msg = &ts2375[0].message_text;
    assert!(
        msg.contains("'OtherAlias'"),
        "TS2375 target display must include the inline-typedef name `'OtherAlias'`, got: {msg}"
    );
}

/// Without `exactOptionalPropertyTypes`, the same `@property {T} [name]` pattern
/// must still accept `name: undefined` because the property type widens to
/// `T | undefined` under classic optional semantics. This guards against a
/// fix that drops the widening unconditionally.
#[test]
fn test_jsdoc_property_optional_without_exact_optional_accepts_undefined() {
    let source = r#"
/**
 * @typedef {object} A
 * @property {number} [value]
 */

/** @type {A} */
const a = { value: undefined };
"#;
    // strict (so strictNullChecks=true) but exactOptionalPropertyTypes=false
    let diagnostics = check_js(source);
    let ts2375 = diagnostics.iter().filter(|d| d.code == 2375).count();
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2375,
        0,
        "TS2375 must not fire without exactOptionalPropertyTypes, got codes: {:?}",
        diagnostic_codes(&diagnostics)
    );
    assert_eq!(
        ts2322,
        0,
        "TS2322 must not fire when classic optional semantics widen to `T | undefined`, \
         got codes: {:?}",
        diagnostic_codes(&diagnostics)
    );
}
