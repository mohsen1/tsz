//! Tests for TS1501: Regular expression flag target validation.
//!
//! tsc gates `s` (dotAll) on ES2018, `d` (hasIndices) on ES2022, and `v`
//! (unicodeSets) on ES2024; `u` and `y` require only ES2015 and never fire
//! on a reachable target. Every row below is pinned against the
//! `typescript@7.0.2` oracle.

use tsz_checker::context::CheckerOptions;
use tsz_common::common::ScriptTarget;

fn diagnostics_for(source: &str, target: ScriptTarget) -> Vec<tsz_common::diagnostics::Diagnostic> {
    tsz_checker::test_utils::check_with_options(
        source,
        CheckerOptions {
            target,
            ..Default::default()
        },
    )
}

fn has_ts1501(source: &str, target: ScriptTarget) -> bool {
    diagnostics_for(source, target)
        .iter()
        .any(|d| d.code == 1501)
}

fn ts1501_diagnostic(source: &str, target: ScriptTarget) -> tsz_common::diagnostics::Diagnostic {
    diagnostics_for(source, target)
        .into_iter()
        .find(|d| d.code == 1501)
        .expect("expected TS1501 diagnostic")
}

fn ts1501_diagnostics(
    source: &str,
    target: ScriptTarget,
) -> Vec<tsz_common::diagnostics::Diagnostic> {
    diagnostics_for(source, target)
        .into_iter()
        .filter(|d| d.code == 1501)
        .collect()
}

#[test]
fn v_flag_requires_es2024_or_later() {
    assert!(has_ts1501("var x = /foo/v;", ScriptTarget::ES5));
    assert!(has_ts1501("var x = /foo/v;", ScriptTarget::ES2022));
    assert!(has_ts1501("var x = /foo/v;", ScriptTarget::ES2023));
    assert!(has_ts1501("const r = /[a&&b]/v;", ScriptTarget::ES2022));
}

#[test]
fn v_flag_ts1501_points_to_flag() {
    let diagnostic = ts1501_diagnostic("const r = /[a&&b]/v;", ScriptTarget::ES2022);

    assert_eq!(diagnostic.start, 18);
    assert_eq!(diagnostic.length, 1);
    assert_eq!(
        diagnostic.message_text,
        "This regular expression flag is only available when targeting 'es2024' or later."
    );
}

#[test]
fn v_flag_is_allowed_for_es2024_or_later() {
    assert!(!has_ts1501("var x = /foo/v;", ScriptTarget::ES2024));
    assert!(!has_ts1501("var x = /foo/v;", ScriptTarget::ES2025));
    assert!(!has_ts1501("var x = /foo/v;", ScriptTarget::ESNext));
}

#[test]
fn ts1501_not_emitted_for_flags_requiring_only_es2015() {
    assert!(!has_ts1501("var x = /foo/u;", ScriptTarget::ES2015));
    assert!(!has_ts1501("var x = /foo/y;", ScriptTarget::ES2015));
    assert!(!has_ts1501("var x = /foo/gim;", ScriptTarget::ES2015));
    assert!(!has_ts1501("var x = /foo/uy;", ScriptTarget::ES2015));
}

#[test]
fn s_flag_requires_es2018_or_later() {
    assert!(has_ts1501("var x = /foo/s;", ScriptTarget::ES2015));
    assert!(has_ts1501("var x = /foo/s;", ScriptTarget::ES2017));
    assert!(!has_ts1501("var x = /foo/s;", ScriptTarget::ES2018));
    assert!(!has_ts1501("var x = /foo/s;", ScriptTarget::ES2022));
}

#[test]
fn s_flag_ts1501_points_to_flag() {
    let diagnostic = ts1501_diagnostic("var a = /foo/s;", ScriptTarget::ES2015);

    assert_eq!(diagnostic.start, 13);
    assert_eq!(diagnostic.length, 1);
    assert_eq!(
        diagnostic.message_text,
        "This regular expression flag is only available when targeting 'es2018' or later."
    );
}

#[test]
fn d_flag_requires_es2022_or_later() {
    assert!(has_ts1501("var x = /foo/d;", ScriptTarget::ES2018));
    assert!(has_ts1501("var x = /foo/d;", ScriptTarget::ES2021));
    assert!(!has_ts1501("var x = /foo/d;", ScriptTarget::ES2022));
    assert!(!has_ts1501("var x = /foo/d;", ScriptTarget::ESNext));
}

#[test]
fn d_flag_ts1501_points_to_flag() {
    let diagnostic = ts1501_diagnostic("var a = /foo/d;", ScriptTarget::ES2018);

    assert_eq!(diagnostic.start, 13);
    assert_eq!(diagnostic.length, 1);
    assert_eq!(
        diagnostic.message_text,
        "This regular expression flag is only available when targeting 'es2022' or later."
    );
}

#[test]
fn v_flag_that_loses_the_uv_conflict_check_does_not_also_report_ts1501() {
    // `u` is accepted first, so `v` immediately conflicts (TS1502, scanner-
    // side) and never reaches tsc's `checkRegularExpressionFlagAvailability`
    // — the TS1502 diagnostic itself is asserted in
    // `tsz-core/tests/regex_flag_tests.rs`.
    assert!(!has_ts1501("const r = /x/uv;", ScriptTarget::ES2022));
    assert!(!has_ts1501("const r = /x/uv;", ScriptTarget::ESNext));
}

#[test]
fn v_flag_accepted_before_a_later_conflicting_u_still_reports_ts1501() {
    // `v` is accepted first (no `u` seen yet), so it still gets the
    // availability check; the later `u` is what conflicts (TS1502).
    assert!(has_ts1501("const r = /x/vu;", ScriptTarget::ES2022));
    let diagnostic = ts1501_diagnostic("const r = /x/vu;", ScriptTarget::ES2022);
    assert_eq!(diagnostic.start, 13);
    assert_eq!(
        diagnostic.message_text,
        "This regular expression flag is only available when targeting 'es2024' or later."
    );
}

#[test]
fn duplicate_v_flag_second_occurrence_does_not_also_report_ts1501() {
    // The first `v` is accepted (and gated); the second is a plain
    // duplicate (TS1500, no `u` involved) and does not re-check availability.
    assert_eq!(
        ts1501_diagnostics("const r = /x/vv;", ScriptTarget::ES2022).len(),
        1
    );
}

#[test]
fn multiple_offending_flags_each_report_at_their_own_position_in_source_order() {
    let diagnostics = ts1501_diagnostics("var a = /foo/dsv;", ScriptTarget::ES2015);

    assert_eq!(diagnostics.len(), 3);
    assert_eq!(diagnostics[0].start, 13);
    assert_eq!(
        diagnostics[0].message_text,
        "This regular expression flag is only available when targeting 'es2022' or later."
    );
    assert_eq!(diagnostics[1].start, 14);
    assert_eq!(
        diagnostics[1].message_text,
        "This regular expression flag is only available when targeting 'es2018' or later."
    );
    assert_eq!(diagnostics[2].start, 15);
    assert_eq!(
        diagnostics[2].message_text,
        "This regular expression flag is only available when targeting 'es2024' or later."
    );
}
