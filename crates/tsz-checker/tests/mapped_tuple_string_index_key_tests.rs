//! Tests for issue #9725: the iteration variable of a homomorphic mapped type
//! over a tuple/array source must bind to the STRING-literal index key
//! ("0", "1", ...), matching `keyof tuple` semantics — not the numeric-literal
//! index (`0`, `1`, ...).
//!
//! Structural rule: when evaluating `{ [K in keyof T]: ... }` over a tuple
//! source, tsc substitutes `K` with the string-literal index key. So
//! `{ [K in keyof T]: K }` over `["a","b"]` yields `["0","1"]`, and template
//! remapping `` `item${K & string}` `` produces `["item0","item1"]`. The
//! template body `T[K]` still resolves because tuple indexed access accepts
//! numeric string-literal keys.
//!
//! Tests deliberately vary the iteration-variable name, tuple length, and
//! readonly-ness so the fix expresses the structural rule, not a spelling.

use tsz_checker::test_utils::check_source_diagnostics;

fn assert_no_errors(label: &str, source: &str) {
    let diags = check_source_diagnostics(source);
    assert!(
        diags.is_empty(),
        "{label}: expected no diagnostics, got {diags:#?}"
    );
}

fn assert_has_code(label: &str, source: &str, expected: u32) {
    let diags = check_source_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.code == expected),
        "{label}: expected TS{expected}, got {diags:#?}"
    );
}

// 1. Reported repro: `{ [K in keyof T]: K }` over `["a","b"]` => `["0","1"]`.
#[test]
fn reported_repro_index_key_is_string_literal() {
    assert_no_errors(
        "IX[0] is the string literal \"0\"",
        r#"
        type Idx<T> = { [K in keyof T]: K };
        type IX = Idx<["a", "b"]>;
        type IX0 = IX[0];
        const x: "0" = null as any as IX0;
        const y: "1" = null as any as IX[1];
        "#,
    );
}

// Negative control: the index key must NOT be the numeric literal `0`.
// If `K` were numeric, `IX[0]` would be `0` and assigning it to `"0"` is fine,
// but assigning to numeric `0` would be wrong. Prove `IX[0]` is a string by
// rejecting assignment to numeric `0`.
#[test]
fn index_key_is_not_numeric_literal() {
    assert_has_code(
        "IX0 (\"0\") is not assignable to numeric 0",
        r#"
        type Idx<T> = { [K in keyof T]: K };
        type IX = Idx<["a", "b"]>;
        const bad: 0 = null as any as IX[0];
        "#,
        2322,
    );
}

// 2. Renamed iteration variable (`Pos`) — structural, not name-based.
#[test]
fn renamed_iteration_variable_same_behavior() {
    assert_no_errors(
        "renamed Pos iteration variable",
        r#"
        type Keys<Arr> = { [Pos in keyof Arr]: Pos };
        type R = Keys<[string, number, boolean]>;
        const a: "0" = null as any as R[0];
        const b: "1" = null as any as R[1];
        const c: "2" = null as any as R[2];
        "#,
    );
}

// 3. Template-literal remap over the key.
#[test]
fn template_literal_remap_over_index_key() {
    assert_no_errors(
        "Prefixed<[\"x\",\"y\"]> is [\"item0\",\"item1\"]",
        r#"
        type Prefixed<T> = { [K in keyof T]: `item${K & string}` };
        type P = Prefixed<["x", "y"]>;
        const p0: "item0" = null as any as P[0];
        const p1: "item1" = null as any as P[1];
        "#,
    );
}

// 4. CONTROL: object mapped type key stays correct (was already correct).
#[test]
fn object_mapped_key_control() {
    assert_no_errors(
        "object mapped key { [P in keyof O]: P }",
        r#"
        type ObjKeys<O> = { [P in keyof O]: P };
        type OR = ObjKeys<{ a: 1; b: 2 }>;
        const a: "a" = null as any as OR["a"];
        const b: "b" = null as any as OR["b"];
        "#,
    );
}

// 5. Longer tuple `[a,b,c]` => `["0","1","2"]`.
#[test]
fn longer_tuple_string_index_keys() {
    assert_no_errors(
        "three-element tuple string indices",
        r#"
        type Idx<T> = { [K in keyof T]: K };
        type R = Idx<[number, number, number]>;
        const a: "0" = null as any as R[0];
        const b: "1" = null as any as R[1];
        const c: "2" = null as any as R[2];
        "#,
    );
}

// 6. readonly tuple source => same string-literal keys.
#[test]
fn readonly_tuple_source_string_index_keys() {
    assert_no_errors(
        "readonly tuple string indices",
        r#"
        type Idx<T> = { [K in keyof T]: K };
        type R = Idx<readonly ["a", "b"]>;
        const a: "0" = null as any as R[0];
        const b: "1" = null as any as R[1];
        "#,
    );
}

// Homomorphic value template `T[K]` must still resolve through the
// string-literal key (e.g. Partial-style mapping preserves element types).
#[test]
fn homomorphic_value_template_resolves_through_string_key() {
    assert_no_errors(
        "T[K] resolves with string-literal K",
        r#"
        type Copy<T> = { [K in keyof T]: T[K] };
        type R = Copy<[number, string]>;
        const a: number = null as any as R[0];
        const b: string = null as any as R[1];
        "#,
    );
}
