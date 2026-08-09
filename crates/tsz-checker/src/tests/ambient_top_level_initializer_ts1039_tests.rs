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

/// In a `.d.ts` file every top-level declaration is implicitly ambient, so a
/// bare (no `declare` keyword) annotated initializer is an ambient-initializer
/// grammar error. Oracle-verified against `typescript@7.0.2`: `tsc` reports
/// TS1039 for this shape. tsz previously missed it because the check gated on
/// the arena-only `is_in_ambient_context` (explicit `declare` ancestor only);
/// the fix (#17086) gates on the file-aware `is_ambient_declaration`.
#[test]
fn dts_file_bare_annotated_const_initializer_reports_ts1039() {
    let got = codes_in_file("test.d.ts", "const x: number = 1;");
    assert!(
        got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "expected TS1039 for a bare .d.ts annotated const initializer, got {got:?}"
    );
}

#[test]
fn dts_file_exported_annotated_const_initializer_reports_ts1039() {
    let got = codes_in_file("test.d.ts", "export const a: number = 1;");
    assert!(
        got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "expected TS1039 for an exported .d.ts annotated const initializer, got {got:?}"
    );
}

#[test]
fn dts_file_exported_let_initializer_reports_ts1039() {
    let got = codes_in_file("test.d.ts", "export let a: number = 1;");
    assert!(
        got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "expected TS1039 for an exported .d.ts let initializer, got {got:?}"
    );
}

/// A bare `const` in a `.d.ts` with a non-literal initializer and no
/// annotation is the TS1254 sibling, not TS1039.
#[test]
fn dts_file_exported_const_object_initializer_reports_ts1254() {
    let got = codes_in_file("test.d.ts", "export const a = {};");
    assert!(
        got.contains(&CONST_INITIALIZER_MUST_BE_LITERAL),
        "expected TS1254 for a .d.ts untyped const with an object initializer, got {got:?}"
    );
    assert!(
        !got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "did not expect TS1039 for the no-annotation case, got {got:?}"
    );
}

/// A renamed binder must behave identically — the check is structural, not
/// name-driven.
#[test]
fn dts_file_renamed_binder_reports_ts1039() {
    let got = codes_in_file(
        "test.d.ts",
        "export const totallyDifferentName: number = 1;",
    );
    assert!(
        got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "expected TS1039 for a renamed .d.ts binder, got {got:?}"
    );
}

/// A literal-initialized untyped `const` in a `.d.ts` is a valid ambient
/// const and stays clean on both codes.
#[test]
fn dts_file_untyped_const_literal_initializer_is_clean() {
    let got = codes_in_file("test.d.ts", "export const a = 1;");
    assert!(
        !got.contains(&CONST_INITIALIZER_MUST_BE_LITERAL)
            && !got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "expected no ambient-initializer diagnostics for a literal const, got {got:?}"
    );
}

/// Negative control: a `.d.ts` declaration without an initializer stays clean.
#[test]
fn dts_file_declaration_without_initializer_is_clean() {
    let got = codes_in_file("test.d.ts", "export const a: number;");
    assert!(
        !got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT)
            && !got.contains(&CONST_INITIALIZER_MUST_BE_LITERAL),
        "expected no ambient-initializer diagnostics for an initializer-less .d.ts decl, got {got:?}"
    );
}

/// An explicit `declare const` in a `.d.ts` must still report a single TS1039
/// — the file-aware gate must not double-emit with the `declare`-ancestor path.
#[test]
fn dts_file_explicit_declare_const_reports_single_ts1039() {
    let diags = check_source_diagnostics("test.d.ts", "declare const d: number = 1;");
    let ts1039_count = diags
        .iter()
        .filter(|d| d.code == INITIALIZERS_NOT_ALLOWED_IN_AMBIENT)
        .count();
    assert_eq!(
        ts1039_count, 1,
        "expected exactly one TS1039 for an explicit declare const in a .d.ts, got {ts1039_count}"
    );
}

/// A `.ts` (non-declaration) file without a `declare` ancestor must remain
/// unaffected — the gate widening is scoped to declaration files only.
#[test]
fn ts_file_bare_annotated_const_initializer_stays_clean() {
    let got = codes_in_file("test.ts", "const x: number = 1;");
    assert!(
        !got.contains(&INITIALIZERS_NOT_ALLOWED_IN_AMBIENT),
        "a normal .ts initializer must not report TS1039, got {got:?}"
    );
}
