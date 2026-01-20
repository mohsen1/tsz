//! Tests for declaration_emitter.rs

use crate::declaration_emitter::*;
use crate::parser::ParserState;
use serde_json::Value;

fn emit_declaration(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.emit(root)
}

#[test]
fn test_interface_declaration() {
    let output = emit_declaration("export interface Foo { x: number; y: string; }");
    eprintln!("Output: [{}]", output);
    assert!(
        output.contains("interface"),
        "Should contain interface: {}",
        output
    );
}

#[test]
fn test_type_alias_declaration() {
    let source = "export type Foo = string | number;";
    let output = emit_declaration(source);
    // Verify export type alias is emitted correctly
    assert!(
        output.contains("export type Foo"),
        "Should contain 'export type Foo': {}",
        output
    );
    assert!(
        output.contains("string"),
        "Should contain string: {}",
        output
    );
    assert!(
        output.contains("number"),
        "Should contain number: {}",
        output
    );
}

#[test]
fn test_enum_declaration() {
    let output = emit_declaration("export enum Color { Red, Green, Blue }");
    assert!(
        output.contains("enum Color"),
        "Should contain enum Color: {}",
        output
    );
    assert!(output.contains("Red"), "Should contain Red: {}", output);
}

#[test]
fn test_type_only_export() {
    // This should be emitted as: export type { Foo };
    let output = emit_declaration("export type { Foo } from './foo';");
    assert!(
        output.contains("export"),
        "Should contain export: {}",
        output
    );
}

#[test]
fn test_export_assignment() {
    let output = emit_declaration("export = Foo;");
    assert!(
        output.contains("export = Foo;"),
        "Should emit export assignment: {}",
        output
    );
}

#[test]
fn test_export_default_function() {
    let source = "export default function add(a: number, b: number): number { return a + b; }";
    let output = emit_declaration(source);
    assert!(
        output.contains("export default function add"),
        "Should emit export default function: {}",
        output
    );
    assert!(
        output.contains(": number"),
        "Should include return type: {}",
        output
    );
}

#[test]
fn test_export_star_as_namespace() {
    let output = emit_declaration("export * as ns from './module';");
    assert!(
        output.contains("export * as ns from \"./module\";"),
        "Should emit export namespace re-export: {}",
        output
    );
}

#[test]
fn test_export_default_reexport_forms() {
    let source = "export { default } from './mod'; export { default as Foo } from './mod';";
    let output = emit_declaration(source);
    assert!(
        output.contains("export { default } from \"./mod\";"),
        "Should emit default re-export: {}",
        output
    );
    assert!(
        output.contains("export { default as Foo } from \"./mod\";"),
        "Should emit default re-export with alias: {}",
        output
    );
}

#[test]
fn test_type_only_named_export_reexport() {
    let source = "export type { Foo } from './foo'; export { type Bar as Baz } from './bar';";
    let output = emit_declaration(source);
    assert!(
        output.contains("export type { Foo } from \"./foo\";"),
        "Should emit type-only re-export clause: {}",
        output
    );
    assert!(
        output.contains("export { type Bar as Baz } from \"./bar\";"),
        "Should emit specifier-level type-only re-export: {}",
        output
    );
}

#[test]
fn test_function_declaration() {
    let output =
        emit_declaration("export function add(a: number, b: number): number { return a + b; }");
    assert!(
        output.contains("function add"),
        "Should contain function add: {}",
        output
    );
    assert!(
        output.contains("number"),
        "Should contain number: {}",
        output
    );
}

#[test]
fn test_class_declaration() {
    let output = emit_declaration("export class MyClass { constructor(public name: string) {} }");
    assert!(
        output.contains("class MyClass"),
        "Should contain class MyClass: {}",
        output
    );
}

#[test]
fn test_class_heritage_clauses() {
    let source = "export class Child extends Base<T> implements IFace, Other {}";
    let output = emit_declaration(source);
    assert!(
        output.contains("class Child"),
        "Should contain class Child: {}",
        output
    );
    assert!(
        output.contains("extends Base<T>"),
        "Should contain extends clause: {}",
        output
    );
    assert!(
        output.contains("implements IFace, Other"),
        "Should contain implements clause: {}",
        output
    );
}

#[test]
fn test_interface_with_methods() {
    let output =
        emit_declaration("export interface Service { start(): void; stop(): Promise<void>; }");
    assert!(
        output.contains("interface Service"),
        "Should contain interface Service: {}",
        output
    );
    assert!(output.contains("void"), "Should contain void: {}", output);
}

#[test]
fn test_interface_generic_call_construct_signatures() {
    let source = "export interface Factory { <T>(value: T): T; new <T>(value: T): Factory<T>; }";
    let output = emit_declaration(source);
    assert!(
        output.contains("interface Factory"),
        "Should contain interface Factory: {}",
        output
    );
    assert!(
        output.contains("<T>("),
        "Should contain generic call signature: {}",
        output
    );
    assert!(
        output.contains("new <T>("),
        "Should contain generic construct signature: {}",
        output
    );
    assert!(
        output.contains("Factory<T>"),
        "Should contain Factory<T> return type: {}",
        output
    );
}

#[test]
fn test_generic_interface() {
    let output = emit_declaration("export interface Container<T> { value: T; }");
    assert!(
        output.contains("interface Container"),
        "Should contain interface Container: {}",
        output
    );
    assert!(
        output.contains("<T>") || output.contains("< T >"),
        "Should contain generic param: {}",
        output
    );
}

#[test]
fn test_interface_heritage_clauses() {
    let source = "export interface Child extends Base<T>, ns.Other {}";
    let output = emit_declaration(source);
    assert!(
        output.contains("interface Child"),
        "Should contain interface Child: {}",
        output
    );
    assert!(
        output.contains("extends Base<T>, ns.Other"),
        "Should contain extends clause list: {}",
        output
    );
}

#[test]
fn test_namespace_export() {
    let output = emit_declaration("export * from './module';");
    assert!(
        output.contains("export"),
        "Should contain export: {}",
        output
    );
}

#[test]
fn test_named_exports() {
    let output = emit_declaration("export { foo, bar } from './module';");
    assert!(
        output.contains("export"),
        "Should contain export: {}",
        output
    );
}

#[test]
fn test_declaration_source_map_basic() {
    let source = "export interface Foo { x: number; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.set_source_map_text(parser.get_source_text());
    emitter.enable_source_map("test.d.ts", "test.ts");
    let _output = emitter.emit(root);

    let map_json = emitter.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(
        mappings.contains(',') || mappings.contains(';'),
        "expected non-trivial mappings, got: {mappings}"
    );
}

#[test]
fn test_declaration_source_map_type_alias() {
    let source = "export type Result<T, E> = { ok: true; value: T } | { ok: false; error: E };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.set_source_map_text(parser.get_source_text());
    emitter.enable_source_map("test.d.ts", "test.ts");
    let output = emitter.emit(root);

    // Verify output contains type alias
    assert!(
        output.contains("type Result"),
        "expected type alias in output: {output}"
    );

    let map_json = emitter.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(
        !mappings.is_empty(),
        "expected non-empty mappings for type alias, got: {mappings}"
    );
}

#[test]
fn test_declaration_source_map_function() {
    let source = "export function greet(name: string): string { return 'Hello ' + name; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.set_source_map_text(parser.get_source_text());
    emitter.enable_source_map("test.d.ts", "test.ts");
    let output = emitter.emit(root);

    // Verify output contains function declaration
    assert!(
        output.contains("function greet"),
        "expected function declaration in output: {output}"
    );

    let map_json = emitter.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(
        !mappings.is_empty(),
        "expected non-empty mappings for function declaration, got: {mappings}"
    );
}

#[test]
fn test_declaration_source_map_class() {
    let source = r#"export class Container<T> {
    private value: T;
    constructor(value: T) { this.value = value; }
    getValue(): T { return this.value; }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.set_source_map_text(parser.get_source_text());
    emitter.enable_source_map("test.d.ts", "test.ts");
    let output = emitter.emit(root);

    // Verify output contains class declaration
    assert!(
        output.contains("class Container"),
        "expected class declaration in output: {output}"
    );

    let map_json = emitter.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(
        !mappings.is_empty(),
        "expected non-empty mappings for class declaration, got: {mappings}"
    );

    // Verify source map structure
    let sources = map_value.get("sources").and_then(|v| v.as_array());
    assert!(
        sources.is_some() && !sources.unwrap().is_empty(),
        "expected sources array in source map"
    );
}

#[test]
fn test_declaration_source_map_enum() {
    let source = "export enum Status { Active = 'active', Inactive = 'inactive' }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.set_source_map_text(parser.get_source_text());
    emitter.enable_source_map("test.d.ts", "test.ts");
    let output = emitter.emit(root);

    // Verify output contains enum declaration
    assert!(
        output.contains("enum Status"),
        "expected enum declaration in output: {output}"
    );

    let map_json = emitter.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(
        !mappings.is_empty(),
        "expected non-empty mappings for enum declaration, got: {mappings}"
    );
}

#[test]
fn test_declaration_source_map_multiple_declarations() {
    let source = r#"export interface Point { x: number; y: number; }
export type Distance = number;
export function distance(a: Point, b: Point): Distance { return 0; }
export const origin: Point = { x: 0, y: 0 };"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut emitter = DeclarationEmitter::new(&parser.arena);
    emitter.set_source_map_text(parser.get_source_text());
    emitter.enable_source_map("test.d.ts", "test.ts");
    let output = emitter.emit(root);

    // Verify multiple declarations in output
    assert!(
        output.contains("interface Point"),
        "expected interface in output: {output}"
    );
    assert!(
        output.contains("type Distance"),
        "expected type alias in output: {output}"
    );

    let map_json = emitter.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    // With multiple declarations, we should have mappings spanning multiple lines
    let line_count = mappings.split(';').count();
    assert!(
        line_count >= 2,
        "expected mappings for multiple lines, got {} lines in: {mappings}",
        line_count
    );
}
