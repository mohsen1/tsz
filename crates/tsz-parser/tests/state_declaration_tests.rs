//! Tests for declaration parsing in the parser.
use crate::parser::{NodeIndex, ParserState, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

fn parse_source(source: &str) -> (ParserState, NodeIndex) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

#[test]
fn parse_declaration_modules_with_generic_and_type_aliases() {
    let (parser, root) = parse_source(
        "declare module 'mod' {\n  export interface Alias<T> {\n    value: T;\n  }\n}\ndeclare function ready(): void;\n",
    );
    assert_eq!(parser.get_diagnostics().len(), 0);
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    assert_eq!(sf.statements.nodes.len(), 2);
}

#[test]
fn parse_declaration_with_recovery_for_invalid_member() {
    let (parser, root) = parse_source(
        "declare namespace NS {\n  export interface I {\n    prop: string = 1;\n  }\n}\n",
    );
    assert!(!parser.get_diagnostics().is_empty());
    let sf = parser.get_arena().get_source_file_at(root).unwrap();
    assert_eq!(sf.statements.nodes.len(), 1);
}

#[test]
fn parse_import_equals_declaration_with_targeted_error_recovery() {
    let (parser, _root) = parse_source("import = 'invalid';\nfunction ok() { return 1; }");
    assert!(!parser.get_diagnostics().is_empty());
}

#[test]
fn parse_namespace_recovery_from_missing_closing_brace() {
    let (parser, _root) = parse_source("namespace Recover {\\n  export const value = 1;\\n");
    assert!(
        !parser.get_diagnostics().is_empty(),
        "expected diagnostics for unclosed namespace body"
    );
}

#[test]
fn parse_declare_using_as_single_variable_statement() {
    // `declare using y: null;` should parse as one VARIABLE_STATEMENT with declare modifier,
    // not as two statements (declare; + using y;)
    let (parser, root) = parse_source("declare using y: null;");
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    assert_eq!(
        sf.statements.nodes.len(),
        1,
        "declare using should be a single statement"
    );
    let stmt_node = arena.get(sf.statements.nodes[0]).unwrap();
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::VARIABLE_STATEMENT,
        "declare using should produce a VARIABLE_STATEMENT"
    );
    let var_stmt = arena.get_variable(stmt_node).unwrap();
    assert!(
        arena.has_modifier(&var_stmt.modifiers, SyntaxKind::DeclareKeyword),
        "VARIABLE_STATEMENT should have declare modifier"
    );
}

#[test]
fn parse_declare_await_using_as_single_variable_statement() {
    // `declare await using y: null;` should parse as one VARIABLE_STATEMENT with declare modifier
    let (parser, root) = parse_source("declare await using y: null;");
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    assert_eq!(
        sf.statements.nodes.len(),
        1,
        "declare await using should be a single statement"
    );
    let stmt_node = arena.get(sf.statements.nodes[0]).unwrap();
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::VARIABLE_STATEMENT,
        "declare await using should produce a VARIABLE_STATEMENT"
    );
    let var_stmt = arena.get_variable(stmt_node).unwrap();
    assert!(
        arena.has_modifier(&var_stmt.modifiers, SyntaxKind::DeclareKeyword),
        "VARIABLE_STATEMENT should have declare modifier"
    );
}

#[test]
fn parse_declare_export_function_as_single_statement() {
    // `declare export function f() { }` should parse as one FUNCTION_DECLARATION with declare modifier,
    // not as two statements (declare; + export function f() { })
    let (parser, root) = parse_source("declare export function f() { }");
    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    assert_eq!(
        sf.statements.nodes.len(),
        1,
        "declare export function should be a single statement"
    );
    let stmt_node = arena.get(sf.statements.nodes[0]).unwrap();
    assert_eq!(
        stmt_node.kind,
        syntax_kind_ext::FUNCTION_DECLARATION,
        "declare export function should produce a FUNCTION_DECLARATION"
    );
}
