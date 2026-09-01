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

/// tsc's `checkComputedPropertyName` (TS1170, "must be a literal or unique
/// symbol type") and its type-of-property-key check (TS2464, "must be of
/// type string/number/symbol/any") are independent: both fire together when
/// a type-literal property's computed name is neither a literal/entity-name
/// expression NOR of a valid property-key type. Verified against
/// `tsc@7.0.2`: `type U = { [b as unknown as boolean]: number }` (`b:
/// boolean`) reports both TS1170 and TS2464 at the same position.
#[test]
fn ts2464_fires_alongside_ts1170_type_literal_property() {
    let codes = diag_codes(
        r#"
declare const b: boolean;
type U = { [b as unknown as boolean]: number };
"#,
    );
    assert!(
        codes.contains(&1170),
        "Expected TS1170 for non-literal computed name in type literal. Got: {codes:?}"
    );
    assert!(
        codes.contains(&2464),
        "Expected TS2464 alongside TS1170 — the type-literal literal-form gate must not \
         suppress the independent property-key-type check. Got: {codes:?}"
    );
}

/// Same independence, for a type literal's computed *method* name (the
/// signature-member arm, a separate code path from the property-member arm
/// above).
#[test]
fn ts2464_fires_alongside_ts1170_type_literal_method() {
    let codes = diag_codes(
        r#"
declare const b: boolean;
type U = { [b as unknown as boolean](): number };
"#,
    );
    assert!(
        codes.contains(&1170),
        "Expected TS1170 for non-literal computed method name in type literal. Got: {codes:?}"
    );
    assert!(
        codes.contains(&2464),
        "Expected TS2464 alongside TS1170 for a type-literal method's computed name. Got: {codes:?}"
    );
}

/// Anti-hardcoding cover: same TS1170+TS2464 co-occurrence with a renamed
/// binder and a type alias name — the fix must not depend on identifier
/// spelling.
#[test]
fn ts2464_fires_alongside_ts1170_renamed_binder() {
    let codes = diag_codes(
        r#"
declare const flag: boolean;
type Renamed = { [flag as unknown as boolean]: number };
"#,
    );
    assert!(
        codes.contains(&1170) && codes.contains(&2464),
        "Renamed binders must not change the TS1170+TS2464 co-occurrence rule. Got: {codes:?}"
    );
}

/// Negative control: TS1170 can fire alone when the syntactic form is
/// invalid (parenthesized entity-name access breaks the entity-name chain)
/// but the computed type is still a genuinely valid property key (`string`).
/// TS2464 must NOT fire spuriously just because TS1170 did — these are two
/// independently-gated checks, not a package deal in either direction.
/// Covers both the property and method member shapes.
#[test]
fn ts1170_alone_when_type_is_valid_property() {
    let codes = diag_codes(
        r#"
declare const ns: { sym: string };
type X = {
    [(ns).sym]: number,
};
"#,
    );
    assert!(
        codes.contains(&1170),
        "Expected TS1170 for parenthesized property access. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2464),
        "TS2464 must not fire when the computed name's type is a valid property key (string). \
         Got: {codes:?}"
    );
}

#[test]
fn ts1170_alone_when_type_is_valid_method() {
    let codes = diag_codes(
        r#"
declare const ns: { sym: string };
type X = {
    [(ns).sym](): number,
};
"#,
    );
    assert!(
        codes.contains(&1170),
        "Expected TS1170 for parenthesized method access. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2464),
        "TS2464 must not fire when the computed name's type is a valid property key (string). \
         Got: {codes:?}"
    );
}

/// Anti-regression: a genuinely valid (literal-or-entity-name, valid-key-type)
/// computed name in a type literal must still emit neither code.
#[test]
fn ts1170_and_ts2464_absent_for_valid_type_literal_computed_name() {
    let codes = diag_codes(
        r#"
declare const k: unique symbol;
type X = { [k]: number };
"#,
    );
    assert!(
        !codes.contains(&1170) && !codes.contains(&2464),
        "Valid unique-symbol computed name must not trigger TS1170/TS2464. Got: {codes:?}"
    );
}
