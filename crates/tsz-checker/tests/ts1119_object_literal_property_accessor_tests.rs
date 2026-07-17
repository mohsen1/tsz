//! Tests for TS1119 "An object literal cannot have property and accessor with
//! the same name." — a data property and a get/set accessor sharing a name in
//! the same object literal, in either order, with key normalization. Distinct
//! from TS1117 (two data properties) and TS1118 (two same-kind accessors).

use tsz_checker::test_utils::check_source_codes;

#[test]
fn property_then_accessor_reports_ts1119() {
    // Data property first, accessor second. Vary binder names.
    for source in [
        "var widget = { size: 0, get size() { return 0; } };",
        "var store = { count: 0, set count(v) {} };",
    ] {
        let codes = check_source_codes(source).to_vec();
        assert!(
            codes.contains(&1119) && !codes.contains(&1117),
            "expected TS1119 (not TS1117) for `{source}`: {codes:?}"
        );
    }
}

#[test]
fn accessor_then_property_reports_ts1119() {
    // Accessor first, data property second (the reverse order).
    for source in [
        "var widget = { get size() { return 0; }, size: 0 };",
        "var store = { set count(v) {}, count: 0 };",
    ] {
        let codes = check_source_codes(source).to_vec();
        assert!(
            codes.contains(&1119) && !codes.contains(&1117),
            "expected TS1119 (not TS1117) for `{source}`: {codes:?}"
        );
    }
}

#[test]
fn normalized_numeric_and_string_key_reports_ts1119() {
    // 1.0 and "1" name the same property key.
    let codes = check_source_codes("var k = { 1.0: 0, get \"1\"() { return 0; } };").to_vec();
    assert!(
        codes.contains(&1119),
        "expected TS1119 for a normalized numeric/string key clash: {codes:?}"
    );
}

#[test]
fn complementary_get_set_pair_is_clean() {
    // A get/set pair for the same name is legal — no TS1119 / TS1118 / TS1117.
    let codes =
        check_source_codes("var w = { get size() { return 0; }, set size(v) {} };").to_vec();
    assert!(
        !codes.contains(&1119) && !codes.contains(&1118) && !codes.contains(&1117),
        "a get/set accessor pair must not report a duplicate-member error: {codes:?}"
    );
}

#[test]
fn two_data_properties_stay_ts1117() {
    // Two data properties with the same name remain TS1117, not TS1119.
    let codes = check_source_codes("var w = { size: 0, size: 1 };").to_vec();
    assert!(
        codes.contains(&1117) && !codes.contains(&1119),
        "two data properties must report TS1117, not TS1119: {codes:?}"
    );
}
