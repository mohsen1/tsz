//! Numeric-literal → string conversion parity with ECMAScript `Number::toString`
//! at semantic decision sites (issue #15668).
//!
//! Structural rule: when a numeric literal type is converted to string form
//! (template literal type evaluation, indexed-access/property key derivation,
//! property-name canonicalization, `infer`-pattern round-trip), tsc produces
//! ECMAScript `Number::toString(10)` output; tsz owns this in one shared
//! `js_number_to_string` helper consumed by solver and checker. Number literal
//! types are keyed by `SameValueZero`, so `-0` and `0` intern to one type.

use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::check_source;

fn codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

fn no_errors(source: &str) -> bool {
    codes(source).is_empty()
}

// === 1. Template literal type evaluation ===

/// tsc: `` `${1e21}` `` evaluates to `"1e+21"` (JS scientific form), not
/// Rust's expanded `"1000000000000000000000"`.
#[test]
fn template_literal_large_number_uses_js_scientific_form() {
    assert!(
        no_errors(r#"type N1 = `${1e21}`; const n1: N1 = "1e+21";"#),
        "`${{1e21}}` must evaluate to \"1e+21\""
    );
}

/// Negative control: the Rust `Display` expansion must NOT be accepted.
#[test]
fn template_literal_large_number_rejects_rust_display_form() {
    assert!(
        codes(r#"type N1 = `${1e21}`; const n1: N1 = "1000000000000000000000";"#).contains(&2322),
        "Rust-style expansion must not inhabit `${{1e21}}`"
    );
}

/// tsc: `` `${1e-7}` `` evaluates to `"1e-7"`, not `"0.0000001"`.
#[test]
fn template_literal_small_number_uses_js_scientific_form() {
    assert!(
        no_errors(r#"type N3 = `${1e-7}`; const n3: N3 = "1e-7";"#),
        "`${{1e-7}}` must evaluate to \"1e-7\""
    );
}

/// Negative control for the small-magnitude form.
#[test]
fn template_literal_small_number_rejects_fixed_point_form() {
    assert!(
        codes(r#"type N3 = `${1e-7}`; const n3: N3 = "0.0000001";"#).contains(&2322),
        "fixed-point expansion must not inhabit `${{1e-7}}`"
    );
}

/// tsc: `` `${-0}` `` evaluates to `"0"` (JS `String(-0) === "0"`).
#[test]
fn template_literal_negative_zero_stringifies_to_zero() {
    assert!(
        no_errors(r#"type N4 = `${-0}`; const n4: N4 = "0";"#),
        "`${{-0}}` must evaluate to \"0\""
    );
}

/// Negative control: `"-0"` must be rejected.
#[test]
fn template_literal_negative_zero_rejects_minus_zero_text() {
    assert!(
        codes(r#"type N4 = `${-0}`; const n4: N4 = "-0";"#).contains(&2322),
        "\"-0\" must not inhabit `${{-0}}`"
    );
}

/// Boundary values inside the fixed-point range stay fixed-point.
#[test]
fn template_literal_fixed_point_boundaries() {
    assert!(
        no_errors(
            r#"
type A = `${1e20}`; const a: A = "100000000000000000000";
type B = `${1e-6}`; const b: B = "0.000001";
type C = `${123.5}`; const c: C = "123.5";
"#
        ),
        "in-range values must keep JS fixed-point form"
    );
}

/// Alias/wrapper form: the conversion is structural, not syntax-driven.
#[test]
fn template_literal_scientific_through_alias() {
    assert!(
        no_errors(
            r#"
type Big = 1e21;
type Wrap<T extends number> = `${T}`;
const w: Wrap<Big> = "1e+21";
"#
        ),
        "alias/generic application must use the same JS string form"
    );
}

// === 2. Indexed access / element access key canonicalization ===

/// tsc: `O[1e21]` resolves the property declared as `1e21` (both canonicalize
/// to the JS name `"1e+21"`).
#[test]
fn indexed_access_type_large_numeric_key_resolves() {
    assert!(
        no_errors(r#"type O = { 1e21: string }; type V1 = O[1e21]; const v: V1 = "x";"#),
        "O[1e21] must resolve the 1e21 property"
    );
}

/// tsc: element access `o[1e21]` on a value resolves the same property.
#[test]
fn element_access_value_large_numeric_key_resolves() {
    assert!(
        no_errors(
            r#"
type O = { 1e21: string };
declare const o: O;
const s1: string = o[1e21];
"#
        ),
        "o[1e21] must resolve the declared 1e21 property (no TS7053)"
    );
}

/// Same shape with a renamed binder and small-magnitude key.
#[test]
fn element_access_value_small_numeric_key_resolves() {
    assert!(
        no_errors(
            r#"
type Rec = { 1e-7: number };
declare const store: Rec;
const got: number = store[1e-7];
"#
        ),
        "store[1e-7] must resolve the declared 1e-7 property"
    );
}

/// Negative control: a genuinely missing numeric key still errors.
#[test]
fn element_access_missing_numeric_key_still_errors() {
    assert!(
        !codes(
            r#"
type O = { 1e21: string };
declare const o: O;
const s1: string = o[123];
"#
        )
        .is_empty(),
        "a missing numeric key must still be reported"
    );
}

// === 3. -0 literal identity (SameValueZero keying) ===

/// tsc interns one literal type for `-0` and `0`.
#[test]
fn negative_zero_and_zero_are_one_literal_type() {
    assert!(
        no_errors("const a: -0 = 0; const b: 0 = -0;"),
        "-0 and 0 must be the same number literal type"
    );
}

/// Alias form of the same identity.
#[test]
fn negative_zero_alias_identity() {
    assert!(
        no_errors("type Z = -0; type P = 0; const z: Z = 0; const p: P = -0; const q: Z = p;"),
        "-0/0 identity must hold through aliases"
    );
}

/// Negative control: other numeric literals keep their distinct identity.
#[test]
fn distinct_number_literals_still_reject() {
    assert!(
        codes("const a: 1 = 2;").contains(&2322),
        "distinct literals must still be rejected"
    );
}

// === 4. Template inference capture coercion ===

/// tsc: a capture for a number-constrained type parameter that round-trips
/// through `Number::toString` infers the numeric literal type.
#[test]
fn template_inference_number_constraint_infers_numeric_literal() {
    assert!(
        no_errors(
            r#"
declare function parse<T extends number>(s: `${T}px`): T;
const pz = parse("42px");
const pzc: 42 = pz;
"#
        ),
        "T must be inferred as the literal 42"
    );
}

/// Renamed binders + different segment text.
#[test]
fn template_inference_number_constraint_renamed_binders() {
    assert!(
        no_errors(
            r#"
declare function extract<Value extends number>(input: `size-${Value}`): Value;
const width = extract("size-800");
const check: 800 = width;
"#
        ),
        "renamed binder must infer the literal 800"
    );
}

/// Non-round-trip capture must NOT collapse to a numeric literal: tsc keeps
/// the inference at `number` when the text does not round-trip.
#[test]
fn template_inference_non_round_trip_capture_widens_to_number() {
    assert!(
        no_errors(
            r#"
declare function parse<T extends number>(s: `${T}px`): T;
const p = parse("042px");
const w: number = p;
"#
        ),
        "non-round-trip capture stays assignable to number"
    );
    assert!(
        codes(
            r#"
declare function parse<T extends number>(s: `${T}px`): T;
const p = parse("042px");
const w: 42 = p;
"#
        )
        .contains(&2322),
        "non-round-trip capture must not be the literal 42"
    );
}

/// String-constrained parameters keep the string capture (negative control).
#[test]
fn template_inference_string_constraint_keeps_string_literal() {
    assert!(
        no_errors(
            r#"
declare function parse<T extends string>(s: `${T}px`): T;
const p = parse("42px");
const c: "42" = p;
"#
        ),
        "string-constrained capture stays the string literal"
    );
}

/// Scientific-form capture round-trips to the same numeric literal.
#[test]
fn template_inference_scientific_capture_round_trips() {
    assert!(
        no_errors(
            r#"
declare function parse<T extends number>(s: `${T}u`): T;
const p = parse("1e+21u");
const c: 1e21 = p;
"#
        ),
        "\"1e+21\" must round-trip to the numeric literal 1e21"
    );
}

/// `infer ... extends number` in conditional types uses the same round-trip
/// rule (adjacent form of the same structural rule).
#[test]
fn conditional_infer_extends_number_round_trip() {
    assert!(
        no_errors(
            r#"
type Px<S extends string> = S extends `${infer N extends number}px` ? N : never;
const n: Px<"42px"> = 42;
declare const big: Px<"1e+21px">;
const exact: 1e21 = big;
"#
        ),
        "infer-extends-number captures must round-trip through JS Number::toString"
    );
    assert!(
        codes(
            r#"
type Px<S extends string> = S extends `${infer N extends number}px` ? N : never;
declare const rust: Px<"1000000000000000000000px">;
const exact: 1e21 = rust;
"#
        )
        .contains(&2322),
        "the Rust Display spelling must widen to number, not the literal"
    );
}

// === Salvaged witnesses from superseded #15675 (tsc-verified 2026-07-11) ===

/// tsc: a numeric-declared property is addressable by its canonical
/// `Number::toString` string name (`{ 1e21: string }` has key `"1e+21"`).
#[test]
fn indexed_access_type_with_canonical_string_key_resolves() {
    assert!(
        no_errors(
            r#"
type Rec = { 1e21: string };
type Val = Rec["1e+21"];
const v: Val = "ok";
"#
        ),
        "Rec[\"1e+21\"] must resolve the property declared as `1e21`"
    );
}

/// Negative control: the raw source spelling is NOT the property's name;
/// tsc reports TS2339 for `Rec["1e21"]`.
#[test]
fn indexed_access_type_rejects_source_spelling_string_key() {
    assert!(
        codes(
            r#"
type Rec = { 1e21: string };
type Val = Rec["1e21"];
"#
        )
        .contains(&2339),
        "source spelling \"1e21\" must miss the canonical \"1e+21\" property"
    );
}

/// tsc: boolean and bigint template captures coerce like their literal
/// grammar — `is:${B}` captures `true`; `${G}n` captures `7n`.
#[test]
fn call_inference_coerces_boolean_and_bigint_captures() {
    assert!(
        no_errors(
            r#"
declare function flag<B extends boolean>(s: `is:${B}`): B;
const yes: true = flag("is:true");
declare function big<G extends bigint>(s: `${G}n`): G;
const g: 7n = big("7n");
"#
        ),
        "boolean/bigint template captures must infer their literal types"
    );
}

/// tsc: enum member values flow through template literal types via the same
/// `Number::toString` owner (`Scale.Big = 1e21` → `"1e+21"`).
#[test]
fn template_over_enum_member_with_exotic_value() {
    assert!(
        no_errors(
            r#"
enum Scale { Big = 1e21 }
type S = `${Scale.Big}`;
const s: S = "1e+21";
"#
        ),
        "`${{Scale.Big}}` must evaluate through Number::toString"
    );
}
