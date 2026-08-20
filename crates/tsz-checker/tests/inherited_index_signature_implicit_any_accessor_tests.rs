//! A get-accessor with no return annotation (`get a();`) and a set-accessor
//! with no parameter annotation (`set a(v);`) are *legally* implicit `any` in
//! TypeScript — exactly like an unannotated property signature. #17822 fixed the
//! property-signature path (`lower_type_element`), but the accessor paths still
//! lowered the missing annotation through `lower_type(NodeIndex::NONE)`, which
//! yields the `error` sentinel. That `error` poisoned the containing object
//! type, so `check_index_signature_compatibility` (which clears an index whose
//! value "contains an error") silently dropped an inherited `string` index and
//! suppressed TS2413 — the same false-negative family as #17815, one member kind
//! over.
//!
//! The fix routes every object-type member value/return/parameter annotation
//! through the shared `lower_member_value_annotation` helper (missing → implicit
//! `any`), so property signatures and get/set accessors are corrected uniformly
//! at the lowering source. TS7008 is unaffected (raised separately by the
//! checker from the annotation node).
//!
//! Binder names are varied per case so nothing keys on a specific identifier.

use tsz_checker::test_utils::{check_source_code_messages, check_source_non_strict_codes};

const TS2413: u32 = 2413; // '<x>' index type '<a>' is not assignable to '<y>' index type '<b>'.

/// A set-accessor with no parameter annotation is implicit `any` the same way.
#[test]
fn inherited_index_conflict_implicit_any_setter_reports_ts2413() {
    let codes = check_source_non_strict_codes(
        r#"
interface Sa { [s: string]: { set m(v); }; }
interface Sd { [s: number]: {}; }
interface Se extends Sa, Sd {}
"#,
    );
    assert!(
        codes.contains(&TS2413),
        "implicit-any setter parameter must not drop the inherited string index; expected TS2413, got: {codes:?}",
    );
}

/// A get-accessor with no return annotation inside a `string`-index value must
/// not drop the inherited index: the value is `{ m: any }`, and the empty
/// `number`-index value `{}` is not assignable to it, so TS2413 fires (the
/// accessor sibling of the `inheritedStringIndexersFromDifferentBaseTypes2`
/// property witness). An implicit-any getter must also behave identically to an
/// explicit `get m(): any`.
#[test]
fn implicit_any_getter_matches_explicit_any_getter() {
    let implicit = check_source_non_strict_codes(
        r#"
interface Ia { [s: string]: { get m(); }; }
interface Id { [s: number]: {}; }
interface Ie extends Ia, Id {}
"#,
    );
    let explicit = check_source_non_strict_codes(
        r#"
interface Ia { [s: string]: { get m(): any; }; }
interface Id { [s: number]: {}; }
interface Ie extends Ia, Id {}
"#,
    );
    assert!(
        implicit.contains(&TS2413),
        "implicit-any getter: expected TS2413, got: {implicit:?}"
    );
    assert!(
        explicit.contains(&TS2413),
        "explicit-any getter: expected TS2413, got: {explicit:?}"
    );
    assert_eq!(
        implicit, explicit,
        "implicit-any getter must produce the same diagnostics as an explicit `any` getter",
    );
}

/// The rendered TS2413 message must name the getter-typed member as `any`
/// (proving the return type is implicit `any`, not the dropped `error` sentinel).
#[test]
fn ts2413_message_renders_implicit_any_getter_as_any() {
    let diags = check_source_code_messages(
        r#"
interface Ba { [s: string]: { get m(); }; }
interface Bd { [s: number]: {}; }
interface Be extends Ba, Bd {}
"#,
    );
    let has_2413_any = diags
        .iter()
        .any(|(code, msg)| *code == TS2413 && msg.contains("any"));
    assert!(
        has_2413_any,
        "TS2413 message must render the implicit-any getter member as 'any', got: {diags:#?}",
    );
}

/// A get/set pair where both annotations are missing is still a single
/// implicit-`any` property; the inherited-index conflict must report TS2413.
#[test]
fn implicit_any_get_set_pair_reports_ts2413() {
    let codes = check_source_non_strict_codes(
        r#"
interface Pa { [s: string]: { get m(); set m(v); }; }
interface Pd { [s: number]: {}; }
interface Pe extends Pa, Pd {}
"#,
    );
    assert!(
        codes.contains(&TS2413),
        "implicit-any get/set pair must report TS2413, got: {codes:?}",
    );
}

/// An implicit-any getter is `any`, not `error`: reading it and assigning to a
/// concrete type stays clean, and no spurious diagnostics are introduced.
#[test]
fn implicit_any_getter_type_is_any_stays_clean() {
    let codes = check_source_non_strict_codes(
        r#"
interface Ha { get v(); }
declare const h: Ha;
const n: number = h.v;
"#,
    );
    assert!(
        codes.is_empty(),
        "implicit-any getter must read as `any` without diagnostics, got: {codes:?}",
    );
}
