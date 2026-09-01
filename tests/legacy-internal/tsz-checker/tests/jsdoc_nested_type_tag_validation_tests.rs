//! Validation of JSDoc `@type` tags on variable statements inside function bodies.
//!
//! `tsc` validates a `@type` annotation wherever it appears, not only at the top
//! level: `function f() { /** @type {Missing} */ var y }` reports TS2304, and the
//! same shape naming a value reports TS2749. tsz's `@type` scan only visited
//! comments leading top-level statements, so nested annotations went unchecked
//! entirely — the corpus witnesses are `enumTag` and `jsDeclarationsEnumTag`,
//! where `/** @type {Target} */` inside a function body should report TS2749
//! (tsc 7.0.2 does not implement `@enum`, so `Target` is value-only).
//!
//! Inline expression casts (`value => /** @type {T} */(x)`) lead no statement and
//! are deliberately still excluded.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn js_codes(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

// --- Nested `@type` is now validated. ---

#[test]
fn nested_type_tag_reports_missing_name() {
    let source = "function g() {\n    /** @type {NoSuchTypeAtAll} */\n    var z\n    return z\n}\n";
    assert!(js_codes(source).contains(&2304));
}

#[test]
fn nested_type_tag_reports_value_used_as_type() {
    let source = "var v0 = 1\nfunction f() {\n    /** @type {v0} */\n    var y\n    return y\n}\n";
    assert!(js_codes(source).contains(&2749));
}

/// The `enumTag` witness: `@enum` does not declare a type in tsc 7.0.2, so a
/// nested `@type {Target}` is a value used as a type.
#[test]
fn nested_type_tag_reports_enum_tag_name_as_value() {
    let source = concat!(
        "/** @enum {string} */\n",
        "const Target = { A: \"a\" }\n",
        "function consume() {\n",
        "    /** @type {Target} */\n",
        "    var v = Target.A\n",
        "    return v\n",
        "}\n",
    );
    assert!(js_codes(source).contains(&2749));
}

/// A renamed binder in a differently named function: the rule is structural.
#[test]
fn nested_type_tag_rule_is_not_name_specific() {
    let source = "var counter = 1\nfunction helper() {\n    /** @type {counter} */\n    var c\n    return c\n}\n";
    assert!(js_codes(source).contains(&2749));
}

// --- Scope: a `@typedef` in the same function body is still in scope. ---

/// Witness `typedefScope1`: `B` is declared and used inside `B1`, so the nested
/// `@type {B}` must not report. This scan carries no function scopes, so it
/// defers to the `@typedef` rather than emitting a false TS2304.
#[test]
fn nested_typedef_is_in_scope_for_nested_type_tag() {
    let source = concat!(
        "function B1() {\n",
        "    /** @typedef {number} B */\n",
        "    /** @type {B} */\n",
        "    var ok1 = 0;\n",
        "    return ok1;\n",
        "}\n",
    );
    assert!(!js_codes(source).contains(&2304));
}

/// Two sibling functions each declaring their own `@typedef` of the same name
/// stay silent, as they do in `typedefScope1`.
#[test]
fn sibling_function_typedefs_do_not_collide() {
    let source = concat!(
        "function B1() {\n",
        "    /** @typedef {number} B */\n",
        "    /** @type {B} */\n",
        "    var ok1 = 0;\n",
        "    return ok1;\n",
        "}\n",
        "function B2() {\n",
        "    /** @typedef {string} B */\n",
        "    /** @type {B} */\n",
        "    var ok2 = 'hi';\n",
        "    return ok2;\n",
        "}\n",
    );
    assert!(!js_codes(source).contains(&2304));
}

// --- Top-level behaviour is unchanged. ---

#[test]
fn top_level_type_tag_still_reports_missing_name() {
    assert!(js_codes("/** @type {NoSuchTypeAtAll} */\nvar z\n").contains(&2304));
}

#[test]
fn top_level_type_tag_still_reports_value_used_as_type() {
    assert!(js_codes("var v = 1\n/** @type {v} */\nvar y\n").contains(&2749));
}

/// A resolvable nested annotation stays silent.
#[test]
fn nested_type_tag_with_known_type_is_silent() {
    let source = "function f() {\n    /** @type {string} */\n    var s = 'x'\n    return s\n}\n";
    let codes = js_codes(source);
    assert!(!codes.contains(&2304) && !codes.contains(&2749));
}

// --- Expression statements carry annotations too. ---

/// A JS class declares its fields as `/** @type {T} */ this.x = ...` inside the
/// constructor — an *expression* statement, not a variable statement. tsc
/// validates that annotation (witness:
/// `jsDeclarationsReferenceToClassInstanceCrossFile`).
#[test]
fn nested_type_tag_on_this_assignment_reports_missing_name() {
    let source = concat!(
        "class Render {\n",
        "  constructor() {\n",
        "    /** @type {NoSuchTypeAtAll} */\n",
        "    this.objects = [];\n",
        "  }\n",
        "}\n",
        "new Render()\n",
    );
    assert!(js_codes(source).contains(&2304));
}

/// The array suffix does not change the rule — the element name is still checked.
#[test]
fn nested_type_tag_on_this_assignment_checks_array_element() {
    let source = concat!(
        "class Render {\n",
        "  constructor() {\n",
        "    /** @type {NoSuchTypeAtAll[]} */\n",
        "    this.objects = [];\n",
        "  }\n",
        "}\n",
        "new Render()\n",
    );
    assert!(js_codes(source).contains(&2304));
}

/// A plain expression statement outside a class behaves the same.
#[test]
fn nested_type_tag_on_plain_expression_statement_reports() {
    let source = concat!(
        "var o = {};\n",
        "function f() {\n",
        "  /** @type {NoSuchTypeAtAll} */\n",
        "  o.field = 1;\n",
        "}\n",
        "f\n",
    );
    assert!(js_codes(source).contains(&2304));
}

/// A resolvable annotation on an expression statement stays silent.
#[test]
fn nested_type_tag_on_this_assignment_with_known_type_is_silent() {
    let source = concat!(
        "class Render {\n",
        "  constructor() {\n",
        "    /** @type {string[]} */\n",
        "    this.names = [];\n",
        "  }\n",
        "}\n",
        "new Render()\n",
    );
    let codes = js_codes(source);
    assert!(!codes.contains(&2304) && !codes.contains(&2749));
}
