//! Regression tests for `TS2300` when a class's static member name carries
//! 2+ local overload signatures and merges with a namespace export of the
//! same name (`compiler/missingFunctionImplementation.ts`'s `C8`, oracle-
//! verified against `typescript@7.0.2` with `--strict false --target
//! es2015`).
//!
//! Structural rule: when a class+namespace merge produces a name collision
//! between a class static member and a namespace export, tsc reports
//! `TS2300` on *every* local declaration of the class-side name — including
//! every bodyless overload signature — not just the first one found. tsz's
//! `report_duplicate_on_class_static_member`
//! (`declarations/namespace_checker.rs`) returned as soon as it reported the
//! first matching static member, so a second (or later) overload signature
//! of the same static name never got its own `TS2300`. Fixed by having the
//! loop keep scanning and reporting on every match, returning `true` only to
//! signal that at least one direct match was found (unchanged caller
//! contract).
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

/// Diagnostics restricted to the overload/implementation family, so rows
/// stay pinned even when an unrelated family fires on the same fixture.
fn family(source: &str) -> Vec<Diagnostic> {
    check_module(source)
        .into_iter()
        .filter(|d| matches!(d.code, 2300 | 2391))
        .collect()
}

fn assert_family_exactly(source: &str, shapes: &[DiagnosticShape]) {
    assert_diagnostic_shapes_exactly(source, &family(source), shapes);
}

/// Positive control (the fixture's own repro, `C8`): a class with two static
/// overload signatures for `m`, merged with a namespace that implements `m`.
/// tsc: `TS2300` on both static signatures and on the namespace's `m`, plus
/// `TS2391` on the second (last) static signature — the normal
/// missing-implementation diagnostic a bodyless overload group still owes.
#[test]
fn two_static_overloads_both_get_ts2300() {
    assert_family_exactly(
        "class C8 {\n\
         \x20 static m(a): void;\n\
         \x20 static m(a, b): void;\n\
         }\n\
         namespace C8 {\n\
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

/// Negative control (this change's non-regression witness, the fixture's
/// `C9`): a *single* static member (with its own body) merged with a
/// namespace's bodyless signature. Only one static declaration exists, so
/// the fix's "keep scanning" change must not create a duplicate report here.
#[test]
fn single_static_member_still_gets_exactly_one_ts2300() {
    assert_family_exactly(
        "class C9 {\n\
         \x20 static m(a): void { }\n\
         }\n\
         namespace C9 {\n\
         \x20 export function m(a): void;\n\
         }\n",
        &[
            DiagnosticShape::code(2300).at(2, 10),
            DiagnosticShape::code(2300).at(5, 19),
            DiagnosticShape::code(2391).at(5, 19),
        ],
    );
}

/// Adjacent case: renamed binder/member name and a *three*-way local
/// overload group (not just two) — every signature still gets its own
/// `TS2300`, proving the fix isn't hardcoded to a 2-overload shape.
#[test]
fn renamed_binder_three_static_overloads_all_get_ts2300() {
    assert_family_exactly(
        "class Widget {\n\
         \x20 static render(a): void;\n\
         \x20 static render(a, b): void;\n\
         \x20 static render(a, b, c): void;\n\
         }\n\
         namespace Widget {\n\
         \x20 export function render(a?, b?, c?): void { }\n\
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

/// Adjacent case: a class static member that does *not* collide with the
/// namespace (different name) stays clean — the fix must not start
/// over-reporting unrelated overload groups.
#[test]
fn non_colliding_static_overloads_stay_clean() {
    assert_family_exactly(
        "class C4 {\n\
         \x20 static other(a): void;\n\
         \x20 static other(a, b): void;\n\
         \x20 static other(a?, b?): void { }\n\
         }\n\
         namespace C4 {\n\
         \x20 export function m(a): void { }\n\
         }\n",
        &[],
    );
}
