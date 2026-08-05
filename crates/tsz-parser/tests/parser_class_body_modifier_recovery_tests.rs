//! `parse_statements`' unclosed-`{` recovery heuristic (a class-modifier
//! keyword followed by an identifier/keyword inside a class-body-nested
//! block terminates the block, on the assumption a class member's `{` was
//! never closed) must not fire when the modifier is actually followed by a
//! real declaration keyword (`const`/`class`/`function`/...): tsc parses
//! that shape as a (misplaced) modified declaration and reports a single
//! grammar diagnostic from the statement's own modifier dispatch, not this
//! block-termination recovery. #16377.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

fn assert_single_diagnostic(source: &str, expected_code: u32, expected_pos: u32) {
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert_eq!(
        diagnostics
            .iter()
            .filter(|d| d.code == expected_code && d.start == expected_pos)
            .count(),
        1,
        "expected exactly one TS{expected_code} at position {expected_pos} for {source:?}, got {diagnostics:?}"
    );
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one diagnostic for {source:?}, got {diagnostics:?}"
    );
}

#[test]
fn modifier_before_declaration_in_method_body_reports_single_ts1184() {
    let source = "class C { method() { public const z = 1; return z; } }";
    let pos = source.find("public").unwrap() as u32;
    assert_single_diagnostic(source, diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE, pos);
}

#[test]
fn modifier_before_declaration_in_getter_body_reports_single_ts1184() {
    let source = "class C { get x() { public const z = 1; return z; } }";
    let pos = source.find("public").unwrap() as u32;
    assert_single_diagnostic(source, diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE, pos);
}

#[test]
fn modifier_before_declaration_in_setter_body_reports_single_ts1184() {
    let source = "class C { set x(v: number) { static const z = 1; } }";
    let pos = source.find("static").unwrap() as u32;
    assert_single_diagnostic(source, diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE, pos);
}

#[test]
fn modifier_before_declaration_in_constructor_body_reports_single_ts1184() {
    let source = "class C { constructor() { public const z = 1; } }";
    let pos = source.find("public").unwrap() as u32;
    assert_single_diagnostic(source, diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE, pos);
}

#[test]
fn modifier_before_declaration_in_nested_block_inside_method_reports_single_ts1184() {
    let source = "class C { method() { { public const z = 1; } } }";
    let pos = source.find("public").unwrap() as u32;
    assert_single_diagnostic(source, diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE, pos);
}

#[test]
fn every_class_modifier_keyword_before_a_declaration_reports_single_ts1184_in_a_method_body() {
    // `abstract` is deliberately excluded: `parse_statement_abstract_keyword`
    // reports TS1242 unconditionally regardless of container, a separate,
    // pre-existing bug unrelated to this heuristic (#16380). `override` is
    // also excluded: it is never a valid statement/declaration modifier
    // outside a class member, so a method body (itself a Block) gets the
    // same unconditional TS1434 as every other container, not TS1184
    // (oracle-pinned, #16403 slice 2; see the dedicated test below).
    for keyword in ["public", "private", "protected", "static", "readonly"] {
        let source = format!("class C {{ method() {{ {keyword} const z = 1; }} }}");
        let pos = source.find(keyword).unwrap() as u32;
        assert_single_diagnostic(&source, diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE, pos);
    }
}

#[test]
fn override_before_a_declaration_in_a_method_body_reports_ts1434_not_ts1184() {
    let source = "class C { method() { override const z = 1; } }";
    let pos = source.find("override").unwrap() as u32;
    assert_single_diagnostic(
        source,
        diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
        pos,
    );
}

#[test]
fn modifier_before_class_keyword_in_method_body_reports_single_ts1184() {
    let source = "class C { method() { static class D {} } }";
    let pos = source.find("static").unwrap() as u32;
    assert_single_diagnostic(source, diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE, pos);
}

#[test]
fn modifier_before_function_keyword_in_method_body_reports_single_ts1184() {
    let source = "class C { method() { static function f() {} } }";
    let pos = source.find("static").unwrap() as u32;
    assert_single_diagnostic(source, diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE, pos);
}

#[test]
fn abstract_class_declaration_in_method_body_is_legal_and_reports_nothing() {
    // `abstract class D {}` is a legal local class declaration; the modifier
    // precedes a real declaration keyword (`class`), so the unclosed-brace
    // recovery must not intercept it.
    let source = "class C { method() { abstract class D {} } }";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics for a legal local abstract class, got {diagnostics:?}"
    );
}

#[test]
fn genuinely_unclosed_method_body_before_a_class_member_still_recovers() {
    // Positive regression for the recovery this heuristic exists for: a
    // missing `}` on `method()`'s body, followed by what tsc's parser reads
    // as a new class member declaration (`public` here is NOT followed by a
    // declaration keyword, so this is not a modified-declaration shape).
    let source = "class C {\n  method() {\n    doStuff();\n  public method2() {}\n}\n";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let pos = source.find("public").unwrap() as u32;
    assert_eq!(
        diagnostics
            .iter()
            .filter(|d| d.code == diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED)
            .count(),
        1,
        "expected exactly one recovery diagnostic, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(
            |d| d.code == diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED && d.start == pos
        ),
        "expected the recovery diagnostic at the modifier token, got {diagnostics:?}"
    );
}

#[test]
fn genuinely_unclosed_method_body_before_a_class_field_still_recovers() {
    let source = "class C {\n  method() {\n    doStuff();\n  public field = 1;\n}\n";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let pos = source.find("public").unwrap() as u32;
    assert_eq!(
        diagnostics
            .iter()
            .filter(|d| d.code == diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED)
            .count(),
        1,
        "expected exactly one recovery diagnostic, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(
            |d| d.code == diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED && d.start == pos
        ),
        "expected the recovery diagnostic at the modifier token, got {diagnostics:?}"
    );
}
