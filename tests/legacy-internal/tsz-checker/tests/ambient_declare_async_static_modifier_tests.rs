//! `declare`/`async`/`static` interaction on a class method (issue #16291
//! follow-up, flagged twice on #16838's own board comments): a third modifier
//! joining an already-conflicting `declare`/`async` pair previously produced
//! a spurious extra diagnostic instead of tsc's single one.
//!
//! tsc's `checkGrammarModifiers` walks a member's modifiers in SOURCE ORDER
//! and reports exactly the FIRST problem found, then stops — so which single
//! code wins depends entirely on which of these two conflicts is reached
//! first while scanning left to right:
//! - a `static`/`async` ordering violation (TS1029, `static` scanned after
//!   `async` was already seen), or
//! - a `declare`+`async` ambient conflict (TS1040, `async` and `declare`
//!   co-occur in either order) or, when `declare` is the very first modifier
//!   scanned (so no ordering/ambient conflict precedes it), the
//!   declare-invalid-on-a-method check (TS1031).
//!
//! All 6 permutations of `{declare, async, static}` are pinned against
//! `typescript@7.0.2` (`--noEmit --strict --target es2022 --lib es2022`).

use crate::test_utils::check_source_codes_with_parse_health;

const TS1029: u32 = 1029; // '{0}' modifier must precede '{1}' modifier.
const TS1031: u32 = 1031; // 'declare' modifier cannot appear on class elements of this kind.
const TS1040: u32 = 1040; // 'async' modifier cannot be used in an ambient context.

/// Grammar codes this suite is about, filtered so assertions stay immune to
/// unrelated harness noise (e.g. a no-lib `Promise` return type also draws
/// TS1064/TS2583 that the real CLI, with the lib present, never emits).
const GRAMMAR_CODES: [u32; 3] = [TS1029, TS1031, TS1040];

fn codes(source: &str) -> Vec<u32> {
    let mut v: Vec<u32> = check_source_codes_with_parse_health(source)
        .into_iter()
        .filter(|c| GRAMMAR_CODES.contains(c))
        .collect();
    v.sort_unstable();
    v
}

// --- all 6 permutations of declare/async/static on a method ----------------

#[test]
fn declare_async_static_reports_ts1031_only() {
    // `declare` is scanned first with no prior conflict, so the
    // declare-invalid-on-a-method check wins before `async`/`static` are
    // even reached.
    assert_eq!(
        codes("class C { declare async static m(): Promise<void>; }"),
        vec![TS1031]
    );
}

#[test]
fn declare_static_async_reports_ts1031_only() {
    assert_eq!(
        codes("class C { declare static async m(): Promise<void>; }"),
        vec![TS1031]
    );
}

#[test]
fn async_declare_static_reports_ts1040_only() {
    // `async` then `declare`: the ambient conflict is known as soon as
    // `declare` is reached, before the trailing `static` is scanned.
    assert_eq!(
        codes("class C { async declare static m(): Promise<void>; }"),
        vec![TS1040]
    );
}

#[test]
fn async_static_declare_reports_ts1029_only() {
    // `static` scanned right after `async` reports the ordering violation
    // first; the walk stops there, so the trailing `declare` never gets its
    // own ambient-conflict diagnostic.
    assert_eq!(
        codes("class C { async static declare m(): Promise<void>; }"),
        vec![TS1029]
    );
}

#[test]
fn static_declare_async_reports_ts1031_only() {
    assert_eq!(
        codes("class C { static declare async m(): Promise<void>; }"),
        vec![TS1031]
    );
}

#[test]
fn static_async_declare_reports_ts1040_only() {
    assert_eq!(
        codes("class C { static async declare m(): Promise<void>; }"),
        vec![TS1040]
    );
}

// --- adjacent controls: unaffected by this change ---------------------------

#[test]
fn async_static_without_declare_still_reports_ts1029() {
    // No `declare` at all: the ordinary static/async ordering check is
    // untouched.
    assert_eq!(
        codes("class C { async static m(): Promise<void> {} }"),
        vec![TS1029]
    );
}

#[test]
fn declare_readonly_static_property_still_reports_ts1029() {
    // `declare` present but the member is a legal property (no `async`): the
    // static/readonly ordering violation is unrelated to the async/declare
    // suppression and must still fire.
    assert_eq!(
        codes("class C { declare readonly static p: number; }"),
        vec![TS1029]
    );
}

#[test]
fn readonly_static_declare_property_still_reports_ts1029() {
    assert_eq!(
        codes("class C { readonly static declare p: number; }"),
        vec![TS1029]
    );
}

#[test]
fn plain_declare_async_method_reports_ts1031_only() {
    // Sibling #16838 pair (no `static`), regression guard.
    assert_eq!(
        codes("class C { declare async m(): Promise<void>; }"),
        vec![TS1031]
    );
}

#[test]
fn plain_async_declare_method_reports_ts1040_only() {
    assert_eq!(
        codes("class C { async declare m(): Promise<void>; }"),
        vec![TS1040]
    );
}
