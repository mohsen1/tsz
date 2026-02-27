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
