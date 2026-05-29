//! Tests for control-flow narrowing across function/closure boundaries.
//!
//! Structural rule: a property-access / element-access reference (e.g. `x.a`,
//! `x["a"]`) read inside a nested arrow function, function expression, or
//! object-literal method does NOT inherit the enclosing control-flow narrowing.
//! Such a closure may run after the property has been reassigned, so tsc resets
//! the reference to its declared type at the function boundary and re-emits the
//! relevant `strictNullChecks` diagnostic (TS18048 / TS2532). This mirrors the
//! `PropertyAccessExpression` / `ElementAccessExpression` / `this` exclusion in
//! TypeScript's `getTypeAtFlowNode` `FlowStart` handling.
//!
//! Conversely, narrowing of a constant-like reference (a `const` local or a
//! never-reassigned parameter — a plain identifier) IS preserved across the
//! boundary, and an immediately-invoked function expression (IIFE) keeps the
//! narrowing because it executes inline.

use tsz_checker::test_utils::check_source_strict_codes;

// ---------------------------------------------------------------------------
// Positive cases: property/element narrowing must be DROPPED in a closure.
// ---------------------------------------------------------------------------

/// Reported repro: property access inside a stored arrow function.
#[test]
fn property_narrowing_dropped_in_arrow() {
    let codes = check_source_strict_codes(
        r#"
function f(x: { a?: { b: number } }) {
  if (x.a) {
    const g = () => x.a.b;
    return g();
  }
}
"#,
    );
    assert!(
        codes.contains(&18048),
        "expected TS18048 for property narrowing lost across arrow boundary, got: {codes:?}"
    );
}

/// Equivalent shape #1: function expression instead of an arrow.
#[test]
fn property_narrowing_dropped_in_function_expression() {
    let codes = check_source_strict_codes(
        r#"
function f(x: { a?: { b: number } }) {
  if (x.a) {
    const g = function () { return x.a.b; };
    return g();
  }
}
"#,
    );
    assert!(
        codes.contains(&18048),
        "expected TS18048 for property narrowing lost across function-expression boundary, got: {codes:?}"
    );
}

/// Equivalent shape #2: object-literal method.
#[test]
fn property_narrowing_dropped_in_object_method() {
    let codes = check_source_strict_codes(
        r#"
function f(x: { a?: { b: number } }) {
  if (x.a) {
    const o = { m() { return x.a.b; } };
    return o.m();
  }
}
"#,
    );
    assert!(
        codes.contains(&18048),
        "expected TS18048 for property narrowing lost across object-method boundary, got: {codes:?}"
    );
}

/// Equivalent shape #3: callback passed as an argument (the common real-world
/// `.map`/`.forEach` pattern).
#[test]
fn property_narrowing_dropped_in_callback_argument() {
    let codes = check_source_strict_codes(
        r#"
function f(x: { a?: { b: number } }) {
  if (x.a) {
    return [1].map(() => x.a.b);
  }
}
"#,
    );
    assert!(
        codes.contains(&18048),
        "expected TS18048 for property narrowing lost across callback boundary, got: {codes:?}"
    );
}

/// Different property/variable spelling — proves the rule is structural, not a
/// match against a particular identifier.
#[test]
fn property_narrowing_dropped_independent_of_names() {
    let codes = check_source_strict_codes(
        r#"
function handle(payload: { result?: { count: number } }) {
  if (payload.result) {
    const run = () => payload.result.count;
    return run();
  }
}
"#,
    );
    assert!(
        codes.contains(&18048),
        "expected TS18048 regardless of identifier spelling, got: {codes:?}"
    );
}

/// Element-access reference (`x["a"]`) is also dropped — tsc reports TS2532.
#[test]
fn element_access_narrowing_dropped_in_arrow() {
    let codes = check_source_strict_codes(
        r#"
function f(x: { a?: { b: number } }) {
  if (x["a"]) {
    const g = () => x["a"].b;
    return g();
  }
}
"#,
    );
    assert!(
        codes.contains(&2532),
        "expected TS2532 for element-access narrowing lost across arrow boundary, got: {codes:?}"
    );
}

/// `this.prop` in a method closure is a property access too, so its narrowing
/// is dropped (tsc reports TS2532 here).
#[test]
fn this_property_narrowing_dropped_in_arrow() {
    let codes = check_source_strict_codes(
        r#"
class C {
  a?: { b: number };
  m() { if (this.a) { const g = () => this.a.b; return g(); } }
}
"#,
    );
    assert!(
        codes.contains(&2532),
        "expected TS2532 for this-property narrowing lost across arrow boundary, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative cases: narrowing that MUST still be preserved.
// ---------------------------------------------------------------------------

/// A `const` local that captured the narrowed value keeps its type in a closure.
#[test]
fn const_local_narrowing_preserved_in_arrow() {
    let codes = check_source_strict_codes(
        r#"
function f(x: { a?: { b: number } }) {
  const a = x.a;
  if (a) {
    const g = () => a.b;
    return g();
  }
}
"#,
    );
    assert!(
        !codes.contains(&18048) && !codes.contains(&2532),
        "const-local narrowing must be preserved across the arrow boundary, got: {codes:?}"
    );
}

/// A never-reassigned parameter (a plain identifier) keeps its narrowing.
#[test]
fn const_parameter_narrowing_preserved_in_arrow() {
    let codes = check_source_strict_codes(
        r#"
function f(p?: number) {
  if (p !== undefined) {
    const g = () => p.toFixed();
    return g();
  }
}
"#,
    );
    assert!(
        !codes.contains(&18048),
        "parameter identifier narrowing must be preserved across the arrow boundary, got: {codes:?}"
    );
}

/// An immediately-invoked function expression executes inline, so the property
/// narrowing is preserved (no `START` boundary is crossed).
#[test]
fn property_narrowing_preserved_in_iife() {
    let codes = check_source_strict_codes(
        r#"
function f(x: { a?: { b: number } }) {
  if (x.a) {
    return (() => x.a.b)();
  }
}
"#,
    );
    assert!(
        !codes.contains(&18048),
        "IIFE property narrowing must be preserved, got: {codes:?}"
    );
}
