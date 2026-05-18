//! Integration tests for the typed checker recovery API (#8273).
//!
//! These tests check the full parse → bind → check pipeline and assert that
//! the [`tsz_checker::recovery::RecoverySites`] registry on the checker
//! context is populated for the migrated dispatch fallback families and is
//! NOT populated for legitimate `: any` declarations or other code that
//! reaches `TypeId::ANY` through normal type evaluation. They are the
//! integration counterpart to the unit tests in
//! `crates/tsz-checker/src/recovery.rs`.

use tsz_checker::test_utils::check_source_recovery_sites;
use tsz_checker::{CheckerOptions, RecoveryReason};

fn check_sites(source: &str) -> Vec<(u32, RecoveryReason)> {
    check_source_recovery_sites(source, "test.ts", CheckerOptions::default())
}

#[test]
fn real_declared_any_annotation_is_not_recorded_as_recovery() {
    // A user-written `: any` annotation must not appear in the recovery
    // registry. Real `any` semantics are distinct from recovery fallbacks
    // even though both produce TypeId::ANY in the type universe.
    let sites = check_sites(
        r#"
        const a: any = 1;
        const b: any = "two";
        function f(x: any): any { return x; }
        "#,
    );
    assert!(
        sites.is_empty(),
        "expected zero recovery sites for declared `: any`; got {sites:?}"
    );
}

#[test]
fn yield_outside_generator_records_typed_recovery() {
    // `yield` in a non-generator function — TS1163 is emitted by the parser
    // and the checker recovers to TypeId::ANY. The recovery site MUST be
    // recorded with the typed reason, not just traced as a free-form string.
    let sites = check_sites(
        r#"
        function notGenerator() {
            const v = yield 1;
        }
        "#,
    );
    let count = sites
        .iter()
        .filter(|(_, r)| *r == RecoveryReason::YieldOutsideGenerator)
        .count();
    assert!(
        count >= 1,
        "expected at least one YieldOutsideGenerator recovery; got {sites:?}"
    );
}

#[test]
fn yield_outside_generator_records_regardless_of_binding_name() {
    // The reported repro uses `const v = yield 1`. The same recovery rule
    // must fire for renamed bindings; otherwise the migration would be
    // accidentally keyed on the source spelling rather than on the
    // structural fallback condition.
    let sites_v = check_sites(
        r#"
        function notGenerator1() {
            const v = yield 1;
        }
        "#,
    );
    let sites_renamed = check_sites(
        r#"
        function notGenerator2() {
            const longerName = yield 1;
        }
        "#,
    );
    let count_for = |sites: &[(u32, RecoveryReason)]| {
        sites
            .iter()
            .filter(|(_, r)| *r == RecoveryReason::YieldOutsideGenerator)
            .count()
    };
    assert!(count_for(&sites_v) >= 1);
    assert!(count_for(&sites_renamed) >= 1);
}

#[test]
fn yield_inside_generator_with_typed_next_does_not_record_recovery() {
    // A yield inside a real generator with a fully resolved `next` type
    // must NOT register as recovery. This is the negative counterpart that
    // confirms the registry isn't being indiscriminately populated by any
    // yield expression.
    let sites = check_sites(
        r#"
        function* gen(): Generator<number, void, number> {
            const n = yield 1;
        }
        "#,
    );
    let recovery_yield = sites
        .iter()
        .any(|(_, r)| matches!(r, RecoveryReason::YieldOutsideGenerator));
    assert!(
        !recovery_yield,
        "yield inside a generator with a known next-type must not record YieldOutsideGenerator; got {sites:?}"
    );
}

#[test]
fn empty_source_records_no_recovery_sites() {
    // Baseline: a program with no recovery-triggering expressions must
    // leave the registry empty. Guards against accidental seeding from
    // checker startup paths.
    let sites = check_sites("");
    assert!(sites.is_empty(), "expected empty registry; got {sites:?}");
}
