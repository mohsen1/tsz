//! TS1518 — anything that would possibly match more than a single character
//! is invalid inside a negated character class (the `v` flag).
//!
//! A negated class complements its contents, and a complement is only defined
//! over single characters, so an operand denoting a *set of strings* is
//! rejected. Two constructs produce such an operand under `v`:
//!
//! - `\q{...}`, when any `|`-separated alternative is not exactly one code
//!   point — including the empty alternative of `\q{}`;
//! - `\p{...}` naming one of the seven binary properties *of strings*
//!   (`RGI_Emoji` and friends), which `BINARY_UNICODE_PROPERTIES_OF_STRINGS`
//!   already enumerates for TS1528.
//!
//! `\P{...}` over such a property is rejected outright — in or out of a
//! negated class — because complementing a set of strings is not defined.
//! tsc reports that one on the property *name* rather than on the escape.
//!
//! Every row below is pinned against `typescript@7.0.2`
//! (`--noEmit --strict --target es2024 --lib es2024`), including the reported
//! column. `scripts/conformance/typescript-versions.json` `current` resolves
//! to that exact npm version.
//!
//! ## One row family here encodes a tsc bug on purpose
//!
//! In a UNION, tsc consults only the class's FIRST operand, so
//! `/[^\q{xy}b]/v` is reported and `/[^b\q{xy}]/v` is not. The ECMAScript
//! grammar makes `MayContainStrings` of a union true when *any* operand may
//! contain strings, so the un-reported member of each pair is a tsc miss.
//! Parity with tsc is the contract, so the miss is matched deliberately and
//! pinned by the `union_only_consults_the_first_operand_matching_tsc` tests
//! below — if a future change starts reporting those rows, it has diverged
//! from tsc even though it has moved toward the spec.
//!
//! Intersection is *not* subject to that miss and is spec-exact in tsc:
//! `A&&B` may contain strings only when every operand does.
use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

const TS1518: u32 =
    diagnostic_codes::ANYTHING_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_INVALID_INSID;

/// `(code, zero-based offset)` pairs — the offset is what the CLI renders as a
/// column, and an operand-anchoring mistake is only visible here.
fn regex_codes_at(source: &str) -> Vec<(u32, u32)> {
    let (parser, _root) = parse_source(source);
    parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.start))
        .collect()
}

fn regex_codes(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    parser.get_diagnostics().iter().map(|d| d.code).collect()
}

/// Offset of `needle` in `source`, for anchoring the expected report.
fn at(source: &str, needle: &str) -> u32 {
    source.find(needle).expect("needle in source") as u32
}

// ---------------------------------------------------------------------------
// `\q{...}` string disjunctions

#[test]
fn negated_class_rejects_multi_character_string_alternative() {
    let src = r"const a = /[^\q{ab}]/v;";
    assert_eq!(regex_codes_at(src), vec![(TS1518, at(src, r"\q{ab}"))]);
}

#[test]
fn negated_class_accepts_single_character_string_alternative() {
    assert_eq!(regex_codes(r"const a = /[^\q{a}]/v;"), Vec::<u32>::new());
}

#[test]
fn negated_class_rejects_disjunction_when_any_alternative_is_multi_character() {
    let src = r"const a = /[^\q{ab|c}]/v;";
    assert_eq!(regex_codes_at(src), vec![(TS1518, at(src, r"\q{ab|c}"))]);
}

#[test]
fn negated_class_accepts_disjunction_of_single_characters() {
    assert_eq!(regex_codes(r"const a = /[^\q{a|b}]/v;"), Vec::<u32>::new());
}

#[test]
fn negated_class_rejects_empty_string_alternative() {
    // `\q{}` matches the empty string, which is a string of length 0 — not a
    // single character — so the class is still rejected.
    let src = r"const a = /[^\q{}]/v;";
    assert_eq!(regex_codes_at(src), vec![(TS1518, at(src, r"\q{}"))]);
}

#[test]
fn negated_class_sizes_an_escaped_alternative_by_code_points_not_source_bytes() {
    // `\q{\u{1F600}}` is one code point spelled in ten source bytes. Judging
    // it by its raw text would false-positive.
    assert_eq!(
        regex_codes(r"const a = /[^\q{\u{1F600}}]/v;"),
        Vec::<u32>::new()
    );
}

#[test]
fn non_negated_class_accepts_a_string_alternative() {
    assert_eq!(regex_codes(r"const a = /[\q{ab}]/v;"), Vec::<u32>::new());
}

// ---------------------------------------------------------------------------
// Properties of strings

#[test]
fn negated_class_rejects_a_property_of_strings() {
    let src = r"const a = /[^\p{RGI_Emoji}]/v;";
    assert_eq!(
        regex_codes_at(src),
        vec![(TS1518, at(src, r"\p{RGI_Emoji}"))]
    );
}

#[test]
fn negated_class_rejects_a_second_property_of_strings_under_a_different_name() {
    // The set is a table lookup, not a spelling match on one witness.
    let src = r"const a = /[^\p{Basic_Emoji}]/v;";
    assert_eq!(
        regex_codes_at(src),
        vec![(TS1518, at(src, r"\p{Basic_Emoji}"))]
    );
}

#[test]
fn negated_class_accepts_an_ordinary_single_character_property() {
    assert_eq!(
        regex_codes(r"const a = /[^\p{Letter}]/v;"),
        Vec::<u32>::new()
    );
}

#[test]
fn complemented_property_of_strings_is_rejected_outside_a_negated_class() {
    // `\P` over a set of strings is not defined at all, so the class need not
    // be negated — and tsc anchors this one on the property NAME.
    let src = r"const a = /[\P{RGI_Emoji}]/v;";
    assert_eq!(regex_codes_at(src), vec![(TS1518, at(src, "RGI_Emoji"))]);
}

#[test]
fn complemented_property_of_strings_inside_a_negated_class_reports_once() {
    // Both the `\P` rule and the negated-class rule could speak here; tsc
    // emits exactly one, on the property name.
    let src = r"const a = /[^\P{RGI_Emoji}]/v;";
    assert_eq!(regex_codes_at(src), vec![(TS1518, at(src, "RGI_Emoji"))]);
}

#[test]
fn complemented_ordinary_property_is_accepted() {
    assert_eq!(
        regex_codes(r"const a = /[^\P{Letter}]/v;"),
        Vec::<u32>::new()
    );
}

// ---------------------------------------------------------------------------
// Nesting

#[test]
fn negated_class_rejects_a_nested_class_that_may_contain_strings() {
    // Reported on the nested class's `[`, which is the outer class's operand.
    let src = r"const a = /[^[\q{ab}]]/v;";
    assert_eq!(regex_codes_at(src), vec![(TS1518, at(src, r"[\q{ab}]]"))]);
}

#[test]
fn a_negated_nested_class_reports_for_itself() {
    let src = r"const a = /[[^\q{ab}]]/v;";
    assert_eq!(regex_codes_at(src), vec![(TS1518, at(src, r"\q{ab}"))]);
}

#[test]
fn negated_class_accepts_a_nested_class_of_single_characters() {
    assert_eq!(regex_codes(r"const a = /[^[ab]]/v;"), Vec::<u32>::new());
}

// ---------------------------------------------------------------------------
// Class-set operators

#[test]
fn subtraction_takes_its_first_operand_answer() {
    let src = r"const a = /[^\p{RGI_Emoji}--\q{a}]/v;";
    assert_eq!(
        regex_codes_at(src),
        vec![(TS1518, at(src, r"\p{RGI_Emoji}"))]
    );
}

#[test]
fn intersection_is_rejected_only_when_every_operand_may_contain_strings() {
    let src = r"const a = /[^\q{ab}&&\q{cd}]/v;";
    assert_eq!(regex_codes_at(src), vec![(TS1518, at(src, r"\q{ab}"))]);
}

#[test]
fn intersection_with_a_single_character_operand_is_accepted() {
    // `\q{ab} && \q{a}` cannot contain a string, so the complement is defined.
    // Both orders are clean, which is what distinguishes a real intersection
    // rule from the union's first-operand-only behaviour.
    assert_eq!(
        regex_codes(r"const a = /[^\q{ab}&&\q{a}]/v;"),
        Vec::<u32>::new()
    );
    assert_eq!(
        regex_codes(r"const a = /[^\q{a}&&\q{ab}]/v;"),
        Vec::<u32>::new()
    );
}

// ---------------------------------------------------------------------------
// Deliberate tsc-bug parity: a union consults only its first operand.
// See the module header. These rows pin tsc's behaviour, not the spec's.

#[test]
fn union_only_consults_the_first_operand_matching_tsc_string_disjunction() {
    let reported = r"const a = /[^\q{xy}b]/v;";
    assert_eq!(
        regex_codes_at(reported),
        vec![(TS1518, at(reported, r"\q{xy}"))]
    );

    // Same set, operands swapped — tsc misses it, so tsz must too.
    assert_eq!(regex_codes(r"const a = /[^b\q{xy}]/v;"), Vec::<u32>::new());
    assert_eq!(
        regex_codes(r"const a = /[^a-z\q{xy}]/v;"),
        Vec::<u32>::new()
    );
}

#[test]
fn union_only_consults_the_first_operand_matching_tsc_property_of_strings() {
    let reported = r"const a = /[^\p{RGI_Emoji}a]/v;";
    assert_eq!(
        regex_codes_at(reported),
        vec![(TS1518, at(reported, r"\p{RGI_Emoji}"))]
    );

    assert_eq!(
        regex_codes(r"const a = /[^a\p{RGI_Emoji}]/v;"),
        Vec::<u32>::new()
    );
}

#[test]
fn a_range_upper_bound_is_not_a_class_operand() {
    // The first operand is `\q{xy}`, so the class is reported for that and
    // the trailing range does not add a second report.
    let src = r"const a = /[^\q{xy}a-z]/v;";
    assert_eq!(regex_codes_at(src), vec![(TS1518, at(src, r"\q{xy}"))]);
}

// ---------------------------------------------------------------------------
// Flag gating: TS1518 is a `v`-mode rule, and the `u`-mode siblings still own
// their own shapes.

#[test]
fn under_u_a_property_of_strings_keeps_its_own_diagnostic() {
    // TS1528, not TS1518: without `v` the property is unavailable at all.
    let src = r"const a = /[^\p{RGI_Emoji}]/u;";
    assert_eq!(
        regex_codes_at(src),
        vec![(
            diagnostic_codes::ANY_UNICODE_PROPERTY_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_O,
            at(src, "RGI_Emoji")
        )]
    );
}

#[test]
fn under_u_a_string_disjunction_keeps_its_own_diagnostic() {
    // TS1535: `\q` is not grammar without `v`, so it is judged as an
    // ordinary un-escapable character instead.
    assert_eq!(
        regex_codes(r"const a = /[^\q{ab}]/u;"),
        vec![diagnostic_codes::THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION]
    );
}
