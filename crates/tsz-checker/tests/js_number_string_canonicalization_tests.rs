//! Numeric-literal → string conversion must use ECMAScript `Number::toString`.
//!
//! Structural rule: whenever a numeric literal type is converted to its string
//! form — template literal type evaluation, indexed-access/element-access key
//! canonicalization, property-name derivation — tsc produces the value's
//! `Number::toString(10)` text (`1e21` → `"1e+21"`, `1e-7` → `"1e-7"`,
//! `-0` → `"0"`), never the source spelling or a formatter that diverges on
//! the exponential thresholds. Number literal types themselves are keyed by
//! `SameValueZero`, so `-0` and `0` are one type. Template-literal inference
//! that captures a segment for a number-constrained type variable infers the
//! numeric literal when the segment round-trips through `Number::toString`.
//!
//! Binder names are varied across cases so the rules stay structural.

use tsz_checker::test_utils::check_source_codes;

fn assert_clean(source: &str) {
    let codes = check_source_codes(source);
    assert!(codes.is_empty(), "expected no diagnostics, got {codes:?}");
}

fn assert_codes(source: &str, expected: &[u32]) {
    let codes = check_source_codes(source);
    assert_eq!(codes, expected, "diagnostic codes mismatch");
}

// ── Template literal type evaluation ────────────────────────────────────────

#[test]
fn template_literal_large_magnitude_uses_exponential_form() {
    assert_clean(
        r#"
type Big = `${1e21}`;
const big: Big = "1e+21";
type Huge = `${1.5e22}`;
const huge: Huge = "1.5e+22";
"#,
    );
}

#[test]
fn template_literal_small_magnitude_uses_exponential_form() {
    assert_clean(
        r#"
type Tiny = `${1e-7}`;
const tiny: Tiny = "1e-7";
type Fine = `${0.000001}`;
const fine: Fine = "0.000001";
"#,
    );
}

#[test]
fn template_literal_negative_zero_prints_as_zero() {
    assert_clean(
        r#"
type Zed = `${-0}`;
const zed: Zed = "0";
"#,
    );
}

#[test]
fn template_literal_positional_21_digit_integer() {
    assert_clean(
        r#"
type Wide = `${1e20}`;
const wide: Wide = "100000000000000000000";
"#,
    );
}

#[test]
fn template_literal_rejects_source_spelling_of_exponential_number() {
    // Negative case: the source spelling "1e21" is NOT the canonical string.
    assert_codes(
        r#"
type Big = `${1e21}`;
const bad: Big = "1e21";
"#,
        &[2322],
    );
}

#[test]
fn template_literal_exotic_number_inside_generic_alias() {
    // Alias/wrapper + generic instantiation form of the same rule.
    assert_clean(
        r#"
type Tag<N extends number> = `v${N}`;
type Wrapped = Tag<1e21>;
const w: Wrapped = "v1e+21";
"#,
    );
}

// ── Indexed access / element access key canonicalization ───────────────────

#[test]
fn indexed_access_type_with_exponential_numeric_key() {
    assert_clean(
        r#"
type Rec = { 1e21: string };
type Val = Rec[1e21];
const v: Val = "ok";
"#,
    );
}

#[test]
fn indexed_access_type_with_canonical_string_key() {
    assert_clean(
        r#"
type Rec = { 1e21: string };
type Val = Rec["1e+21"];
const v: Val = "ok";
"#,
    );
}

#[test]
fn indexed_access_type_rejects_source_spelling_string_key() {
    // The property's canonical name is "1e+21"; the raw spelling misses.
    assert_codes(
        r#"
type Rec = { 1e21: string };
type Val = Rec["1e21"];
"#,
        &[2339],
    );
}

#[test]
fn element_access_with_exponential_numeric_key() {
    assert_clean(
        r#"
declare const box: { 1e21: string };
const v: string = box[1e21];
const w: string = box["1e+21"];
"#,
    );
}

#[test]
fn element_access_with_small_exponential_numeric_key() {
    assert_clean(
        r#"
declare const bag: { 1e-7: number };
const n: number = bag[1e-7];
const m: number = bag["1e-7"];
"#,
    );
}

// ── SameValueZero literal identity ──────────────────────────────────────────

#[test]
fn negative_zero_and_zero_are_one_literal_type() {
    assert_clean(
        r#"
type NegZero = -0;
const a: NegZero = 0;
const b: 0 = -0 as const;
let c = -0 as -0;
const d: 0 = c;
"#,
    );
}

#[test]
fn negative_zero_still_rejects_other_numbers() {
    assert_codes(
        r#"
const bad: -0 = 1;
"#,
        &[2322],
    );
}

// ── Template-literal inference capture coercion ─────────────────────────────

#[test]
fn call_inference_coerces_numeric_capture_for_number_constraint() {
    assert_clean(
        r#"
declare function fromPx<W extends number>(s: `${W}px`): W;
const forty: 42 = fromPx("42px");
const half: 1.5 = fromPx("1.5px");
"#,
    );
}

#[test]
fn call_inference_keeps_string_capture_for_string_constraint() {
    assert_clean(
        r#"
declare function tail<S extends string>(s: `x-${S}`): S;
const t: "42" = tail("x-42");
"#,
    );
}

#[test]
fn call_inference_widens_non_round_trip_numeric_capture() {
    // "042" parses to 42 but does not round-trip, so tsc infers `number`,
    // which cannot satisfy the literal check.
    assert_codes(
        r#"
declare function fromPx<W extends number>(s: `${W}px`): W;
const bad: 42 = fromPx("042px");
"#,
        &[2322],
    );
}

#[test]
fn call_inference_coerces_boolean_and_bigint_captures() {
    assert_clean(
        r#"
declare function flag<B extends boolean>(s: `is:${B}`): B;
const yes: true = flag("is:true");
declare function big<G extends bigint>(s: `${G}n`): G;
const g: 7n = big("7n");
"#,
    );
}

// ── Enum member values through templates ────────────────────────────────────

#[test]
fn template_over_enum_member_with_exotic_value() {
    assert_clean(
        r#"
enum Scale { Big = 1e21 }
type S = `${Scale.Big}`;
const s: S = "1e+21";
"#,
    );
}
