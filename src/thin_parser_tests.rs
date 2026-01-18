//! Tests for ThinParser - Cache-optimized parser using ThinNodeArena.
//!
//! This module contains tests organized into sections:
//! - Basic parsing (expressions, statements, functions)
//! - Syntax constructs (classes, interfaces, generics, JSX)
//! - Error recovery and diagnostics
//! - Edge cases and performance

use crate::checker::types::diagnostics::diagnostic_codes;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::thin_parser::ThinParserState;
use std::mem::size_of;

// =============================================================================
// Basic Parsing Tests
// =============================================================================

#[test]
fn test_thin_parser_simple_expression() {
    let mut parser = ThinParserState::new("test.ts".to_string(), "1 + 2".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(parser.arena.len() > 0);

    // Should have: SourceFile, ExpressionStatement, BinaryExpression, 2 NumericLiterals
    assert!(
        parser.arena.len() >= 5,
        "Expected at least 5 nodes, got {}",
        parser.arena.len()
    );
}

#[test]
fn test_thin_parser_reset_clears_arena() {
    let mut parser = ThinParserState::new("test.ts".to_string(), "const a = 1;".to_string());
    parser.parse_source_file();

    let arena = parser.get_arena();
    assert!(
        arena
            .identifiers
            .iter()
            .any(|ident| ident.escaped_text == "a"),
        "Expected identifier 'a' after first parse"
    );

    parser.reset("test.ts".to_string(), "const b = 2;".to_string());
    parser.parse_source_file();

    let arena = parser.get_arena();
    assert!(
        arena
            .identifiers
            .iter()
            .any(|ident| ident.escaped_text == "b"),
        "Expected identifier 'b' after reset parse"
    );
    assert!(
        !arena
            .identifiers
            .iter()
            .any(|ident| ident.escaped_text == "a"),
        "Did not expect identifier 'a' after reset parse"
    );
}

#[test]
fn test_thin_parser_numeric_separator_invalid_diagnostic() {
    let source = "let x = 1_;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::NUMERIC_SEPARATORS_NOT_ALLOWED_HERE)
        .expect(&format!(
            "Expected numeric separator diagnostic, got: {:?}",
            diagnostics
        ));
    let underscore_pos = source.find('_').expect("underscore not found") as u32;
    assert_eq!(diag.start, underscore_pos);
    assert_eq!(diag.length, 1);
}

#[test]
fn test_thin_parser_numeric_separator_consecutive_diagnostic() {
    let source = "let x = 1__0;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let diag = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_NOT_PERMITTED
        })
        .expect(&format!(
            "Expected consecutive separator diagnostic, got: {:?}",
            diagnostics
        ));
    let underscore_pos = source.find("__").expect("double underscore not found") as u32 + 1;
    assert_eq!(diag.start, underscore_pos);
    assert_eq!(diag.length, 1);
}

#[test]
fn test_thin_parser_function() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "function add(a, b) { return a + b; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Unexpected errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_variable_declaration() {
    let mut parser = ThinParserState::new("test.ts".to_string(), "let x = 42;".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_thin_parser_if_statement() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "if (x > 0) { return x; } else { return -x; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_thin_parser_while_loop() {
    let mut parser =
        ThinParserState::new("test.ts".to_string(), "while (x < 10) { x++; }".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_thin_parser_for_loop() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "for (let i = 0; i < 10; i++) { console.log(i); }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_thin_parser_object_literal() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let obj = { a: 1, b: 2 };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_array_literal() {
    let mut parser =
        ThinParserState::new("test.ts".to_string(), "let arr = [1, 2, 3];".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_thin_parser_array_binding_pattern_span() {
    let source = "const [foo] = bar;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let arena = parser.get_arena();
    let binding = arena
        .nodes
        .iter()
        .find(|node| node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
        .expect("array binding pattern not found");
    let expected_end = source.find(']').expect("] not found") as u32 + 1;
    assert!(
        binding.end == expected_end,
        "span: '{}' ({}..{})",
        &source[binding.pos as usize..binding.end as usize],
        binding.pos,
        binding.end
    );
}

#[test]
fn test_thin_parser_static_keyword_member_name() {
    let source = "declare class C { static static(p): number; }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    assert!(
        parser.get_diagnostics().is_empty(),
        "Unexpected diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_modifier_keyword_as_member_name() {
    let source = "class C { static public() {} }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    assert!(
        parser.get_diagnostics().is_empty(),
        "Unexpected diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_get_accessor_type_parameters_report_ts1094() {
    let source = "class C { get foo<T>() { return 1; } }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS),
        "Expected TS1094 diagnostics: {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::TOKEN_EXPECTED),
        "Unexpected TS1005 diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_set_accessor_return_type_report_ts1095() {
    let source = "class C { set foo(value: number): number { } }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::SETTER_CANNOT_HAVE_RETURN_TYPE),
        "Expected TS1095 diagnostics: {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::TOKEN_EXPECTED),
        "Unexpected TS1005 diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_object_get_accessor_parameters_report_ts1054() {
    let source = "var v = { get foo(v: number) { } };";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::GETTER_MUST_NOT_HAVE_PARAMETERS),
        "Expected TS1054 diagnostics: {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::TOKEN_EXPECTED),
        "Unexpected TS1005 diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_duplicate_extends_reports_ts1172() {
    let source = "class C extends A extends B {}";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::EXTENDS_CLAUSE_ALREADY_SEEN),
        "Expected TS1172 diagnostics: {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::TOKEN_EXPECTED),
        "Unexpected TS1005 diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_async_function_expression_keyword_name() {
    let source = "var v = async function await(): Promise<void> { }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        !codes.contains(&diagnostic_codes::TOKEN_EXPECTED),
        "Unexpected TS1005 diagnostics: {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Unexpected TS1109 diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_static_block_with_modifiers() {
    let source = "class C { async static { } }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        !codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "Unexpected TS1128 diagnostics: {:?}",
        parser.get_diagnostics()
    );
    assert!(
        codes.contains(&diagnostic_codes::MODIFIERS_NOT_ALLOWED_HERE),
        "Expected TS1184 diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_enum_computed_property_reports_ts1164() {
    let source = "enum E { [e] = 1 }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::COMPUTED_PROPERTY_NAME_IN_ENUM),
        "Expected TS1164 diagnostics: {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::TOKEN_EXPECTED),
        "Unexpected TS1005 diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_type_assertion_in_new_expression_reports_ts1109() {
    let source = "new <T>Foo()";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Expected TS1109 diagnostics: {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::TOKEN_EXPECTED),
        "Unexpected TS1005 diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_heritage_clause_reports_specific_error() {
    // Test that invalid tokens in extends/implements clauses report specific error
    let source = "class A extends ! {}";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1109_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .collect();

    assert!(
        !ts1109_errors.is_empty(),
        "Expected TS1109 error for invalid heritage clause: {:?}",
        diagnostics
    );

    // Check that the error message is specific to heritage clauses
    let error_msg = &ts1109_errors[0].message;
    assert!(
        error_msg.contains("Class name or type expression expected"),
        "Expected 'Class name or type expression expected', got: {}",
        error_msg
    );
}

#[test]
fn test_thin_parser_implements_clause_reports_specific_error() {
    // Test that invalid tokens in implements clauses report specific error
    let source = "class C implements + {}";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1109_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .collect();

    assert!(
        !ts1109_errors.is_empty(),
        "Expected TS1109 error for invalid implements clause: {:?}",
        diagnostics
    );

    // Check that the error message is specific to heritage clauses
    let error_msg = &ts1109_errors[0].message;
    assert!(
        error_msg.contains("Class name or type expression expected"),
        "Expected 'Class name or type expression expected', got: {}",
        error_msg
    );
}

#[test]
fn test_thin_parser_generic_default_missing_type_reports_ts1110() {
    let source = "type Box<T = > = T;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected TS1110 diagnostics: {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::TOKEN_EXPECTED),
        "Unexpected TS1005 diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_jsx_like_syntax_in_ts_recovers() {
    let source = "const x = <div />;\nconst y = 1;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::TOKEN_EXPECTED),
        "Expected TS1005 diagnostics: {:?}",
        parser.get_diagnostics()
    );

    let arena = parser.get_arena();
    assert!(
        arena
            .identifiers
            .iter()
            .any(|ident| ident.escaped_text == "y"),
        "Expected identifier 'y' to be parsed after JSX-like syntax"
    );
}

#[test]
fn test_thin_parser_object_binding_pattern_span() {
    let source = "const { foo } = bar;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let arena = parser.get_arena();
    let binding = arena
        .nodes
        .iter()
        .find(|node| node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN)
        .expect("object binding pattern not found");
    let expected_end = source.find('}').expect("} not found") as u32 + 1;
    assert!(
        binding.end == expected_end,
        "span: '{}' ({}..{})",
        &source[binding.pos as usize..binding.end as usize],
        binding.pos,
        binding.end
    );
}

#[test]
fn test_thin_parser_no_substitution_template_literal_span() {
    let source = "const message = `hello`;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let arena = parser.get_arena();
    let literal = arena
        .nodes
        .iter()
        .find(|node| node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16)
        .expect("template literal not found");
    let expected_end = source.rfind('`').expect("` not found") as u32 + 1;
    assert!(
        literal.end == expected_end,
        "span: '{}' ({}..{})",
        &source[literal.pos as usize..literal.end as usize],
        literal.pos,
        literal.end
    );
}

#[test]
fn test_thin_parser_template_expression_spans() {
    let source = "const message = `hello ${name}!`;";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let arena = parser.get_arena();
    let expr = arena
        .nodes
        .iter()
        .find(|node| node.kind == syntax_kind_ext::TEMPLATE_EXPRESSION)
        .expect("template expression not found");
    let head = arena
        .nodes
        .iter()
        .find(|node| node.kind == SyntaxKind::TemplateHead as u16)
        .expect("template head not found");
    let tail = arena
        .nodes
        .iter()
        .find(|node| node.kind == SyntaxKind::TemplateTail as u16)
        .expect("template tail not found");

    let expected_head_end = source.find("${").expect("${ not found") as u32 + 2;
    let expected_tail_end = source.rfind('`').expect("` not found") as u32 + 1;
    assert!(
        head.end == expected_head_end,
        "span: '{}' ({}..{})",
        &source[head.pos as usize..head.end as usize],
        head.pos,
        head.end
    );
    assert!(
        tail.end == expected_tail_end,
        "span: '{}' ({}..{})",
        &source[tail.pos as usize..tail.end as usize],
        tail.pos,
        tail.end
    );
    assert!(
        expr.end == expected_tail_end,
        "span: '{}' ({}..{})",
        &source[expr.pos as usize..expr.end as usize],
        expr.pos,
        expr.end
    );
}

#[test]
fn test_thin_parser_unterminated_template_expression_no_crash() {
    let source = "var v = `foo ${ a";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser
            .get_diagnostics()
            .iter()
            .any(|diag| diag.code == diagnostic_codes::TOKEN_EXPECTED),
        "Expected a token expected diagnostic, got: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_unterminated_template_literal_reports_ts1160() {
    let source = "`";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser
            .get_diagnostics()
            .iter()
            .any(|diag| diag.code == diagnostic_codes::UNTERMINATED_TEMPLATE_LITERAL),
        "Expected unterminated template literal diagnostic, got: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_template_literal_property_name_no_ts1160() {
    let source = "var x = { `abc${ 123 }def${ 456 }ghi`: 321 };";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED),
        "Expected property assignment expected diagnostic, got: {:?}",
        diagnostics
    );
    assert!(
        diagnostics
            .iter()
            .all(|diag| diag.code != diagnostic_codes::UNTERMINATED_TEMPLATE_LITERAL),
        "Did not expect unterminated template literal diagnostic, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_thin_parser_call_expression() {
    let mut parser = ThinParserState::new("test.ts".to_string(), "foo(1, 2, 3);".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_thin_parser_property_access() {
    let mut parser = ThinParserState::new("test.ts".to_string(), "obj.foo.bar;".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_thin_parser_new_expression() {
    let mut parser = ThinParserState::new("test.ts".to_string(), "new Foo(1, 2);".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_thin_parser_class_declaration() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Foo { x = 1; bar() { return this.x; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_class_with_constructor() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Point { constructor(x, y) { this.x = x; this.y = y; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_class_member_named_var() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Foo { var() { return 1; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_class_extends() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Child extends Parent { constructor() { super(); } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    // May have some diagnostics for super() but should parse successfully
}

#[test]
fn test_thin_parser_class_extends_call() {
    // Class extends a mixin call
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Child extends Mixin(Parent) {}".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_class_extends_property_access() {
    // Class extends a property access
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Child extends Base.Parent {}".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_decorator_class() {
    let mut parser =
        ThinParserState::new("test.ts".to_string(), "@Component class Foo {}".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_decorator_with_call() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "@Component({ selector: 'app' }) class AppComponent {}".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_multiple_decorators() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "@Component @Injectable class Service {}".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_decorator_abstract_class() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "@Serializable abstract class Base {}".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_class_extends_and_implements() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Foo extends Base implements A, B, C {}".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_abstract_class() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "abstract class Base { abstract method(): void; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_abstract_class_in_iife() {
    // This was causing crashes before - abstract class inside IIFE
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "(function() { abstract class Foo {} return Foo; })()".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    // Should parse without crashing
}

#[test]
fn test_thin_parser_get_accessor() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Foo { get value(): number { return 42; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_set_accessor() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Foo { set value(v: number) { this._v = v; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_empty_accessor_body() {
    // Empty accessor body edge case (for ambient declarations)
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "declare class Foo { get value(): number; set value(v: number); }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    // Should parse without crashing
}

#[test]
fn test_thin_parser_get_set_pair() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Foo { private _x: number = 0; get x() { return this._x; } set x(v) { this._x = v; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_memory_efficiency() {
    // Verify that ThinParserState uses less memory per node
    let source = "let x = 1 + 2 + 3 + 4 + 5;".to_string();
    let mut parser = ThinParserState::new("test.ts".to_string(), source);
    parser.parse_source_file();

    // Calculate memory usage
    let thin_node_size = size_of::<crate::parser::thin_node::ThinNode>();
    assert_eq!(thin_node_size, 16, "ThinNode should be 16 bytes");

    // Each node uses 16 bytes + data pool entry
    // This is much better than 208 bytes per fat Node
    let total_nodes = parser.arena.len();
    let thin_memory = total_nodes * 16;
    let fat_memory = total_nodes * 208;

    println!("Nodes: {}", total_nodes);
    println!("ThinNode memory: {} bytes", thin_memory);
    println!("Fat Node memory: {} bytes", fat_memory);
    println!("Memory savings: {}x", fat_memory / thin_memory.max(1));

    assert!(
        fat_memory / thin_memory.max(1) >= 10,
        "Should have at least 10x memory savings"
    );
}

#[test]
fn test_thin_parser_interface_declaration() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "interface User { name: string; age: number; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_interface_with_methods() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "interface Service { getName(): string; setName(name: string): void; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_interface_extends() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "interface Admin extends User { role: string; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_type_alias() {
    let mut parser = ThinParserState::new("test.ts".to_string(), "type ID = string;".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_type_alias_object() {
    // Test type alias with object type (unions not yet supported)
    let mut parser = ThinParserState::new("test.ts".to_string(), "type Point = Coord;".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_index_signature() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "interface StringMap { [key: string]: string; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_readonly_index_signature() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "interface ReadonlyMap { readonly [key: string]: string; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_readonly_property_signature() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "interface Config { readonly name: string; readonly value: number; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_arrow_function_simple() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const add = (a, b) => a + b;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_arrow_function_single_param() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const double = x => x * 2;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_arrow_function_block_body() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const greet = (name) => { return name; };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_arrow_function_no_params() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const getTime = () => Date.now();".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_arrow_function_in_object_literal() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const obj = { handler: () => { }, value: 1 };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_type_assertion_angle_bracket() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const value = <number>someValue;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_literal_type_assertion_angle_bracket() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const value = <\"ok\">someValue;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_async_function() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "async function fetchData() { return await fetch(); }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_async_arrow_function() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const fetchData = async () => await fetch();".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_async_arrow_single_param() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const processItem = async item => await process(item);".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_generator_function() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "function* range(n) { for (let i = 0; i < n; i++) yield i; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_yield_expression() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "function* gen() { yield 1; yield 2; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_yield_star() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "function* delegate() { yield* otherGen(); }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_await_expression() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "async function test() { const x = await promise; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_union_type() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let x: string | number | boolean;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_intersection_type() {
    let mut parser = ThinParserState::new("test.ts".to_string(), "let x: A & B & C;".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_union_intersection_mixed() {
    // Intersection binds tighter than union: A & B | C means (A & B) | C
    let mut parser =
        ThinParserState::new("test.ts".to_string(), "let x: A & B | C & D;".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_array_type() {
    let mut parser = ThinParserState::new("test.ts".to_string(), "let arr: string[];".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_nested_array_type() {
    let mut parser =
        ThinParserState::new("test.ts".to_string(), "let matrix: number[][];".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_union_array_type() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let items: (string | number)[];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_tuple_type() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let point: [number, number];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_tuple_type_mixed() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let result: [string, number, boolean];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_tuple_array() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let points: [number, number][];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_generic_type() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let list: Array<string>;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_generic_type_multiple() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let map: Map<string, number>;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_generic_nested() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let nested: Map<string, Array<number>>;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_promise_type() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "async function fetch(): Promise<string> { return ''; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_function_type() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let callback: (x: number) => string;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_function_type_no_params() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let factory: () => Widget;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_function_type_multiple_params() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let handler: (a: string, b: number, c: boolean) => void;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_function_type_optional_param() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let fn: (x: number, y?: string) => void;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_function_type_rest_param() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let fn: (...args: number[]) => void;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_parenthesized_type_still_works() {
    // Ensure parenthesized types still work after adding function type support
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let x: (string | number);".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_literal_type_string() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        r#"let status: "success" | "error";"#.to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_literal_type_number() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let port: 80 | 443 | 8080;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_literal_type_boolean() {
    let mut parser = ThinParserState::new("test.ts".to_string(), "let flag: true;".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_typeof_type() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let copy: typeof original;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_typeof_type_qualified() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let t: typeof console.log;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

// =========================================================================
// Generic Arrow Function Tests
// =========================================================================

#[test]
fn test_thin_parser_generic_arrow_simple() {
    // Basic generic arrow function: <T>(x: T) => T
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const identity = <T>(x: T) => x;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_generic_arrow_tsx_trailing_comma() {
    let mut parser = ThinParserState::new(
        "test.tsx".to_string(),
        "const id = <T,>(x: T): T => x;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_generic_arrow_multiple_params() {
    // Multiple type parameters: <T, U>(x: T, y: U) => [T, U]
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const pair = <T, U>(x: T, y: U) => [x, y];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_generic_arrow_with_constraint() {
    // Type parameter with constraint: <T extends object>(x: T) => T
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const clone = <T extends object>(x: T) => x;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_generic_arrow_with_default() {
    // Type parameter with default: <T = string>(x: T) => T
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const wrap = <T = string>(x: T) => x;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_generic_arrow_with_constraint_and_default() {
    // Type parameter with both constraint and default
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const process = <T extends object = object>(x: T) => x;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_async_generic_arrow() {
    // Async generic arrow function
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const fetchData = async <T>(url: string) => { return url; };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_generic_arrow_expression_body() {
    // Generic arrow with expression body
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const first = <T>(arr: T[]) => arr[0];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_arrow_function_with_return_type() {
    // Arrow function with return type annotation
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const add = (a: number, b: number): number => a + b;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_arrow_type_predicate() {
    // Arrow function with type predicate return type
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const isString = (x: unknown): x is string => typeof x === \"string\";".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_this_type_predicate() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "function isString(this: any): this is string { return true; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_asserts_this_type_predicate() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "function assertString(this: any): asserts this is string { }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_asserts_this_type_predicate_without_is() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "function assertThis(this: any): asserts this { }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_constructor_type() {
    // Constructor type: new () => T
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Ctor = new () => object;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_constructor_type_with_params() {
    // Constructor type with parameters: new (x: T) => U
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Factory<T> = new (value: T) => Wrapper<T>;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_generic_constructor_type() {
    // Generic constructor type: new <T>() => T
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type GenericCtor = new <T>() => T;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

// =========================================================================
// Type Operator Tests (keyof, readonly)
// =========================================================================

#[test]
fn test_thin_parser_keyof_type() {
    // Basic keyof type: keyof T
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Keys = keyof Person;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_keyof_typeof() {
    // keyof typeof: keyof typeof obj
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Keys = keyof typeof obj;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_keyof_in_union() {
    // keyof in union type
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type PropOrKey = string | keyof T;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_readonly_array() {
    // readonly array type
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let items: readonly string[];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_readonly_tuple() {
    // readonly tuple type
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let point: readonly [number, number];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

// =========================================================================
// Indexed Access Type Tests
// =========================================================================

#[test]
fn test_thin_parser_indexed_access_type() {
    // Basic indexed access: T[K]
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Value = Person[\"name\"];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_indexed_access_keyof() {
    // Indexed access with keyof: T[keyof T]
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Values = Person[keyof Person];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_indexed_access_chain() {
    // Chained indexed access: T[K1][K2]
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Deep = Obj[\"level1\"][\"level2\"];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_indexed_access_with_array() {
    // Mix of indexed access and array: T[K][]
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Names = Person[\"name\"][];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_indexed_access_number() {
    // Indexed access with number: T[number]
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Item = Items[number];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

// =========================================================================
// Conditional Type Tests
// =========================================================================

#[test]
fn test_thin_parser_conditional_type_simple() {
    // Basic conditional type: T extends U ? X : Y
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type IsString<T> = T extends string ? true : false;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_conditional_type_nested() {
    // Nested conditional types
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type TypeName<T> = T extends string ? \"string\" : T extends number ? \"number\" : \"other\";".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_conditional_type_with_infer() {
    // Conditional type with infer
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type ReturnType<T> = T extends (...args: any[]) => infer R ? R : never;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_conditional_type_distributive() {
    // Distributive conditional type
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type NonNullable<T> = T extends null | undefined ? never : T;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_infer_type() {
    // Infer in array element position
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Flatten<T> = T extends Array<infer U> ? U : T;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

// =========================================================================
// Mapped Type Tests
// =========================================================================

#[test]
fn test_thin_parser_mapped_type_simple() {
    // Basic mapped type: { [K in keyof T]: U }
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Partial<T> = { [K in keyof T]?: T[K] };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_mapped_type_readonly() {
    // Mapped type with readonly modifier
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Readonly<T> = { readonly [K in keyof T]: T[K] };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_mapped_type_required() {
    // Mapped type removing optional: -?
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Required<T> = { [K in keyof T]-?: T[K] };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_mapped_type_as_clause() {
    // Mapped type with key remapping (as clause)
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Pick<T, K> = { [P in K as P]: T[P] };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_type_literal() {
    // Object type literal
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Point = { x: number; y: number };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_type_literal_method() {
    // Object type literal with method signature
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Calculator = { add(a: number, b: number): number; subtract(a: number, b: number): number };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

// =========================================================================
// Template Literal Type Tests
// =========================================================================

#[test]
fn test_thin_parser_template_literal_type_simple() {
    // Simple template literal type with no substitutions
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Greeting = `hello`;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_template_literal_type_with_substitution() {
    // Template literal type with type substitution
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Greeting<T extends string> = `hello ${T}`;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_template_literal_type_multiple_substitutions() {
    // Template literal type with multiple substitutions
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type FullName<F extends string, L extends string> = `${F} ${L}`;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_template_literal_type_with_union() {
    // Template literal type with union in substitution
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type EventName = `on${\"click\" | \"focus\" | \"blur\"}`;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_template_literal_type_uppercase() {
    // Template literal type with intrinsic type (Uppercase)
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Getter<K extends string> = `get${Uppercase<K>}`;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

// =========================================================================
// JSX Tests
// =========================================================================

#[test]
fn test_thin_parser_jsx_self_closing() {
    // Self-closing JSX element
    let mut parser = ThinParserState::new(
        "test.tsx".to_string(),
        "const x = <Component />;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_jsx_with_children() {
    // JSX element with children
    let mut parser = ThinParserState::new(
        "test.tsx".to_string(),
        "const x = <div><span /></div>;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_jsx_with_attributes() {
    // JSX with attributes
    let mut parser = ThinParserState::new(
        "test.tsx".to_string(),
        "const x = <div className=\"foo\" id={bar} disabled />;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_jsx_with_expression() {
    // JSX with expression children
    let mut parser = ThinParserState::new(
        "test.tsx".to_string(),
        "const x = <div>{items.map(i => <span>{i}</span>)}</div>;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_jsx_fragment() {
    // JSX fragment
    let mut parser = ThinParserState::new(
        "test.tsx".to_string(),
        "const x = <><span /><span /></>;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_jsx_spread_attribute() {
    // JSX with spread attribute
    let mut parser = ThinParserState::new(
        "test.tsx".to_string(),
        "const x = <Component {...props} />;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_jsx_namespaced() {
    // JSX with namespaced tag name
    let mut parser = ThinParserState::new(
        "test.tsx".to_string(),
        "const x = <svg:rect width={100} />;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_jsx_member_expression() {
    // JSX with member expression tag
    let mut parser = ThinParserState::new(
        "test.tsx".to_string(),
        "const x = <Foo.Bar.Baz />;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

// =========================================================================
// Import/Export Tests
// =========================================================================

#[test]
fn test_thin_parser_import_default() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        r#"import foo from "bar";"#.to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_import_named() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        r#"import { foo, bar } from "baz";"#.to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_import_namespace() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        r#"import * as foo from "bar";"#.to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_import_side_effect() {
    let mut parser = ThinParserState::new("test.ts".to_string(), r#"import "foo";"#.to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_export_function() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "export function foo() { return 1; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_export_const() {
    let mut parser =
        ThinParserState::new("test.ts".to_string(), "export const x = 42;".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_export_default() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "export default function foo() { }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_re_export() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        r#"export { foo } from "bar";"#.to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_default_re_export_specifiers() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        r#"export { default } from "bar"; export { default as Foo } from "bar";"#.to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_export_star() {
    let mut parser =
        ThinParserState::new("test.ts".to_string(), r#"export * from "foo";"#.to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

// =========================================================================
// Additional tests for common TypeScript patterns
// =========================================================================

#[test]
fn test_thin_parser_static_members() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Foo { static count: number = 0; static increment() { Foo.count++; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    // May have diagnostics for static but should parse
}

#[test]
fn test_thin_parser_private_protected() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Foo { private x: number; protected y: string; public z: boolean; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_readonly() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Foo { readonly name: string; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_constructor_parameter_properties() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Person { constructor(public name: string, private age: number) {} }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_optional_chaining() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let x = obj?.prop?.method?.()".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_optional_chain_call_with_type_arguments() {
    let mut parser =
        ThinParserState::new("test.ts".to_string(), "let x = obj?.<T>(value)".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_relational_with_parenthesized_rhs() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "if (context.flags & NodeBuilderFlags.WriteTypeParametersInQualifiedName && index < (chain.length - 1)) { }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_every_type_arrow_conditional_comma() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "if (everyType(type, t => !!t.symbol?.parent && isArrayOrTupleSymbol(t.symbol.parent) && (!memberName ? (memberName = t.symbol.escapedName, true) : memberName === t.symbol.escapedName))) { }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_thin_parser_every_type_arrow_conditional_comma_expression() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const ok = everyType(type, t => !!t.symbol?.parent && (!memberName ? (memberName = t.symbol.escapedName, true) : memberName === t.symbol.escapedName));".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_thin_parser_checker_every_type_arrow_optional_chain() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let memberName: __String; if (everyType(type, t => !!t.symbol?.parent && isArrayOrTupleSymbol(t.symbol.parent) && (!memberName ? (memberName = t.symbol.escapedName, true) : memberName === t.symbol.escapedName))) { }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_thin_parser_checker_every_type_arrow_optional_chain_line() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "if (everyType(type, t => !!t.symbol?.parent && isArrayOrTupleSymbol(t.symbol.parent) && (!memberName ? (memberName = t.symbol.escapedName, true) : memberName === t.symbol.escapedName))) { }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_thin_parser_arrow_optional_chain_with_ternary_comma() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const f = (t: any) => !!t.symbol?.parent && (!memberName ? (memberName = t.symbol.escapedName, true) : memberName === t.symbol.escapedName);".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_thin_parser_spread_in_call_arguments() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "foo(...args, 1, ...rest)".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_as_expression_followed_by_logical_or() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const x = (value as readonly number[] | undefined) || fallback".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_keyword_identifier_in_expression() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const set = new Set<number>(); set.add(1)".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_arrow_param_keyword_identifier() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const f = symbol => symbol".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_type_predicate_keyword_param() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "function isSymbol(symbol: unknown): symbol is Symbol { return true; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_namespace_identifier_assignment_statement() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let namespace = 1; namespace = 2;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_type_identifier_assignment_statement() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let type = { intrinsicName: \"\" }; type.intrinsicName = \"x\";".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_nullish_coalescing() {
    let mut parser = ThinParserState::new("test.ts".to_string(), "let x = a ?? b ?? c".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_type_predicate() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "function isString(x: any): x is string { return typeof x === 'string'; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_mapped_type() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Readonly<T> = { readonly [K in keyof T]: T[K] }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_conditional_type() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type IsString<T> = T extends string ? true : false".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_infer_type_complex() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type ReturnType<T> = T extends (...args: any[]) => infer R ? R : never".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_rest_spread() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "function foo(...args: number[]) { let [first, ...rest] = args; return [...rest, first]; }"
            .to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_destructuring_default() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let { x = 1, y = 2 } = obj; let [a = 1, b = 2] = arr;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_computed_property() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let obj = { [key]: value, ['computed']: 42 }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_symbol_property() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let obj = { [Symbol.iterator]() { } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_bigint_literal() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let x: bigint = 123n; let y = 0xFFn;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_numeric_separator() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "let x = 1_000_000; let y = 0xFF_FF_FF;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_private_identifier() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Foo { #privateField = 1; #privateMethod() {} }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_satisfies() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const obj = { x: 1, y: 2 } satisfies Record<string, number>".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_using_declaration() {
    // ECMAScript explicit resource management
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "using file = openFile(); await using conn = getConnection();".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
}

#[test]
fn test_thin_parser_static_property() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Foo { static count = 0; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_static_method() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Foo { static create(): Foo { return new Foo(); } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_private_property() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Foo { private secret: string = 'hidden'; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_protected_method() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Base { protected init(): void {} }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_readonly_property() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Foo { readonly id: number = 1; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_public_constructor() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Foo { public constructor(x: number) {} }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_static_get_accessor() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Foo { static get instance(): Foo { return _instance; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_private_set_accessor() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Foo { private set value(v: number) { this._value = v; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_multiple_modifiers() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Foo { static readonly MAX_SIZE: number = 100; private static instance: Foo; }"
            .to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_override_method() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Child extends Parent { override doSomething(): void {} }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_async_method() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "class Foo { async fetchData(): Promise<void> {} }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_abstract_method_in_class() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "abstract class Shape { abstract getArea(): number; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_call_signature() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "interface Callable { (): string; (x: number): number; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_construct_signature() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "interface Constructable { new (): MyClass; new (x: number): MyClass; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_interface_with_call_and_construct() {
    // This is a common pattern for class constructors
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        r#"interface FooConstructor {
            new (): Foo;
            prototype: Foo;
        }
        interface Foo {
            (): string;
            bar(key: string): string;
        }"#
        .to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_type_literal_with_call_signature() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type Fn = { (): void; message: string }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_accessor_signature_in_type() {
    // Accessor signatures in type context (allowed syntactically)
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type A = { get foo(): number; set foo(v: number); }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_accessor_body_in_type_context() {
    // Accessor bodies in type context (error recovery - bodies not allowed but should parse)
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "type A = { get foo() { return 0 } };".to_string(),
    );
    let root = parser.parse_source_file();

    // Should parse (checker will report error about body)
    assert!(!root.is_none());
    // Parser may report an error about unexpected token, but should recover
}

#[test]
fn test_thin_parser_interface_accessor_signature() {
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "interface X { get foo(): number; set foo(v: number); }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

// =============================================================================
// TS1005/TS1068 False Positive Regression Tests
// =============================================================================

#[test]
fn test_thin_parser_class_semicolon_element_ts1068() {
    // Regression test: Empty statement (semicolon) in class body should not error
    // Previously incorrectly reported TS1068 "Unexpected token"
    let mut parser = ThinParserState::new("test.ts".to_string(), "class C { ; }".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Class with semicolon element should not error: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_class_multiple_semicolons() {
    // Multiple semicolons in class body should be valid
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        r#"class C {
            ;
            x: number;
            ;
            ;
            y: string;
        }"#
        .to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Class with multiple semicolons should not error: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_await_as_type_name() {
    // 'await' should be valid as a type name in type annotations
    let mut parser = ThinParserState::new("test.ts".to_string(), "var v: await;".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "'await' as type name should not error: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_await_as_parameter_name() {
    // 'await' should be valid as parameter name outside async functions
    let mut parser =
        ThinParserState::new("test.ts".to_string(), "function f(await) { }".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "'await' as parameter name should not error: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_await_as_identifier_with_default() {
    // 'await' as parameter with default value (references itself)
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "function f(await = await) { }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "'await = await' should not error: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_await_in_async_function() {
    // 'await' should still work as await expression inside async functions
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "async function foo() { await bar(); }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "await in async function should not error: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_await_in_async_arrow() {
    // await in async arrow function body
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        "const f = async () => { await x(); };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "await in async arrow should not error: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_await_in_async_method() {
    // await in async class method
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        r#"class A {
            async method() {
                await this.foo();
            }
        }"#
        .to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "await in async method should not error: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_yield_as_type_name() {
    // 'yield' should be valid as a type name
    let mut parser = ThinParserState::new("test.ts".to_string(), "var v: yield;".to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "'yield' as type name should not error: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_await_type_in_async_context() {
    // 'await' as type inside async context (in type annotation, not expression)
    let mut parser = ThinParserState::new(
        "test.ts".to_string(),
        r#"var foo = async (): Promise<void> => {
            var v: await;
        }"#
        .to_string(),
    );
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(
        parser.get_diagnostics().is_empty(),
        "'await' as type in async context should not error: {:?}",
        parser.get_diagnostics()
    );
}

// Error Recovery Tests for TS1005/TS1109/TS1068/TS1128 (ArrowFunctions + Expressions)

#[test]
fn test_thin_parser_arrow_function_missing_param_type() {
    // ArrowFunction1.ts: var v = (a: ) => {};
    // Should emit TS1110 (Type expected), not TS1005
    let source = "var v = (a: ) => {};";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected TS1110 for missing parameter type: {:?}",
        parser.get_diagnostics()
    );
    // Should not emit generic "identifier expected" TS1005
    assert!(
        !codes.contains(&diagnostic_codes::TOKEN_EXPECTED),
        "Should not emit TS1005 for missing type, got: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_arrow_function_missing_param_type_paren() {
    // parserX_ArrowFunction1.ts: var v = (a: ) => {};
    // Similar to above, ensure we emit TS1110
    let source = "var v = (a: ) => { };";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected TS1110 for missing parameter type: {:?}",
        parser.get_diagnostics()
    );
}

// Parser Recovery Tests for TS1164 (Computed Property Names in Enums) and TS1005 (Missing Equals)

#[test]
fn test_thin_parser_enum_computed_property_name() {
    // Test: enum E { [x] = 1 }
    // Should emit TS1164 (Computed property names are not allowed in enums), not TS1005
    let source = "enum E { [x] = 1 }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::COMPUTED_PROPERTY_NAME_IN_ENUM),
        "Expected TS1164 for computed property name in enum: {:?}",
        parser.get_diagnostics()
    );
    // Should not emit generic "identifier expected" TS1005
    assert!(
        !codes.contains(&diagnostic_codes::TOKEN_EXPECTED),
        "Should not emit TS1005 for computed enum member, got: {:?}",
        parser.get_diagnostics()
    );
    // Parser should recover and continue parsing
    assert!(
        !parser.arena.nodes.is_empty(),
        "Parser should recover and build AST"
    );
}

#[test]
fn test_thin_parser_type_alias_missing_equals() {
    // Test: type T { x: number }
    // Should emit TS1005 ("=' expected") but recover and parse the type
    let source = "type T { x: number }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::TOKEN_EXPECTED),
        "Expected TS1005 for missing equals token: {:?}",
        parser.get_diagnostics()
    );
    // Parser should recover and build an AST node
    assert!(
        !parser.arena.nodes.is_empty(),
        "Parser should recover and build AST"
    );
}

#[test]
fn test_thin_parser_type_alias_missing_equals_recovers_with_object_type() {
    // Test: type T { x: number }
    // The parser should recover by recognizing '{' as start of an object literal type
    let source = "type T { x: number }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should emit TS1005 for missing '='
    let diags = parser.get_diagnostics();
    assert!(
        diags
            .iter()
            .any(|d| d.code == diagnostic_codes::TOKEN_EXPECTED),
        "Expected TS1005 diagnostic: {:?}",
        diags
    );
    // Parser should successfully build the AST despite the error
    assert!(
        !parser.arena.nodes.is_empty(),
        "Parser should build AST with recovery"
    );
}

#[test]
fn test_thin_parser_function_keyword_in_class_recovers() {
    // Test: class C { function foo() {} }
    // The parser should recover and treat 'function' as a property name or emit a specific error
    let source = "class C { function foo() {} }";
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Parser should recover and build the class AST
    assert!(
        !parser.arena.nodes.is_empty(),
        "Parser should recover and build class AST"
    );
    // Should not emit TS1068 (unexpected token in class) - should handle gracefully
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        !codes.contains(&diagnostic_codes::UNEXPECTED_TOKEN_CLASS_MEMBER),
        "Should not emit TS1068 for function keyword in class, got: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_throw_statement_line_break_reports_ts1109() {
    // Critical ASI bug fix: throw must have expression on same line
    // Line break between throw and expression should report TS1109 (EXPRESSION_EXPECTED)
    let source = r#"
function f() {
    throw
    new Error("test");
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should report TS1109 (EXPRESSION_EXPECTED) for the line break
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Should emit TS1109 for line break after throw, got: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_throw_statement_same_line_ok() {
    // throw with expression on same line should parse without error
    let source = r#"
function f() {
    throw new Error("test");
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should NOT report any errors
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Should not emit TS1109 for throw on same line, got: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_thin_parser_throw_statement_eof_ok() {
    // throw at EOF (before closing brace) should be fine
    let source = r#"
function f() {
    throw new Error("test")
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should NOT report any errors
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Should not emit TS1109 for throw before closing brace, got: {:?}",
        parser.get_diagnostics()
    );
}

// =============================================================================
// Break/Continue Label Storage Tests (Worker-4)
// =============================================================================

#[test]
fn test_thin_parser_break_with_label_stores_label() {
    use crate::parser::thin_node::JumpData;

    let source = r#"
outer: for (let i = 0; i < 10; i++) {
    for (let j = 0; j < 10; j++) {
        if (i === j) break outer;
    }
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(parser.get_diagnostics().is_empty());

    // Verify the label is stored
    let arena = parser.get_arena();
    let break_node = arena
        .nodes
        .iter()
        .find(|node| node.kind == syntax_kind_ext::BREAK_STATEMENT)
        .expect("break statement not found");

    let jump_data = arena.get_jump_data(break_node).expect("jump data not found");
    assert!(!jump_data.label.is_none(), "Label should be stored, not NONE");

    // Verify the label is the identifier "outer"
    if let Some(label_node) = arena.get(jump_data.label) {
        assert_eq!(label_node.kind, crate::scanner::SyntaxKind::Identifier as u16);
        if let Some(ident) = arena.get_identifier(label_node) {
            assert_eq!(ident.escaped_text, "outer");
        } else {
            panic!("Expected identifier for label");
        }
    } else {
        panic!("Label node not found in arena");
    }
}

#[test]
fn test_thin_parser_continue_with_label_stores_label() {
    use crate::parser::thin_node::JumpData;

    let source = r#"
outer: for (let i = 0; i < 10; i++) {
    for (let j = 0; j < 10; j++) {
        if (i === j) continue outer;
    }
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(parser.get_diagnostics().is_empty());

    // Verify the label is stored
    let arena = parser.get_arena();
    let continue_node = arena
        .nodes
        .iter()
        .find(|node| node.kind == syntax_kind_ext::CONTINUE_STATEMENT)
        .expect("continue statement not found");

    let jump_data = arena
        .get_jump_data(continue_node)
        .expect("jump data not found");
    assert!(!jump_data.label.is_none(), "Label should be stored, not NONE");

    // Verify the label is the identifier "outer"
    if let Some(label_node) = arena.get(jump_data.label) {
        assert_eq!(label_node.kind, crate::scanner::SyntaxKind::Identifier as u16);
        if let Some(ident) = arena.get_identifier(label_node) {
            assert_eq!(ident.escaped_text, "outer");
        } else {
            panic!("Expected identifier for label");
        }
    } else {
        panic!("Label node not found in arena");
    }
}

#[test]
fn test_thin_parser_break_without_label_has_none() {
    use crate::parser::thin_node::JumpData;

    let source = r#"
for (let i = 0; i < 10; i++) {
    if (i > 5) break;
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(parser.get_diagnostics().is_empty());

    // Verify no label is stored (should be NONE)
    let arena = parser.get_arena();
    let break_node = arena
        .nodes
        .iter()
        .find(|node| node.kind == syntax_kind_ext::BREAK_STATEMENT)
        .expect("break statement not found");

    let jump_data = arena.get_jump_data(break_node).expect("jump data not found");
    assert!(jump_data.label.is_none(), "Label should be NONE for break without label");
}

#[test]
fn test_thin_parser_continue_without_label_has_none() {
    use crate::parser::thin_node::JumpData;

    let source = r#"
for (let i = 0; i < 10; i++) {
    if (i > 5) continue;
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(parser.get_diagnostics().is_empty());

    // Verify no label is stored (should be NONE)
    let arena = parser.get_arena();
    let continue_node = arena
        .nodes
        .iter()
        .find(|node| node.kind == syntax_kind_ext::CONTINUE_STATEMENT)
        .expect("continue statement not found");

    let jump_data = arena
        .get_jump_data(continue_node)
        .expect("jump data not found");
    assert!(
        jump_data.label.is_none(),
        "Label should be NONE for continue without label"
    );
}

#[test]
fn test_thin_parser_labeled_statement_parses() {
    let source = r#"
myLabel: while (true) {
    break myLabel;
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());
    assert!(parser.get_diagnostics().is_empty());

    // Verify labeled statement is parsed
    let arena = parser.get_arena();
    let labeled_node = arena
        .nodes
        .iter()
        .find(|node| node.kind == syntax_kind_ext::LABELED_STATEMENT)
        .expect("labeled statement not found");

    assert!(labeled_node.pos > 0, "Labeled statement should have position");
}

#[test]
fn test_thin_parser_break_with_asi_before_label() {
    use crate::parser::thin_node::JumpData;

    // ASI applies before label on new line
    let source = r#"
outer: for (;;) {
    break
    outer;  // This becomes a separate expression statement (unused label)
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    assert!(!root.is_none());

    // The break should have NONE for label due to ASI
    let arena = parser.get_arena();
    let break_node = arena
        .nodes
        .iter()
        .find(|node| node.kind == syntax_kind_ext::BREAK_STATEMENT)
        .expect("break statement not found");

    let jump_data = arena.get_jump_data(break_node).expect("jump data not found");
    // After ASI, the label on the next line is a separate statement
    assert!(jump_data.label.is_none(), "Label should be NONE due to ASI after break");
}
