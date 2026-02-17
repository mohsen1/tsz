//! Tests for declaration parsing in the parser.
use crate::parser::{NodeIndex, ParserState};

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
