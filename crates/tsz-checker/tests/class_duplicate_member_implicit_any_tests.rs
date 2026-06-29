//! Implicit-`any` (`TS7008`) reporting for duplicate, un-annotated class members.
//!
//! Structural rule: `tsc` computes a member's implicit-`any` error once per
//! member *symbol*, anchored at the symbol's first declaration. When a class
//! declares the same member name twice without a type annotation or
//! initializer, `tsc` emits `TS7008` once (on the first declaration) plus
//! `TS2300` (duplicate identifier) on the redeclaration — never a second
//! `TS7008`. tsz previously ran the per-declaration implicit-any check on every
//! declaration node sharing the name, emitting an extra `TS7008` on the
//! redeclaration. Owner: `state_checking_members/ambient_signature_checks.rs`
//! (`check_property_declaration_with_request`), gated by
//! `member_redeclares_earlier_property`.
//!
//! Static and instance members live in separate namespaces, so `static x` and
//! instance `x` are not duplicates and each keeps its own `TS7008`. These tests
//! vary the class/member binder names to confirm no identifier string drives
//! the logic.

use tsz_checker::test_utils::check_source_strict_codes;

const TS7008: u32 = 7008; // Member implicitly has an 'any' type.
const TS2300: u32 = 2300; // Duplicate identifier.

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

#[test]
fn duplicate_private_untyped_member_emits_ts7008_once() {
    // Witness from #14842: `#x; #x;` — one TS7008 on the first decl, one TS2300
    // on the redeclaration (no second TS7008).
    let codes = check_source_strict_codes("class C { #x; #x; }");
    assert_eq!(
        count(&codes, TS7008),
        1,
        "TS7008 once per symbol: {codes:?}"
    );
    assert_eq!(
        count(&codes, TS2300),
        1,
        "duplicate still flagged: {codes:?}"
    );
}

#[test]
fn duplicate_public_untyped_member_emits_ts7008_once_renamed() {
    // Same shape, public field, different binder names: still one TS7008.
    let codes = check_source_strict_codes("class Widget { handle; handle; }");
    assert_eq!(count(&codes, TS7008), 1, "{codes:?}");
    assert_eq!(count(&codes, TS2300), 1, "{codes:?}");
}

#[test]
fn triple_untyped_member_emits_ts7008_once() {
    // Three declarations: TS7008 once, TS2300 on each redeclaration.
    let codes = check_source_strict_codes("class Box { slot; slot; slot; }");
    assert_eq!(count(&codes, TS7008), 1, "{codes:?}");
    assert_eq!(count(&codes, TS2300), 2, "{codes:?}");
}

#[test]
fn interleaved_distinct_names_each_get_one_ts7008() {
    // `p; q; p;`: distinct first declarations of `p` and `q` each emit TS7008;
    // the second `p` is the redeclaration (TS2300 only).
    let codes = check_source_strict_codes("class Grid { row; col; row; }");
    assert_eq!(count(&codes, TS7008), 2, "{codes:?}");
    assert_eq!(count(&codes, TS2300), 1, "{codes:?}");
}

#[test]
fn static_and_instance_same_name_are_not_duplicates() {
    // `static x; x;` are separate symbols: TS7008 on both, no TS2300.
    let codes = check_source_strict_codes("class Reg { static entry; entry; }");
    assert_eq!(count(&codes, TS7008), 2, "{codes:?}");
    assert_eq!(count(&codes, TS2300), 0, "{codes:?}");
}

#[test]
fn duplicate_static_untyped_member_emits_ts7008_once() {
    // `static t; static t;` is a duplicate static symbol: TS7008 once + TS2300.
    let codes = check_source_strict_codes("class Pool { static node; static node; }");
    assert_eq!(count(&codes, TS7008), 1, "{codes:?}");
    assert_eq!(count(&codes, TS2300), 1, "{codes:?}");
}

#[test]
fn redeclaration_after_initialized_member_has_no_ts7008() {
    // First decl has an initializer (typed `number`), so the symbol is not
    // implicit-any; the un-annotated redeclaration must not get TS7008.
    let codes = check_source_strict_codes("class Acc { total = 1; total; }");
    assert_eq!(count(&codes, TS7008), 0, "{codes:?}");
    assert_eq!(count(&codes, TS2300), 1, "{codes:?}");
}

#[test]
fn redeclaration_after_typed_member_has_no_ts7008() {
    // First decl is typed; the un-annotated redeclaration must not get TS7008.
    let codes = check_source_strict_codes("class Pt { value: number; value; }");
    assert_eq!(count(&codes, TS7008), 0, "{codes:?}");
    assert_eq!(count(&codes, TS2300), 1, "{codes:?}");
}

#[test]
fn single_untyped_member_still_emits_ts7008() {
    // Fallback: a non-duplicate un-annotated field still gets exactly one
    // TS7008 (the suppression must not over-fire).
    let codes = check_source_strict_codes("class One { lone; }");
    assert_eq!(count(&codes, TS7008), 1, "{codes:?}");
    assert_eq!(count(&codes, TS2300), 0, "{codes:?}");
}

#[test]
fn distinct_untyped_members_each_emit_ts7008() {
    // Two distinct un-annotated fields each get their own TS7008, no TS2300.
    let codes = check_source_strict_codes("class Two { first; second; }");
    assert_eq!(count(&codes, TS7008), 2, "{codes:?}");
    assert_eq!(count(&codes, TS2300), 0, "{codes:?}");
}
