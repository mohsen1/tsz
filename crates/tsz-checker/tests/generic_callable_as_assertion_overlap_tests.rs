//! An `as`-assertion to a GENERIC callable/constructable type must recognize the
//! overlap when the source is itself callable/constructable, instead of emitting
//! a spurious TS2352.
//!
//! Regression for #14325 (arktype): `class {} as new <t extends object>(base: t) => t`
//! casts a constructable anonymous class to a generic construct signature. The
//! signature's own bound type parameter has no concrete shape, so the structural
//! overlap check failed and emitted TS2352. tsc's `isTypeComparableTo`
//! instantiates the generic signature and finds the two constructors overlap.
//! A non-callable source (`"s" as <T>(v: T) => T`) is still a genuine mismatch
//! and keeps TS2352.

use tsz_checker::test_utils::check_source_code_messages;

fn ts2352_count(source: &str) -> usize {
    check_source_code_messages(source)
        .into_iter()
        .filter(|(code, _)| *code == 2352)
        .count()
}

#[test]
fn constructable_source_to_generic_construct_signature_no_ts2352() {
    let src = r#"
const B = class {} as new <t extends object>(base: t) => t;
export { B };
"#;
    assert_eq!(
        ts2352_count(src),
        0,
        "a constructable anonymous class cast to a generic construct signature must not emit TS2352"
    );
}

#[test]
fn callable_source_to_generic_call_signature_no_ts2352() {
    let src = r#"
const f = (() => 0) as <T>(v: T) => T;
export { f };
"#;
    assert_eq!(
        ts2352_count(src),
        0,
        "a callable source cast to a generic call signature must not emit TS2352"
    );
}

// Binder-name variation: the fix is structural (both sides callable/constructable
// + generic signature target), not keyed on any identifier.
#[test]
fn constructable_source_to_renamed_generic_construct_signature_no_ts2352() {
    let src = r#"
const Make = class {} as new <Elem extends object>(seed: Elem) => Elem;
export { Make };
"#;
    assert_eq!(
        ts2352_count(src),
        0,
        "renamed generic construct-signature target must not emit TS2352"
    );
}

// Negative control: a non-callable source has no overlap with a generic callable
// target, so TS2352 must still fire (the suppression is gated on the source being
// callable/constructable).
#[test]
fn noncallable_source_to_generic_call_signature_still_ts2352() {
    let src = r#"
const x = "str" as <T>(v: T) => T;
export { x };
"#;
    assert!(
        ts2352_count(src) >= 1,
        "a non-callable `string` source cast to a generic call signature must still emit TS2352"
    );
}
