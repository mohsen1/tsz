#[test]
fn test_parser_yield_as_type_name() {
    // 'yield' should be valid as a type name
    let mut parser = ParserState::new("test.ts".to_string(), "var v: yield;".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "'yield' as type name should not error: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_await_type_in_async_context() {
    // 'await' as type inside async context (in type annotation, not expression)
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"var foo = async (): Promise<void> => {
            var v: await;
        }"#
        .to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "'await' as type in async context should not error: {:?}",
        parser.get_diagnostics()
    );
}

// Error Recovery Tests for TS1005/TS1109/TS1068/TS1128 (ArrowFunctions + Expressions)

#[test]
fn test_parser_arrow_function_missing_param_type() {
    // ArrowFunction1.ts: var v = (a: ) => {};
    // TSC emits TS1110 "Type expected" when a type annotation colon is followed
    // by a closing paren (no type provided).
    let source = "var v = (a: ) => {};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Should emit TS1110 for missing type before ')': {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_arrow_function_missing_param_type_paren() {
    // parserX_ArrowFunction1.ts: var v = (a: ) => {};
    // TSC emits TS1110 for missing type after colon before ')'.
    let source = "var v = (a: ) => { };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Should emit TS1110 for missing type before ')': {:?}",
        parser.get_diagnostics()
    );
}

// Parser Recovery Tests for TS1164 (Computed Property Names in Enums) and TS1005 (Missing Equals)

#[test]
fn test_parser_enum_computed_property_name() {
    // Test: enum E { [x] = 1 }
    // TS1164 is now emitted by the checker (grammar check), not the parser.
    // Parser should recover gracefully and not emit TS1005.
    let source = "enum E { [x] = 1 }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    // Should not emit generic "identifier expected" TS1005
    assert!(
        !codes.contains(&diagnostic_codes::EXPECTED),
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
fn test_parser_type_alias_missing_equals() {
    // Test: type T { x: number }
    // Should emit TS1005 ("=' expected") but recover and parse the type
    let source = "type T { x: number }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
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
fn test_parser_type_alias_missing_equals_recovers_with_object_type() {
    // Test: type T { x: number }
    // The parser should recover by recognizing '{' as start of an object literal type
    let source = "type T { x: number }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should emit TS1005 for missing '='
    let diags = parser.get_diagnostics();
    assert!(
        diags.iter().any(|d| d.code == diagnostic_codes::EXPECTED),
        "Expected TS1005 diagnostic: {diags:?}"
    );
    // Parser should successfully build the AST despite the error
    assert!(
        !parser.arena.nodes.is_empty(),
        "Parser should build AST with recovery"
    );
}

#[test]
fn test_parser_function_keyword_in_class_recovers() {
    // Test: class C { function foo() {} }
    // tsc emits TS1068 at `function` plus TS1128 recovery at the class close.
    let source = "class C { function foo() {} }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Parser should recover and build the class AST
    assert!(
        !parser.arena.nodes.is_empty(),
        "Parser should recover and build class AST"
    );
    // Match tsc recovery: TS1068 + TS1128.
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(
            &diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED
        ),
        "Expected TS1068 for function keyword in class, got: {:?}",
        parser.get_diagnostics()
    );
    assert!(
        codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED)
            || codes.contains(&diagnostic_codes::EXPECTED),
        "Expected TS1128 or TS1005 recovery after invalid function keyword in class, got: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_throw_statement_line_break_reports_ts1142() {
    // Critical ASI bug fix: throw must have expression on same line
    // Line break between throw and expression should report TS1142 (LINE_BREAK_NOT_PERMITTED_HERE)
    let source = r#"
function f() {
    throw
    new Error("test");
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should report TS1142 (LINE_BREAK_NOT_PERMITTED_HERE) for the line break
    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::LINE_BREAK_NOT_PERMITTED_HERE),
        "Should emit TS1142 for line break after throw, got: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_throw_statement_same_line_ok() {
    // throw with expression on same line should parse without error
    let source = r#"
function f() {
    throw new Error("test");
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
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

