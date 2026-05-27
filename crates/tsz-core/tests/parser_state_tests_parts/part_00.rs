#[test]
fn test_parser_simple_expression() {
    let (parser, root) = parse_test_source("1 + 2");

    assert!(root.is_some());
    assert!(!parser.arena.is_empty());

    // Should have: SourceFile, ExpressionStatement, BinaryExpression, 2 NumericLiterals
    assert!(
        parser.arena.len() >= 5,
        "Expected at least 5 nodes, got {}",
        parser.arena.len()
    );
}

#[test]
fn test_parser_reset_clears_arena() {
    let mut parser = ParserState::new("test.ts".to_string(), "const a = 1;".to_string());
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
fn test_parser_numeric_separator_invalid_diagnostic() {
    let source = "let x = 1_;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE)
        .unwrap_or_else(|| panic!("Expected numeric separator diagnostic, got: {diagnostics:?}"));
    let underscore_pos = source.find('_').expect("underscore not found") as u32;
    assert_eq!(diag.start, underscore_pos);
    assert_eq!(diag.length, 1);
}

#[test]
fn test_parser_numeric_separator_consecutive_diagnostic() {
    let source = "let x = 1__0;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let diag = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_ARE_NOT_PERMITTED
        })
        .unwrap_or_else(|| {
            panic!("Expected consecutive separator diagnostic, got: {diagnostics:?}")
        });
    let underscore_pos = source.find("__").expect("double underscore not found") as u32 + 1;
    assert_eq!(diag.start, underscore_pos);
    assert_eq!(diag.length, 1);
}

#[test]
fn test_parser_function() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function add(a, b) { return a + b; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Unexpected errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_variable_declaration() {
    let mut parser = ParserState::new("test.ts".to_string(), "let x = 42;".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_if_statement() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "if (x > 0) { return x; } else { return -x; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_while_loop() {
    let mut parser = ParserState::new("test.ts".to_string(), "while (x < 10) { x++; }".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_for_loop() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "for (let i = 0; i < 10; i++) { console.log(i); }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_object_literal() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let obj = { a: 1, b: 2 };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_array_literal() {
    let mut parser = ParserState::new("test.ts".to_string(), "let arr = [1, 2, 3];".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_array_binding_pattern_span() {
    let source = "const [foo] = bar;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
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
fn test_parser_static_keyword_member_name() {
    let source = "declare class C { static static(p): number; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    assert!(
        parser.get_diagnostics().is_empty(),
        "Unexpected diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_modifier_keyword_as_member_name() {
    let source = "class C { static public() {} }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    assert!(
        parser.get_diagnostics().is_empty(),
        "Unexpected diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_get_accessor_type_parameters_report_ts1094() {
    let source = "class C { get foo<T>() { return 1; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::AN_ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS),
        "Expected TS1094 diagnostics: {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPECTED),
        "Unexpected TS1005 diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_set_accessor_return_type_report_ts1095() {
    let source = "class C { set foo(value: number): number { } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::A_SET_ACCESSOR_CANNOT_HAVE_A_RETURN_TYPE_ANNOTATION),
        "Expected TS1095 diagnostics: {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPECTED),
        "Unexpected TS1005 diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_object_get_accessor_parameters_report_ts1054() {
    let source = "var v = { get foo(v: number) { } };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::A_GET_ACCESSOR_CANNOT_HAVE_PARAMETERS),
        "Expected TS1054 diagnostics: {:?}",
        parser.get_diagnostics()
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPECTED),
        "Unexpected TS1005 diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_duplicate_extends_reports_ts1172() {
    let source = "class C extends A extends B {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
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
        !codes.contains(&diagnostic_codes::EXPECTED),
        "Unexpected TS1005 diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_async_function_expression_keyword_name() {
    let source = "var v = async function await(): Promise<void> { }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        !codes.contains(&diagnostic_codes::EXPECTED),
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
fn test_parser_static_block_with_modifiers() {
    let source = "class C { async static { } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
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
        codes.contains(&diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE),
        "Expected TS1184 diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_enum_computed_property_reports_ts1164() {
    // TS1164 is now emitted by the checker (grammar check), not the parser,
    // matching tsc where it's a grammar error. Parser just recovers gracefully.
    let source = "enum E { [e] = 1 }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        !codes.contains(&diagnostic_codes::EXPECTED),
        "Unexpected TS1005 diagnostics: {:?}",
        parser.get_diagnostics()
    );
    // Parser should still produce valid AST
    assert!(
        !parser.arena.nodes.is_empty(),
        "Parser should recover and build AST"
    );
}

#[test]
fn test_parser_type_assertion_in_new_expression_reports_ts1109() {
    let source = "new <T>Foo()";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
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
        !codes.contains(&diagnostic_codes::EXPECTED),
        "Unexpected TS1005 diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_heritage_clause_reports_specific_error() {
    // Test that invalid tokens in extends/implements clauses report specific error
    let source = "class A extends ! {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1109_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .collect();

    assert!(
        !ts1109_errors.is_empty(),
        "Expected TS1109 error for invalid heritage clause: {diagnostics:?}"
    );

    // Check that the error message is specific to heritage clauses
    let error_msg = &ts1109_errors[0].message;
    assert!(
        error_msg.contains("Class name or type expression expected"),
        "Expected 'Class name or type expression expected', got: {error_msg}"
    );
}

#[test]
fn test_parser_implements_clause_reports_specific_error() {
    // Test that invalid tokens in implements clauses report specific error
    let source = "class C implements + {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1109_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .collect();

    assert!(
        !ts1109_errors.is_empty(),
        "Expected TS1109 error for invalid implements clause: {diagnostics:?}"
    );

    // Check that the error message is specific to heritage clauses
    let error_msg = &ts1109_errors[0].message;
    assert!(
        error_msg.contains("Class name or type expression expected"),
        "Expected 'Class name or type expression expected', got: {error_msg}"
    );
}

#[test]
fn test_parser_generic_default_missing_type_reports_ts1110() {
    let source = "type Box<T = > = T;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
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
        !codes.contains(&diagnostic_codes::EXPECTED),
        "Unexpected TS1005 diagnostics: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_jsx_like_syntax_in_ts_recovers() {
    let source = "const x = <div />;\nconst y = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
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

/// Test that object binding pattern spans are correct
///
/// NOTE: Currently ignored - object binding pattern span computation is not
/// fully implemented. The span doesn't correctly extend to include the entire pattern.
#[test]
fn test_parser_object_binding_pattern_span() {
    let source = "const { foo } = bar;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
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
fn test_parser_no_substitution_template_literal_span() {
    let source = "const message = `hello`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
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
fn test_parser_template_expression_spans() {
    let source = "const message = `hello ${name}!`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
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
fn test_parser_unterminated_template_expression_no_crash() {
    let source = "var v = `foo ${ a";
    let (parser, root) = parse_test_source(source);

    assert!(root.is_some());
    assert!(
        parser
            .get_diagnostics()
            .iter()
            .any(|diag| diag.code == diagnostic_codes::EXPECTED),
        "Expected a token expected diagnostic, got: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_unterminated_template_literal_reports_ts1160() {
    let source = "`";
    let (parser, root) = parse_test_source(source);

    assert!(root.is_some());
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
fn test_parser_template_literal_property_name_no_ts1160() {
    let source = "var x = { `abc${ 123 }def${ 456 }ghi`: 321 };";
    let (parser, root) = parse_test_source(source);

    assert!(root.is_some());
    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED),
        "Expected property assignment expected diagnostic, got: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|diag| diag.code != diagnostic_codes::UNTERMINATED_TEMPLATE_LITERAL),
        "Did not expect unterminated template literal diagnostic, got: {diagnostics:?}"
    );
}

#[test]
fn test_parser_double_comma_emits_ts1136() {
    let source = "Boolean({ x: 0,, });";
    let (parser, root) = parse_test_source(source);

    assert!(root.is_some());
    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::PROPERTY_ASSIGNMENT_EXPECTED),
        "Expected TS1136 property assignment expected diagnostic for double comma, got: {diagnostics:?}"
    );
}

#[test]
fn test_parser_call_expression() {
    let mut parser = ParserState::new("test.ts".to_string(), "foo(1, 2, 3);".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_property_access() {
    let mut parser = ParserState::new("test.ts".to_string(), "obj.foo.bar;".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_new_expression() {
    let mut parser = ParserState::new("test.ts".to_string(), "new Foo(1, 2);".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());
}

#[test]
fn test_parser_class_declaration() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { x = 1; bar() { return this.x; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_class_with_constructor() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Point { constructor(x, y) { this.x = x; this.y = y; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_class_member_named_var() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { var() { return 1; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_class_extends() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Child extends Parent { constructor() { super(); } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    // May have some diagnostics for super() but should parse successfully
}

#[test]
fn test_parser_class_extends_call() {
    // Class extends a mixin call
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Child extends Mixin(Parent) {}".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_class_extends_property_access() {
    // Class extends a property access
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Child extends Base.Parent {}".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_decorator_class() {
    let (parser, root) = parse_test_source("@Component class Foo {}");

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_decorator_with_call() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "@Component({ selector: 'app' }) class AppComponent {}".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_multiple_decorators() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "@Component @Injectable class Service {}".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_decorator_abstract_class() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "@Serializable abstract class Base {}".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_class_extends_and_implements() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo extends Base implements A, B, C {}".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_abstract_class() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "abstract class Base { abstract method(): void; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_abstract_class_in_iife() {
    // This was causing crashes before - abstract class inside IIFE
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "(function() { abstract class Foo {} return Foo; })()".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    // Should parse without crashing
}

#[test]
fn test_parser_get_accessor() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { get value(): number { return 42; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_set_accessor() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { set value(v: number) { this._v = v; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_empty_accessor_body() {
    // Empty accessor body edge case (for ambient declarations)
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "declare class Foo { get value(): number; set value(v: number); }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    // Should parse without crashing
}

#[test]
fn test_parser_get_set_pair() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "class Foo { private _x: number = 0; get x() { return this._x; } set x(v) { this._x = v; } }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_memory_efficiency() {
    // Verify that ParserState uses less memory per node
    let source = "let x = 1 + 2 + 3 + 4 + 5;".to_string();
    let mut parser = ParserState::new("test.ts".to_string(), source);
    parser.parse_source_file();

    // Calculate memory usage
    let node_size = size_of::<crate::parser::node::Node>();
    assert_eq!(node_size, 16, "Node should be 16 bytes");

    // Each node uses 16 bytes + data pool entry
    // This is much better than 208 bytes per fat Node
    let total_nodes = parser.arena.len();
    let node_memory = total_nodes * 16;
    let fat_memory = total_nodes * 208;

    println!("Nodes: {total_nodes}");
    println!("Node memory: {node_memory} bytes");
    println!("Fat Node memory: {fat_memory} bytes");
    println!("Memory savings: {}x", fat_memory / node_memory.max(1));

    assert!(
        fat_memory / node_memory.max(1) >= 10,
        "Should have at least 10x memory savings"
    );
}

#[test]
fn test_parser_interface_declaration() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface User { name: string; age: number; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_interface_with_methods() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface Service { getName(): string; setName(name: string): void; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_interface_extends() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface Admin extends User { role: string; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_type_alias() {
    let mut parser = ParserState::new("test.ts".to_string(), "type ID = string;".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_type_alias_object() {
    // Test type alias with object type (unions not yet supported)
    let mut parser = ParserState::new("test.ts".to_string(), "type Point = Coord;".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_index_signature() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface StringMap { [key: string]: string; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_readonly_index_signature() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface ReadonlyMap { readonly [key: string]: string; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_readonly_property_signature() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface Config { readonly name: string; readonly value: number; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_arrow_function_simple() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const add = (a, b) => a + b;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_arrow_function_single_param() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const double = x => x * 2;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_arrow_function_block_body() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const greet = (name) => { return name; };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_arrow_function_no_params() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const getTime = () => Date.now();".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_arrow_function_in_object_literal() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const obj = { handler: () => { }, value: 1 };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_type_assertion_angle_bracket() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const value = <number>someValue;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_literal_type_assertion_angle_bracket() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const value = <\"ok\">someValue;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_async_function() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "async function fetchData() { return await fetch(); }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_async_arrow_function() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const fetchData = async () => await fetch();".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_async_arrow_single_param() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const processItem = async item => await process(item);".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_generator_function() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function* range(n) { for (let i = 0; i < n; i++) yield i; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_yield_expression() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function* gen() { yield 1; yield 2; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_yield_star() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function* delegate() { yield* otherGen(); }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_await_expression() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "async function test() { const x = await promise; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_union_type() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let x: string | number | boolean;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_intersection_type() {
    let mut parser = ParserState::new("test.ts".to_string(), "let x: A & B & C;".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_union_intersection_mixed() {
    // Intersection binds tighter than union: A & B | C means (A & B) | C
    let mut parser = ParserState::new("test.ts".to_string(), "let x: A & B | C & D;".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_array_type() {
    let mut parser = ParserState::new("test.ts".to_string(), "let arr: string[];".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_nested_array_type() {
    let mut parser = ParserState::new("test.ts".to_string(), "let matrix: number[][];".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_union_array_type() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let items: (string | number)[];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_tuple_type() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let point: [number, number];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_tuple_type_mixed() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let result: [string, number, boolean];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_tuple_array() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let points: [number, number][];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_generic_type() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let list: Array<string>;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_generic_type_multiple() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let map: Map<string, number>;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_generic_nested() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let nested: Map<string, Array<number>>;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_promise_type() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "async function fetch(): Promise<string> { return ''; }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_function_type() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let callback: (x: number) => string;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_function_type_no_params() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let factory: () => Widget;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_function_type_multiple_params() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let handler: (a: string, b: number, c: boolean) => void;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_function_type_optional_param() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let fn: (x: number, y?: string) => void;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_function_type_rest_param() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let fn: (...args: number[]) => void;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_parenthesized_type_still_works() {
    // Ensure parenthesized types still work after adding function type support
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let x: (string | number);".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_literal_type_string() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"let status: "success" | "error";"#.to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_literal_type_number() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let port: 80 | 443 | 8080;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_literal_type_boolean() {
    let mut parser = ParserState::new("test.ts".to_string(), "let flag: true;".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_typeof_type() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let copy: typeof original;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_typeof_type_qualified() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "let t: typeof console.log;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
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
fn test_parser_generic_arrow_simple() {
    // Basic generic arrow function: <T>(x: T) => T
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const identity = <T>(x: T) => x;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_generic_arrow_tsx_trailing_comma() {
    let mut parser = ParserState::new(
        "test.tsx".to_string(),
        "const id = <T,>(x: T): T => x;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_generic_arrow_multiple_params() {
    // Multiple type parameters: <T, U>(x: T, y: U) => [T, U]
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const pair = <T, U>(x: T, y: U) => [x, y];".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_generic_arrow_with_constraint() {
    // Type parameter with constraint: <T extends object>(x: T) => T
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const clone = <T extends object>(x: T) => x;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_generic_arrow_with_default() {
    // Type parameter with default: <T = string>(x: T) => T
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const wrap = <T = string>(x: T) => x;".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

