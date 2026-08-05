//! `declare export namespace N {}` / `declare export module M {}` (#16403
//! residual). `parse_ambient_declaration_with_modifiers`'s `ExportKeyword` arm
//! had an explicit exclusion that skipped TS1029 ("'export' modifier must
//! precede 'declare' modifier.") for these two forms, with a comment claiming
//! "tsc 6.0 accepts this form without TS1029 for ambient module/namespace
//! declarations." That is no longer true on the pinned oracle: `tsc` 7.0.2
//! reports TS1029 for both forms, at the source-file top level and nested in
//! a namespace body alike — only a Block silences it (the nested-namespace's
//! own TS1235 wins there instead, matching every sibling modifier family).
//!
//! Every expectation oracle-pinned against `typescript@7.0.2`
//! (`--strict --target es2022 --module es2022`).

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

fn codes(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    let mut codes: Vec<u32> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes.dedup();
    codes
}

fn assert_ts1029_export_before_declare(source: &str) {
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let export_pos = source.find("export").unwrap() as u32;
    assert!(
        diagnostics.iter().any(
            |d| d.code == diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER && d.start == export_pos
        ),
        "expected TS1029 anchored at 'export' ({export_pos}) for {source:?}, got {diagnostics:?}"
    );
}

#[test]
fn declare_export_namespace_at_top_level_reports_ts1029() {
    assert_ts1029_export_before_declare("declare export namespace N {}");
}

#[test]
fn declare_export_module_at_top_level_reports_ts1029() {
    assert_ts1029_export_before_declare("declare export module M {}");
}

#[test]
fn declare_export_namespace_dotted_name_at_top_level_reports_ts1029() {
    assert_ts1029_export_before_declare("declare export namespace N.M {}");
}

#[test]
fn declare_export_namespace_in_a_namespace_body_reports_ts1029() {
    // Nesting is legal here (unlike a Block), so tsc still reports the
    // ordering violation — the same "namespace body is not a Block" split
    // every sibling modifier family already draws.
    assert_ts1029_export_before_declare("namespace M { declare export namespace N {} }");
}

#[test]
fn declare_export_namespace_in_a_block_reports_no_modifier_diagnostic() {
    // A nested namespace declaration is itself illegal inside a Block
    // (TS1235, checker-side, outside this parser-only harness) and that
    // placement diagnostic wins outright — no TS1029 alongside it, matching
    // every sibling modifier family's own `ModuleDeclaration` handling.
    let source = "function f() { declare export namespace N {} }";
    assert_eq!(
        codes(source),
        Vec::<u32>::new(),
        "expected no parser diagnostic for {source:?}, got {:?}",
        codes(source)
    );
}

#[test]
fn plain_declare_namespace_without_export_stays_clean() {
    // Negative control: no stray `export` before `declare`, nothing to
    // reorder.
    assert_eq!(codes("declare namespace N {}"), Vec::<u32>::new());
}

#[test]
fn declare_export_namespace_already_ambient_reports_no_ts1029() {
    // Nested inside another ambient declaration, tsc emits TS1038 instead
    // (checker-side) and never TS1029 alongside it — the pre-existing
    // ambient-context exclusion in the same guard, unaffected by this fix.
    assert_eq!(
        codes("declare module \"m\" { declare export namespace N {} }"),
        Vec::<u32>::new()
    );
}
