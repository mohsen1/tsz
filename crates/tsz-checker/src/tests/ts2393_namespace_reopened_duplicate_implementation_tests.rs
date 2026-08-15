//! Regression tests for `TS2393` ("Duplicate function implementation") over a
//! namespace reopened with two independent function implementations of the
//! same exported name (`compiler/missingFunctionImplementation.ts`,
//! oracle-verified against `typescript@6.0.2`/`7.0.2` with `--strict false
//! --target es2015`).
//!
//! Structural rule: once a merged symbol has 2+ *local* (same-file)
//! function-family declarations that carry a body, tsc treats the *entire*
//! declaration group as a duplicate-implementation family — every other
//! local declaration of that symbol, including bodyless overload signatures,
//! is reported `TS2393` too. tsz previously only flagged the bodied
//! declarations (missing the bodyless ones) and additionally ran the normal
//! single-implementation overload-compatibility check
//! (`check_overload_compatibility`, `overload_compatibility.rs`) over the
//! group, producing a spurious `TS2394` on one of the bodyless signatures.
//! Fixed by (1) having `check_overload_compatibility` stand down entirely
//! once a symbol already has 2+ local implementations — that case is owned
//! by the duplicate-identifier pass, not overload/implementation
//! compatibility — and (2) extending the duplicate-identifier `TS2393` pass
//! (`duplicate_identifiers.rs`) to also flag the bodyless signatures once a
//! duplicate-implementation family is found.
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
        .filter(|d| matches!(d.code, 2300 | 2391 | 2393 | 2394))
        .collect()
}

fn assert_family_exactly(source: &str, shapes: &[DiagnosticShape]) {
    assert_diagnostic_shapes_exactly(source, &family(source), shapes);
}

/// Positive control (the fixture's own repro): a namespace reopened twice,
/// each reopening providing its own overload signature plus implementation
/// for the same exported name. tsc: `TS2393` on all four declarations of the
/// merged `m` — the two bodyless signatures and the two implementations —
/// with no `TS2394` anywhere.
#[test]
fn reopened_namespace_two_implementations_flags_every_declaration() {
    assert_family_exactly(
        "namespace Merged {\n\
         \x20 export function m(a): void;\n\
         \x20 export function m(): void;\n\
         \x20 export function m(a?): void { }\n\
         }\n\
         namespace Merged {\n\
         \x20 export function m(a): void { }\n\
         }\n",
        &[
            DiagnosticShape::code(2393).at(2, 19),
            DiagnosticShape::code(2393).at(3, 19),
            DiagnosticShape::code(2393).at(4, 19),
            DiagnosticShape::code(2393).at(7, 19),
        ],
    );
}

/// Negative control (this change's non-regression witness): a namespace
/// reopened with exactly one bodyless signature and one implementation (the
/// fixture's `N10`). A single local implementation is not a
/// duplicate-implementation family, so this stays outside the new code path
/// entirely — but the signature and implementation are still in different
/// reopenings of the namespace, so tsc does not consider the implementation
/// to "immediately follow" the signature: `TS2391`, not `TS2393`/clean.
#[test]
fn reopened_namespace_single_implementation_reports_missing_implementation() {
    assert_family_exactly(
        "namespace Merged {\n\
         \x20 export function m(a): void;\n\
         }\n\
         namespace Merged {\n\
         \x20 export function m(a): void { }\n\
         }\n",
        &[DiagnosticShape::code(2391).at(2, 19)],
    );
}

/// Negative control: a single (non-reopened) namespace with a normal
/// overload set and one implementation. Also clean.
#[test]
fn single_namespace_normal_overload_group_stays_clean() {
    assert_family_exactly(
        "namespace Ns {\n\
         \x20 export function m(a: string): void;\n\
         \x20 export function m(a: number): void;\n\
         \x20 export function m(a: any): void { }\n\
         }\n",
        &[],
    );
}

/// Adjacent case: renamed binder and 3-way reopening (three separate bodies)
/// still flags every local declaration of the merged symbol.
#[test]
fn reopened_namespace_three_implementations_flags_every_declaration() {
    assert_family_exactly(
        "namespace Widget {\n\
         \x20 export function render(x): void { }\n\
         }\n\
         namespace Widget {\n\
         \x20 export function render(x): void { }\n\
         }\n\
         namespace Widget {\n\
         \x20 export function render(x): void { }\n\
         }\n",
        &[
            DiagnosticShape::code(2393).at(2, 19),
            DiagnosticShape::code(2393).at(5, 19),
            DiagnosticShape::code(2393).at(8, 19),
        ],
    );
}

/// Adjacent case: a plain (non-namespaced) top-level function reopened with
/// two bodies is unaffected by this change — it was already correctly
/// flagged via the same-scope pairwise path, and stays that way with no
/// bodyless siblings to extend to.
#[test]
fn top_level_function_two_implementations_unchanged() {
    assert_family_exactly(
        "function f(): void { }\n\
         function f(): void { }\n",
        &[
            DiagnosticShape::code(2393).at(1, 10),
            DiagnosticShape::code(2393).at(2, 10),
        ],
    );
}
