//! Regression coverage for tsc's isValidIndexKeyType AST surface.
//!
//! tsc accepts an intersection like `string & Brand` or two pattern-literal
//! templates intersected as a valid index-signature parameter type, and
//! accepts unions like `string | number | symbol` similarly. These resolve
//! to composite `TypeIds` that don't match the primitive equality check tsz
//! used to perform, so the AST shape must be inspected as a fallback.

use crate::test_utils::check_source_codes;

/// `string & Brand` should be a valid index signature parameter type — no TS1268.
#[test]
fn intersection_of_string_and_brand_is_valid_index_param() {
    let codes = check_source_codes(
        r#"
type Brand = { __brand: 'id' };
type Tagged = string & Brand;

declare let inline: { [key: Tagged]: number };
interface I { [key: Tagged]: number }
type Aliased = { [key: Tagged]: number };
"#,
    );
    assert!(
        !codes.contains(&1268),
        "TS1268 should not fire for `string & Brand` index sig param: {codes:?}"
    );
}

/// Intersection of two pattern-literal templates is valid.
#[test]
fn intersection_of_template_literals_is_valid_index_param() {
    let codes = check_source_codes(
        r#"
declare let v: { [x: `${string}xxx${string}` & `${string}yyy${string}`]: string };
"#,
    );
    assert!(
        !codes.contains(&1268),
        "TS1268 should not fire for intersection of template literal types: {codes:?}"
    );
}

/// `string | number` and other unions of valid types must not trigger TS1268.
#[test]
fn union_of_valid_index_keywords_is_valid_index_param() {
    let codes = check_source_codes(
        r#"
type T1 = { [key: string | number]: any };
type T2 = { [key: number | symbol]: any };
type T3 = { [key: symbol | `foo${string}`]: any };
declare let inline: { [key: string | number]: any };
"#,
    );
    assert!(
        !codes.contains(&1268),
        "TS1268 should not fire for unions of string/number/symbol/template-literal: {codes:?}"
    );
}

/// `T & string` is generic — must trigger TS1337 (literal/generic), not TS1268.
#[test]
fn intersection_with_type_parameter_emits_ts1337_not_ts1268() {
    let codes = check_source_codes(
        r#"
type Invalid<T extends string> = {
    [key: T & string]: string;
};
"#,
    );
    assert!(
        codes.contains(&1337),
        "TS1337 expected for `T & string` (generic intersection): {codes:?}"
    );
    assert!(
        !codes.contains(&1268),
        "TS1268 should not fire when TS1337 already covers the case: {codes:?}"
    );
}

/// Genuinely invalid index sig param types still trigger TS1268.
#[test]
fn boolean_index_sig_param_still_emits_ts1268() {
    let codes = check_source_codes(
        r#"
declare let v: { [key: boolean]: string };
"#,
    );
    assert!(
        codes.contains(&1268),
        "TS1268 expected for `boolean` index sig param: {codes:?}"
    );
}
