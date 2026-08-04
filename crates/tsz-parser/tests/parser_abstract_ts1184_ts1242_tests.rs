//! `abstract` before a variable or function declaration (`abstract const x =
//! 1;`, `abstract function f() {}`) is a grammar error in every container,
//! but tsc picks its diagnostic from the *container*, the same split
//! `parse_statement_top_level_modifier` uses for the sibling class modifiers
//! (`public`/`private`/`protected`/`static`/`override`/`readonly`, #16368):
//! a `Block` body (function body, a nested block, or a class static block)
//! reports the generic TS1184 ("Modifiers cannot appear here"); a
//! module/namespace body or the source file's own top level — neither of
//! which is a `Block` — keeps the specific TS1242 ("'abstract' modifier can
//! only appear on a class, method, or property declaration."). #16380.

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
fn abstract_const_in_function_body_reports_ts1184() {
    let source = "function mm() { abstract const z = 1; return z; }";
    let pos = source.find("abstract").unwrap() as u32;
    assert_single_diagnostic(source, diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE, pos);
}

#[test]
fn abstract_function_in_function_body_reports_ts1184() {
    // Deliberately a plain function body, not a class method body: a class
    // body's nested-block recovery heuristic (`state_statements.rs:625`) is a
    // separate, pre-existing bug (#16377) that unconditionally terminates the
    // inner block on any class-modifier-like keyword followed by an
    // identifier, `abstract` included, regardless of container. That bug has
    // its own owner and claim; this file only covers the diagnostic-choice
    // fix in `parse_statement_abstract_keyword`.
    let source = "function mm() { abstract function f() {} }";
    let pos = source.find("abstract").unwrap() as u32;
    assert_single_diagnostic(source, diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE, pos);
}

#[test]
fn abstract_var_and_let_in_function_body_report_ts1184() {
    for keyword in ["var", "let"] {
        let source = format!("function mm() {{ abstract {keyword} z = 1; }}");
        let pos = source.find("abstract").unwrap() as u32;
        assert_single_diagnostic(&source, diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE, pos);
    }
}

#[test]
fn abstract_const_in_nested_block_inside_function_body_reports_ts1184() {
    let source = "function mm() { { abstract const z = 1; } }";
    let pos = source.find("abstract").unwrap() as u32;
    assert_single_diagnostic(source, diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE, pos);
}

#[test]
fn abstract_const_in_class_static_block_reports_ts1184() {
    let source = "class C { static { abstract const z = 1; } }";
    let pos = source.find("abstract").unwrap() as u32;
    assert_single_diagnostic(source, diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE, pos);
}

#[test]
fn abstract_const_at_namespace_top_level_reports_ts1242() {
    let source = "namespace N { abstract const z = 1; }";
    let pos = source.find("abstract").unwrap() as u32;
    assert_single_diagnostic(
        source,
        diagnostic_codes::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION,
        pos,
    );
}

#[test]
fn abstract_const_at_source_file_top_level_reports_ts1242() {
    let source = "abstract const z = 1;";
    assert_single_diagnostic(
        source,
        diagnostic_codes::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION,
        0,
    );
}

#[test]
fn abstract_before_namespace_nested_in_function_body_still_reports_ts1242() {
    // The modifier's own container is the namespace body, not the enclosing
    // function block, even though the namespace itself sits inside one.
    let source = "function mm() { namespace N { abstract const z = 1; } }";
    let pos = source.rfind("abstract").unwrap() as u32;
    assert_single_diagnostic(
        source,
        diagnostic_codes::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION,
        pos,
    );
}
