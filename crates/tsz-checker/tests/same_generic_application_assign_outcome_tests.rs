//! Regression coverage for the `assign_relation_outcome` path on
//! same-generic `C<A..>` vs `C<B..>` mismatches.
//!
//! Before #12239 the TS2322 family routed through `assign_relation_outcome`
//! could observe a `(related=false, failure=None)` outcome OR an outcome
//! carrying a property-wrapper reason ("Types of property 'x' are
//! incompatible") instead of tsc's direct type-argument elaboration: the
//! checker-side `is_assignable_to` fast-paths rejected the relation while
//! the solver's evaluated-shape pass either yielded no reason or yielded a
//! wrapper reason from the evaluated object shape.
//!
//! The fix routes the raw-input `same_generic_application_failure_reason`
//! detector through `assign_relation_outcome` BEFORE the boundary's
//! evaluated-shape pass, so the direct argument elaboration always wins.
//! `analyze_assignability_failure` had this ordering already; this test
//! exercises the parallel `assign_relation_outcome` path that emits TS2322.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
}

/// Collect the full diagnostic chain text from `message_text` plus any
/// `related_information` sub-messages, recursively. TS2322 elaborations
/// (e.g. "Types of property 'x' are incompatible." → "Type 'string' is
/// not assignable to type 'number'.") live as nested related-information
/// entries on the top-level diagnostic, not as separate top-level diags.
fn full_chain_text(diag: &Diagnostic) -> String {
    let mut out = diag.message_text.clone();
    for related in &diag.related_information {
        out.push_str(" / ");
        out.push_str(&related.message_text);
    }
    out
}

/// Assignment from `Box<string>` to `Box<number>` (same generic, differing
/// args) must produce a TS2322 whose elaboration chain mentions the
/// differing type arguments — not a bare top-level mismatch with no inner
/// cause.
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
    let chain = ts2322
        .iter()
        .map(|d| full_chain_text(d))
        .collect::<Vec<_>>()
        .join(" || ");
    let elaborates_arguments = chain.contains("'string'") && chain.contains("'number'");
    assert!(
        elaborates_arguments,
        "TS2322 must elaborate the differing type arguments (string vs number) \
         instead of a bare top-level mismatch with no inner cause. \
         got chain: {chain}"
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
    let chain = ts2322
        .iter()
        .map(|d| full_chain_text(d))
        .collect::<Vec<_>>()
        .join(" || ");
    let elaborates_arguments = chain.contains("'boolean'") && chain.contains("'string'");
    assert!(
        elaborates_arguments,
        "TS2322 must elaborate the differing type arguments (boolean vs string) \
         regardless of identifier names. got chain: {chain}"
    );
}
