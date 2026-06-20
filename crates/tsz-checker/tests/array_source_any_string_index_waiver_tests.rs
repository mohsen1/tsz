//! `tsc` waives the missing-string-index requirement for a named array/tuple
//! source when the *value type* of the target's string index signature is `any`
//! (`{ [x: string]: any }`, including the `{ [P in any]: any }` mapped form).
//!
//! A named array/tuple has a numeric index but no string index of its own, so
//! it is normally rejected against a string-index target — and that rejection
//! is correct for a concrete value type (`unknown`, `boolean | number`, …).
//! The divergence covered here is *exactly and only* the `any` value type,
//! which is an `any`-propagation (Lawyer) quirk rather than a structural
//! invariant. Covers both the assignment relation (TS2322) and the generic
//! constraint relation (TS2344). See issue #14162.

use tsz_checker::context::CheckerOptions;

fn strict_diagnostics(source: &str) -> Vec<(u32, String)> {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    tsz_checker::test_utils::check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn relation_codes(diags: &[(u32, String)]) -> Vec<u32> {
    diags
        .iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2344 | 2345))
        .map(|(code, _)| *code)
        .collect()
}

#[test]
fn tuple_and_array_satisfy_string_index_any_target() {
    let diagnostics = strict_diagnostics(
        r#"
type StrIdxAny = { [x: string]: any };
declare const tup: [boolean, number];
declare const arr: boolean[];
const a: StrIdxAny = tup;
const b: StrIdxAny = arr;
const c: { [x: string]: any } = tup;
"#,
    );
    let codes = relation_codes(&diagnostics);
    assert!(
        codes.is_empty(),
        "array/tuple source should satisfy a string-index-`any` target, got: {diagnostics:#?}"
    );
}

#[test]
fn readonly_array_satisfies_string_index_any_target() {
    let diagnostics = strict_diagnostics(
        r#"
declare const ro: ReadonlyArray<number>;
declare const tup: readonly [string, number];
const a: { [x: string]: any } = ro;
const b: { [x: string]: any } = tup;
"#,
    );
    let codes = relation_codes(&diagnostics);
    assert!(
        codes.is_empty(),
        "readonly array/tuple source should satisfy a string-index-`any` target, got: {diagnostics:#?}"
    );
}

#[test]
fn mapped_index_any_form_satisfied_by_array() {
    // `{ [P in any]: any }` materializes to the same string+number index-`any`
    // shape and must accept an array/tuple source identically.
    let diagnostics = strict_diagnostics(
        r#"
type MappedAny = { [P in any]: any };
declare const tup: [boolean, number];
declare const arr: string[];
const a: MappedAny = tup;
const b: MappedAny = arr;
"#,
    );
    let codes = relation_codes(&diagnostics);
    assert!(
        codes.is_empty(),
        "`{{ [P in any]: any }}` should accept an array/tuple source, got: {diagnostics:#?}"
    );
}

#[test]
fn number_index_any_target_still_accepted() {
    // Boundary control: a `{ [x: number]: any }` target was already accepted via
    // the source's numeric index and must remain so.
    let diagnostics = strict_diagnostics(
        r#"
declare const tup: [boolean, number];
declare const arr: boolean[];
const a: { [x: number]: any } = tup;
const b: { [x: number]: any } = arr;
"#,
    );
    let codes = relation_codes(&diagnostics);
    assert!(
        codes.is_empty(),
        "number-index-`any` target should accept an array/tuple source, got: {diagnostics:#?}"
    );
}

#[test]
fn string_index_unknown_target_still_rejected() {
    // Negative control: `unknown` is not `any`; the missing-string-index
    // requirement still applies, so an array/tuple source must be rejected.
    let diagnostics = strict_diagnostics(
        r#"
declare const tup: [boolean, number];
declare const arr: boolean[];
const a: { [x: string]: unknown } = tup;
const b: { [x: string]: unknown } = arr;
"#,
    );
    let codes = relation_codes(&diagnostics);
    assert_eq!(
        codes,
        vec![2322, 2322],
        "string-index-`unknown` target must still reject array/tuple sources, got: {diagnostics:#?}"
    );
}

#[test]
fn string_index_concrete_union_target_still_rejected() {
    // Negative control: a concrete value type still requires a real string index.
    let diagnostics = strict_diagnostics(
        r#"
declare const tup: [boolean, number];
const a: { [x: string]: boolean | number } = tup;
"#,
    );
    let codes = relation_codes(&diagnostics);
    assert_eq!(
        codes,
        vec![2322],
        "string-index-`boolean | number` target must still reject a tuple, got: {diagnostics:#?}"
    );
}

#[test]
fn string_index_any_inside_intersection_target_satisfied() {
    // The waiver composes structurally: an intersection whose only object
    // constituent is a string-index-`any` shape is still satisfied by a
    // tuple/array source (the relation reaches the same per-constituent
    // index decision).
    let diagnostics = strict_diagnostics(
        r#"
declare const tup: [boolean, number];
const a: { [x: string]: any } & {} = tup;
"#,
    );
    let codes = relation_codes(&diagnostics);
    assert!(
        codes.is_empty(),
        "string-index-`any` intersection target should accept a tuple source, got: {diagnostics:#?}"
    );
}

#[test]
fn string_index_any_with_extra_property_still_rejected() {
    // The waiver only removes the *missing string index* requirement. A named
    // member the array/tuple lacks is still a genuine mismatch, so adding a
    // required property re-introduces the rejection.
    let diagnostics = strict_diagnostics(
        r#"
declare const tup: [boolean, number];
const a: { [x: string]: any; tag: string } = tup;
"#,
    );
    let codes = relation_codes(&diagnostics);
    assert!(
        !codes.is_empty(),
        "a missing required property must still be rejected even with a string-index-`any`, got: {diagnostics:#?}"
    );
}
