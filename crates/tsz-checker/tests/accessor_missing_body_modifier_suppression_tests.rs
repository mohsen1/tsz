//! A class get/set accessor's missing-`{`-body check
//! (`check_accessor_declaration_with_request` in
//! `crates/tsz-checker/src/state/state_checking_members/ambient_signature_checks.rs`)
//! must not pile a `'{' expected` (TS1005) diagnostic on top of a member
//! whose modifier list already carries its own grammar violation —
//! `readonly` (TS1024), `in`/`out` (TS1274), or a duplicate `accessor`
//! (TS1275). tsc's `checkGrammarModifiers(node) || checkGrammarAccessor(node)`
//! OR-chain reports only the first problem and short-circuits the rest; tsz's
//! checker previously ran the accessor-body check unconditionally, so these
//! members got a spurious second diagnostic (#17062 item 3).
//!
//! `declare` (TS1031) already gets this "for free" via
//! `NodeArena::is_in_ambient_context`, which treats any node carrying a
//! `declare` modifier as ambient regardless of whether that placement is
//! itself valid — no test needed for that shape here, it's covered by the
//! plain bodyless-accessor baseline already matching tsc.

use tsz_checker::test_utils::check_source_with_parse_health;

const TS1005_EXPECTED: u32 = 1005;
const TS1024_READONLY_MODIFIER: u32 = 1024;
const TS1274_VARIANCE_MODIFIER: u32 = 1274;
const TS1275_ACCESSOR_MODIFIER: u32 = 1275;

/// Combined parser + checker diagnostic codes. TS1275 (duplicate `accessor`
/// modifier) is parser-emitted; TS1005/TS1024/TS1274 here are all
/// checker-emitted — `check_source_diagnostics` alone only sees the latter.
fn codes(source: &str) -> Vec<u32> {
    let (mut parser_codes, checker_codes) = check_source_with_parse_health(source);
    parser_codes.extend(checker_codes);
    parser_codes.sort_unstable();
    parser_codes
}

// Baseline: a plain bodyless getter with no other modifier problem keeps its
// TS1005 — this must NOT regress to being silently dropped.
#[test]
fn plain_bodyless_getter_reports_ts1005() {
    let codes = codes("class C { get x(): number; }");
    assert_eq!(codes, vec![TS1005_EXPECTED], "got {codes:?}");
}

#[test]
fn plain_bodyless_setter_reports_ts1005() {
    let codes = codes("class C { set x(v: number) {} get x(): number; }");
    assert!(codes.contains(&TS1005_EXPECTED), "got {codes:?}");
}

// `readonly` on a getter: TS1024 only, no TS1005.
#[test]
fn readonly_bodyless_getter_reports_only_ts1024() {
    let codes = codes("class C { readonly get x(): number; }");
    assert_eq!(codes, vec![TS1024_READONLY_MODIFIER], "got {codes:?}");
}

#[test]
fn static_readonly_bodyless_getter_reports_only_ts1024() {
    let codes = codes("class C { static readonly get x(): number; }");
    assert_eq!(codes, vec![TS1024_READONLY_MODIFIER], "got {codes:?}");
}

// `in`/`out` on a class member accessor: TS1274 only, no TS1005.
#[test]
fn in_modifier_bodyless_getter_reports_only_ts1274() {
    let codes = codes("class C { in get x(): number; }");
    assert_eq!(codes, vec![TS1274_VARIANCE_MODIFIER], "got {codes:?}");
}

#[test]
fn out_modifier_bodyless_setter_reports_only_ts1274() {
    let codes = codes("class C { out set x(v: number); }");
    assert_eq!(codes, vec![TS1274_VARIANCE_MODIFIER], "got {codes:?}");
}

// Renamed accessor/property binder — the suppression is structural (reads
// the modifier list), not keyed to a specific identifier.
#[test]
fn in_modifier_bodyless_getter_renamed_binder_reports_only_ts1274() {
    let codes = codes("class Widget { in get value(): string; }");
    assert_eq!(codes, vec![TS1274_VARIANCE_MODIFIER], "got {codes:?}");
}

// A duplicate `accessor` modifier on a get/set accessor: TS1275 only, no
// TS1005.
#[test]
fn accessor_modifier_on_getter_reports_only_ts1275() {
    let codes = codes("class C { accessor get x(): number; }");
    assert_eq!(codes, vec![TS1275_ACCESSOR_MODIFIER], "got {codes:?}");
}

// Negative control: `abstract` legitimately allows a bodyless accessor with
// no diagnostic at all — must stay clean, not accidentally suppressed-then-
// re-triggered by the new modifier scan.
#[test]
fn abstract_bodyless_getter_in_abstract_class_stays_clean() {
    let codes = codes("abstract class C { abstract get x(): number; }");
    assert!(codes.is_empty(), "got {codes:?}");
}
