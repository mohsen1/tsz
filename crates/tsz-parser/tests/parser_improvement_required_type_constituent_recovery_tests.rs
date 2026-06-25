//! Parser recovery for a *structurally required* type constituent that is
//! missing before a type-terminator token.
//!
//! tsc parses the constituent after a consumed `|`/`&` separator or a
//! `keyof`/`unique`/`readonly` type operator unconditionally; the absent
//! constituent goes through `createMissingNode(... Type_expected)` so TS1110
//! fires even before `;`/`)`/EOF. tsz historically suppressed TS1110 for any
//! type-terminator token, which over-applied the genuinely-optional recovery
//! (`let x: ;`, `f(a: )`) to required positions. See issue #14836.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

/// Number of TS1110 `Type expected` diagnostics whose reported start equals
/// the byte offset of `needle` in `source` (the terminator token position).
fn type_expected_at(source: &str, needle: &str) -> usize {
    let pos = source
        .rfind(needle)
        .unwrap_or_else(|| panic!("needle {needle:?} not found in {source:?}"))
        as u32;
    let (parser, _root) = parse_source(source);
    parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_EXPECTED && d.start == pos)
        .count()
}

fn type_expected_count(source: &str) -> usize {
    let (parser, _root) = parse_source(source);
    parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_EXPECTED)
        .count()
}

#[test]
fn trailing_union_bar_reports_type_expected() {
    assert_eq!(type_expected_at("type T = string |;", ";"), 1);
}

#[test]
fn trailing_intersection_amp_reports_type_expected() {
    assert_eq!(type_expected_at("type T = string &;", ";"), 1);
}

#[test]
fn multi_union_trailing_bar_reports_type_expected() {
    assert_eq!(type_expected_at("type T = string | number |;", ";"), 1);
}

#[test]
fn keyof_missing_operand_reports_type_expected() {
    assert_eq!(type_expected_at("type T = keyof ;", ";"), 1);
}

#[test]
fn unique_missing_operand_reports_type_expected() {
    // The parser reports TS1110; the `'symbol' expected` grammar error lives in
    // the checker and is suppressed when the file has parse diagnostics.
    assert_eq!(type_expected_at("type T = unique ;", ";"), 1);
}

#[test]
fn readonly_missing_operand_reports_type_expected() {
    assert_eq!(type_expected_at("type T = readonly ;", ";"), 1);
}

#[test]
fn union_in_annotation_position_reports_type_expected() {
    assert_eq!(type_expected_at("let x: string |;", ";"), 1);
}

#[test]
fn union_in_parameter_position_reports_type_expected() {
    assert_eq!(type_expected_at("function f(a: number |) {}", ")"), 1);
}

#[test]
fn leading_bar_then_missing_reports_type_expected() {
    // `type T = | ;` — leading bar makes the first constituent required.
    assert_eq!(type_expected_at("type T = | ;", ";"), 1);
}

#[test]
fn leading_amp_then_missing_reports_type_expected() {
    assert_eq!(type_expected_at("type T = & ;", ";"), 1);
}

#[test]
fn nested_keyof_missing_operand_reports_type_expected() {
    assert_eq!(type_expected_at("type T = keyof keyof ;", ";"), 1);
}

#[test]
fn union_missing_member_inside_type_literal_reports_type_expected() {
    assert_eq!(type_expected_at("type T = { x: string | };", "}"), 1);
}

// Binder-name variance: the surrounding alias/parameter names must not affect
// the structural recovery.
#[test]
fn required_constituent_recovery_is_name_independent() {
    for name in ["Alpha", "_zz", "Renamed"] {
        let src = format!("type {name} = string |;");
        assert_eq!(
            type_expected_at(&src, ";"),
            1,
            "expected TS1110 for {src:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Contrast: genuinely-optional positions and well-formed types must be
// unchanged (no spurious TS1110). These exercise the suppression that the fix
// must preserve.
// ---------------------------------------------------------------------------

#[test]
fn well_formed_union_has_no_type_expected() {
    assert_eq!(type_expected_count("type T = string | number;"), 0);
}

#[test]
fn well_formed_keyof_has_no_type_expected() {
    assert_eq!(type_expected_count("type T = keyof string;"), 0);
}

#[test]
fn leading_bar_union_has_no_type_expected() {
    assert_eq!(type_expected_count("type T = | string | number;"), 0);
}

#[test]
fn tuple_trailing_comma_has_no_type_expected() {
    // A trailing comma in a tuple is a valid optional position, not a missing
    // required constituent.
    assert_eq!(type_expected_count("type T = [string, ];"), 0);
}

#[test]
fn parameter_trailing_comma_has_no_type_expected() {
    assert_eq!(type_expected_count("function h(a: string, ) {}"), 0);
}

#[test]
fn optional_annotation_omission_still_reports_single_type_expected() {
    // `let y: ;` is the genuinely-optional omission and already reported TS1110;
    // the fix must keep exactly one (no duplicate from the constituent path).
    assert_eq!(type_expected_count("let y: ;"), 1);
}
