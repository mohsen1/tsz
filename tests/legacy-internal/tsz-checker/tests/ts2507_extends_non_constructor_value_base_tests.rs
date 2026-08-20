//! TS2507 for non-constructor `extends <expr>` bases beyond plain identifiers.
//!
//! Structural rule: when a class `extends <expr>` and `<expr>`'s computed type
//! is a concrete (non-`any`, non-`error`) type with no construct signatures,
//! tsc reports TS2507 ("Type '{0}' is not a constructor function type.")
//! regardless of the expression's syntactic shape — tsc types the base via
//! `checkExpression`. tsz previously only ran this check for a plain
//! identifier base; `this`, parenthesized, `new`, and array/object-literal
//! bases fell through with no diagnostic at all (#17391).

use crate::context::CheckerOptions;
use crate::test_utils::{check_source, diagnostic_count};

fn strict() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
}

fn ts2507_count(source: &str) -> usize {
    let diags = check_source(source, "test.ts", strict());
    diagnostic_count(&diags, 2507)
}

#[test]
fn parenthesized_non_constructor_base_reports_ts2507() {
    assert_eq!(
        ts2507_count("class C extends (5) {}\n"),
        1,
        "a parenthesized non-constructor value base must report TS2507"
    );
}

#[test]
fn new_expression_base_reports_ts2507() {
    assert_eq!(
        ts2507_count("class Local {}\nclass C extends new Local() {}\n"),
        1,
        "a `new` expression base (an instance, not a constructor) must report TS2507"
    );
}

#[test]
fn array_literal_base_reports_ts2507() {
    assert_eq!(
        ts2507_count("class C extends [] {}\n"),
        1,
        "an array-literal base must report TS2507"
    );
}

#[test]
fn object_literal_base_reports_ts2507() {
    assert_eq!(
        ts2507_count("class C extends ({}) {}\n"),
        1,
        "an object-literal base must report TS2507"
    );
}

#[test]
fn renamed_binders_still_report_ts2507() {
    for (class_name, other_name) in [("C", "Local"), ("Widget", "Base"), ("_X0", "_Y0")] {
        let src = format!(
            "class {other_name} {{}}\nclass {class_name} extends new {other_name}() {{}}\n"
        );
        assert_eq!(
            ts2507_count(&src),
            1,
            "class={class_name} other={other_name}: got {:?}",
            check_source(&src, "test.ts", strict())
        );
    }
}

// --- Positive controls: these must stay clean -----------------------------

#[test]
fn parenthesized_null_base_is_clean() {
    // `extends null` (and any depth of parenthesization around it) is a
    // special case tsc accepts — a class with no prototype chain, not a
    // TS2507. Regression case: `TypeScript/tests/cases/compiler/classExtendingNull.ts`.
    for src in [
        "class C extends null {}\n",
        "class C extends (null) {}\n",
        "class C extends ((null)) {}\n",
    ] {
        assert_eq!(
            ts2507_count(src),
            0,
            "source={src}: `extends null` must stay clean"
        );
    }
}

#[test]
fn parenthesized_class_expression_base_is_clean() {
    assert_eq!(
        ts2507_count("const C = class {};\nclass D extends (C) {}\n"),
        0,
        "a parenthesized constructor value must not report TS2507"
    );
}

#[test]
fn call_expression_base_is_unaffected_by_this_check() {
    // Out of scope per #17391: a non-constructor call base keeps its own
    // (currently absent) handling — this test just proves the new
    // non-identifier branch does not accidentally start flagging it, since
    // that would be a behavior change beyond this fix's scope.
    assert_eq!(
        ts2507_count("function mix() { return class {}; }\nclass C extends mix() {}\n"),
        0,
        "a call-expression base returning a constructor must stay clean"
    );
}

#[test]
fn new_expression_constructor_result_is_clean() {
    // `new` of something whose return is itself constructible is not a
    // witness here (new always yields an instance) — this just confirms a
    // *valid* base elsewhere in the same file doesn't regress.
    assert_eq!(
        ts2507_count("class Base {}\nclass C extends Base {}\n"),
        0,
        "plain identifier constructor base must stay clean"
    );
}

// --- `this` heritage scoping ------------------------------------------
//
// A class's `extends <expr>` is evaluated in the scope that *contains* the
// class, not inside it (tsc's `getThisContainer`), so a bare `extends this`
// sees the *outer* `this` — `typeof globalThis` in a script, `undefined` in
// a module — never the class's own (still-being-declared) instance type.

#[test]
fn this_base_in_script_reports_ts2507_typeof_global_this() {
    let diags = check_source("class C extends this {}\n", "test.ts", strict());
    let messages: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2507)
        .map(|d| d.message_text.as_str())
        .collect();
    assert_eq!(
        messages,
        vec!["Type 'typeof globalThis' is not a constructor function type."],
        "got {diags:?}"
    );
}

#[test]
fn this_base_in_module_reports_ts2507_undefined() {
    // `export {}` makes this file an external module, so top-level `this` is
    // `undefined` rather than `typeof globalThis`.
    let diags = check_source("class C extends this {}\nexport {};\n", "test.ts", strict());
    let messages: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2507)
        .map(|d| d.message_text.as_str())
        .collect();
    assert_eq!(
        messages,
        vec!["Type 'undefined' is not a constructor function type."],
        "got {diags:?}"
    );
}

#[test]
fn this_base_in_static_method_of_constructor_this_is_clean() {
    // A *static* method's `this` is the enclosing class's constructor type
    // (has construct signatures), so using it as a nested class's base is a
    // legitimate mixin-style pattern and must stay clean. This exercises the
    // *unmodified* path (`ctx.enclosing_class` already correctly set — the
    // heritage-clause boundary this fix touches doesn't apply here, since
    // `this` is inside Outer's body, not Outer's own heritage clause).
    let diags = check_source(
        "class Outer {\n    static method() {\n        class Inner extends this {}\n        return Inner;\n    }\n}\n",
        "test.ts",
        strict(),
    );
    assert_eq!(
        diagnostic_count(&diags, 2507),
        0,
        "a static method's `this` (the constructor type) must be a valid base: {diags:?}"
    );
}

#[test]
fn this_base_in_instance_method_of_non_constructor_this_reports_ts2507() {
    // The mirror negative control: an *instance* method's `this` is the
    // class's instance type, which has no construct signatures.
    let diags = check_source(
        "class Outer {\n    method() {\n        class Inner extends this {}\n        return Inner;\n    }\n}\n",
        "test.ts",
        strict(),
    );
    assert_eq!(
        diagnostic_count(&diags, 2507),
        1,
        "an instance method's `this` (the instance type) must not be a valid base: {diags:?}"
    );
}
