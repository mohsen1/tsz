//! A concrete tuple base indexed by a generic type-parameter chain
//! (`Table[D1][0]`, `AddDigitTable[Carry][T][U]`) must derive the tuple's
//! element-value key-space, accepting the further index exactly as `tsc` does —
//! not a spurious TS2536.
//!
//! Structural rule: when `Base[I]` has a concrete tuple base `Base` and a
//! generic index `I extends C` whose constraint lies in the tuple's numeric
//! element-index domain (`number`, or numeric literals `0..len`), `tsc` resolves
//! `Base[I]` to the tuple's element-value union (`Base[number]`), whose key-space
//! includes the element indices, so a chained index `Base[I][J]` is valid. Owner:
//! solver-backed checker indexed-access constraint recovery
//! (`indexed_access_helpers::generic_tuple_chain_index_access_allows_index`).
//!
//! Previously `tsz`'s last-resort recovery keyed the value union off
//! `Base[keyof Base]`, which for a tuple base pollutes the union with
//! `length`/array-method values and so spuriously rejected the element index.
//!
//! Negative controls (must still emit TS2536): an out-of-range inner literal
//! constraint, an inner constraint of `keyof Base` (which includes
//! `length`/methods), and an outer index that is genuinely absent from the
//! element-value key-space. Binder names are varied so the fix cannot be a
//! spelling-specific point patch.

use tsz_common::diagnostics::Diagnostic;

fn check(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source_diagnostics(source)
}

fn codes(diags: &[Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

fn has_code(diags: &[Diagnostic], code: u32) -> bool {
    diags.iter().any(|d| d.code == code)
}

/// Reported repro (#13655): a tuple-of-tuples indexed by a generic param chain.
/// Varied alias/param spellings prove the rule is structural.
#[test]
fn concrete_tuple_generic_index_chain_is_accepted() {
    for (alias, param) in [("Table", "D1"), ("Lookup", "Row"), ("Grid", "Ix")] {
        let src = format!(
            r#"
type {alias} = [[10, 11], [20, 21]];
type Nested<{param} extends 0 | 1> = {alias}[{param}][0];
"#
        );
        let diags = check(&src);
        assert!(
            !has_code(&diags, 2536),
            "concrete tuple chain (alias {alias}, param {param}) must not emit TS2536, got: {:?}",
            codes(&diags)
        );
    }
}

/// The faithful `hotscript` `AddDigitTable[Carry][T][U]` shape: a 3-level chain
/// over a carry x digit x digit lookup tuple.
#[test]
fn three_level_concrete_tuple_chain_is_accepted() {
    let diags = check(
        r#"
type Digit = 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9;
type AddDigitTable = [
  [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9], [1, 2, 3, 4, 5, 6, 7, 8, 9, 0],
    [2, 3, 4, 5, 6, 7, 8, 9, 0, 1], [3, 4, 5, 6, 7, 8, 9, 0, 1, 2],
    [4, 5, 6, 7, 8, 9, 0, 1, 2, 3], [5, 6, 7, 8, 9, 0, 1, 2, 3, 4],
    [6, 7, 8, 9, 0, 1, 2, 3, 4, 5], [7, 8, 9, 0, 1, 2, 3, 4, 5, 6],
    [8, 9, 0, 1, 2, 3, 4, 5, 6, 7], [9, 0, 1, 2, 3, 4, 5, 6, 7, 8]
  ],
  [
    [1, 2, 3, 4, 5, 6, 7, 8, 9, 0], [2, 3, 4, 5, 6, 7, 8, 9, 0, 1],
    [3, 4, 5, 6, 7, 8, 9, 0, 1, 2], [4, 5, 6, 7, 8, 9, 0, 1, 2, 3],
    [5, 6, 7, 8, 9, 0, 1, 2, 3, 4], [6, 7, 8, 9, 0, 1, 2, 3, 4, 5],
    [7, 8, 9, 0, 1, 2, 3, 4, 5, 6], [8, 9, 0, 1, 2, 3, 4, 5, 6, 7],
    [9, 0, 1, 2, 3, 4, 5, 6, 7, 8], [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
  ]
];
type AddDigit<T extends Digit, U extends Digit, Carry extends 0 | 1 = 0> =
  AddDigitTable[Carry][T][U];
"#,
    );
    assert!(
        !has_code(&diags, 2536),
        "AddDigitTable[Carry][T][U] must not emit TS2536, got: {:?}",
        codes(&diags)
    );
}

/// A `readonly` tuple of `readonly` tuples behaves identically to the mutable
/// form (the gap was tuple-specific, both variants must apply).
#[test]
fn readonly_concrete_tuple_chain_is_accepted() {
    let diags = check(
        r#"
type Table = readonly [readonly [10, 11], readonly [20, 21]];
type Nested<D1 extends 0 | 1> = Table[D1][0];
"#,
    );
    assert!(
        !has_code(&diags, 2536),
        "readonly tuple chain must not emit TS2536, got: {:?}",
        codes(&diags)
    );
}

/// An inner index constrained to the abstract `number` domain (rather than a
/// literal union) still resolves to the element value and is accepted.
#[test]
fn number_constrained_inner_index_is_accepted() {
    let diags = check(
        r#"
type Table = [[10, 11], [20, 21]];
type Nested<D1 extends number> = Table[D1][0];
"#,
    );
    assert!(
        !has_code(&diags, 2536),
        "number-constrained inner index must not emit TS2536, got: {:?}",
        codes(&diags)
    );
}

/// The outer index is checked against the element-value key-space, which for a
/// tuple element includes any numeric index (array-like leniency), even one past
/// the element's own length — matching `tsc`.
#[test]
fn outer_numeric_index_past_element_length_is_accepted() {
    let diags = check(
        r#"
type Table = [[10, 11], [20, 21]];
type Nested<D1 extends 0 | 1> = Table[D1][5];
"#,
    );
    assert!(
        !has_code(&diags, 2536),
        "outer numeric index past element length must not emit TS2536, got: {:?}",
        codes(&diags)
    );
}

/// Tuple of objects: the outer string key is valid only when present on every
/// element of the value union (the key-space is the intersection).
#[test]
fn tuple_of_objects_common_outer_key_is_accepted() {
    let diags = check(
        r#"
type Table = [{ a: 1 }, { a: 2 }];
type Nested<D1 extends 0 | 1> = Table[D1]["a"];
"#,
    );
    assert!(
        !has_code(&diags, 2536),
        "common outer key on tuple-of-objects must not emit TS2536, got: {:?}",
        codes(&diags)
    );
}

// -------------------------------------------------------------------------
// Negative controls: genuine TS2536 must still fire (no over-suppression).
// -------------------------------------------------------------------------

/// An inner index constraint with an out-of-range literal (`0 | 1 | 2` over a
/// 2-tuple) genuinely escapes the element-index domain — `tsc` emits TS2536.
#[test]
fn out_of_range_inner_literal_still_errors() {
    for param in ["D1", "Sel"] {
        let src = format!(
            r#"
type Table = [[10, 11], [20, 21]];
type Bad<{param} extends 0 | 1 | 2> = Table[{param}][0];
"#
        );
        let diags = check(&src);
        assert!(
            has_code(&diags, 2536),
            "out-of-range inner literal (param {param}) must still emit TS2536, got: {:?}",
            codes(&diags)
        );
    }
}

/// An inner constraint of `keyof Base` includes `length`/array-method names, so
/// `Base[I]` is not guaranteed to be an element value — `tsc` emits TS2536.
#[test]
fn keyof_constrained_inner_index_still_errors() {
    let diags = check(
        r#"
type Table = [[10, 11], [20, 21]];
type Bad<D1 extends keyof Table> = Table[D1][0];
"#,
    );
    assert!(
        has_code(&diags, 2536),
        "keyof-constrained inner index must still emit TS2536, got: {:?}",
        codes(&diags)
    );
}

/// An outer string key that is genuinely absent from the element-value union
/// (a tuple of numeric-literal tuples has no string keys) — `tsc` emits TS2536.
#[test]
fn absent_outer_string_key_still_errors() {
    let diags = check(
        r#"
type Table = [[10, 11], [20, 21]];
type Bad<D1 extends 0 | 1> = Table[D1]["nope"];
"#,
    );
    assert!(
        has_code(&diags, 2536),
        "absent outer string key must still emit TS2536, got: {:?}",
        codes(&diags)
    );
}

/// Tuple of objects where the outer key is present on only one element: the
/// element-value key-space is the intersection, so the key is absent — TS2536.
#[test]
fn partial_outer_key_on_tuple_of_objects_still_errors() {
    let diags = check(
        r#"
type Table = [{ a: 1 }, { b: 2 }];
type Bad<D1 extends 0 | 1> = Table[D1]["a"];
"#,
    );
    assert!(
        has_code(&diags, 2536),
        "partial outer key on tuple-of-objects must still emit TS2536, got: {:?}",
        codes(&diags)
    );
}
