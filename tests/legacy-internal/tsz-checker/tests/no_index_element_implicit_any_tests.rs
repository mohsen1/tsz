//! A no-index-signature element access resolves to the implicit-`any` element
//! type, not `undefined`.
//!
//! Structural rule: when `obj[key]` cannot index `obj` (no matching index
//! signature; e.g. a `unique symbol` key on a plain object), tsc emits TS7053
//! ("element implicitly has an 'any' type") AND resolves the element type to
//! `any`. tsz emitted the correct TS7053 but left the result as the `undefined`
//! default, which cascaded into spurious TS2322 / TS2352 / TS2532 on the result
//! (witness: mobx). Owner: `types/computation/access.rs` element-access
//! `report_no_index` read branch.

use crate::test_utils::check_source_strict_codes as check_strict;

const TS7053: u32 = 7053; // Element implicitly has an 'any' type (no index sig).
const TS2322: u32 = 2322; // Type X is not assignable to type Y.
const TS2352: u32 = 2352; // Conversion may be a mistake.
const TS2532: u32 = 2532; // Object is possibly 'undefined'.

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

#[test]
fn no_index_unique_symbol_read_emits_only_ts7053() {
    // `o[sym]` assigned to a typed var: TS7053 stands, but the result is `any`,
    // so there is no spurious TS2322 cascade.
    let codes = check_strict(
        r#"
declare const sym: unique symbol;
interface Admin { kind: string }
declare const o: { a: number };
const a1: Admin = o[sym];
"#,
    );
    assert!(codes.contains(&TS7053), "TS7053 must still fire: {codes:?}");
    assert_eq!(
        count(&codes, TS2322),
        0,
        "no-index element is `any`, not `undefined`, so no TS2322: {codes:?}"
    );
}

#[test]
fn no_index_element_any_assignable_to_concrete() {
    // The `any` result is assignable to a concrete annotation without TS2322.
    let codes = check_strict(
        r#"
declare const sym: unique symbol;
declare const o: { a: number };
const n: number = o[sym];
"#,
    );
    assert!(codes.contains(&TS7053), "{codes:?}");
    assert_eq!(count(&codes, TS2322), 0, "{codes:?}");
}

#[test]
fn no_index_element_any_cast_no_ts2352() {
    let codes = check_strict(
        r#"
declare const sym: unique symbol;
interface Admin { kind: string }
declare const o: { a: number };
const a2 = (o[sym] as Admin).kind;
"#,
    );
    assert!(codes.contains(&TS7053), "{codes:?}");
    assert_eq!(
        count(&codes, TS2352),
        0,
        "`any` casts to anything: no TS2352: {codes:?}"
    );
}

#[test]
fn no_index_element_any_member_access_no_ts2532() {
    let codes = check_strict(
        r#"
declare const sym: unique symbol;
declare const o: { a: number };
const a3 = o[sym].kind;
"#,
    );
    assert!(codes.contains(&TS7053), "{codes:?}");
    assert_eq!(
        count(&codes, TS2532),
        0,
        "`any` is not possibly-undefined: no TS2532: {codes:?}"
    );
}

#[test]
fn index_signature_element_keeps_its_type() {
    // Negative control: this path is `report_no_index = false`, untouched by the
    // fix — a string index signature still types the element (number), so
    // assigning it to a string is still TS2322. Proves the fix does not
    // over-broaden every element access to `any`.
    let codes = check_strict(
        r#"
declare const o: { [k: string]: number };
const s: string = o["x"];
"#,
    );
    assert_eq!(
        count(&codes, TS2322),
        1,
        "index-signature element keeps its declared type: {codes:?}"
    );
}
