//! A top-level ambient variable declaration (`declare const/let/var x: T = v`,
//! or a non-`const` `let`/`var` with any initializer) drops TS1039
//! (`Initializers are not allowed in ambient contexts.`) — see #17073.
//!
//! Structural rule: `tsc`'s ambient-initializer grammar check fires whenever
//! an ambient variable declaration carries both a type annotation and an
//! initializer (or is a non-`const` `let`/`var` with any initializer). tsz's
//! checker computes and emits this diagnostic correctly (confirmed via a
//! direct probe reaching `error_at_node` at `state/variable_checking/core.rs`)
//! but it never reached output at the top level: the initializer's
//! contextual-typing "pre-contextual diagnostic reset" in
//! `state/variable_checking/initializer_policy.rs` unconditionally clears
//! every diagnostic anchored inside the initializer's own span before
//! re-evaluating the initializer with its contextual type, and TS1039 is
//! anchored at that same initializer node. That reset path only runs when a
//! type annotation is present (`has_type_annotation` gates the whole block),
//! which is exactly why the *no-annotation* sibling case (TS1254) was never
//! affected, and why the namespace-nested / class-property paths (which
//! route through a different check, `check_initializers_in_ambient_body`,
//! never through this contextual-typing reset) were already correct.
//!
//! Fixed by adding TS1039 to the reset's structural-diagnostic allow-list,
//! alongside the other position-anchored-in-initializer checks (TS2693,
//! TS2454, TS2348, TS2538, TS2339, TS2304, ...) that already survive it for
//! the same reason: they are not artifacts of the pre-contextual type
//! computation.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;

const INITIALIZERS_NOT_ALLOWED_IN_AMBIENT: u32 = 1039;
const CONST_INITIALIZER_MUST_BE_LITERAL: u32 = 1254;
const TYPE_NOT_ASSIGNABLE: u32 = 2322;

fn check_source_diagnostics(file_name: &str, source: &str) -> Vec<Diagnostic> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let source_file = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), source_file);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        CheckerOptions::default(),
    );
    checker.enable_source_file_test_pragmas();
    checker.ctx.set_lib_contexts(Vec::new());
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(source_file);
    checker.ctx.diagnostics.clone()
}

fn codes(source: &str) -> Vec<u32> {
    let mut c: Vec<u32> = check_source_diagnostics("test.ts", source)
        .into_iter()
        .map(|d| d.code)
        .collect();
    c.sort_unstable();
    c
}

fn codes_in_file(file_name: &str, source: &str) -> Vec<u32> {
    let mut c: Vec<u32> = check_source_diagnostics(file_name, source)
        .into_iter()
        .map(|d| d.code)
        .collect();
    c.sort_unstable();
    c
}

#[test]
fn top_level_declare_const_annotated_literal_reports_ts1039() {
    let got = codes("declare const x: number = 1;");
    assert!(
        got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "expected TS1039, got {got:?}"
    );
}

#[test]
fn top_level_declare_let_annotated_literal_reports_ts1039() {
    let got = codes("declare let x: number = 1;");
    assert!(
        got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "expected TS1039, got {got:?}"
    );
}

#[test]
fn top_level_declare_var_annotated_literal_reports_ts1039() {
    let got = codes("declare var x: number = 1;");
    assert!(
        got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "expected TS1039, got {got:?}"
    );
}

#[test]
fn top_level_declare_const_annotated_computed_initializer_reports_ts1039() {
    let got = codes("declare const x: number = 1 + 1;");
    assert!(
        got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "expected TS1039, got {got:?}"
    );
}

#[test]
fn top_level_declare_const_renamed_binder_reports_ts1039() {
    let got = codes("declare const totallyDifferentName: number = 1;");
    assert!(
        got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "expected TS1039, got {got:?}"
    );
}

#[test]
fn top_level_declare_const_mismatched_annotation_reports_ts1039_and_ts2322() {
    let got = codes("declare const x: string = 1;");
    assert!(
        got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "expected TS1039, got {got:?}"
    );
    assert!(
        got.contains(&TYPE_NOT_ASSIGNABLE),
        "expected TS2322 to still fire alongside TS1039, got {got:?}"
    );
}

/// Negative control: no annotation, non-literal initializer keeps its
/// existing TS1254 behavior and must NOT gain a spurious TS1039 (`1 + 1` is
/// not a `const`-with-annotation case, so it never enters the fixed path).
#[test]
fn top_level_declare_const_no_annotation_computed_initializer_keeps_ts1254() {
    let got = codes("declare const x = 1 + 1;");
    assert!(
        got.contains(&CONST_INITIALIZER_MUST_BE_LITERAL),
        "expected TS1254, got {got:?}"
    );
    assert!(
        !got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "did not expect TS1039 for the no-annotation case, got {got:?}"
    );
}

/// Negative control: a plain literal ambient const without annotation is
/// clean on both codes.
#[test]
fn top_level_declare_const_no_annotation_literal_is_clean() {
    let got = codes("declare const x = 1;");
    assert!(
        !got.contains(&CONST_INITIALIZER_MUST_BE_LITERAL)
            && !got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "expected no ambient-initializer diagnostics, got {got:?}"
    );
}

/// Regression guard: the namespace-nested path already worked (a different
/// check, `check_initializers_in_ambient_body`) and must keep working.
#[test]
fn namespace_nested_ambient_const_still_reports_ts1039() {
    let got = codes("declare namespace M { const x: number = 1; }");
    assert!(
        got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "expected TS1039 for namespace-nested ambient const, got {got:?}"
    );
}

/// Regression guard: the ambient class `readonly` property-initializer path
/// already worked and must keep working.
#[test]
fn ambient_class_readonly_property_still_reports_ts1039() {
    let got = codes("declare class C { readonly x: number = 1; }");
    assert!(
        got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "expected TS1039 for ambient class readonly property, got {got:?}"
    );
}

// ---------------------------------------------------------------------------
// The `.d.ts` emission gap (follow-up to #17080).
//
// A `.d.ts` file is entirely ambient, so tsc's ambient-initializer grammar
// check fires for every top-level declaration in it — even with no `declare`
// keyword — exactly as it does for an explicit `declare` in a `.ts` file
// (oracle: pinned `typescript@7.0.2`, `--noEmit --strict --lib es2022`). The
// emission was previously gated on `is_in_ambient_context` (explicit `declare`
// ancestor only), which is `false` for a bare/`export`-modified `.d.ts` member,
// so tsz dropped TS1039/TS1254 there — a false-negative #17080 identified and
// deliberately left out of scope. The gate now uses `is_ambient_declaration`
// (`.d.ts` file OR explicit ambient context), which closes it.
// ---------------------------------------------------------------------------

/// `const x: number = 1;` in a `.d.ts` (no `declare`) — tsc emits TS1039
/// (`case.d.ts(1,19): error TS1039`), so tsz now must too.
#[test]
fn dts_file_bare_annotated_initializer_reports_ts1039() {
    let got = codes_in_file("test.d.ts", "const x: number = 1;");
    assert!(
        got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "a bare-.d.ts annotated initializer is ambient — expected TS1039; got {got:?}"
    );
}

/// `export const x: number = 1;` in a `.d.ts` — an `export` modifier is not a
/// `declare` keyword, but the file is still ambient. tsc emits TS1039.
#[test]
fn dts_file_export_const_annotated_initializer_reports_ts1039() {
    let got = codes_in_file("test.d.ts", "export const x: number = 1;");
    assert!(
        got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "an exported .d.ts annotated initializer is ambient — expected TS1039; got {got:?}"
    );
}

/// `export let x: number = 1;` — a non-`const` binding gets no literal
/// exception; tsc reports TS1039 in a `.d.ts`.
#[test]
fn dts_file_export_let_annotated_initializer_reports_ts1039() {
    let got = codes_in_file("test.d.ts", "export let x: number = 1;");
    assert!(
        got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "an exported .d.ts `let` initializer is ambient — expected TS1039; got {got:?}"
    );
}

/// `export var x = 5;` — `var`/`let` never get the const-literal exception, so
/// even a bare numeric literal initializer is TS1039 in a `.d.ts`.
#[test]
fn dts_file_export_var_literal_initializer_reports_ts1039() {
    let got = codes_in_file("test.d.ts", "export var x = 5;");
    assert!(
        got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "a `var` literal initializer in a .d.ts is TS1039; got {got:?}"
    );
}

/// `let x = 1;` (bare, no `declare`) — non-`const` literal is still TS1039.
#[test]
fn dts_file_bare_let_literal_initializer_reports_ts1039() {
    let got = codes_in_file("test.d.ts", "let x = 1;");
    assert!(
        got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "a bare .d.ts `let` literal initializer is TS1039; got {got:?}"
    );
}

/// `const x = 1 + 1;` (bare, no annotation) — the const-with-non-literal path is
/// TS1254, not TS1039, in a `.d.ts` just as under `declare`.
#[test]
fn dts_file_bare_const_computed_initializer_reports_ts1254_not_ts1039() {
    let got = codes_in_file("test.d.ts", "const x = 1 + 1;");
    assert!(
        got.contains(&CONST_INITIALIZER_MUST_BE_LITERAL)
            && !got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "a non-literal ambient const initializer is TS1254 in a .d.ts; got {got:?}"
    );
}

/// Negative control: the const-literal exception still holds in a `.d.ts`, so a
/// bare-literal `const` stays clean (no TS1039, no TS1254) — whether written
/// bare, `export`ed, or `declare`d.
#[test]
fn dts_file_const_literal_initializers_stay_clean() {
    for source in [
        "const x = 1;",
        "export const x = 1;",
        "declare const x = 1;",
    ] {
        let got = codes_in_file("test.d.ts", source);
        assert!(
            !got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT)
                && !got.contains(&CONST_INITIALIZER_MUST_BE_LITERAL),
            "a bare-literal ambient const is legal in a .d.ts (`{source}`); got {got:?}"
        );
    }
}
