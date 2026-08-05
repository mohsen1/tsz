//! `declare export default class {}` / `declare export default function f()
//! {}` (#16403). `parse_ambient_declaration_with_modifiers`'s `ExportKeyword`
//! arm dispatches on the token that follows `export`, but had no case for
//! `default` — so it fell into the `_ => error_declaration_expected()`
//! fallback and reported TS1146 ("Declaration expected.") plus a cascading
//! TS1211/TS1005 from the abandoned parse, where tsc reports TS1029
//! ("'export' modifier must precede 'declare' modifier.") for the
//! declaration forms.
//!
//! `declare export default <expr>` (no class/function keyword) is the
//! sibling gap covered by the existing `EqualsToken`-shaped assignment path:
//! same missing `DefaultKeyword` arm, same TS1146 fallback beforehand.
//!
//! Every expectation oracle-pinned against `typescript@7.0.2`
//! (`--strict --target es2022 --module es2022`).
//!
//! Residual, not fixed here (see #16403 follow-up): tsc additionally reports
//! TS1183 ("An implementation cannot be declared in ambient contexts.") for
//! the function form, and suppresses this crate's own namespace-body TS1319
//! ("A default export can only be used in an ECMAScript-style module.") when
//! TS1029 already fired for the same node. Both need the `declare`/`export`
//! modifiers threaded onto the produced node (this fix reuses
//! `parse_export_default`, which does not carry them) rather than a parser
//! shape change, so they are left for a follow-up slice.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

fn assert_no_declaration_expected_fallback(source: &str) {
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::DECLARATION_EXPECTED),
        "expected no TS1146 fallback for {source:?}, got {diagnostics:?}"
    );
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

// -- top level --

#[test]
fn declare_export_default_class_top_level_reports_ts1029_not_ts1146() {
    let source = "declare export default class {}";
    assert_no_declaration_expected_fallback(source);
    assert_ts1029_export_before_declare(source);
}

#[test]
fn declare_export_default_function_top_level_reports_ts1029_not_ts1146() {
    let source = "declare export default function f(): void;";
    assert_no_declaration_expected_fallback(source);
    assert_ts1029_export_before_declare(source);
}

#[test]
fn declare_export_default_named_class_top_level_reports_ts1029_not_ts1146() {
    let source = "declare export default class Foo {}";
    assert_no_declaration_expected_fallback(source);
    assert_ts1029_export_before_declare(source);
}

// -- namespace body --

#[test]
fn declare_export_default_class_in_namespace_reports_no_ts1146() {
    assert_no_declaration_expected_fallback("namespace N { declare export default class {} }");
}

#[test]
fn declare_export_default_function_in_namespace_reports_no_ts1146() {
    assert_no_declaration_expected_fallback(
        "namespace N { declare export default function f(): void; }",
    );
}

// -- block body: parser must not itself emit a diagnostic here (the
// checker's own grammar pass owns TS1184 in a Block, per the sibling
// modifier families) but it must still classify the declaration correctly
// rather than falling into the expression-statement recovery path.

#[test]
fn declare_export_default_class_in_block_reports_no_ts1146() {
    assert_no_declaration_expected_fallback(
        "function outer() { { declare export default class {} } }",
    );
}

#[test]
fn declare_export_default_function_in_block_reports_no_ts1146() {
    assert_no_declaration_expected_fallback(
        "function outer() { { declare export default function f(): void; } }",
    );
}

// -- negative control: a non-declaration `default` expression must not be
// swept into the declaration path (`declare export default 1;` is the
// TS1120-style export-assignment shape, not a class/function).

#[test]
fn declare_export_default_expression_reports_no_ts1146() {
    assert_no_declaration_expected_fallback("declare export default 1;");
}
