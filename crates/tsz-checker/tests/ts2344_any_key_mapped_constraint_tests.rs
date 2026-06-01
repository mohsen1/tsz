//! Tests for `{ [P in any]: V }` (`Record<any, V>`) constraint assignability.
//!
//! Structural rule: when a mapped type has `any` as its key constraint and no
//! `as` remapping clause, it acts as a universal index-signature type
//! `{ [key: string]: V; [key: number]: V }`. Any object type whose property
//! values are all assignable to V must satisfy this constraint without `TS2344`.
//!
//! `tsc` never emits `TS2344` for plain-object, named-property, or index-signature
//! sources against `Record<any, any>` or `{ [P in any]: any }`.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::diagnostic_codes;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn check_source_diagnostics(source: &str) -> Vec<u32> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    checker.check_source_file(root);
    diagnostic_codes(&checker.ctx.diagnostics)
}

// ── Positive cases: these must NOT emit TS2344 ───────────────────────────────

#[test]
fn plain_object_literal_satisfies_record_any_any() {
    let codes = check_source_diagnostics(
        r#"
type Record<K extends keyof any, T> = { [P in K]: T };
type Test<T extends Record<any, any>> = T;
type Ok = Test<{ a: 1; b: 2 }>;
"#,
    );
    assert!(
        !codes.contains(&2344),
        "plain object literal must satisfy Record<any, any>; got errors: {codes:?}"
    );
}

#[test]
fn named_property_object_satisfies_record_any_any() {
    let codes = check_source_diagnostics(
        r#"
type Record<K extends keyof any, T> = { [P in K]: T };
type Test<T extends Record<any, any>> = T;
type Ok1 = Test<{ name: string }>;
type Ok2 = Test<{ count: number; label: string }>;
type Ok3 = Test<{ foo: boolean; bar: null }>;
"#,
    );
    assert!(
        !codes.contains(&2344),
        "named-property objects must satisfy Record<any, any>; got errors: {codes:?}"
    );
}

#[test]
fn string_index_signature_satisfies_record_any_any() {
    let codes = check_source_diagnostics(
        r#"
type Record<K extends keyof any, T> = { [P in K]: T };
type Test<T extends Record<any, any>> = T;
type Ok = Test<{ [key: string]: any }>;
"#,
    );
    assert!(
        !codes.contains(&2344),
        "index-signature object must satisfy Record<any, any>; got errors: {codes:?}"
    );
}

#[test]
fn inline_any_keyed_mapped_type_as_constraint_accepts_plain_object() {
    let codes = check_source_diagnostics(
        r#"
type Test<T extends { [P in any]: any }> = T;
type Ok1 = Test<{ x: number }>;
type Ok2 = Test<{ a: 1; b: 2; c: 3 }>;
"#,
    );
    assert!(
        !codes.contains(&2344),
        "inline `{{ [P in any]: any }}` constraint must accept plain objects; got: {codes:?}"
    );
}

#[test]
fn renamed_iteration_variable_does_not_change_semantics() {
    // The fix is structural (constraint == any), not keyed on a specific
    // iteration-variable spelling. Verify that `K`, `P`, `Q`, and `X` all work.
    let codes = check_source_diagnostics(
        r#"
type TestK<T extends { [K in any]: any }> = T;
type TestQ<T extends { [Q in any]: any }> = T;
type TestX<T extends { [X in any]: any }> = T;
type OkK = TestK<{ n: number }>;
type OkQ = TestQ<{ n: number }>;
type OkX = TestX<{ n: number }>;
"#,
    );
    assert!(
        !codes.contains(&2344),
        "iteration-variable name must not affect semantics; got: {codes:?}"
    );
}

#[test]
fn record_any_any_with_record_string_value() {
    let codes = check_source_diagnostics(
        r#"
type Record<K extends keyof any, T> = { [P in K]: T };
type Test<T extends Record<any, Record<string, number>>> = T;
type Ok = Test<{ nested: { count: number; total: number } }>;
"#,
    );
    assert!(
        !codes.contains(&2344),
        "nested Record constraint must accept matching object; got: {codes:?}"
    );
}

#[test]
fn values_type_pattern_from_ts_essentials() {
    // Simplified version of ts-essentials' ValuesType that exposed the bug.
    // T extends Record<any, any> is the key constraint.
    let codes = check_source_diagnostics(
        r#"
type Record<K extends keyof any, T> = { [P in K]: T };

type ValuesType<T extends Record<any, any>> = T[keyof T];

type V1 = ValuesType<{ a: 1; b: 2 }>;
type V2 = ValuesType<{ name: string; age: number }>;
"#,
    );
    assert!(
        !codes.contains(&2344),
        "ValuesType<{{ a: 1; b: 2 }}> must not emit TS2344; got: {codes:?}"
    );
}

// ── Negative cases: these MUST still emit TS2344 ─────────────────────────────

#[test]
fn primitive_string_does_not_satisfy_record_any_any() {
    let codes = check_source_diagnostics(
        r#"
type Record<K extends keyof any, T> = { [P in K]: T };
type Test<T extends Record<any, any>> = T;
type Bad = Test<string>;
"#,
    );
    assert!(
        codes.contains(&2344),
        "string primitive must NOT satisfy Record<any, any>; got: {codes:?}"
    );
}

#[test]
fn primitive_number_does_not_satisfy_record_any_any() {
    let codes = check_source_diagnostics(
        r#"
type Record<K extends keyof any, T> = { [P in K]: T };
type Test<T extends Record<any, any>> = T;
type Bad = Test<number>;
"#,
    );
    assert!(
        codes.contains(&2344),
        "number primitive must NOT satisfy Record<any, any>; got: {codes:?}"
    );
}

#[test]
fn primitive_boolean_does_not_satisfy_record_any_any() {
    let codes = check_source_diagnostics(
        r#"
type Record<K extends keyof any, T> = { [P in K]: T };
type Test<T extends Record<any, any>> = T;
type Bad = Test<boolean>;
"#,
    );
    assert!(
        codes.contains(&2344),
        "boolean primitive must NOT satisfy Record<any, any>; got: {codes:?}"
    );
}
