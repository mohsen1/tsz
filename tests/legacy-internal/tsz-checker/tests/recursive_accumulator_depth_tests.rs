//! Regression tests for spurious `TS2589` on *terminating* recursive
//! accumulator type aliases (issue #10875).
//!
//! Structural rule: a tail-recursive conditional alias that grows an
//! accumulator by a bounded amount per step and stops at a concrete bound —
//! e.g.
//! `type BuildTuple<L extends number, T extends any[] = []> =
//!    T['length'] extends L ? T : BuildTuple<L, [...T, any]>` — terminates and
//! must not be flagged as excessively deep. `tsc` evaluates such an alias up to
//! its tail-recursion limit (1000 instantiations): `BuildTuple<999>` is clean,
//! `BuildTuple<1000>` reports TS2589. The divergent-growth detector that powers
//! tsz's use-site TS2589 probe must use that same 1000-step ceiling rather than
//! firing on the first couple dozen growth steps, so terminating accumulators
//! evaluate fully while genuinely unbounded growth is still reported at the
//! boundary `tsc` uses.
//!
//! Anti-hardcoding: the rule is exercised with renamed binders, a different
//! per-step growth increment, and a non-tuple (numeric counter) accumulator so
//! the fix cannot be satisfied by matching a specific identifier or body shape.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn strict_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

const TS2589: u32 = 2589;

const BUILD_TUPLE: &str = r#"
type BuildTuple<L extends number, T extends any[] = []> =
  T['length'] extends L ? T : BuildTuple<L, [...T, any]>;
"#;

/// Core repro: a terminating accumulator that needs far more than a couple
/// dozen steps (40) must not emit TS2589 — `tsc` accepts it.
#[test]
fn terminating_build_tuple_deep_is_not_excessively_deep() {
    let source = format!(
        r#"
{BUILD_TUPLE}
type R = BuildTuple<40>;
declare const r: R;
const n: number = r.length;
"#
    );
    let codes = strict_codes(&source);
    assert!(
        !codes.contains(&TS2589),
        "BuildTuple<40> terminates; TS2589 is a false positive. Got: {codes:?}"
    );
}

/// Just under tsc's tail-recursion limit: still clean.
#[test]
fn terminating_build_tuple_at_999_is_clean() {
    let source = format!(
        r#"
{BUILD_TUPLE}
type R = BuildTuple<999>;
declare const r: R;
const n: number = r.length;
"#
    );
    let codes = strict_codes(&source);
    assert!(
        codes.is_empty(),
        "BuildTuple<999> matches tsc (clean). Got: {codes:?}"
    );
}

/// At/over the 1000-iteration limit, divergence detection must still fire —
/// matching tsc, which reports TS2589 for `BuildTuple<1000>`.
#[test]
fn build_tuple_at_limit_still_reports_ts2589() {
    let source = format!(
        r#"
{BUILD_TUPLE}
type R = BuildTuple<1000>;
declare const r: R;
"#
    );
    let codes = strict_codes(&source);
    assert!(
        codes.contains(&TS2589),
        "BuildTuple<1000> reaches the tail-recursion limit; TS2589 expected. Got: {codes:?}"
    );
}

/// A genuinely non-terminating accumulator (the length check can never match
/// because the accumulator grows two elements per step against an odd bound)
/// must still report TS2589.
#[test]
fn non_terminating_accumulator_reports_ts2589() {
    let codes = strict_codes(
        r#"
type Never<L extends number, T extends any[] = []> =
  T['length'] extends L ? T : Never<L, [...T, any, any]>;
type R = Never<3>;
declare const r: R;
"#,
    );
    assert!(
        codes.contains(&TS2589),
        "Never<3> never converges; TS2589 expected. Got: {codes:?}"
    );
}

/// Anti-hardcoding: renamed binders (`L`/`T` -> `Len`/`Acc`) must behave
/// identically.
#[test]
fn renamed_binders_terminating_accumulator_is_clean() {
    let codes = strict_codes(
        r#"
type Build<Len extends number, Acc extends any[] = []> =
  Acc['length'] extends Len ? Acc : Build<Len, [...Acc, any]>;
type R = Build<60>;
declare const r: R;
const n: number = r.length;
"#,
    );
    assert!(
        !codes.contains(&TS2589),
        "Renamed terminating accumulator must not emit TS2589. Got: {codes:?}"
    );
}

/// Anti-hardcoding: a non-tuple accumulator (numeric-string length counter via
/// a recursive defaulted alias) that terminates must also be accepted.
#[test]
fn terminating_string_accumulator_is_clean() {
    let codes = strict_codes(
        r#"
type Repeat<N extends number, S extends string = "", C extends any[] = []> =
  C['length'] extends N ? S : Repeat<N, `${S}x`, [...C, 1]>;
type R = Repeat<50>;
const r: R = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
"#,
    );
    assert!(
        !codes.contains(&TS2589),
        "Terminating string accumulator must not emit TS2589. Got: {codes:?}"
    );
}
