use super::*;
use crate::TypeInterner;

#[test]
fn test_is_numeric_literal_name() {
    // Special values
    assert!(is_numeric_literal_name("NaN"));
    assert!(is_numeric_literal_name("Infinity"));
    assert!(is_numeric_literal_name("-Infinity"));

    // Regular numbers
    assert!(is_numeric_literal_name("0"));
    assert!(is_numeric_literal_name("1"));
    assert!(is_numeric_literal_name("42"));
    assert!(is_numeric_literal_name("-1"));
    assert!(is_numeric_literal_name("3.14"));

    // Non-numeric strings
    assert!(!is_numeric_literal_name("foo"));
    assert!(!is_numeric_literal_name(""));
    assert!(!is_numeric_literal_name("abc123"));
}

#[test]
fn test_type_id_ext_non_never() {
    // Test non_never
    assert_eq!(TypeId::UNKNOWN.non_never(), Some(TypeId::UNKNOWN));
    assert_eq!(TypeId::NEVER.non_never(), None);
}

#[test]
fn test_union_or_single() {
    let interner = TypeInterner::new();
    let db: &dyn TypeDatabase = &interner;

    // Empty list -> NEVER
    let result = union_or_single(db, vec![]);
    assert_eq!(result, TypeId::NEVER);

    // Single element -> that element
    let result = union_or_single(db, vec![TypeId::STRING]);
    assert_eq!(result, TypeId::STRING);

    // Multiple elements -> union
    let result = union_or_single(db, vec![TypeId::STRING, TypeId::NUMBER]);
    assert_ne!(result, TypeId::STRING);
    assert_ne!(result, TypeId::NUMBER);
}

#[test]
fn test_intersection_or_single() {
    let interner = TypeInterner::new();
    let db: &dyn TypeDatabase = &interner;

    let result = intersection_or_single(db, vec![]);
    assert_eq!(result, TypeId::NEVER);

    let result = intersection_or_single(db, vec![TypeId::STRING]);
    assert_eq!(result, TypeId::STRING);

    let result = intersection_or_single(db, vec![TypeId::STRING, TypeId::NUMBER]);
    assert_ne!(result, TypeId::STRING);
    assert_ne!(result, TypeId::NUMBER);
}

// =========================================================================
// Extended `is_numeric_literal_name` coverage: canonical-form requirement
// =========================================================================

/// Non-canonical numeric forms must NOT be classified as numeric literal
/// names. tsc only treats a property name as numeric when its string form
/// matches what `String(value)` produces — so leading zeros, trailing
/// `.0`, redundant `+`, and `0x` hex prefixes are all rejected even though
/// they parse as `f64`.
#[test]
fn is_numeric_literal_name_rejects_non_canonical_forms() {
    // Leading zero is not canonical (`Number("01") === 1` → "1" ≠ "01").
    assert!(!is_numeric_literal_name("01"));
    assert!(!is_numeric_literal_name("007"));
    // Redundant trailing `.` and `.0` are not canonical (round-trip "1").
    assert!(!is_numeric_literal_name("1."));
    assert!(!is_numeric_literal_name("1.0"));
    assert!(!is_numeric_literal_name("0.0"));
    // Redundant leading `+`.
    assert!(!is_numeric_literal_name("+1"));
    assert!(!is_numeric_literal_name("+0"));
    // Hex/octal/binary prefixes — Rust's f64 parse rejects these, so the
    // function returns false at the parse step.
    assert!(!is_numeric_literal_name("0x10"));
    assert!(!is_numeric_literal_name("0o10"));
    assert!(!is_numeric_literal_name("0b10"));
    // Whitespace-padded forms.
    assert!(!is_numeric_literal_name(" 1"));
    assert!(!is_numeric_literal_name("1 "));
}

/// Canonical exponent form is accepted; non-canonical exponent forms
/// (e.g. `1e+1` for what canonicalizes to `10`) are rejected.
#[test]
fn is_numeric_literal_name_canonical_exponent_threshold() {
    // 1e21 is canonical for very large numbers (above the 1e21 threshold
    // in `js_number_to_string`). Below 1e21, the canonical form is the
    // decimal expansion.
    assert!(!is_numeric_literal_name("1e2")); // canonical = "100"
    assert!(!is_numeric_literal_name("1e+2")); // canonical = "100"
    // Very small numbers below 1e-6 use exponent form.
    assert!(is_numeric_literal_name("1e-7"));
}

/// `-0` round-trips through JS's `String(-0) === "0"`, so the literal
/// `"-0"` is NOT a canonical numeric name.
#[test]
fn is_numeric_literal_name_rejects_negative_zero_literal() {
    assert!(!is_numeric_literal_name("-0"));
    assert!(is_numeric_literal_name("0"));
}

/// `Infinity` and `NaN` are special-cased before the f64 parse path.
#[test]
fn is_numeric_literal_name_special_value_strings_only() {
    assert!(is_numeric_literal_name("NaN"));
    assert!(is_numeric_literal_name("Infinity"));
    assert!(is_numeric_literal_name("-Infinity"));
    // Non-special-cased variants: `+Infinity`, `nan`, `infinity` (lowercase)
    // would fail the special-case check AND fail the f64 parse round-trip.
    assert!(!is_numeric_literal_name("+Infinity"));
    assert!(!is_numeric_literal_name("nan"));
    assert!(!is_numeric_literal_name("infinity"));
}

// =========================================================================
// `canonicalize_numeric_name` coverage (was completely uncovered)
// =========================================================================

#[test]
fn canonicalize_numeric_name_returns_canonical_form_for_finite_numbers() {
    // Equivalent numerics canonicalize to the same form.
    assert_eq!(canonicalize_numeric_name("1"), Some("1".to_string()));
    assert_eq!(canonicalize_numeric_name("1."), Some("1".to_string()));
    assert_eq!(canonicalize_numeric_name("1.0"), Some("1".to_string()));
    assert_eq!(canonicalize_numeric_name("01"), Some("1".to_string()));
    assert_eq!(canonicalize_numeric_name("+1"), Some("1".to_string()));
    // Negative.
    assert_eq!(canonicalize_numeric_name("-1"), Some("-1".to_string()));
    // Decimals.
    assert_eq!(canonicalize_numeric_name("3.14"), Some("3.14".to_string()));
    // Scientific notation collapses to decimal expansion below 1e21.
    assert_eq!(canonicalize_numeric_name("1e2"), Some("100".to_string()));
}

#[test]
fn canonicalize_numeric_name_rejects_non_numeric() {
    assert_eq!(canonicalize_numeric_name("foo"), None);
    assert_eq!(canonicalize_numeric_name(""), None);
    assert_eq!(canonicalize_numeric_name("abc123"), None);
}

#[test]
fn canonicalize_numeric_name_rejects_infinity_via_finite_guard() {
    // `parse_numeric_literal_value` parses Infinity as a finite-ish float,
    // but the `is_finite` guard filters it out. Returns None.
    assert_eq!(canonicalize_numeric_name("Infinity"), None);
    assert_eq!(canonicalize_numeric_name("-Infinity"), None);
}

// =========================================================================
// `required_param_count` and `required_element_count` (was uncovered)
// =========================================================================

#[test]
fn required_param_count_filters_optional_and_rest() {
    use crate::types::ParamInfo;
    use tsz_common::interner::Atom;

    let required = ParamInfo {
        name: Some(Atom::NONE),
        type_id: TypeId::NUMBER,
        optional: false,
        rest: false,
    };
    let optional = ParamInfo {
        optional: true,
        ..required
    };
    let rest = ParamInfo {
        rest: true,
        optional: false,
        ..required
    };

    assert_eq!(required_param_count(&[]), 0);
    assert_eq!(required_param_count(&[required]), 1);
    assert_eq!(required_param_count(&[required, required]), 2);
    assert_eq!(required_param_count(&[optional]), 0);
    assert_eq!(required_param_count(&[rest]), 0);
    assert_eq!(
        required_param_count(&[required, optional, required, rest]),
        2,
        "only the two required params count; optional and rest are excluded"
    );
}

#[test]
fn required_element_count_filters_optional_and_rest() {
    use crate::types::TupleElement;

    let required = TupleElement {
        type_id: TypeId::STRING,
        name: None,
        optional: false,
        rest: false,
    };
    let optional = TupleElement {
        optional: true,
        ..required
    };
    let rest = TupleElement {
        rest: true,
        optional: false,
        ..required
    };

    assert_eq!(required_element_count(&[]), 0);
    assert_eq!(required_element_count(&[required]), 1);
    assert_eq!(required_element_count(&[optional]), 0);
    assert_eq!(required_element_count(&[rest]), 0);
    assert_eq!(
        required_element_count(&[required, required, optional, rest, required]),
        3,
        "only the three required elements count"
    );
}

// =========================================================================
// `is_numeric_property_name` (was uncovered) — bridges the public string
// helper through the interner's atom-resolution path.
// =========================================================================

#[test]
fn is_numeric_property_name_via_interner_round_trip() {
    let interner = TypeInterner::new();
    let db: &dyn TypeDatabase = &interner;

    let numeric_atom = db.intern_string("42");
    assert!(is_numeric_property_name(db, numeric_atom));

    let nan_atom = db.intern_string("NaN");
    assert!(is_numeric_property_name(db, nan_atom));

    let non_numeric_atom = db.intern_string("foo");
    assert!(!is_numeric_property_name(db, non_numeric_atom));

    let non_canonical_atom = db.intern_string("01");
    assert!(
        !is_numeric_property_name(db, non_canonical_atom),
        "non-canonical numeric forms (e.g. '01') must not classify as numeric property names"
    );
}
