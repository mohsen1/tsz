//! The `noImplicitAny` class-member family: the missing get-accessor arm
//! (`TS7033`) and the surface rule that decides when the family is suppressed.
//!
//! Structural rule, one sentence per half:
//!
//! 1. When a `get` accessor has no body and no return-type annotation, `tsc`
//!    reports `TS7033` — the accessor analogue of the bodyless-method `TS7010`
//!    arm — unless a paired `set` accessor's annotated parameter supplies the
//!    type (`isGetAccessorWithAnnotatedSetAccessor`). tsz does this through
//!    `state_checking_members/ambient_signature_checks.rs`
//!    (`check_accessor_declaration_with_request`).
//! 2. When a class member is hidden from the observable surface of an ambient
//!    declaration — `private`, or named by a private identifier, *and* inside a
//!    `declare class` or a `.d.ts` — `tsc` reports none of the family for it.
//!    tsz does this through `member_hidden_from_ambient_declaration_surface`.
//!
//! Neither condition of the conjunction in (2) suppresses on its own: an
//! ordinary-named ambient member still reports, and a private-identifier member
//! of a *non-ambient* class still reports. Both directions are pinned below,
//! because a guard keyed on either condition alone passes half of this file.
//!
//! Every expectation here was recorded from `typescript@7.0.2` under
//! `--noEmit --strict --lib es2022 --target es2022`.

use tsz_checker::test_utils::check_source_strict_codes;

const TS7008: u32 = 7008; // Member implicitly has an 'any' type.
const TS7010: u32 = 7010; // Lacks return-type annotation, implicitly 'any' return.
const TS7033: u32 = 7033; // Property implicitly 'any', get accessor lacks return type.

fn has(codes: &[u32], code: u32) -> bool {
    codes.contains(&code)
}

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

// ---------------------------------------------------------------------------
// (1) TS7033 — the arm that did not exist. Issue #16179.
// ---------------------------------------------------------------------------

#[test]
fn ambient_class_bodyless_getter_reports_ts7033() {
    let codes = check_source_strict_codes("declare class J { get g(); }");
    assert!(has(&codes, TS7033), "expected TS7033: {codes:?}");
}

#[test]
fn ambient_class_annotated_getter_is_clean() {
    let codes = check_source_strict_codes("declare class J { get g(): number; }");
    assert!(
        !has(&codes, TS7033),
        "annotation supplies the type: {codes:?}"
    );
}

#[test]
fn abstract_getter_without_annotation_reports_ts7033() {
    // A bodyless getter is legal outside an ambient context too.
    let codes = check_source_strict_codes("abstract class A { abstract get g(); }");
    assert!(has(&codes, TS7033), "expected TS7033: {codes:?}");
}

#[test]
fn abstract_annotated_getter_is_clean() {
    let codes = check_source_strict_codes("abstract class A { abstract get g(): number; }");
    assert!(
        !has(&codes, TS7033),
        "annotation supplies the type: {codes:?}"
    );
}

#[test]
fn ambient_static_bodyless_getter_reports_ts7033() {
    let codes = check_source_strict_codes("declare class J { static get g(); }");
    assert!(
        has(&codes, TS7033),
        "static is not part of the rule: {codes:?}"
    );
}

#[test]
fn getter_paired_with_annotated_setter_is_clean() {
    // The exemption the bodyless-method TS7010 arm does not need, and the one
    // that matters most in practice: `get g(); set g(v: T);` is ordinary in real
    // declaration files, and tsc reports nothing for it.
    let codes = check_source_strict_codes("declare class J { get g(); set g(v: number); }");
    assert!(
        !has(&codes, TS7033),
        "paired annotated setter supplies the getter's type: {codes:?}"
    );
}

#[test]
fn setter_without_paired_getter_never_reports_ts7033() {
    // TS7033 is a *get* accessor diagnostic; a lone setter is not in scope.
    let codes = check_source_strict_codes("declare class J { set s(v: number); }");
    assert!(!has(&codes, TS7033), "setter-only: {codes:?}");
}

#[test]
fn ambient_class_reports_all_three_family_codes_together() {
    // tsc: TS7008 (x), TS7010 (m), TS7033 (g) — the three arms are independent
    // and co-emit on one class.
    let codes = check_source_strict_codes("declare class D { x; m(); get g(); }");
    assert!(has(&codes, TS7008), "expected TS7008: {codes:?}");
    assert!(has(&codes, TS7010), "expected TS7010: {codes:?}");
    assert!(has(&codes, TS7033), "expected TS7033: {codes:?}");
}

#[test]
fn ts7033_is_reported_once_per_getter() {
    let codes = check_source_strict_codes("declare class D { get g(); }");
    assert_eq!(count(&codes, TS7033), 1, "exactly one TS7033: {codes:?}");
}

// ---------------------------------------------------------------------------
// (2) The surface rule. Issue #16178 — TS7010 fired where tsc is silent.
// ---------------------------------------------------------------------------

#[test]
fn ambient_private_named_method_is_clean() {
    // The #16178 witness.
    let codes = check_source_strict_codes("declare class C { #m(); }");
    assert!(!has(&codes, TS7010), "hidden from the surface: {codes:?}");
}

#[test]
fn ambient_private_named_property_is_clean() {
    let codes = check_source_strict_codes("declare class C { #x; }");
    assert!(!has(&codes, TS7008), "hidden from the surface: {codes:?}");
}

#[test]
fn ambient_private_named_getter_is_clean() {
    // The new TS7033 arm must respect the same surface rule it was added under,
    // or fixing #16179 would have re-opened #16178 through a different code.
    let codes = check_source_strict_codes("declare class C { get #g(); }");
    assert!(!has(&codes, TS7033), "hidden from the surface: {codes:?}");
}

#[test]
fn ambient_static_private_named_method_is_clean() {
    let codes = check_source_strict_codes("declare class C { static #m(); }");
    assert!(!has(&codes, TS7010), "hidden from the surface: {codes:?}");
}

#[test]
fn ambient_private_modifier_members_stay_clean() {
    // Pre-existing behavior, kept as a control: the `private` keyword is the
    // other way a member leaves the surface, and it must keep working.
    let codes = check_source_strict_codes("declare class F { private m(); private get g(); }");
    assert!(!has(&codes, TS7010), "private keyword: {codes:?}");
    assert!(!has(&codes, TS7033), "private keyword: {codes:?}");
}

#[test]
fn ambient_whole_private_named_class_is_clean() {
    // Renamed binders throughout — no identifier string drives the rule.
    let codes = check_source_strict_codes("declare class Zebra { #alpha; #beta(); get #gamma(); }");
    assert!(!has(&codes, TS7008), "renamed binders: {codes:?}");
    assert!(!has(&codes, TS7010), "renamed binders: {codes:?}");
    assert!(!has(&codes, TS7033), "renamed binders: {codes:?}");
}

// --- negative controls: neither half of the conjunction suppresses alone -----

#[test]
fn non_ambient_private_named_method_still_reports_ts7010() {
    // A `#m()` outside an ambient context is NOT hidden — its implicit `any`
    // still affects the enclosing class body's inferred type. A guard keyed on
    // the private-identifier name alone would wrongly silence this.
    let codes = check_source_strict_codes("class E { #m(); }");
    assert!(has(&codes, TS7010), "non-ambient private name: {codes:?}");
}

#[test]
fn non_ambient_abstract_private_named_method_still_reports_ts7010() {
    let codes = check_source_strict_codes("abstract class G { abstract #m(); }");
    assert!(has(&codes, TS7010), "non-ambient private name: {codes:?}");
}

#[test]
fn ambient_ordinary_named_members_still_report() {
    // An ambient declaration *is* the public API for ordinary names. A guard
    // keyed on the ambient context alone would wrongly silence all of these.
    let codes = check_source_strict_codes("declare class D { m(); }");
    assert!(has(&codes, TS7010), "ordinary ambient name: {codes:?}");
    let codes = check_source_strict_codes("declare class D { x; }");
    assert!(has(&codes, TS7008), "ordinary ambient name: {codes:?}");
    let codes = check_source_strict_codes("declare class D { get g(); }");
    assert!(has(&codes, TS7033), "ordinary ambient name: {codes:?}");
}

#[test]
fn ambient_private_named_annotated_members_are_clean_either_way() {
    // Annotated hidden members are clean for two independent reasons; pinned so
    // a later change to either reason does not silently start reporting.
    let codes = check_source_strict_codes("declare class C { get #g(): number; #m(): void; }");
    assert!(!has(&codes, TS7033), "annotated + hidden: {codes:?}");
    assert!(!has(&codes, TS7010), "annotated + hidden: {codes:?}");
}
