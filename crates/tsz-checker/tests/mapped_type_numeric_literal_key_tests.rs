//! Tests for issue #6633: numeric literal keys in mapped types must be
//! substituted as numeric-literal `TypeId`s, not as the stringified atom.
//!
//! Structural rule: when a mapped type's constraint iterates over a numeric
//! literal type (a literal like `1`, the result of `T[number]` over a numeric
//! tuple, or a property declared with a bare numeric name such as `{ 1: ... }`),
//! the iteration must bind `K` to the matching `LiteralValue::Number` so that
//! the template body — and any value-position appearance of `K` in it —
//! retains the original numeric type. Substituting the property-name atom as a
//! string literal silently turns `[K in 1 | 2 | 3]: K` into `{ 1: "1"; 2: "2";
//! 3: "3" }`.
//!
//! The tests deliberately vary type-parameter names, alias spellings, and
//! constraint shapes so the fix expresses the structural rule rather than the
//! original reproduction's exact identifiers.

use tsz_checker::test_utils::check_source_diagnostics;

fn assert_no_errors(label: &str, source: &str) {
    let diags = check_source_diagnostics(source);
    assert!(
        diags.is_empty(),
        "{label}: expected no diagnostics, got {diags:#?}"
    );
}

#[test]
fn original_repro_tuple_to_object_numeric_literal_keys() {
    assert_no_errors(
        "TupleToObject<readonly [1,2,3]>",
        r#"
        type TupleToObject<T extends readonly any[]> = { [K in T[number]]: K };
        const numTuple = [1, 2, 3] as const;
        type NumResult = TupleToObject<typeof numTuple>;
        const nr: NumResult = { 1: 1, 2: 2, 3: 3 };
        "#,
    );
}

#[test]
fn iteration_variable_rename_invariance() {
    // Same rule, different iteration variable name. If the fix were keyed on
    // the literal spelling `K`, this would fail.
    assert_no_errors(
        "iteration variable name `P`",
        r#"
        type TupleToObjectP<T extends readonly any[]> = { [P in T[number]]: P };
        const t = [10, 20, 30] as const;
        const r: TupleToObjectP<typeof t> = { 10: 10, 20: 20, 30: 30 };
        "#,
    );
}

#[test]
fn direct_numeric_literal_union_constraint() {
    assert_no_errors(
        "[K in 1 | 2 | 3]: K",
        r#"
        type M = { [K in 1 | 2 | 3]: K };
        declare const m: M;
        const v1: 1 = m[1];
        const v2: 2 = m[2];
        const v3: 3 = m[3];
        const lit: M = { 1: 1, 2: 2, 3: 3 };
        "#,
    );
}

#[test]
fn single_numeric_literal_constraint() {
    // The single-literal arm hits `TypeData::Literal(LiteralValue::Number)`
    // directly in `extract_mapped_keys`, while the union arm hits its
    // `literal_number` branch. Both must use the numeric-literal substitution.
    assert_no_errors(
        "[K in 5]: K",
        r#"
        type M = { [K in 5]: K };
        declare const m: M;
        const v: 5 = m[5];
        const lit: M = { 5: 5 };
        "#,
    );
}

#[test]
fn string_literal_keys_remain_strings() {
    // Regression guard: keys whose origin is a string literal must keep
    // substituting `K` as a string literal, even when the atom happens to look
    // numeric. Switching the origin to numeric would corrupt every existing
    // `[K in "a" | "b"]: K`-style alias.
    assert_no_errors(
        "[K in \"a\" | \"b\"]: K (identifier-shaped string keys)",
        r#"
        type M = { [K in "a" | "b"]: K };
        declare const m: M;
        const a: "a" = m.a;
        const b: "b" = m.b;
        "#,
    );
    assert_no_errors(
        "[K in \"1\" | \"2\"]: K (numeric-looking string keys)",
        r#"
        type M = { [K in "1" | "2"]: K };
        declare const m: M;
        const a: "1" = m["1"];
        const b: "2" = m["2"];
        "#,
    );
}

#[test]
fn mixed_string_and_numeric_keys() {
    // Each key carries its own origin-kind; substitution must be per-key, not
    // a single flag for the whole iteration.
    assert_no_errors(
        "[K in \"a\" | 1]: K",
        r#"
        type M = { [K in "a" | 1]: K };
        declare const m: M;
        const k1: 1 = m[1];
        const ka: "a" = m.a;
        "#,
    );
}

#[test]
fn numeric_keys_through_indexed_access() {
    // `T[number]` over a tuple of numeric literals produces a numeric-literal
    // union — exercising the path that originally surfaced #6633.
    assert_no_errors(
        "FromObj[keyof FromObj] with numeric literal values",
        r#"
        type FromObj = { x: 1; y: 2 };
        type Vals = FromObj[keyof FromObj];
        type M = { [K in Vals]: K };
        declare const m: M;
        const v1: 1 = m[1];
        const v2: 2 = m[2];
        "#,
    );
}

#[test]
fn homomorphic_over_numeric_named_properties_preserves_numeric_keys() {
    // `[K in keyof T]: T[K]` over `{ 1: ...; 2: ... }` exercises the
    // collect-properties path of `extract_mapped_keys`. The property's
    // `is_string_named=false` plus a numeric-looking atom must drive the
    // numeric-literal substitution for `K`.
    assert_no_errors(
        "Cloned numeric-keyed object via [K in keyof T]: T[K]",
        r#"
        type Source = { 100: string; 200: number };
        type Cloned = { [K in keyof Source]: Source[K] };
        const c: Cloned = { 100: "x", 200: 42 };
        "#,
    );
}

#[test]
fn keyof_over_numeric_named_properties_yields_numeric_literals() {
    // Generalization: the same structural rule that drives mapped-type
    // key substitution also applies to `keyof T` over a numeric-keyed
    // object. Before the fix, `keyof { 1: ...; 2: ... }` produced the
    // string-literal union `"1" | "2"`; tsc produces the number-literal
    // union `1 | 2`.
    assert_no_errors(
        "keyof numeric-keyed object produces numeric literals",
        r#"
        type N = { 1: string; 2: number };
        type K = keyof N;
        const a: 1 | 2 = 1;
        const b: 1 | 2 = 2;
        const fromK: K = 1;
        function pick<P extends K>(p: P) {
            const known: 1 | 2 = p;
        }
        "#,
    );
}

#[test]
fn negative_case_still_errors() {
    // After the fix, mapped-type evaluation must still emit TS2345 for an
    // index that is not part of the numeric key union. This guards against an
    // overly-permissive fix that simply silenced the diagnostic.
    let diags = check_source_diagnostics(
        r#"
        type TupleToObject<T extends readonly any[]> = { [K in T[number]]: K };
        const t = [1, 2, 3] as const;
        declare const r: TupleToObject<typeof t>;
        const bad: 4 = r[1];
        "#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "expected TS2322 for assigning value-type 1 to expected type 4, got: {diags:#?}"
    );
}
