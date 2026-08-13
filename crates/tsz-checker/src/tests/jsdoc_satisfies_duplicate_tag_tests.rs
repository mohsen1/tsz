//! Regression tests: duplicate JSDoc `@satisfies` tags are allowed (no TS1223).
//!
//! tsc 7.0.2 (the pinned oracle) does not treat `@satisfies` as a singleton
//! JSDoc tag: repeated `@satisfies` tags — attached, inline, or both — compile
//! clean, with no `TS1223: '{0}' tag already specified`. (tsc still emits
//! TS1223 for a duplicate `@type`, a separate tag tsz does not model; tracked
//! as follow-up.) tsz previously reported a spurious TS1223 on the second and
//! later `@satisfies` occurrence, which failed the oracle-clean conformance
//! test `checkJsdocSatisfiesTag11.ts`.
//!
//! Every expectation is pinned against `tsc@7.0.2 --noEmit --allowJs --checkJs`.

use crate::test_utils::{check_js_source_diagnostics, diagnostic_codes};

/// Two attached `@satisfies` tags on one comment: clean, no TS1223.
#[test]
fn duplicate_attached_satisfies_tags_report_no_ts1223() {
    let source = r#"
/**
 * @typedef {Object} T1
 * @property {number} a
 */
/**
 * @typedef {Object} T2
 * @property {number} a
 */
/**
 * @satisfies {T1}
 * @satisfies {T2}
 */
const t1 = { a: 1 };
"#;
    let codes = diagnostic_codes(&check_js_source_diagnostics(source));
    assert!(
        !codes.contains(&1223),
        "duplicate attached @satisfies tags must not report TS1223, got: {codes:?}",
    );
}

/// An attached `@satisfies` plus an inline `@satisfies` on the initializer:
/// clean, no TS1223.
#[test]
fn attached_plus_inline_satisfies_tags_report_no_ts1223() {
    let source = r#"
/**
 * @satisfies {number}
 */
const t2 = /** @satisfies {number} */ (1);
"#;
    let codes = diagnostic_codes(&check_js_source_diagnostics(source));
    assert!(
        !codes.contains(&1223),
        "an attached + inline @satisfies pair must not report TS1223, got: {codes:?}",
    );
}

/// The exact `checkJsdocSatisfiesTag11.ts` shape (both duplication forms in one
/// file): clean, no TS1223 at all.
#[test]
fn satisfies_tag_11_shape_reports_no_ts1223() {
    let source = r#"
/**
 * @typedef {Object} T1
 * @property {number} a
 */

/**
 * @typedef {Object} T2
 * @property {number} a
 */

/**
 * @satisfies {T1}
 * @satisfies {T2}
 */
const t1 = { a: 1 };

/**
 * @satisfies {number}
 */
const t2 = /** @satisfies {number} */ (1);
"#;
    let codes = diagnostic_codes(&check_js_source_diagnostics(source));
    assert!(
        !codes.contains(&1223),
        "the checkJsdocSatisfiesTag11 shape must be TS1223-clean, got: {codes:?}",
    );
}

/// A single well-formed `@satisfies` whose value genuinely does not satisfy the
/// asserted type still reports its own relation diagnostic — removing the
/// duplicate-tag check must not silence real `@satisfies` checking.
#[test]
fn single_satisfies_mismatch_still_reports() {
    let source = r#"
/**
 * @typedef {Object} Named
 * @property {string} name
 */
/**
 * @satisfies {Named}
 */
const bad = { other: 1 };
"#;
    let codes = diagnostic_codes(&check_js_source_diagnostics(source));
    assert!(
        !codes.is_empty(),
        "a genuine @satisfies mismatch must still report a diagnostic, got none",
    );
    assert!(
        !codes.contains(&1223),
        "the mismatch must not be reported as TS1223, got: {codes:?}",
    );
}
