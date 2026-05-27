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

#[test]
fn test_parser_throw_statement_eof_ok() {
    // throw at EOF (before closing brace) should be fine
    let source = r#"
function f() {
    throw new Error("test")
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
        "Should not emit TS1109 for throw before closing brace, got: {:?}",
        parser.get_diagnostics()
    );
}

// =============================================================================
// Break/Continue Label Storage Tests (Worker-4)
// =============================================================================

#[test]
fn test_parser_break_with_label_stores_label() {
    let source = r#"
outer: for (let i = 0; i < 10; i++) {
    for (let j = 0; j < 10; j++) {
        if (i === j) break outer;
    }
}
"#;
    let (parser, root) = parse_test_source(source);

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());

    // Verify the label is stored
    let arena = parser.get_arena();
    let break_node = arena
        .nodes
        .iter()
        .find(|node| node.kind == syntax_kind_ext::BREAK_STATEMENT)
        .expect("break statement not found");

    let jump_data = arena
        .get_jump_data(break_node)
        .expect("jump data not found");
    assert!(
        jump_data.label.is_some(),
        "Label should be stored, not NONE"
    );

    // Verify the label is the identifier "outer"
    if let Some(label_node) = arena.get(jump_data.label) {
        assert_eq!(
            label_node.kind,
            crate::scanner::SyntaxKind::Identifier as u16
        );
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
fn test_parser_continue_with_label_stores_label() {
    let source = r#"
outer: for (let i = 0; i < 10; i++) {
    for (let j = 0; j < 10; j++) {
        if (i === j) continue outer;
    }
}
"#;
    let (parser, root) = parse_test_source(source);

    assert!(root.is_some());
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
    assert!(
        jump_data.label.is_some(),
        "Label should be stored, not NONE"
    );

    // Verify the label is the identifier "outer"
    if let Some(label_node) = arena.get(jump_data.label) {
        assert_eq!(
            label_node.kind,
            crate::scanner::SyntaxKind::Identifier as u16
        );
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
fn test_parser_break_without_label_has_none() {
    let source = r#"
for (let i = 0; i < 10; i++) {
    if (i > 5) break;
}
"#;
    let (parser, root) = parse_test_source(source);

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());

    // Verify no label is stored (should be NONE)
    let arena = parser.get_arena();
    let break_node = arena
        .nodes
        .iter()
        .find(|node| node.kind == syntax_kind_ext::BREAK_STATEMENT)
        .expect("break statement not found");

    let jump_data = arena
        .get_jump_data(break_node)
        .expect("jump data not found");
    assert!(
        jump_data.label.is_none(),
        "Label should be NONE for break without label"
    );
}

#[test]
fn test_parser_continue_without_label_has_none() {
    let source = r#"
for (let i = 0; i < 10; i++) {
    if (i > 5) continue;
}
"#;
    let (parser, root) = parse_test_source(source);

    assert!(root.is_some());
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
fn test_parser_labeled_statement_parses() {
    let source = r#"
myLabel: while (true) {
    break myLabel;
}
"#;
    let (parser, root) = parse_test_source(source);

    assert!(root.is_some());
    assert!(parser.get_diagnostics().is_empty());

    // Verify labeled statement is parsed
    let arena = parser.get_arena();
    let labeled_node = arena
        .nodes
        .iter()
        .find(|node| node.kind == syntax_kind_ext::LABELED_STATEMENT)
        .expect("labeled statement not found");

    assert!(
        labeled_node.pos > 0,
        "Labeled statement should have position"
    );
}

#[test]
fn test_parser_break_with_asi_before_label() {
    // ASI applies before label on new line
    let source = r#"
outer: for (;;) {
    break
    outer;  // This becomes a separate expression statement (unused label)
}
"#;
    let (parser, root) = parse_test_source(source);

    assert!(root.is_some());

    // The break should have NONE for label due to ASI
    let arena = parser.get_arena();
    let break_node = arena
        .nodes
        .iter()
        .find(|node| node.kind == syntax_kind_ext::BREAK_STATEMENT)
        .expect("break statement not found");

    let jump_data = arena
        .get_jump_data(break_node)
        .expect("jump data not found");
    // After ASI, the label on the next line is a separate statement
    assert!(
        jump_data.label.is_none(),
        "Label should be NONE due to ASI after break"
    );
}

// ============================================================================
// TS1038 Tests: 'declare' modifier in ambient contexts
// ============================================================================

// TS1038 Tests: 'declare' modifier in ambient contexts
// ============================================================================
#[test]
fn test_ts1038_declare_inside_declare_namespace() {
    let source = r#"declare namespace X { declare var y: number; }"#;
    let (_parser, root) = parse_test_source(source);
    assert!(root.is_some());
}

#[test]
fn test_ts1038_declare_inside_regular_namespace() {
    let source = r#"namespace M { declare module 'nope' { } }"#;
    let (_parser, root) = parse_test_source(source);
    assert!(root.is_some());
}

// =============================================================================
// Reserved Word Tests (TS1359)
// =============================================================================

#[test]
fn test_reserved_word_emits_ts1359() {
    // In a variable declaration context, reserved words should now emit TS1389
    // (the specific "not allowed as variable declaration name" error) instead of
    // the generic TS1359. TS1359 is still used in non-variable-declaration contexts.
    let source = "var break = 5;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _source_file = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();

    // Should have at least one diagnostic
    assert!(
        !diagnostics.is_empty(),
        "Expected at least one diagnostic for 'break' reserved word"
    );

    // Should have TS1389 (not TS1359) for variable declaration context
    let ts1389_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 1389).collect();
    assert!(
        !ts1389_errors.is_empty(),
        "Expected TS1389 error for 'break' in var declaration, got {diagnostics:?}"
    );

    println!("TS1389 errors: {ts1389_errors:?}");
}

#[test]
fn test_abstract_interface_emits_ts1242_not_ts1184() {
    // 'abstract interface I {}' should emit TS1242, not TS1184.
    // TSC gives the specific "'abstract' modifier can only appear on a class, method, or property declaration."
    use tsz_common::diagnostics::diagnostic_codes;

    let mut parser = ParserState::new("test.ts".to_string(), "abstract interface I {}".to_string());
    parser.parse_source_file();
    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(
            &diagnostic_codes::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION
        ),
        "Expected TS1242 for abstract interface, got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE),
        "Should NOT emit TS1184 for abstract interface, got: {codes:?}"
    );
}
