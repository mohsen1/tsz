//! Tests for TS2352 when object types share a property whose type is a
//! disjoint literal (e.g., phantom-type brands with incompatible string literals).
//!
//! The structural rule: when asserting `A as B`, tsc checks
//! `isTypeComparableTo(A, B)` which is bidirectional assignability.  For
//! two object types that share a property with literal types, if the literal
//! values are distinct (e.g., `"draft"` vs `"published"`), neither is
//! assignable to the other, so the types are NOT comparable and TS2352 must
//! fire.
//!
//! Previously tsz used `std::mem::discriminant` to consider two distinct
//! string/number literals "comparable" just because they share the same
//! primitive kind.  This caused a silent miss of TS2352 for phantom-type
//! patterns and discriminated unions whose properties have incompatible
//! literal types.

use crate::test_utils::check_source_strict_codes as check_strict;

// ---------------------------------------------------------------------------
// Core repro: phantom-type brands (from issue #5968)
// ---------------------------------------------------------------------------

/// The canonical phantom-type repro: asserting from `Draft` to `Published`
/// must emit TS2352 because `_phantom?: "draft"` is incompatible with
/// `_phantom?: "published"` — neither literal is assignable to the other.
#[test]
fn phantom_types_assert_emits_ts2352() {
    for (p_name, t_name) in [("P", "T"), ("Brand", "Val"), ("K", "V")] {
        let source = format!(
            r#"
interface Phantom<{p_name}, {t_name}> {{
    _phantom?: {p_name};
    value: {t_name};
}}
type Draft = Phantom<"draft", string>;
type Published = Phantom<"published", string>;
function publish(draft: Draft): Published {{
    return draft as Published;
}}
"#
        );
        let codes = check_strict(&source);
        let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
        assert!(
            !ts2352.is_empty(),
            "TS2352 expected — `Draft` and `Published` have incompatible phantom brands \
             (<{p_name}={:?}, {t_name}=string>). Got: {codes:?}",
            "draft"
        );
    }
}

/// Same-brand assertion must NOT emit TS2352 (trivially comparable).
#[test]
fn same_phantom_brand_assert_no_ts2352() {
    let source = r#"
interface Phantom<P, T> {
    _phantom?: P;
    value: T;
}
type Draft = Phantom<"draft", string>;
declare let d: Draft;
const d2 = d as Draft;
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — asserting `Draft` to `Draft` is trivial. Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Discriminated unions: incompatible `kind` property literals
// ---------------------------------------------------------------------------

/// `{ kind: "a" } as { kind: "b" }` must emit TS2352.
/// The `kind` property has incompatible literal types: `"a"` !~ `"b"`.
#[test]
fn discriminant_kind_literal_emits_ts2352() {
    let source = r#"
declare let a: { kind: "a"; value: number };
let b = a as { kind: "b"; value: number };
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        !ts2352.is_empty(),
        "TS2352 expected — `kind: \"a\"` is not comparable to `kind: \"b\"`. Got: {codes:?}"
    );
}

/// `{ kind: "a" | "b" } as { kind: "a" }` must NOT emit TS2352 because
/// `{ kind: "a" }` is assignable to `{ kind: "a" | "b" }` (one direction holds).
#[test]
fn discriminant_union_to_member_no_ts2352() {
    let source = r#"
declare let x: { kind: "a" | "b"; value: number };
let y = x as { kind: "a"; value: number };
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — `{{ kind: \"a\" }}` is assignable to `{{ kind: \"a\" | \"b\" }}`. \
         Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Numeric literal properties
// ---------------------------------------------------------------------------

/// `{ code: 200 } as { code: 404 }` must emit TS2352.
/// Numeric literals with different values are not comparable.
#[test]
fn numeric_literal_property_mismatch_emits_ts2352() {
    let source = r#"
declare let ok: { code: 200; body: string };
let notFound = ok as { code: 404; body: string };
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        !ts2352.is_empty(),
        "TS2352 expected — `code: 200` is not comparable to `code: 404`. Got: {codes:?}"
    );
}

/// `{ code: 200 } as { code: 200 }` must NOT emit TS2352 (same literal).
#[test]
fn same_numeric_literal_property_no_ts2352() {
    let source = r#"
declare let ok: { code: 200; body: string };
let ok2 = ok as { code: 200; body: string };
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — `code: 200` is comparable to `code: 200`. Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Optional literal properties: the `_phantom?` pattern specifically
// ---------------------------------------------------------------------------

/// When the shared property is optional and has disjoint literal types,
/// TS2352 must still fire.  Optional `_phantom?` does not make the types
/// comparable: the effective domain of `_phantom?: "draft"` is `"draft" |
/// undefined`, which has no overlap with `"published" | undefined` (the
/// `undefined` pieces are the same, but that alone cannot carry the whole
/// comparison).
#[test]
fn optional_disjoint_literal_property_emits_ts2352() {
    let source = r#"
declare let a: { _tag?: "A"; id: number };
let b = a as { _tag?: "B"; id: number };
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        !ts2352.is_empty(),
        "TS2352 expected — `_tag?: \"A\"` is not comparable to `_tag?: \"B\"`. Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Sanity: valid assertions that must NOT emit TS2352
// ---------------------------------------------------------------------------

/// Two objects with the same literal property must be comparable.
#[test]
fn same_literal_property_no_ts2352() {
    let source = r#"
declare let x: { kind: "ok"; value: number };
let y = x as { kind: "ok"; extra?: string };
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — `kind: \"ok\"` is comparable to itself. Got: {codes:?}"
    );
}

/// Literal to its base primitive must be comparable (subtype relation holds).
#[test]
fn literal_to_primitive_property_no_ts2352() {
    let source = r#"
declare let x: { kind: "foo"; value: number };
let y = x as { kind: string; value: number };
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — `\"foo\"` is a subtype of `string`. Got: {codes:?}"
    );
}

/// Direct primitive literal assertions remain comparable in tsc even when the
/// literal values differ. The stricter literal value check is only for
/// structural property overlap like `{ kind: "a" } as { kind: "b" }`.
#[test]
fn direct_distinct_literal_assertion_no_ts2352() {
    let source = r#"
let x = "foo" as "bar";
declare let y: string;
let z = y as "baz";
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected for direct primitive literal assertions. Got: {codes:?}"
    );
}
