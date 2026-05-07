//! Regression coverage for assertion-function target diagnostics and reachability.

use crate::context::CheckerOptions;
use crate::test_utils::{
    check_js_source_diagnostics, check_source, check_source_codes, check_source_strict_codes,
};

#[test]
fn unannotated_assertion_identifier_emits_ts2775() {
    let codes = check_source_codes(
        r#"
function f(x: unknown) {
    const assert = (value: unknown): asserts value => {};
    assert(typeof x === "string");
}
"#,
    );
    assert!(
        codes.contains(&2775),
        "expected TS2775 for assertion variable without explicit declaration type, got {codes:?}"
    );
}

#[test]
fn invalid_assertion_alias_does_not_narrow_after_ts2775() {
    let codes = check_source_strict_codes(
        r#"
function assertString(x: unknown): asserts x is string {
    if (typeof x !== "string") throw "";
}
const f = assertString;
let v: unknown;
f(v);
v.toUpperCase();
"#,
    );
    assert!(
        codes.contains(&2775),
        "expected TS2775 for assertion alias without explicit type annotation, got {codes:?}"
    );
    assert!(
        codes.contains(&18046),
        "invalid assertion alias must not narrow unknown value, got {codes:?}"
    );
}

#[test]
fn assertion_element_access_emits_ts2776() {
    let codes = check_source_codes(
        r#"
const assert: (value: unknown) => asserts value = value => {};
const a = [assert];
a[0](true);
"#,
    );
    assert!(
        codes.contains(&2776),
        "expected TS2776 for assertion call through element access, got {codes:?}"
    );
}

#[test]
fn asserts_this_method_does_not_require_receiver_annotation() {
    let codes = check_source_codes(
        r#"
class Test {
    assertIsTest(): asserts this is Test {}
}
function f(items: Test[]) {
    for (let item of items) {
        item.assertIsTest();
    }
}
"#,
    );
    assert!(
        !codes.contains(&2775),
        "asserts-this methods should not require a receiver annotation, got {codes:?}"
    );
}

#[test]
fn assertion_false_condition_emits_unreachable_code() {
    let diagnostics = check_source(
        r#"
const assert: (value: unknown) => asserts value = value => {};
function f(x: unknown) {
    assert(false && x === undefined);
    x;
}
"#,
        "test.ts",
        CheckerOptions {
            allow_unreachable_code: Some(false),
            ..CheckerOptions::default()
        },
    );
    let codes: Vec<u32> = diagnostics.iter().map(|diag| diag.code).collect();
    assert!(
        codes.contains(&7027),
        "expected TS7027 after assert(false && ...), got {codes:?}"
    );
}

#[test]
fn jsdoc_returns_asserts_predicate_on_arrow_var_does_not_emit_ts2775() {
    // `const foo = (a) => { … }` with `@returns {asserts a is B}` is an
    // explicit assertion annotation in JS files. Without the JSDoc-asserts
    // arm in `declaration_has_explicit_assertion_annotation`, every
    // arrow-bound assertion target in JS would fire a spurious TS2775 at
    // its call site (regression: `assertionTypePredicates2.ts`).
    let diagnostics = check_js_source_diagnostics(
        r#"
/**
 * @typedef {{ x: number }} A
 */
/**
 * @typedef { A & { y: number } } B
 */

/**
 * @param {A} a
 * @returns { asserts a is B }
 */
const foo = (a) => {
    if (/** @type { B } */ (a).y !== 0) throw TypeError();
    return undefined;
};

/** @type { A } */
const a = { x: 1 };
foo(a);
"#,
    );
    let ts2775: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2775)
        .collect();
    assert!(
        ts2775.is_empty(),
        "did not expect TS2775 when assertion target has @returns predicate, got: {diagnostics:#?}"
    );
}

#[test]
fn constructor_type_predicate_return_emits_ts1228() {
    let codes = check_source_codes("declare let Q: new (x: unknown) => asserts x;");
    assert!(
        codes.contains(&1228),
        "expected TS1228 for predicate return in constructor type, got {codes:?}"
    );
}

#[test]
fn interface_construct_signature_type_predicate_does_not_emit_ts1228() {
    // Construct signatures inside an interface declaration accept type
    // predicates as their return type — tsc allows `interface I { new (...): x is T }`
    // even though the predicate is meaningless at runtime. Only constructor
    // *type* nodes (`new (...) => x is T`) and class constructor declarations
    // emit TS1228.
    let codes = check_source_codes("interface I { new (p: unknown): p is string; }");
    assert!(
        !codes.contains(&1228),
        "did not expect TS1228 for predicate return in interface construct signature, got {codes:?}"
    );
}
