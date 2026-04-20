#[test]
fn test_parser_import_default() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"import foo from "bar";"#.to_string(),
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
fn test_parser_import_named() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"import { foo, bar } from "baz";"#.to_string(),
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
fn test_parser_import_namespace() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"import * as foo from "bar";"#.to_string(),
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
fn test_parser_import_side_effect() {
    let mut parser = ParserState::new("test.ts".to_string(), r#"import "foo";"#.to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_export_function() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "export function foo() { return 1; }".to_string(),
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
fn test_parser_export_const() {
    let mut parser = ParserState::new("test.ts".to_string(), "export const x = 42;".to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_parser_export_default() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        "export default function foo() { }".to_string(),
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
fn test_parser_re_export() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"export { foo } from "bar";"#.to_string(),
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
fn test_parser_default_re_export_specifiers() {
    let mut parser = ParserState::new(
        "test.ts".to_string(),
        r#"export { default } from "bar"; export { default as Foo } from "bar";"#.to_string(),
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
fn test_parser_export_star() {
    let mut parser = ParserState::new("test.ts".to_string(), r#"export * from "foo";"#.to_string());
    let root = parser.parse_source_file();

    assert!(root.is_some());
    assert!(
        parser.get_diagnostics().is_empty(),
        "Errors: {:?}",
        parser.get_diagnostics()
    );
}

// =========================================================================
// Additional tests for common TypeScript patterns
// =========================================================================

