//! Witness matrix for optional-parameter declared-type recovery in the
//! killing-definition assignment path
//! (`FlowAnalyzer::fallback_declared_annotation_type`,
//! `crates/tsz-checker/src/flow/control_flow/assignment_fallback.rs`).
//!
//! Structural rule:
//!
//! > An optional parameter (`p?: T`) has declared type `T | undefined` inside
//! > the function body under `strictNullChecks` — the same widening
//! > `checkers/parameter_checker.rs` applies (via
//! > `optional_parameter_type_with_undefined`) when it first computes the
//! > parameter's type. When a killing-definition reassignment (`p = rhs;`)
//! > needs to recover `p`'s declared type from syntax alone — because the
//! > flow-cached node type for `p`'s declaration isn't available yet — it must
//! > apply the same `?` -> `| undefined` widening tsc applies, not just
//! > resolve the bare annotation node (`T`). Before this fix,
//! > `fallback_declared_annotation_type` resolved only the annotation syntax,
//! > silently dropping the implicit `undefined` member; the reassignment's
//! > narrowing base then excluded `undefined`, so a subsequent possibly-`undefined`
//! > read of `p` never reported `TS18048` — the read behaved as if `p` had
//! > *stayed* narrowed by the earlier construct, even though `p` had been
//! > reassigned to a value tsc itself treats as possibly `undefined`.
//!
//! Every case here is oracle-verified against `tsc` (`--strict`, `--noEmit`).
//! Distinct binder/parameter names are used across cases per the CLAUDE.md
//! anti-hardcoding gate.

use tsz_checker::test_utils::check_source_strict_codes;

const TS18048_POSSIBLY_UNDEFINED: u32 = 18048;

fn assert_possibly_undefined(source: &str) {
    let diags = check_source_strict_codes(source);
    assert!(
        diags.contains(&TS18048_POSSIBLY_UNDEFINED),
        "a genuinely possibly-undefined read after reassignment must report \
         TS18048; got: {diags:?}",
    );
}

fn assert_no_possibly_undefined(source: &str) {
    let diags = check_source_strict_codes(source);
    assert!(
        !diags.contains(&TS18048_POSSIBLY_UNDEFINED),
        "must not report a spurious TS18048; got: {diags:?}",
    );
}

/// Minimal repro: a single reassignment of an optional parameter to another
/// possibly-`undefined` value, straight-line, no loop and no join.
#[test]
fn single_reassignment_of_optional_parameter_still_reports_possibly_undefined() {
    assert_possibly_undefined(
        r#"
function clobber(primary?: number, fallback?: number) {
    primary = fallback;
    const last = primary + 1;
    return last;
}
"#,
    );
}

/// The same shape with a prior narrowing assignment (`x = x || default`)
/// before the killing-definition reassignment — the narrowing from the first
/// assignment must not leak into the declared-type recovery used by the
/// second.
#[test]
fn reassignment_after_prior_narrowing_still_reports_possibly_undefined() {
    assert_possibly_undefined(
        r#"
function clobber(value: number, primary?: number, fallback?: number) {
    primary = primary || 0;
    primary = fallback;
    if (value) {}
    const last = primary + 1;
    return last;
}
"#,
    );
}

/// Renamed binders / parameter names — structural, not identifier-keyed.
#[test]
fn renamed_binders_still_report_possibly_undefined() {
    assert_possibly_undefined(
        r#"
function process(count?: number, backup?: number) {
    count = backup;
    const total = count + 1;
    return total;
}
"#,
    );
}

/// Array-typed optional parameter (element-access read instead of arithmetic)
/// — the same recovery path, applied to a reference type.
#[test]
fn array_typed_optional_parameter_reassignment_still_reports_possibly_undefined() {
    assert_possibly_undefined(
        r#"
function clobber(value: number, primary?: number[], fallback?: number[]) {
    primary = primary || [];
    primary = fallback;
    if (value) {}
    const last = primary[0];
    return last;
}
"#,
    );
}

/// Two back-to-back killing-definition reassignments before the read.
#[test]
fn double_reassignment_of_optional_parameter_still_reports_possibly_undefined() {
    assert_possibly_undefined(
        r#"
function clobber(primary?: number, fallback?: number) {
    primary = 10;
    primary = fallback;
    const last = primary + 1;
    return last;
}
"#,
    );
}

/// Negative control: reassigning to a definitely-non-nullish value must stay
/// clean — the fix must not manufacture a false positive on every
/// reassignment of a former optional parameter.
#[test]
fn reassignment_to_non_nullish_value_stays_clean() {
    assert_no_possibly_undefined(
        r#"
function clobber(primary?: number) {
    primary = 10;
    const last = primary + 1;
    return last;
}
"#,
    );
}

/// Negative control: a non-optional (required) parameter reassigned to a
/// possibly-undefined value is a distinct, pre-existing TS2322 concern at the
/// assignment site — not exercised by this recovery path — and the parameter
/// itself keeps its own non-nullable declared type at the read.
#[test]
fn required_parameter_reassignment_target_keeps_declared_type() {
    let diags = check_source_strict_codes(
        r#"
function clobber(primary: number, fallback?: number) {
    primary = fallback;
    const last = primary + 1;
    return last;
}
"#,
    );
    assert!(
        !diags.contains(&TS18048_POSSIBLY_UNDEFINED),
        "a required parameter's declared type has no implicit undefined \
         member to recover; got: {diags:?}"
    );
}
