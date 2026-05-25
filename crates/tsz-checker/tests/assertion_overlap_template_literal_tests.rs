//! Tests for TS2352 assertion-overlap between string-domain types and
//! template-literal / string-intrinsic types.
//!
//! Casting a string-literal type whose value does not match a template-literal
//! pattern (for example asserting the literal "x" to a number-placeholder
//! template) is legal in tsc: the source literal
//! widens to `string`, and a template-literal type is a subtype of `string`,
//! so the two sufficiently overlap. tsz used to require the literal *value* to
//! match the pattern and emitted a false TS2352. The rule is structural: any
//! string-domain type (`string`, a string literal, a template-literal, or a
//! string-intrinsic such as `Uppercase<S>`) overlaps a template-literal /
//! string-intrinsic type for assertion purposes.

use crate::test_utils::check_source_strict_codes as check_strict;

fn ts2352(source: &str) -> bool {
    check_strict(source).contains(&2352)
}

/// Non-matching literal text asserted to a number-placeholder template. Legal:
/// the literal widens to `string`, and the template is a `string` subtype.
/// (Reported repro.)
#[test]
fn string_literal_to_number_template_no_ts2352() {
    assert!(
        !ts2352(r#"const a = "x" as `a${number}b`;"#),
        "no TS2352 expected — string literal widens to string and overlaps the template"
    );
}

/// String literal asserted to a bare numeric template.
#[test]
fn string_literal_to_bare_number_template_no_ts2352() {
    assert!(
        !ts2352(r#"const b = "x" as `${number}`;"#),
        "no TS2352 expected — string literal overlaps a bare `${{number}}` template"
    );
}

/// String literal asserted to a string-placeholder template.
#[test]
fn string_literal_to_string_template_no_ts2352() {
    assert!(
        !ts2352(r#"const e = "x" as `a${string}`;"#),
        "no TS2352 expected — string literal overlaps a `${{string}}` template"
    );
}

/// Control: a matching literal is accepted by both compilers.
#[test]
fn matching_string_literal_to_template_no_ts2352() {
    assert!(
        !ts2352(r#"const c = "a5b" as `a${number}b`;"#),
        "no TS2352 expected — literal matches the template pattern"
    );
}

/// Control: a `string` source (not a literal) already overlapped the template.
#[test]
fn string_to_template_no_ts2352() {
    assert!(
        !ts2352(r#"declare const s: string; const d = s as `a${number}b`;"#),
        "no TS2352 expected — `string` source overlaps the template"
    );
}

/// Reverse direction: template source asserted to a string literal.
#[test]
fn template_to_string_literal_no_ts2352() {
    assert!(
        !ts2352(r#"declare const t: `a${number}b`; const r = t as "x";"#),
        "no TS2352 expected — template-literal source overlaps a string literal"
    );
}

/// A string literal overlaps a string-intrinsic mapping type, which is also a
/// `string` subtype.
#[test]
fn string_literal_to_string_intrinsic_no_ts2352() {
    assert!(
        !ts2352(r#"type U = Uppercase<`a${number}b`>; const u = "x" as U;"#),
        "no TS2352 expected — string literal overlaps a string-intrinsic type"
    );
}

/// Negative control: a numeric literal must NOT overlap a string template; the
/// widened source is `number`, which is disjoint from `string`.
#[test]
fn number_literal_to_template_emits_ts2352() {
    assert!(
        ts2352(r#"const n = 5 as `a${number}b`;"#),
        "TS2352 expected — a numeric source does not overlap a string template"
    );
}

/// Negative control unchanged: `"x" as 123` (string vs number literal) is still
/// a non-overlapping assertion.
#[test]
fn string_literal_to_number_literal_emits_ts2352() {
    assert!(
        ts2352(r#"const f = "x" as 123;"#),
        "TS2352 expected — string literal does not overlap a numeric literal"
    );
}
