#[test]
fn test_parser_accessor_body_in_type_context() {
    // Accessor bodies in type context (error recovery - bodies not allowed but should parse)
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "type A = { get foo() { return 0 } };".to_string(),
    );
    let root = parser.parse_source_file();

    // Should parse (checker will report error about body)
    assert!(root.is_some());
    // Parser may report an error about unexpected token, but should recover
}

#[test]
fn test_parser_interface_accessor_signature() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "interface X { get foo(): number; set foo(v: number); }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
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
fn test_parser_class_semicolon_element_ts1068() {
    // Regression test: Empty statement (semicolon) in class body should not error
    // Previously incorrectly reported TS1068 "Unexpected token"
    let mut parser = ParserState::new("test.ts".to_string(), "class C { ; }".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Class with semicolon element should not error: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_class_multiple_semicolons() {
    // Multiple semicolons in class body should be valid
    let mut parser = ParserState::new(
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

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Class with multiple semicolons should not error: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_await_as_type_name() {
    // 'await' should be valid as a type name in type annotations
    let mut parser = ParserState::new("test.ts".to_string(), "var v: await;".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "'await' as type name should not error: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_await_as_parameter_name() {
    // 'await' should be valid as parameter name outside async functions
    let mut parser = ParserState::new("test.ts".to_string(), "function f(await) { }".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "'await' as parameter name should not error: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_await_as_identifier_with_default() {
    // 'await' as parameter with default value (references itself)
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "function f(await = await) { }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "'await = await' should not error: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_await_in_async_function() {
    // 'await' should still work as await expression inside async functions
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "async function foo() { await bar(); }".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "await in async function should not error: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_await_in_async_arrow() {
    // await in async arrow function body
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "const f = async () => { await x(); };".to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "await in async arrow should not error: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_await_in_async_method() {
    // await in async class method
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"class A {
            async method() {
                await this.foo();
            }
        }"#
        .to_string(),
    );
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "await in async method should not error: {:?}",
        parser.get_diagnostics()
    );
}

