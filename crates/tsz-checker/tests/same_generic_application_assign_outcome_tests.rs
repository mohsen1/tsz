//! Regression coverage for the TS2322 elaboration chain on same-generic
//! `C<A..>` vs `C<B..>` mismatches.
//!
//! `analyze_assignability_failure` runs `raw_input_failure_reason` as a
//! pre-pass and early-returns the raw same-generic-application reason. That
//! reason is rendered as a nested elaboration entry on the top-level TS2322
//! diagnostic (via `related_information`), not as a separate top-level
//! diagnostic, so the TS2322 chain keeps tsc's direct type-argument
//! elaboration ("Type 'string' is not assignable to type 'number'.") even
//! when the boundary's evaluated-shape pass produces a wrapper reason.
//!
//! Note: `assign_relation_outcome` itself does NOT populate `outcome.failure`
//! with the raw-input reason — doing so changed what
//! `outcome.failure`-reading predicates observe in `core_statement_checks.rs:413-426`
//! and caused a real conformance regression on `coAndContraVariantInferences2.ts`
//! and `correlatedUnions.ts` (see the review discussion on
//! <https://github.com/tsz-org/tsz/pull/12239#discussion_r3342820552>). The
//! TS2322 elaboration tested here flows through `analyze_assignability_failure`
//! directly in `error_reporter/assignability.rs:602`, independent of the
//! `assign_relation_outcome` outcome's `failure` field.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
}

/// Return true iff `diag` (TS2322) has at least one related-information
/// entry whose message asserts the differing direct type arguments. The
/// canonical tsc forms are:
///
/// - `Type 'A' is not assignable to type 'B'.`
/// - `Types of property '<name>' are incompatible.` followed by the above
///
/// We check for the former exact wording, which can only be produced by the
/// nested elaboration — the top-level message wraps the operands in
/// `'Box<A>'`/`'Box<B>'` style and CANNOT yield the bare argument forms.
fn has_argument_elaboration(diag: &Diagnostic, source_arg: &str, target_arg: &str) -> bool {
    let needle = format!("Type '{source_arg}' is not assignable to type '{target_arg}'.");
    diag.related_information
        .iter()
        .any(|related| related.message_text.contains(&needle))
}

/// Pretty-print a TS2322 diagnostic and its related-information chain for
/// assertion failure output.
fn format_chain(diag: &Diagnostic) -> String {
    let mut out = format!("[top] {}", diag.message_text);
    for related in &diag.related_information {
        out.push_str(&format!(
            "\n  [related@{}] {}",
            related.start, related.message_text
        ));
    }
    out
}

/// Assignment from `Box<string>` to `Box<number>` (same generic, differing
/// args) must produce a TS2322 with a nested argument elaboration entry
/// ("Type 'string' is not assignable to type 'number'."), not just a bare
/// top-level "Type 'Box<string>' is not assignable to type 'Box<number>'."
#[test]
fn same_generic_application_assign_keeps_argument_elaboration() {
    let source = r#"
interface Box<T> { value: T; }
declare const a: Box<string>;
declare const b: Box<number> = a;
"#;
    let diags = diagnostics(source);
    let ts2322: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "expected TS2322 on Box<string> -> Box<number>, got: {diags:?}"
    );

    // Must have the nested argument elaboration on at least one TS2322
    // diagnostic. The top-level message embeds the operands as `'Box<string>'`
    // / `'Box<number>'` so a substring check for `'string'`/`'number'` would
    // be satisfied vacuously — assert the exact tsc elaboration wording
    // instead, which can only come from the nested chain.
    let has_elaboration = ts2322
        .iter()
        .any(|d| has_argument_elaboration(d, "string", "number"));
    assert!(
        has_elaboration,
        "TS2322 must include a nested argument elaboration \
         \"Type 'string' is not assignable to type 'number'.\" \
         (mirrors tsc's direct type-argument chain). \
         got diagnostics:\n{}",
        ts2322
            .iter()
            .map(|d| format_chain(d))
            .collect::<Vec<_>>()
            .join("\n---\n")
    );
}

/// Renamed binders must not change the rule — the fix keys on the
/// structural same-base-application shape, not on identifier spelling.
#[test]
fn same_generic_application_assign_keeps_argument_elaboration_renamed() {
    let source = r#"
interface Container<X> { contents: X; }
declare const src: Container<boolean>;
declare const dst: Container<string> = src;
"#;
    let diags = diagnostics(source);
    let ts2322: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "expected TS2322 on Container<boolean> -> Container<string>, got: {diags:?}"
    );

    let has_elaboration = ts2322
        .iter()
        .any(|d| has_argument_elaboration(d, "boolean", "string"));
    assert!(
        has_elaboration,
        "TS2322 must include a nested argument elaboration \
         \"Type 'boolean' is not assignable to type 'string'.\" \
         regardless of identifier names. \
         got diagnostics:\n{}",
        ts2322
            .iter()
            .map(|d| format_chain(d))
            .collect::<Vec<_>>()
            .join("\n---\n")
    );
}
