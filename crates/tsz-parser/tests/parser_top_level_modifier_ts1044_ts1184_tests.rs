//! A misplaced class-modifier keyword (`public`, `private`, `protected`,
//! `static`, `override`, `readonly`) at the start of a statement is a grammar
//! error in every container, but tsc picks its diagnostic from the
//! *container*, not the modifier: a `Block` body (function body, a nested
//! block, or a class static block) reports the generic TS1184 ("Modifiers
//! cannot appear here"); a module/namespace body or the source file's own
//! top level — neither of which is a `Block` — keeps the module/namespace-
//! specific TS1044 ("'{0}' modifier cannot appear on a module or namespace
//! element."). #16368.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

fn assert_single_diagnostic(source: &str, expected_code: u32, expected_pos: u32) {
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == expected_code && d.start == expected_pos),
        "expected TS{expected_code} at position {expected_pos} for {source:?}, got {diagnostics:?}"
    );
}

#[test]
fn modifier_in_function_body_reports_ts1184() {
    let source = "function mm() { public const z = 1; return z; }";
    let pos = source.find("public").unwrap() as u32;
    assert_single_diagnostic(source, diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE, pos);
}

#[test]
fn modifier_in_nested_block_inside_function_body_reports_ts1184() {
    let source = "function mm() { { public const z = 1; } }";
    let pos = source.find("public").unwrap() as u32;
    assert_single_diagnostic(source, diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE, pos);
}

#[test]
fn modifier_in_class_static_block_reports_ts1184() {
    let source = "class C { static { public const z = 1; } }";
    let pos = source.find("public").unwrap() as u32;
    assert_single_diagnostic(source, diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE, pos);
}

#[test]
fn modifier_at_namespace_top_level_reports_ts1044() {
    let source = "namespace N { public const z = 1; }";
    let pos = source.find("public").unwrap() as u32;
    assert_single_diagnostic(
        source,
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
        pos,
    );
}

#[test]
fn modifier_at_source_file_top_level_reports_ts1044() {
    let source = "public const z = 1;";
    assert_single_diagnostic(
        source,
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
        0,
    );
}

#[test]
fn modifier_before_namespace_nested_in_function_body_still_reports_ts1044() {
    // The modifier's own container is the namespace body, not the enclosing
    // function block, even though the namespace itself sits inside one.
    let source = "function mm() { namespace N { public const z = 1; } }";
    let pos = source.rfind("public").unwrap() as u32;
    assert_single_diagnostic(
        source,
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
        pos,
    );
}

#[test]
fn every_top_level_modifier_keyword_reports_ts1184_in_a_block() {
    for keyword in [
        "public",
        "private",
        "protected",
        "static",
        "override",
        "readonly",
    ] {
        let source = format!("function mm() {{ {keyword} const z = 1; }}");
        let pos = source.find(keyword).unwrap() as u32;
        assert_single_diagnostic(&source, diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE, pos);
    }
}
