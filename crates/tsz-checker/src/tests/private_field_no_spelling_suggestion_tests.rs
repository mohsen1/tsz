//! ECMAScript private (`#`) fields are never offered as "did you mean?" spelling
//! suggestions on a public member access.
//!
//! Structural rule: a `#foo` field is unreachable through a public `obj.bar`
//! access, so tsc never lists one as a TS2551 suggestion candidate — an external
//! `c.events` on a class with a `#event` field is plain TS2339, not TS2551 "Did
//! you mean '#event'?". tsz collected the private field into the candidate set.
//! Owner: `error_reporter/suggestions.rs` `collect_accessible_type_property_names`.

use crate::test_utils::check_source_strict_codes as check_strict;

const TS2339: u32 = 2339; // Property does not exist.
const TS2551: u32 = 2551; // Property does not exist. Did you mean '<x>'?

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

#[test]
fn private_field_not_offered_as_suggestion() {
    // `c.events` where the only near-name member is the private `#event` —
    // tsc reports plain TS2339, no suggestion.
    let codes = check_strict(
        r#"
class C { #event = 1; }
declare const c: C;
c.events;
"#,
    );
    assert_eq!(count(&codes, TS2339), 1, "plain TS2339 expected: {codes:?}");
    assert_eq!(
        count(&codes, TS2551),
        0,
        "private `#event` must not be suggested: {codes:?}"
    );
}

#[test]
fn public_field_typo_still_suggested() {
    // Control: a public near-name field still produces the TS2551 suggestion, so
    // the fix only drops private candidates rather than disabling suggestions.
    let codes = check_strict(
        r#"
class C { event = 1; }
declare const c: C;
c.events;
"#,
    );
    assert_eq!(
        count(&codes, TS2551),
        1,
        "public `event` is still suggested: {codes:?}"
    );
    assert_eq!(count(&codes, TS2339), 0, "{codes:?}");
}
