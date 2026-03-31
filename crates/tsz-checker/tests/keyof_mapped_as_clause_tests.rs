//! Tests for keyof on mapped types with as-clauses.
//!
//! When a mapped type uses key remapping (`as` clause), the checker's keyof
//! pre-check in `assignability_checker.rs` must not emit a false TS2322.
//! The pre-check extracts allowed keys from a keyof type, but for complex
//! inner types (Application, Mapped with as-clause, Lazy), it returns an
//! empty set. The empty-set guard ensures we fall through to the solver's
//! full evaluation pipeline rather than treating every string literal as
//! not-in-keys.

use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Helper: parse, bind, check; return diagnostic codes.
fn check_and_get_codes(code: &str) -> Vec<u32> {
    let mut parser = ParserState::new("test.ts".to_string(), code.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn keyof_mapped_type_with_as_clause_no_false_ts2322() {
    // PickByValueType filters keys via `as T[K] extends U ? K : never`.
    // `keyof PickByValueType<Example, string>` should resolve to "foo".
    // Assigning "foo" to that keyof type must NOT produce TS2322.
    let code = r#"
type Example = { foo: string; bar: number };
type PickByValueType<T, U> = {
  [K in keyof T as T[K] extends U ? K : never]: T[K]
};
type T1 = PickByValueType<Example, string>;
type T2 = keyof T1;
const e2: T2 = "foo";
    "#;

    let codes = check_and_get_codes(code);
    let ts2322_count = codes.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 for valid keyof mapped-type-with-as-clause assignment, got codes: {codes:?}"
    );
}

#[test]
fn keyof_mapped_type_with_as_clause_invalid_key_still_errors() {
    // "bar" is filtered OUT by the as-clause (bar: number, not string).
    // Assigning "bar" to keyof PickByValueType<Example, string> should error.
    let code = r#"
type Example = { foo: string; bar: number };
type PickByValueType<T, U> = {
  [K in keyof T as T[K] extends U ? K : never]: T[K]
};
type T1 = PickByValueType<Example, string>;
type T2 = keyof T1;
const e2: T2 = "bar";
    "#;

    let codes = check_and_get_codes(code);
    let ts2322_count = codes.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "Expected TS2322 for invalid key 'bar' not in keyof mapped type, got codes: {codes:?}"
    );
}

#[test]
fn keyof_simple_object_pre_check_still_works() {
    // For a plain object type, the pre-check should still catch invalid keys
    // without needing to fall through to the solver.
    let code = r#"
type Obj = { a: number; b: string };
type K = keyof Obj;
const x: K = "c";
    "#;

    let codes = check_and_get_codes(code);
    let ts2322_count = codes.iter().filter(|&&c| c == 2322).count();
    assert_eq!(
        ts2322_count, 1,
        "Expected TS2322 for 'c' not in keyof simple object, got codes: {codes:?}"
    );
}

#[test]
fn non_homomorphic_mapped_type_solver_delegation() {
    // Non-homomorphic mapped types (constraint is a literal union, not keyof T)
    // should be evaluated by the solver's evaluator via evaluate_type_with_env,
    // not by the checker's manual property expansion loop. This test verifies
    // that the solver-first delegation path produces correct results.
    let code = r#"
type Keys = "a" | "b";
type MyRecord = { [K in Keys]: number };
const r: MyRecord = { a: 1, b: 2 };
const x: number = r.a;
const y: number = r.b;
    "#;

    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2339),
        "Expected no TS2339 for property access on non-homomorphic mapped type, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 for valid assignment to non-homomorphic mapped type, got: {codes:?}"
    );
}

#[test]
fn non_homomorphic_mapped_type_with_template_transform() {
    // Non-homomorphic mapped type with a non-trivial template.
    // The solver's evaluator should correctly expand { [K in "x" | "y"]: Box<K> }
    // to { x: Box<"x">, y: Box<"y"> } (or the evaluated form).
    let code = r#"
type Box<T> = { value: T };
type MyMap = { [K in "x" | "y"]: Box<K> };
const m: MyMap = { x: { value: "x" }, y: { value: "y" } };
    "#;

    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 for non-homomorphic mapped type with template, got: {codes:?}"
    );
}

#[test]
fn mapped_type_as_clause_over_object_union_constraint() {
    // Mapped type with `as` clause where the constraint is a union of objects
    // (not string literals). The solver must iterate over the object members,
    // evaluate the `as` clause for each, and produce a concrete object type.
    //
    // This fixes a false TS2536 in patterns like:
    //   type Baz = { [K in keyof Lookup]: Lookup[K]['name'] }
    // where Lookup is a mapped type with key remapping over an object union.
    let code = r#"
type Lookup = { [Item in ({readonly name: "a"} | {readonly name: "b"}) as Item['name']]: Item };
type Baz = { [K in keyof Lookup]: Lookup[K]['name'] };
    "#;

    let codes = check_and_get_codes(code);
    let ts2536_count = codes.iter().filter(|&&c| c == 2536).count();
    assert_eq!(
        ts2536_count, 0,
        "Expected no TS2536 for indexing mapped type with as-clause over object union, got codes: {codes:?}"
    );
}

#[test]
fn mapped_type_as_clause_over_object_union_produces_concrete_type() {
    // Verify that the mapped type with `as` clause over an object union produces
    // a concrete type where property access works correctly.
    let code = r#"
type Lookup = { [Item in ({name: "a", value: 1} | {name: "b", value: 2}) as Item['name']]: Item };
const x: Lookup = { a: { name: "a", value: 1 }, b: { name: "b", value: 2 } };
const y: { name: "a", value: 1 } = x.a;
    "#;

    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 for accessing mapped type with as-clause over object union, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2339),
        "Expected no TS2339 for property access on mapped type with as-clause, got: {codes:?}"
    );
}

#[test]
fn mapped_type_as_clause_never_filter_over_objects() {
    // When the `as` clause evaluates to `never` for some members, those members
    // should be filtered out (not produce properties).
    let code = r#"
type OnlyA = { [Item in ({name: "a"} | {name: "b"}) as Item extends {name: "a"} ? Item['name'] : never]: Item };
const x: OnlyA = { a: { name: "a" } };
    "#;

    let codes = check_and_get_codes(code);
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 for filtered mapped type with as-clause, got: {codes:?}"
    );
}
