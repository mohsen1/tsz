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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(root.is_some());
}

#[test]
fn test_ts1038_declare_inside_regular_namespace() {
    let source = r#"namespace M { declare module 'nope' { } }"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

