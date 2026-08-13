//! Regression tests: a genuine `any` computed key in a value-position object
//! destructuring is a valid index only when the source permits a dynamic index
//! (issue found by differential probing against `typescript@7.0.2`).
//!
//! `{ [k]: v } = obj` desugars to `v = obj[k]`. `tsc` reports `TS2538`
//! ("Type 'any' cannot be used as an index type.") when the source is a concrete
//! object with no index signature — exactly as the element access would — and
//! accepts the key when the source is `any`, is generic, or carries a string or
//! number index signature. tsz previously exempted every `any` key
//! unconditionally, silently accepting the invalid cases.
//!
//! Binder names are varied so no identifier string is load-bearing.

use crate::test_utils::check_source_codes;

const TS2538: u32 = 2538;

/// Empty object source has no index signature: an `any` key is `TS2538`.
#[test]
fn any_key_on_empty_object_source_reports_ts2538() {
    let source = r#"
declare const key: any;
declare const bag: {};
const { [key]: picked } = bag;
"#;
    assert!(
        check_source_codes(source).contains(&TS2538),
        "an `any` computed key on a no-index source must report TS2538"
    );
}

/// A concrete object with named members but no index signature: still `TS2538`.
#[test]
fn any_key_on_named_member_object_source_reports_ts2538() {
    let source = r#"
declare const sel: any;
declare const record: { alpha: number };
const { [sel]: chosen } = record;
"#;
    assert!(
        check_source_codes(source).contains(&TS2538),
        "an `any` key on an object without an index signature must report TS2538"
    );
}

/// Positive guard: an `any` source permits an `any` key (element access of `any`
/// is `any`).
#[test]
fn any_key_on_any_source_is_accepted() {
    let source = r#"
declare const k: any;
declare const anything: any;
const { [k]: v } = anything;
"#;
    assert!(
        !check_source_codes(source).contains(&TS2538),
        "an `any` source must accept an `any` computed key"
    );
}

/// Positive guard: a string index signature permits an `any` key.
#[test]
fn any_key_on_string_index_source_is_accepted() {
    let source = r#"
declare const k: any;
declare const dict: { [entry: string]: number };
const { [k]: v } = dict;
"#;
    assert!(
        !check_source_codes(source).contains(&TS2538),
        "a string-indexed source must accept an `any` computed key"
    );
}

/// Regression guard: a non-`any` invalid key (`boolean`) on a no-index source
/// already reported `TS2538` and must keep doing so — the fix only narrows the
/// `any` exemption, it does not disturb other invalid key types.
#[test]
fn boolean_key_on_no_index_source_still_reports_ts2538() {
    let source = r#"
declare const flag: boolean;
declare const bag: {};
const { [flag]: v } = bag;
"#;
    assert!(
        check_source_codes(source).contains(&TS2538),
        "a boolean computed key on a no-index source must still report TS2538"
    );
}
