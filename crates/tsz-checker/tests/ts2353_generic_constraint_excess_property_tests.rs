//! Tests for TS2353 false positives against generic type-parameter
//! constraints (#6135).
//!
//! Structural rule: for `f<T extends C>(p: T)` called with a fresh object
//! literal, tsc infers `T = widen(arg)` and validates `inferred T <: C`. It
//! does NOT perform fresh-literal excess property checking against `C`
//! itself, because any property of the source not present in `C` becomes
//! part of the inferred `T`, not "excess" relative to `T`.
//!
//! Before the fix, the constraint check at call-resolution time ran
//! `is_assignable(fresh_literal_arg, constraint)`, which invoked the
//! lawyer's `check_excess_properties` and rejected valid calls.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_lib_files};

fn diags(source: &str) -> Vec<(u32, String)> {
    let libs = load_lib_files(&["es5.d.ts"]);
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn generic_class_constructor_accepts_object_literal_with_extra_properties() {
    // Original repro from #6135.
    let source = r#"
class Container<T extends { id: number }> {
  constructor(public item: T) {}
}
const cont = new Container({ id: 1, name: "test" });
"#;
    let ds = diags(source);
    let ts2353: Vec<_> = ds.iter().filter(|d| d.0 == 2353).collect();
    let ts2345: Vec<_> = ds.iter().filter(|d| d.0 == 2345).collect();
    assert!(
        ts2353.is_empty() && ts2345.is_empty(),
        "Expected no TS2353/TS2345 for generic class constructor with extra-property object literal, got: {ds:?}",
    );
}

#[test]
fn generic_function_accepts_object_literal_with_extra_properties() {
    let source = r#"
function wrap<T extends { kind: string }>(obj: T): T {
  return obj;
}
const w1 = wrap({ kind: "a", value: 42 });
"#;
    let ds = diags(source);
    let ts2353: Vec<_> = ds.iter().filter(|d| d.0 == 2353).collect();
    assert!(
        ts2353.is_empty(),
        "Expected no TS2353 for generic function with extra-property object literal, got: {ts2353:?}",
    );
}

#[test]
fn generic_function_preserves_inferred_extra_properties_for_return_type() {
    // After inference, the returned T should retain the extra property `value`
    // — accessing it should not emit TS2339.
    let source = r#"
function wrap<T extends { kind: string }>(obj: T): T {
  return obj;
}
const w1 = wrap({ kind: "a", value: 42 });
const value: number = w1.value;
const kind: string = w1.kind;
"#;
    let ds = diags(source);
    assert!(
        ds.is_empty(),
        "Expected no diagnostics — extra property must remain part of inferred T, got: {ds:?}",
    );
}

#[test]
fn satisfies_on_generic_argument_preserves_full_type() {
    // From the issue-thread reproduction (#6135 comment).
    let source = r#"
function validate<T extends { id: number }>(obj: T): T {
  return obj;
}
const validated = validate({ id: 1, name: "test" } satisfies { id: number; name: string });
const name: string = validated.name;
"#;
    let ds = diags(source);
    assert!(
        ds.is_empty(),
        "Expected no diagnostics for satisfies-typed argument to generic function, got: {ds:?}",
    );
}

#[test]
fn generic_function_taking_array_of_constrained_type_accepts_extra_properties() {
    // The parameter type CONTAINS a type parameter (T[]) rather than being one
    // — the fix must apply to nested cases too.
    let source = r#"
function first<T extends { tag: string }>(items: T[]): T {
  return items[0];
}
const f = first([{ tag: "a", extra: 1 }, { tag: "b", extra: 2 }]);
const extra: number = f.extra;
"#;
    let ds = diags(source);
    assert!(
        ds.is_empty(),
        "Expected no diagnostics for array-typed generic param, got: {ds:?}",
    );
}

#[test]
fn generic_function_with_object_wrapper_param_accepts_extra_properties() {
    let source = r#"
function pack<T extends { count: number }>(data: { wrapped: T }): T {
  return data.wrapped;
}
const r = pack({ wrapped: { count: 5, label: "x" } });
const label: string = r.label;
"#;
    let ds = diags(source);
    assert!(
        ds.is_empty(),
        "Expected no diagnostics for object-wrapper generic param, got: {ds:?}",
    );
}

#[test]
fn generic_constraint_still_rejects_missing_required_properties() {
    // The fix must not break genuine constraint violations — an argument that
    // doesn't satisfy the constraint structurally must still error.
    let source = r#"
function strict<T extends { kind: string }>(obj: T): T {
  return obj;
}
const s = strict({ value: 42 });
"#;
    let ds = diags(source);
    let ts2345: Vec<_> = ds.iter().filter(|d| d.0 == 2345).collect();
    assert!(
        !ts2345.is_empty(),
        "Expected TS2345 for argument missing required constraint property, got: {ds:?}",
    );
}

#[test]
fn generic_constraint_still_rejects_wrong_property_types() {
    // Property is present but with wrong type — must still fail with some
    // form of type-mismatch diagnostic (TS2345 from the constraint gate or
    // TS2322/TS2353 from downstream elaboration). The exact code is not the
    // contract; the contract is that the bad call is rejected.
    let source = r#"
function strict<T extends { kind: string }>(obj: T): T {
  return obj;
}
const s = strict({ kind: 42, value: "x" });
"#;
    let ds = diags(source);
    let has_failure_code = ds.iter().any(|d| matches!(d.0, 2322 | 2345 | 2353));
    assert!(
        has_failure_code,
        "Expected TS2322/TS2345/TS2353 for argument with wrong-typed constraint property, got: {ds:?}",
    );
}

#[test]
fn literal_type_constraint_still_preserved() {
    // Regression check: literal constraints like `T extends "a" | "b"` must
    // still narrow inferred T to the literal type (no widening to `string`).
    let source = r#"
function literal<T extends "a" | "b">(x: T): T {
  return x;
}
const a: "a" = literal("a");
const b: "b" = literal("b");
"#;
    let ds = diags(source);
    assert!(
        ds.is_empty(),
        "Expected literal-type constraint to preserve literal inference, got: {ds:?}",
    );
}

#[test]
fn rest_parameter_with_generic_constraint_accepts_extra_properties() {
    let source = r#"
function many<T extends { id: number }>(...items: T[]): T[] {
  return items;
}
const m = many({ id: 1, name: "x" }, { id: 2, name: "y" });
const name0: string = m[0].name;
"#;
    let ds = diags(source);
    assert!(
        ds.is_empty(),
        "Expected no diagnostics for rest-parameter generic call, got: {ds:?}",
    );
}

#[test]
fn renamed_type_parameter_still_works() {
    // Adjacent-case test (per .claude/CLAUDE.md §25/§26): renaming the bound
    // type-parameter must not break the fix — the rule is structural, not
    // name-based.
    let source = r#"
class Holder<X extends { id: number }> {
  constructor(public value: X) {}
}
const h = new Holder({ id: 1, other: "y" });
const other: string = h.value.other;
"#;
    let ds = diags(source);
    assert!(
        ds.is_empty(),
        "Expected no diagnostics for renamed type parameter, got: {ds:?}",
    );
}
