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

