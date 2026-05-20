//! Regression tests for object-spread merge semantics with optional later
//! members.
//!
//! When a later spread's property is *optional* (`p?: T`), tsc unions the
//! prior contributions with the later type and keeps the prior's optionality
//! — because the later spread might not provide the value at runtime, so the
//! earlier definite contribution still applies. tsz currently overwrites
//! unconditionally, dropping the prior contribution.
//!
//! Source: `compiler/conformance/types/spread/objectSpreadStrictNull.ts`
//! line 12 expects `{ sn: string | number }` from
//! `{ ...definiteBoolean, ...definiteString, ...optionalNumber }`,
//! but tsz produces `{ sn?: number | undefined }`.
//!
//! Note: tsz's existing behavior is correct when the later spread is
//! *required* (even if its type contains `undefined`) — that case is
//! covered by `objectSpreadStrictNull.ts:17` which both tsz and tsc
//! resolve as `{ sn: number | undefined }`. The fix is scoped to the
//! optional-later case.

use crate::test_utils::check_source_diagnostics;

fn diag_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// Optional later spread should union — no TS2322 expected.
#[test]
fn spread_optional_later_unions_with_required_earlier_no_error() {
    let codes = diag_codes(
        r#"
declare const definiteString: { sn: string };
declare const optionalNumber: { sn?: number };
let target: { sn: string | number } = { ...definiteString, ...optionalNumber };
"#,
    );
    assert!(
        !codes.contains(&2322),
        "Optional-later spread should union; got codes: {codes:?}"
    );
}

/// Anti-hardcoding cover: same rule with renamed identifiers.
#[test]
fn spread_optional_later_unions_renamed() {
    let codes = diag_codes(
        r#"
declare const definite: { value: number };
declare const maybeText: { value?: string };
let result: { value: number | string } = { ...definite, ...maybeText };
"#,
    );
    assert!(
        !codes.contains(&2322),
        "Renamed variant: optional-later spread should union; got codes: {codes:?}"
    );
}

/// Required-later spread should fully override — TS2322 still expected
/// when the override doesn't satisfy the target.
#[test]
fn spread_required_later_with_undefined_overrides_required_earlier_keeps_error() {
    let codes = diag_codes(
        r#"
declare const definiteString: { sn: string };
declare const undefinedNumber: { sn: number | undefined };
let target: { sn: string | number } = { ...definiteString, ...undefinedNumber };
"#,
    );
    assert!(
        codes.contains(&2322),
        "Required-later spread (even with undefined in type) overrides; expected TS2322, got: {codes:?}"
    );
}

/// Required-later spread without undefined should also fully override
/// (positive case — no error when override matches target).
#[test]
fn spread_required_later_overrides_required_earlier_no_error_when_compatible() {
    let codes = diag_codes(
        r#"
declare const earlier: { sn: string };
declare const later: { sn: number };
let target: { sn: string | number } = { ...earlier, ...later };
"#,
    );
    assert!(
        !codes.contains(&2322),
        "Required-later spread that matches target should not error. Got: {codes:?}"
    );
}
