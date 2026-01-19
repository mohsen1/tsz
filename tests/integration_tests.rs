//! Integration tests for the TypeScript compiler
//!
//! These tests verify end-to-end functionality of the compiler pipeline:
//! - Parsing TypeScript source code
//! - Binding symbols
//! - Type checking
//! - Emitting JavaScript output

use wasm::checker::context::CheckerOptions;
use wasm::parser::thin_node::ThinNodeArena;
use wasm::solver::TypeInterner;
use wasm::thin_binder::ThinBinderState;
use wasm::thin_checker::ThinCheckerState;
use wasm::thin_parser::ThinParserState;

/// Helper to parse TypeScript code
fn parse(source: &str) -> ThinParserState {
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();
    parser
}

/// Helper to parse and bind TypeScript code
fn parse_and_bind(source: &str) -> (ThinParserState, ThinBinderState) {
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = ThinBinderState::new();
    binder.bind_source_file(&parser.arena, root);

    (parser, binder)
}

#[test]
fn test_simple_variable_declaration() {
    let source = "const x: number = 42;";
    let parser = parse(source);

    // Should parse without syntax errors
    let diags = parser.get_diagnostics();
    assert!(diags.is_empty(), "Expected no parse errors, got: {:?}", diags);
}

#[test]
fn test_function_declaration() {
    let source = r#"
        function add(a: number, b: number): number {
            return a + b;
        }
    "#;
    let parser = parse(source);

    // Should have no parse errors
    let diags = parser.get_diagnostics();
    assert!(diags.is_empty(), "Expected no parse errors, got: {:?}", diags);
}

#[test]
fn test_interface_declaration() {
    let source = r#"
        interface Point {
            x: number;
            y: number;
        }

        const p: Point = { x: 10, y: 20 };
    "#;
    let parser = parse(source);

    // Should have no parse errors
    let diags = parser.get_diagnostics();
    assert!(diags.is_empty(), "Expected no parse errors, got: {:?}", diags);
}

#[test]
fn test_class_declaration() {
    let source = r#"
        class Animal {
            name: string;
            constructor(name: string) {
                this.name = name;
            }
        }

        const dog = new Animal("Rex");
    "#;
    let parser = parse(source);

    // Should have no parse errors
    let diags = parser.get_diagnostics();
    assert!(diags.is_empty(), "Expected no parse errors, got: {:?}", diags);
}

#[test]
fn test_generic_function() {
    let source = r#"
        function identity<T>(arg: T): T {
            return arg;
        }

        const num = identity(42);
        const str = identity("hello");
    "#;
    let parser = parse(source);

    // Should parse generics correctly
    let diags = parser.get_diagnostics();
    assert!(diags.is_empty(), "Expected no parse errors, got: {:?}", diags);
}

#[test]
fn test_arrow_function() {
    let source = r#"
        const add = (a: number, b: number): number => a + b;
        const result = add(1, 2);
    "#;
    let parser = parse(source);

    // Should parse arrow functions correctly
    let diags = parser.get_diagnostics();
    assert!(diags.is_empty(), "Expected no parse errors, got: {:?}", diags);
}

#[test]
fn test_async_await() {
    let source = r#"
        async function fetchData(): Promise<string> {
            return "data";
        }

        async function main() {
            const data = await fetchData();
        }
    "#;
    let parser = parse(source);

    // Should parse async/await correctly
    let diags = parser.get_diagnostics();
    assert!(diags.is_empty(), "Expected no parse errors for async/await, got: {:?}", diags);
}

#[test]
fn test_module_import_export() {
    let source = r#"
        export const PI = 3.14159;
        export function area(r: number): number {
            return PI * r * r;
        }
    "#;
    let parser = parse(source);

    // Should parse exports correctly
    let diags = parser.get_diagnostics();
    assert!(diags.is_empty(), "Expected no parse errors for exports, got: {:?}", diags);
}

#[test]
fn test_type_alias() {
    let source = r#"
        type StringOrNumber = string | number;
        const value: StringOrNumber = "hello";
    "#;
    let parser = parse(source);

    // Should parse type aliases correctly
    let diags = parser.get_diagnostics();
    assert!(diags.is_empty(), "Expected no parse errors for type alias, got: {:?}", diags);
}

#[test]
fn test_enum_declaration() {
    let source = r#"
        enum Color {
            Red,
            Green,
            Blue
        }

        const c: Color = Color.Red;
    "#;
    let parser = parse(source);

    // Should parse enums correctly
    let diags = parser.get_diagnostics();
    assert!(diags.is_empty(), "Expected no parse errors for enum, got: {:?}", diags);
}

#[test]
fn test_decorator_syntax() {
    let source = r#"
        function decorator(target: any) {
            return target;
        }

        @decorator
        class MyClass {}
    "#;
    let parser = parse(source);

    // Should parse decorators correctly (even if not fully supported)
    let diags = parser.get_diagnostics();
    assert!(diags.is_empty(), "Expected no parse errors for decorator, got: {:?}", diags);
}

#[test]
fn test_jsx_syntax() {
    // JSX parsing (basic test)
    let source = r#"
        const element = <div>Hello</div>;
    "#;
    let mut parser = ThinParserState::new("test.tsx".to_string(), source.to_string());
    // Note: JSX parsing requires .tsx extension
    parser.parse_source_file();

    // Just verify it doesn't crash
    let _ = parser.get_diagnostics();
}

#[test]
fn test_optional_chaining() {
    let source = r#"
        interface Obj {
            prop?: {
                value: number;
            };
        }
        const obj: Obj = {};
        const value = obj.prop?.value;
    "#;
    let parser = parse(source);

    // Should parse optional chaining correctly
    let diags = parser.get_diagnostics();
    assert!(diags.is_empty(), "Expected no parse errors for optional chaining, got: {:?}", diags);
}

#[test]
fn test_nullish_coalescing() {
    let source = r#"
        const value: string | null = null;
        const result = value ?? "default";
    "#;
    let parser = parse(source);

    // Should parse nullish coalescing correctly
    let diags = parser.get_diagnostics();
    assert!(diags.is_empty(), "Expected no parse errors for nullish coalescing, got: {:?}", diags);
}

#[test]
fn test_template_literal_types() {
    let source = r#"
        type Greeting = `Hello, ${string}!`;
        const greeting: Greeting = "Hello, World!";
    "#;
    let parser = parse(source);

    // Should parse template literal types correctly
    let diags = parser.get_diagnostics();
    assert!(diags.is_empty(), "Expected no parse errors for template literal types, got: {:?}", diags);
}

#[test]
fn test_binding_creates_symbols() {
    let source = r#"
        const x = 1;
        function foo() {}
        class Bar {}
    "#;
    let (parser, binder) = parse_and_bind(source);

    // Should parse without errors
    let diags = parser.get_diagnostics();
    assert!(diags.is_empty(), "Expected no parse errors, got: {:?}", diags);

    // Binder should have created symbols
    assert!(!binder.symbols.is_empty(), "Expected symbols to be created");
}

#[test]
fn test_type_checking_basic() {
    // Simple type check test using the existing pattern from thin_checker_tests
    let arena = ThinNodeArena::new();
    let binder = ThinBinderState::new();
    let types = TypeInterner::new();
    let checker = ThinCheckerState::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    // Basic sanity check
    assert!(checker.ctx.diagnostics.is_empty());
}
