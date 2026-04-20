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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

