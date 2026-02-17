use super::*;
use tsz_parser::parser::ParserState;

#[test]
fn test_function_declaration() {
    let source = "export function add(a: number, b: number): number { return a + b; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export declare function add"),
        "Expected export declare: {output}"
    );
    assert!(
        output.contains("a: number"),
        "Expected parameter type: {output}"
    );
    assert!(
        output.contains("): number;"),
        "Expected return type: {output}"
    );
}

#[test]
fn test_class_declaration() {
    let source = r#"
    export class Calculator {
        private value: number;
        add(n: number): this {
            this.value += n;
            return this;
        }
    }
    "#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("class Calculator"),
        "Expected class declaration: {output}"
    );
    assert!(output.contains("value"), "Expected property: {output}");
    assert!(
        output.contains("add") && output.contains("number"),
        "Expected method signature with add and number: {output}"
    );
}

#[test]
fn test_interface_declaration() {
    let source = "export interface Point { x: number; y: number; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("interface Point"),
        "Expected interface: {output}"
    );
    assert!(output.contains("number"), "Expected number type: {output}");
}

#[test]
fn test_type_alias() {
    let source = "export type ID = string | number;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    let output = emitter.emit(root);

    assert!(
        output.contains("export type ID = string | number"),
        "Expected type alias: {output}"
    );
}
