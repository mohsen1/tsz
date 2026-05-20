//! Regression tests for TS1166/TS1169/TS1170 — computed property names in
//! class properties / interfaces / type literals must be a literal or an
//! entity-name expression.
//!
//! tsc rejects parenthesized property access (which breaks the entity-name
//! chain) and conditional expressions even when their inner type happens to
//! be a literal or unique-symbol — the syntactic form matters, not just the
//! computed type. Source:
//! `transpile/declarationComputedPropertyNames.ts` lines 17/21/22 (and
//! mirrors in the interface and class).

use crate::test_utils::check_source_diagnostics;

fn diag_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn ts1170_type_literal_paren_property_access() {
    let codes = diag_codes(
        r#"
type X = {
    [(globalThis.Symbol).iterator]: number,
};
"#,
    );
    assert!(
        codes.contains(&1170),
        "Expected TS1170 for parenthesized property access in type literal. Got: {codes:?}"
    );
}

#[test]
fn ts1170_type_literal_conditional() {
    let codes = diag_codes(
        r#"
type X = {
    [Math.random() > 0.5 ? "a" : "b"]: number,
};
"#,
    );
    assert!(
        codes.contains(&1170),
        "Expected TS1170 for conditional in type literal. Got: {codes:?}"
    );
}

#[test]
fn ts1169_interface_paren_property_access() {
    let codes = diag_codes(
        r#"
interface X {
    [(globalThis.Symbol).iterator]: number;
}
"#,
    );
    assert!(
        codes.contains(&1169),
        "Expected TS1169 for parenthesized property access in interface. Got: {codes:?}"
    );
}

#[test]
fn ts1166_class_paren_property_access() {
    let codes = diag_codes(
        r#"
class X {
    [(globalThis.Symbol).iterator]: number = 1;
}
"#,
    );
    assert!(
        codes.contains(&1166),
        "Expected TS1166 for parenthesized property access in class property. Got: {codes:?}"
    );
}

/// Anti-hardcoding cover: entity-name property access (no parens) must NOT
/// trigger TS1170; the rule is structural, not name-based.
#[test]
fn ts1170_not_emitted_for_pure_entity_name_chain() {
    let codes = diag_codes(
        r#"
declare const k: unique symbol;
type X = {
    [k]: number,
};
"#,
    );
    assert!(
        !codes.contains(&1170),
        "TS1170 must NOT fire for entity-name unique-symbol access. Got: {codes:?}"
    );
}

/// Anti-hardcoding cover: same paren-rejection rule with a renamed namespace
/// — the fix must not depend on the literal token "globalThis".
#[test]
fn ts1170_paren_property_access_renamed() {
    let codes = diag_codes(
        r#"
declare const ns: { sym: unique symbol };
type X = {
    [(ns).sym]: number,
};
"#,
    );
    assert!(
        codes.contains(&1170),
        "TS1170 must fire for parenthesized access regardless of identifier names. Got: {codes:?}"
    );
}
