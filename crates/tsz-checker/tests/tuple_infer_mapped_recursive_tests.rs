//! Tests for recursive conditional types whose true branch is a mapped type
//! with infer bindings from a tuple rest pattern.
//!
//! Structural rule: when a conditional type matches `T extends [infer F, ...infer R]`
//! and the true branch is a mapped type `{ [K in F]: Body<R> }`, infer bindings
//! (F and R) must be substituted into every structural position of the mapped type
//! (constraint, `name_type`, template) before evaluation. Without this, the mapped
//! type retains unresolved `Infer` nodes and evaluates to an opaque type, causing
//! false TS2353 "property does not exist" errors on valid object literals.

fn check_strict(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    tsz_checker::test_utils::check_source(
        source,
        "test.ts",
        tsz_checker::context::CheckerOptions {
            strict: true,
            ..Default::default()
        },
    )
}

fn has_error(diags: &[tsz_checker::diagnostics::Diagnostic], code: u32) -> bool {
    diags.iter().any(|d| d.code == code)
}

// ── Reported repro ──────────────────────────────────────────────────────────

/// `TupleToNestedObject<["a","b","c"], number>` must evaluate to
/// `{ a: { b: { c: number } } }` and accept the matching literal.
#[test]
fn tuple_to_nested_object_accepts_correct_literal() {
    let source = r#"
type TupleToNestedObject<T extends string[], V> =
  T extends [infer F extends string, ...infer R extends string[]]
    ? { [K in F]: TupleToNestedObject<R, V> }
    : V;

type Nested = TupleToNestedObject<["a", "b", "c"], number>;
const nested: Nested = { a: { b: { c: 42 } } };
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2353),
        "expected no TS2353 for correct nested literal, got: {diags:?}"
    );
    assert!(
        !has_error(&diags, 2322),
        "expected no TS2322 for correct nested literal, got: {diags:?}"
    );
}

/// Wrong value inside the nested object must still be rejected.
#[test]
fn tuple_to_nested_object_rejects_wrong_inner_value() {
    let source = r#"
type TupleToNestedObject<T extends string[], V> =
  T extends [infer F extends string, ...infer R extends string[]]
    ? { [K in F]: TupleToNestedObject<R, V> }
    : V;

type Nested = TupleToNestedObject<["a", "b", "c"], number>;
const nested: Nested = { a: { b: { c: "wrong" } } };
"#;
    let diags = check_strict(source);
    assert!(
        has_error(&diags, 2322),
        "expected TS2322 for wrong inner value type (string instead of number), got: {diags:?}"
    );
}

// ── Rename-invariance: different variable names prove the fix is structural ──

/// Same pattern with type-parameter names `G`/`S` instead of `F`/`R`.
#[test]
fn tuple_to_nested_object_renamed_vars_accepts_correct_literal() {
    let source = r#"
type BuildNested<T extends string[], V> =
  T extends [infer G extends string, ...infer S extends string[]]
    ? { [K in G]: BuildNested<S, V> }
    : V;

type R = BuildNested<["x", "y"], boolean>;
const r: R = { x: { y: true } };
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2353),
        "renamed-variable TupleToNested should accept correct literal, got: {diags:?}"
    );
    assert!(
        !has_error(&diags, 2322),
        "renamed-variable TupleToNested should not produce TS2322, got: {diags:?}"
    );
}

/// `A`/`B` names: wrong value still rejected.
#[test]
fn tuple_to_nested_object_renamed_vars_rejects_wrong_value() {
    let source = r#"
type BuildNested<T extends string[], V> =
  T extends [infer A extends string, ...infer B extends string[]]
    ? { [K in A]: BuildNested<B, V> }
    : V;

type R = BuildNested<["p", "q"], string>;
const r: R = { p: { q: 99 } };
"#;
    let diags = check_strict(source);
    assert!(
        has_error(&diags, 2322),
        "expected TS2322 for number instead of string in renamed-var TupleToNested, got: {diags:?}"
    );
}

// ── Two-level nesting (simpler recursion) ───────────────────────────────────

#[test]
fn tuple_to_nested_object_two_level_accepts_correct_literal() {
    let source = r#"
type TupleToNested<T extends string[], V> =
  T extends [infer F extends string, ...infer R extends string[]]
    ? { [K in F]: TupleToNested<R, V> }
    : V;

type N2 = TupleToNested<["m", "n"], string>;
const n2: N2 = { m: { n: "hello" } };
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2353),
        "two-level TupleToNested should accept correct literal, got: {diags:?}"
    );
}

// ── Infer only in the template (value position), not in `in` clause ─────────

/// When the infer binding is used only in the mapped template (value), not in
/// the `in` clause, substitution into the template must still occur.
#[test]
fn mapped_template_uses_infer_rest() {
    let source = r#"
type WrapRest<T extends string[], V> =
  T extends [infer _F extends string, ...infer R extends string[]]
    ? { result: WrapRest<R, V> }
    : V;

type W = WrapRest<["a", "b"], number>;
const w: W = { result: { result: 42 } };
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2353),
        "infer-rest in template-only position should produce no TS2353, got: {diags:?}"
    );
    assert!(
        !has_error(&diags, 2322),
        "infer-rest in template-only position should produce no TS2322, got: {diags:?}"
    );
}

// ── as-clause (name_type) substitution ─────────────────────────────────────

/// When the infer binding appears in the `as` clause of a mapped type
/// (`[K in F as Uppercase<K>]`), substitution into `name_type` must occur.
#[test]
fn mapped_as_clause_uses_infer_binding() {
    let source = r#"
type UpperNested<T extends string[], V> =
  T extends [infer F extends string, ...infer R extends string[]]
    ? { [K in F as Uppercase<K>]: UpperNested<R, V> }
    : V;

type U = UpperNested<["a"], number>;
const u: U = { A: 42 };
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2353),
        "infer binding in as-clause should be substituted, got: {diags:?}"
    );
    assert!(
        !has_error(&diags, 2322),
        "infer binding in as-clause should produce no TS2322, got: {diags:?}"
    );
}

// ── Base-case correctness (false branch) ───────────────────────────────────

/// When the tuple is empty the false branch `V` is returned; the type must be
/// exactly `number`. Both the good assignment (`42`) and a bad assignment
/// (`"str"`) verify the type is resolved, not opaque.
#[test]
fn tuple_to_nested_base_case_returns_value_type() {
    let source = r#"
type TupleToNested<T extends string[], V> =
  T extends [infer F extends string, ...infer R extends string[]]
    ? { [K in F]: TupleToNested<R, V> }
    : V;

// Empty tuple → false branch → V = number
type Base = TupleToNested<[], number>;
const b: Base = 42;
"#;
    let diags = check_strict(source);
    assert!(
        !has_error(&diags, 2322),
        "empty tuple should give V=number and accept 42 without error, got: {diags:?}"
    );
}

/// Empty tuple false branch rejects a wrong type.
#[test]
fn tuple_to_nested_base_case_rejects_wrong_value() {
    let source = r#"
type TupleToNested<T extends string[], V> =
  T extends [infer F extends string, ...infer R extends string[]]
    ? { [K in F]: TupleToNested<R, V> }
    : V;

type Base = TupleToNested<[], number>;
const bad: Base = "str";
"#;
    let diags = check_strict(source);
    assert!(
        has_error(&diags, 2322),
        "empty tuple base case should reject string for number type, got: {diags:?}"
    );
}
