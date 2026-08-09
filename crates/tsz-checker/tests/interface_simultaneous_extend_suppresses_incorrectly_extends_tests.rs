//! Parity for tsc's `checkInterfaceDeclaration` gate between TS2320 and TS2430.
//!
//! tsc runs the per-base "incorrectly extends" (TS2430) assignability loop only
//! when `checkInheritedPropertiesAreIdentical` succeeds. When two bases of an
//! interface contribute a shared member with non-identical types, tsc reports
//! TS2320 ("cannot simultaneously extend types '{0}' and '{1}'") and skips the
//! TS2430 loop for that interface entirely — so a conflicting interface never
//! carries both codes.
//!
//! tsz's heritage-compatibility pass emits TS2430 eagerly while iterating bases,
//! so when a *later* base introduces the conflict, a TS2430 already reported
//! against an *earlier* base was not withheld. The checker now reconciles this to
//! tsc by dropping any TS2430 anchored at the same position as a TS2320. These
//! rows pin both directions: the conflict must suppress TS2430, and a genuine
//! incorrect-extends with no cross-base conflict must still report it.
//!
//! This is the structural root-cause fix that replaces the `complexRecursiveCollections`
//! fixture-text rewrite removed from `source_file.rs` (#14141).

use tsz_checker::test_utils::check_source_code_messages;

fn codes(source: &str) -> Vec<u32> {
    check_source_code_messages(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

fn count(source: &str, code: u32) -> usize {
    codes(source).iter().filter(|&&c| c == code).count()
}

// ---------------------------------------------------------------------------
// Positive: a cross-base conflict (TS2320) suppresses TS2430 for that interface,
// even though the derived interface's own member is incompatible with an
// earlier base (which is what makes tsz emit the eager TS2430).
// ---------------------------------------------------------------------------

#[test]
fn conflicting_bases_suppress_incorrectly_extends() {
    // `p` is shared by A and B with different types -> TS2320 on C.
    // `m` on C is incompatible with A.m -> tsz eagerly emits TS2430 (vs A) while
    // iterating base A, before the A/B conflict on `p` is discovered on base B.
    // tsc reports only TS2320 and skips the TS2430 loop; tsz now matches.
    let source = r#"
interface A { p: string; m: string; }
interface B { p: number; }
interface C extends A, B { m: boolean; }
"#;
    let cs = codes(source);
    assert!(
        cs.contains(&2320),
        "two conflicting bases must report TS2320, got: {cs:?}"
    );
    assert!(
        !cs.contains(&2430),
        "an interface with a TS2320 cross-base conflict must not also report \
         TS2430; got: {cs:?}"
    );
}

#[test]
fn conflicting_bases_suppress_incorrectly_extends_renamed_binders() {
    // Same shape, every binder renamed: the reconciliation is by diagnostic code
    // and anchor position, never by identifier text.
    let source = r#"
interface Alpha { field: string; method: string; }
interface Beta { field: number; }
interface Gamma extends Alpha, Beta { method: boolean; }
"#;
    let cs = codes(source);
    assert!(
        cs.contains(&2320),
        "renamed conflicting bases must still report TS2320, got: {cs:?}"
    );
    assert!(
        !cs.contains(&2430),
        "renamed conflicting-base interface must not report TS2430, got: {cs:?}"
    );
}

#[test]
fn conflicting_bases_without_own_incompatible_member_report_only_ts2320() {
    // No derived-vs-base incompatibility at all; only the cross-base `p` conflict.
    let source = r#"
interface A { p: string; }
interface B { p: number; }
interface C extends A, B { }
"#;
    let cs = codes(source);
    assert!(cs.contains(&2320), "expected TS2320, got: {cs:?}");
    assert!(!cs.contains(&2430), "expected no TS2430, got: {cs:?}");
}

#[test]
fn three_bases_two_conflicting_still_suppress_incorrectly_extends() {
    // A third, compatible base does not change the outcome: the A/B conflict on
    // `p` fires TS2320, and the eager TS2430 from C.m-vs-A.m is suppressed.
    let source = r#"
interface A { p: string; m: string; }
interface B { p: number; }
interface D { q: number; }
interface C extends A, D, B { m: boolean; }
"#;
    let cs = codes(source);
    assert!(cs.contains(&2320), "expected TS2320, got: {cs:?}");
    assert!(!cs.contains(&2430), "expected no TS2430, got: {cs:?}");
}

// ---------------------------------------------------------------------------
// Negative controls: with no cross-base conflict, a genuine incorrect-extends
// must still report TS2430 (the reconciliation must not over-suppress).
// ---------------------------------------------------------------------------

#[test]
fn single_base_incompatible_still_reports_ts2430() {
    let source = r#"
interface Base { m: string; }
interface Derived extends Base { m: boolean; }
"#;
    let cs = codes(source);
    assert!(
        cs.contains(&2430),
        "a single-base incompatible override must still report TS2430, got: {cs:?}"
    );
    assert!(
        !cs.contains(&2320),
        "a single base cannot produce a simultaneous-extend conflict, got: {cs:?}"
    );
}

#[test]
fn disjoint_bases_incompatible_override_still_reports_ts2430() {
    // X and Y share no member -> no TS2320. Z.a is incompatible with X.a, so the
    // TS2430 loop runs and must still fire.
    let source = r#"
interface X { a: string; }
interface Y { b: number; }
interface Z extends X, Y { a: boolean; }
"#;
    let cs = codes(source);
    assert!(
        cs.contains(&2430),
        "disjoint bases with an incompatible override must still report TS2430, \
         got: {cs:?}"
    );
    assert!(
        !cs.contains(&2320),
        "disjoint bases must not report a simultaneous-extend conflict, got: {cs:?}"
    );
}

#[test]
fn unrelated_conflicting_interface_does_not_suppress_another_interfaces_ts2430() {
    // One interface has a TS2320 conflict; a *different* interface genuinely
    // incorrectly extends its single base. The second must keep its TS2430 —
    // suppression is scoped to the conflicting interface's own anchor.
    let source = r#"
interface A { p: string; }
interface B { p: number; }
interface C extends A, B { }

interface Base { m: string; }
interface Derived extends Base { m: boolean; }
"#;
    assert_eq!(
        count(source, 2320),
        1,
        "exactly one TS2320 expected: {:?}",
        codes(source)
    );
    assert_eq!(
        count(source, 2430),
        1,
        "the unrelated Derived must keep its TS2430: {:?}",
        codes(source)
    );
}
