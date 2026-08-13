//! A class `extends <expr>` base reports TS2507 whenever the base's computed
//! type is concrete (non-`any`, non-`error`) but not a constructor function
//! type — regardless of the expression's syntactic shape (`this`, `new X()`,
//! `(expr)`, an array/object literal), not only named identifiers and bare
//! literal keywords.
//!
//! Structural rule: a class heritage base is typed via `checkExpression`; a
//! heritage `this` is typed in the class's *enclosing* scope (tsc's
//! `getThisContainer` never treats a class as the `this` container for its own
//! heritage), so `this` there is the outer `this` — `typeof globalThis` in a
//! script and `undefined` in an external module — not the class being declared.
//!
//! Binder names are varied across cases so no user-chosen identifier drives the
//! constructor check.

use crate::context::CheckerOptions;
use crate::test_utils::{
    check_source, check_source_with_file_is_esm, diagnostic_count, has_diagnostic_code_message,
};

const TS2507: u32 = 2507;

fn opts() -> CheckerOptions {
    // The oracle rows were measured under `--strict false`, which is the
    // default here; the divergence is strictness-independent.
    CheckerOptions::default()
}

#[test]
fn extends_this_in_script_reports_ts2507_typeof_global_this() {
    // Script (no import/export): a heritage `this` is `typeof globalThis`.
    let diags = check_source("class Widget extends this {}\n", "test.ts", opts());
    assert_eq!(
        diagnostic_count(&diags, TS2507),
        1,
        "`extends this` in a script must report TS2507: {diags:?}"
    );
    assert!(
        has_diagnostic_code_message(&diags, TS2507, "typeof globalThis"),
        "the reported base type must be `typeof globalThis`: {diags:?}"
    );
}

#[test]
fn extends_this_in_module_reports_ts2507_undefined() {
    // External module: top-level `this` (and a heritage `this`) is `undefined`.
    let diags = check_source_with_file_is_esm(
        "export {};\nclass Widget extends this {}\n",
        "test.ts",
        opts(),
        Some(true),
    );
    assert_eq!(
        diagnostic_count(&diags, TS2507),
        1,
        "`extends this` in a module must report TS2507: {diags:?}"
    );
    assert!(
        has_diagnostic_code_message(&diags, TS2507, "undefined"),
        "the reported base type must be `undefined`: {diags:?}"
    );
}

#[test]
fn extends_new_expression_reports_ts2507_on_instance_type() {
    let src = "class Gadget {}\nclass Holder extends new Gadget() {}\n";
    let diags = check_source(src, "test.ts", opts());
    assert_eq!(
        diagnostic_count(&diags, TS2507),
        1,
        "`extends new Gadget()` is an instance, not a constructor: {diags:?}"
    );
    assert!(
        has_diagnostic_code_message(&diags, TS2507, "Gadget"),
        "the reported base type must be the instance type `Gadget`: {diags:?}"
    );
}

#[test]
fn extends_parenthesized_value_reports_ts2507() {
    // A parenthesized non-constructor value still fails; parentheses do not
    // change the computed type.
    let src = "const count = 5;\nclass Holder extends (count) {}\n";
    let diags = check_source(src, "test.ts", opts());
    assert_eq!(
        diagnostic_count(&diags, TS2507),
        1,
        "`extends (count)` where count: 5 must report TS2507: {diags:?}"
    );
}

#[test]
fn extends_array_literal_reports_ts2507() {
    let diags = check_source("class Holder extends [] {}\n", "test.ts", opts());
    assert_eq!(
        diagnostic_count(&diags, TS2507),
        1,
        "`extends []` must report TS2507: {diags:?}"
    );
}

#[test]
fn extends_object_literal_reports_ts2507() {
    let diags = check_source("class Holder extends {} {}\n", "test.ts", opts());
    assert_eq!(
        diagnostic_count(&diags, TS2507),
        1,
        "`extends {{}}` must report TS2507: {diags:?}"
    );
}

#[test]
fn extends_class_expression_is_clean() {
    // A class expression IS a constructor function type.
    let diags = check_source("class Holder extends class {} {}\n", "test.ts", opts());
    assert_eq!(
        diagnostic_count(&diags, TS2507),
        0,
        "`extends class {{}}` is a valid constructor base: {diags:?}"
    );
}

#[test]
fn extends_parenthesized_class_value_is_clean() {
    let src = "class Base {}\nclass Holder extends (Base) {}\n";
    let diags = check_source(src, "test.ts", opts());
    assert_eq!(
        diagnostic_count(&diags, TS2507),
        0,
        "`extends (Base)` on a class value is a valid constructor base: {diags:?}"
    );
}

#[test]
fn extends_any_value_is_clean() {
    let src = "declare const anyBase: any;\nclass Holder extends anyBase {}\n";
    let diags = check_source(src, "test.ts", opts());
    assert_eq!(
        diagnostic_count(&diags, TS2507),
        0,
        "an `any` base must not report TS2507: {diags:?}"
    );
}

#[test]
fn extends_mixin_call_is_clean() {
    // A call returning a constructor (mixin) must not be flagged; call bases are
    // handled by the dedicated TS2508/TS2315 path, not the value-expression one.
    let src = "declare function mk<T>(b: T): T & (new () => object);\nclass Base {}\nclass Holder extends mk(Base) {}\n";
    let diags = check_source(src, "test.ts", opts());
    assert_eq!(
        diagnostic_count(&diags, TS2507),
        0,
        "a mixin call returning a constructor must not report TS2507: {diags:?}"
    );
}

#[test]
fn extends_null_stays_clean() {
    // `extends null` is a valid special case; the new value-expression path must
    // not steal it.
    let diags = check_source("class Holder extends null {}\n", "test.ts", opts());
    assert_eq!(
        diagnostic_count(&diags, TS2507),
        0,
        "`extends null` must not report TS2507: {diags:?}"
    );
}

#[test]
fn this_in_class_body_is_unaffected() {
    // Regression guard for the `this`-scoping change: a `this` in a class body
    // still resolves to the class instance, not the enclosing scope.
    let src = "class Account { balance = 0; snapshot() { return this.balance; } }\n";
    let diags = check_source(src, "test.ts", opts());
    assert_eq!(
        diagnostic_count(&diags, TS2507),
        0,
        "class-body `this` must not spuriously report TS2507: {diags:?}"
    );
    assert_eq!(
        diagnostic_count(&diags, 2683),
        0,
        "class-body `this` must not be treated as an implicit-any `this`: {diags:?}"
    );
}
