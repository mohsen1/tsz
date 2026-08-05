//! `declare export = <non-identifier>;` (#16403 residual). The resulting
//! `ExportAssignment` node's `declare`/`export` modifiers were dropped at
//! parse time (`parse_export_assignment` always built
//! `ExportAssignmentData { modifiers: None, .. }`), so
//! `is_in_ambient_context` — which reads a declaration's own modifier list
//! before walking to its parent — had no way to see the `declare` keyword on
//! this node. Since the statement sits at the source file's own top level (no
//! ambient ancestor either), the ambient gate in
//! `check_export_declarations_and_assignments` read `false` and TS2714 ("The
//! expression of an export assignment must be an identifier or qualified
//! name in an ambient context.") never fired — even though the sibling
//! parser-level TS1120 ("An export assignment cannot have modifiers.")
//! already did, since that diagnostic is emitted directly at parse time and
//! does not depend on the node's `modifiers` field. This harness
//! (`check_source_codes`) only surfaces checker diagnostics, so TS1120 itself
//! is covered separately in `tsz-parser`'s
//! `parser_declare_export_default_tests.rs`.
//!
//! Every expectation oracle-pinned against `typescript@7.0.2`
//! (`--strict --target es2022 --module es2022`).

use crate::test_utils::check_source_codes;

const THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_IN_AN_AMBIENT_CONTEXT: u32 =
    2714;

#[test]
fn declare_export_equals_numeric_literal_reports_ts2714() {
    let codes = check_source_codes("declare export = 1;");
    assert!(
        codes.contains(
            &THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_IN_AN_AMBIENT_CONTEXT
        ),
        "expected TS2714 — the `declare` modifier must reach ambient-context \
         detection on this node; got {codes:?}"
    );
}

#[test]
fn declare_export_equals_object_literal_reports_ts2714() {
    let codes = check_source_codes("declare export = { a: 1 };");
    assert!(codes.contains(
        &THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_IN_AN_AMBIENT_CONTEXT
    ));
}

// Negative control: an identifier expression is a valid ambient export
// assignment target, so TS2714 must not fire.
#[test]
fn declare_export_equals_identifier_reports_no_ts2714() {
    let codes = check_source_codes("declare const X: number;\ndeclare export = X;");
    assert!(!codes.contains(
        &THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_IN_AN_AMBIENT_CONTEXT
    ));
}

// Negative control: a plain (non-ambient) `export = <expr>;` must not gain
// TS2714 — the file is not ambient at all.
#[test]
fn plain_export_equals_numeric_literal_reports_no_ts2714() {
    let codes = check_source_codes("export = 1;");
    assert!(!codes.contains(
        &THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_IN_AN_AMBIENT_CONTEXT
    ));
}
