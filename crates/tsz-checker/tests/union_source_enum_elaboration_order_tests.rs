//! A union-source `TS2322` elaboration must drill the first-*declared* failing
//! member, even when that member is an enum sharing another enum's structural
//! shape.
//!
//! `tsc` always elaborates a failing union assignment with the union's first
//! member (in the order the union was written); on a tie between two members
//! of the same `TypeFlags` rank (two enums, no other tiebreaker), the printer's
//! own TS7 stable order already keeps enum members in declaration order. The
//! solver's interned union member list, however, is ordered by allocation
//! identity (needed so `E1 | E2` and `E2 | E1` intern to one canonical
//! `TypeId`) — an enum's `DefId`/`TypeId` is allocated lazily, in whatever
//! order the checker first requests its type, which does not track source
//! position. Elaboration selection that walks the interned list directly can
//! therefore name the wrong enum entirely, not just the wrong order.
//!
//! Structural rule: `SubtypeChecker::reorder_enum_members_by_declaration`
//! (`crates/tsz-solver/src/relations/subtype/explain.rs`) reorders only the
//! enum-typed slots of a union-source elaboration list into declaration order
//! before the first-failing-member scan, leaving every other member's slot —
//! and the existing nullish-first order — untouched.
//!
//! Regression guard for #16513.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn single(source: &str, code: u32) -> Diagnostic {
    let diagnostics: Vec<Diagnostic> = check_source_diagnostics(source)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == code)
        .collect();
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one TS{code} diagnostic, got {diagnostics:#?}"
    );
    diagnostics.into_iter().next().unwrap()
}

fn related(diagnostic: &Diagnostic) -> Vec<String> {
    diagnostic
        .related_information
        .iter()
        .map(|related| related.message_text.clone())
        .collect()
}

fn has_related(diagnostic: &Diagnostic, expected: &str) -> bool {
    related(diagnostic)
        .iter()
        .any(|message| message == expected)
}

// ---------------------------------------------------------------------------
// Two same-shaped enums: the elaboration must name whichever is declared
// first, independent of the union annotation's own written order and
// independent of the enums' names (varied below to rule out an alphabetical
// coincidence).
// ---------------------------------------------------------------------------

#[test]
fn union_of_two_enums_elaborates_the_first_declared() {
    let source = "
        enum E1 { A }
        enum E2 { A }
        declare const e: E1 | E2;
        const ee: boolean = e;
    ";
    let diagnostic = single(source, 2322);
    assert!(
        has_related(
            &diagnostic,
            "Type 'E1' is not assignable to type 'boolean'."
        ),
        "expected the first-declared enum E1 in the elaboration, got {:?}",
        related(&diagnostic)
    );
    assert!(
        !has_related(
            &diagnostic,
            "Type 'E2' is not assignable to type 'boolean'."
        ),
        "must not elaborate the second-declared enum E2, got {:?}",
        related(&diagnostic)
    );
}

/// Same shape, renamed binders, and declaration order reversed relative to the
/// annotation's written order (`Beta` declared first, `Alpha` second, but the
/// annotation reads `Alpha | Beta`) — rules out both an alphabetical-name
/// coincidence and a written-annotation-order coincidence.
#[test]
fn union_of_two_enums_elaborates_by_declaration_order_not_annotation_or_name_order() {
    let source = "
        enum Beta { A }
        enum Alpha { A }
        declare const e: Alpha | Beta;
        const ee: boolean = e;
    ";
    let diagnostic = single(source, 2322);
    assert!(
        has_related(
            &diagnostic,
            "Type 'Beta' is not assignable to type 'boolean'."
        ),
        "expected the first-declared enum Beta in the elaboration, got {:?}",
        related(&diagnostic)
    );
    assert!(
        !has_related(
            &diagnostic,
            "Type 'Alpha' is not assignable to type 'boolean'."
        ),
        "must not elaborate the second-declared enum Alpha, got {:?}",
        related(&diagnostic)
    );
}

/// Three same-shaped enums: the elaboration must still name the first
/// declared, not merely "not last".
#[test]
fn union_of_three_enums_elaborates_the_first_declared() {
    let source = "
        enum A1 { X }
        enum A2 { X }
        enum A3 { X }
        declare const e: A1 | A2 | A3;
        const ee: boolean = e;
    ";
    let diagnostic = single(source, 2322);
    assert!(
        has_related(
            &diagnostic,
            "Type 'A1' is not assignable to type 'boolean'."
        ),
        "expected the first-declared enum A1 in the elaboration, got {:?}",
        related(&diagnostic)
    );
}

// ---------------------------------------------------------------------------
// Negative controls: shapes the enum-declaration reorder must leave alone.
// ---------------------------------------------------------------------------

/// A single enum in the union (no tie to break) keeps naming that enum.
#[test]
fn union_of_enum_and_primitive_still_elaborates_the_failing_member() {
    let source = "
        enum E1 { A }
        declare const e: E1 | string;
        const ee: boolean = e;
    ";
    let diagnostic = single(source, 2322);
    assert!(
        has_related(
            &diagnostic,
            "Type 'string' is not assignable to type 'boolean'."
        ),
        "expected the lower-ranked primitive member in the elaboration, got {:?}",
        related(&diagnostic)
    );
}

/// A nullish member ahead of two enums keeps the existing nullish-first
/// elaboration order — the enum-declaration reorder must not disturb it.
#[test]
fn union_of_undefined_and_two_enums_still_elaborates_undefined_first() {
    let source = "
        enum E1 { A }
        enum E2 { A }
        declare const v: undefined | E1 | E2;
        const ee: boolean = v;
    ";
    let diagnostic = single(source, 2322);
    assert!(
        has_related(
            &diagnostic,
            "Type 'undefined' is not assignable to type 'boolean'."
        ),
        "expected the nullish member ahead of both enums in the elaboration, got {:?}",
        related(&diagnostic)
    );
}
