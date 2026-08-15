//! Regression tests for `TS2300` ("Duplicate identifier") over a class whose
//! static side carries 2+ overload signatures of the same name, merged with a
//! namespace exporting the implementation for that name
//! (`compiler/missingFunctionImplementation.ts`'s `C8`, oracle-verified
//! against `typescript@6.0.2`/`7.0.2` with `--strict false --target es2015`).
//!
//! Structural rule: when a class's static side and a merged namespace's
//! exports both declare a member of the same name, tsc reports `TS2300` on
//! *every* declaration of that member — every static overload signature, not
//! just the first, plus the namespace-side declaration. tsz's
//! `report_duplicate_on_class_static_member`
//! (`crates/tsz-checker/src/declarations/namespace_checker.rs`) scanned the
//! class's members for the first one named `name` and returned immediately,
//! so a second (or later) static overload signature of the same name never
//! got its `TS2300`. Fixed by continuing the scan and reporting every
//! matching static member instead of stopping at the first match.
//!
//! Binder names vary across rows; no row depends on identifier spelling.

use crate::context::ScriptTarget;
use crate::test_utils::{DiagnosticShape, assert_diagnostic_shapes_exactly, check_source};
use crate::{CheckerOptions, diagnostics::Diagnostic};

fn check_module(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    )
}

/// Diagnostics restricted to the overload/duplicate-identifier family, so
/// rows stay pinned even when an unrelated family fires on the same fixture.
fn family(source: &str) -> Vec<Diagnostic> {
    check_module(source)
        .into_iter()
        .filter(|d| matches!(d.code, 2300 | 2391 | 2393 | 2394))
        .collect()
}

fn assert_family_exactly(source: &str, shapes: &[DiagnosticShape]) {
    assert_diagnostic_shapes_exactly(source, &family(source), shapes);
}

/// Positive control (the fixture's own repro, `C8`): two bodyless static
/// overload signatures merged with a namespace implementation of the same
/// name. tsc flags `TS2300` on all three declarations — both static
/// signatures and the namespace export — plus `TS2391` on the second
/// (last-before-the-merge-boundary) static signature only.
#[test]
fn two_static_overload_signatures_both_flagged_against_namespace_export() {
    assert_family_exactly(
        "class Widget {\n\
         \x20 static m(a): void;\n\
         \x20 static m(a, b): void;\n\
         }\n\
         namespace Widget {\n\
         \x20 export function m(a?, b?): void { }\n\
         }\n",
        &[
            DiagnosticShape::code(2300).at(2, 10),
            DiagnosticShape::code(2300).at(3, 10),
            DiagnosticShape::code(2391).at(3, 10),
            DiagnosticShape::code(2300).at(6, 19),
        ],
    );
}

/// Adjacent case: three bodyless static overload signatures (extending the
/// group beyond two) all get `TS2300` against the namespace export, renamed
/// binder.
#[test]
fn three_static_overload_signatures_all_flagged_against_namespace_export() {
    assert_family_exactly(
        "class Registry {\n\
         \x20 static add(a): void;\n\
         \x20 static add(a, b): void;\n\
         \x20 static add(a, b, c): void;\n\
         }\n\
         namespace Registry {\n\
         \x20 export function add(a?, b?, c?): void { }\n\
         }\n",
        &[
            DiagnosticShape::code(2300).at(2, 10),
            DiagnosticShape::code(2300).at(3, 10),
            DiagnosticShape::code(2300).at(4, 10),
            DiagnosticShape::code(2391).at(4, 10),
            DiagnosticShape::code(2300).at(7, 19),
        ],
    );
}

/// Negative control / non-regression witness (the fixture's `C9` shape): a
/// single static implementation merged with a single bodyless namespace
/// signature. Only one static declaration exists, so the fix (continue
/// scanning instead of returning on the first match) changes nothing here —
/// still exactly one `TS2300` per side.
#[test]
fn single_static_implementation_against_namespace_signature_unchanged() {
    assert_family_exactly(
        "class Widget {\n\
         \x20 static m(a): void { }\n\
         }\n\
         namespace Widget {\n\
         \x20 export function m(a): void;\n\
         }\n",
        &[
            DiagnosticShape::code(2300).at(2, 10),
            DiagnosticShape::code(2300).at(5, 19),
            DiagnosticShape::code(2391).at(5, 19),
        ],
    );
}

/// Negative control: static overload signatures whose name does not match
/// any namespace export stay outside this diagnostic family entirely — they
/// are an ordinary (if incomplete) overload group, reported as `TS2391`
/// missing-implementation, not `TS2300`.
#[test]
fn static_overload_signatures_with_no_matching_namespace_export_stay_clean_of_ts2300() {
    assert_family_exactly(
        "class Widget {\n\
         \x20 static m(a): void;\n\
         \x20 static m(a, b): void;\n\
         }\n\
         namespace Widget {\n\
         \x20 export function n(): void { }\n\
         }\n",
        &[DiagnosticShape::code(2391).at(3, 10)],
    );
}
