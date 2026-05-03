//! Regression coverage for TS2574 ("A rest element type must be an array type").
//!
//! tsc emits TS2574 when a rest tuple element wraps a non-array, non-tuple,
//! non-type-parameter type — e.g. `[...string]` or `[...string?]`. Variadic
//! type-parameter spreads (`[...T]`) and concrete array/tuple rests
//! (`[...string[]]`, `[...[number, number]]`) remain valid.

use crate::test_utils::check_source_codes;

/// `[...string]` — rest element wrapping a primitive.
#[test]
fn rest_primitive_emits_ts2574() {
    let codes = check_source_codes(
        r#"
type T = [...string];
"#,
    );
    assert!(
        codes.contains(&2574),
        "TS2574 expected for `[...string]`: {codes:?}"
    );
}

/// `[...string[]]` — rest element wrapping an array; valid.
#[test]
fn rest_array_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type T = [...string[]];
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for `[...string[]]`: {codes:?}"
    );
}

/// `[...T]` where `T` is a type parameter — valid (variadic spread).
#[test]
fn rest_type_parameter_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type Wrap<T> = [...T];
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for `[...T]` (type-parameter spread): {codes:?}"
    );
}

/// `[...[number, string]]` — rest element wrapping a tuple; valid.
#[test]
fn rest_tuple_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type T = [...[number, string]];
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for `[...[number, string]]`: {codes:?}"
    );
}

/// `[number, ...boolean]` — primitive rest after fixed elements; still TS2574.
#[test]
fn rest_primitive_after_fixed_emits_ts2574() {
    let codes = check_source_codes(
        r#"
type T = [number, ...boolean];
"#,
    );
    assert!(
        codes.contains(&2574),
        "TS2574 expected for `[number, ...boolean]`: {codes:?}"
    );
}

/// `[...rest: string]` — NAMED rest member wrapping a primitive; TS2574.
#[test]
fn named_rest_primitive_emits_ts2574() {
    let codes = check_source_codes(
        r#"
type T = [...rest: string];
"#,
    );
    assert!(
        codes.contains(&2574),
        "TS2574 expected for named rest `[...rest: string]`: {codes:?}"
    );
}

/// `[...rest: string[]]` — named rest wrapping an array; valid.
#[test]
fn named_rest_array_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type T = [...rest: string[]];
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for named rest `[...rest: string[]]`: {codes:?}"
    );
}

/// `[...rest: T]` where `T` is a type parameter — valid (variadic spread).
#[test]
fn named_rest_type_parameter_does_not_emit_ts2574() {
    let codes = check_source_codes(
        r#"
type Wrap<T> = [...rest: T];
"#,
    );
    assert!(
        !codes.contains(&2574),
        "TS2574 should not fire for named rest `[...rest: T]`: {codes:?}"
    );
}
