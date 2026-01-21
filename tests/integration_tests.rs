//! Integration tests for the TypeScript compiler
//!
//! These tests verify end-to-end functionality of the compiler pipeline:
//! - Parsing TypeScript source code
//! - Binding symbols
//! - Type checking
//! - Emitting JavaScript output

use wasm::binder::BinderState;
use wasm::checker::context::CheckerOptions;
use wasm::checker::state::CheckerState;
use wasm::parser::ParserState;
use wasm::parser::node::NodeArena;
use wasm::solver::TypeInterner;

/// Helper to parse TypeScript code
fn parse(source: &str) -> ParserState {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();
    parser
}

/// Helper to parse and bind TypeScript code
fn parse_and_bind(source: &str) -> (ParserState, BinderState) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(&parser.arena, root);

    (parser, binder)
}

#[test]
fn test_simple_variable_declaration() {
    let source = "const x: number = 42;";
    let parser = parse(source);

    // Should parse without syntax errors
    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "Expected no parse errors, got: {:?}",
        diags
    );
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
    assert!(
        diags.is_empty(),
        "Expected no parse errors, got: {:?}",
        diags
    );
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
    assert!(
        diags.is_empty(),
        "Expected no parse errors, got: {:?}",
        diags
    );
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
    assert!(
        diags.is_empty(),
        "Expected no parse errors, got: {:?}",
        diags
    );
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
    assert!(
        diags.is_empty(),
        "Expected no parse errors, got: {:?}",
        diags
    );
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
    assert!(
        diags.is_empty(),
        "Expected no parse errors, got: {:?}",
        diags
    );
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
    assert!(
        diags.is_empty(),
        "Expected no parse errors for async/await, got: {:?}",
        diags
    );
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
    assert!(
        diags.is_empty(),
        "Expected no parse errors for exports, got: {:?}",
        diags
    );
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
    assert!(
        diags.is_empty(),
        "Expected no parse errors for type alias, got: {:?}",
        diags
    );
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
    assert!(
        diags.is_empty(),
        "Expected no parse errors for enum, got: {:?}",
        diags
    );
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
    assert!(
        diags.is_empty(),
        "Expected no parse errors for decorator, got: {:?}",
        diags
    );
}

#[test]
fn test_jsx_syntax() {
    // JSX parsing (basic test)
    let source = r#"
        const element = <div>Hello</div>;
    "#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
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
    assert!(
        diags.is_empty(),
        "Expected no parse errors for optional chaining, got: {:?}",
        diags
    );
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
    assert!(
        diags.is_empty(),
        "Expected no parse errors for nullish coalescing, got: {:?}",
        diags
    );
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
    assert!(
        diags.is_empty(),
        "Expected no parse errors for template literal types, got: {:?}",
        diags
    );
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
    assert!(
        diags.is_empty(),
        "Expected no parse errors, got: {:?}",
        diags
    );

    // Binder should have created symbols
    assert!(!binder.symbols.is_empty(), "Expected symbols to be created");
}

#[test]
fn test_type_checking_basic() {
    // Simple type check test using the existing pattern from checker state tests
    let arena = NodeArena::new();
    let binder = BinderState::new();
    let types = TypeInterner::new();
    let checker = CheckerState::new(
        &arena,
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    // Basic sanity check
    assert!(checker.ctx.diagnostics.is_empty());
}

// =============================================================================
// Additional Integration Tests
// =============================================================================

#[test]
fn test_complex_generic_constraints() {
    let source = r#"
        interface HasLength {
            length: number;
        }

        function getLength<T extends HasLength>(obj: T): number {
            return obj.length;
        }

        const strLen = getLength("hello");
        const arrLen = getLength([1, 2, 3]);
    "#;
    let parser = parse(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "Expected no parse errors, got: {:?}",
        diags
    );
}

#[test]
fn test_conditional_types() {
    let source = r#"
        type IsString<T> = T extends string ? "yes" : "no";
        type A = IsString<string>;  // "yes"
        type B = IsString<number>;  // "no"

        type Flatten<T> = T extends Array<infer U> ? U : T;
        type C = Flatten<number[]>;  // number
        type D = Flatten<string>;    // string
    "#;
    let parser = parse(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "Expected no parse errors for conditional types, got: {:?}",
        diags
    );
}

#[test]
fn test_mapped_types() {
    let source = r#"
        type Keys = "a" | "b" | "c";
        type Mapped = { [K in Keys]: number };

        type Partial<T> = { [P in keyof T]?: T[P] };
        type Required<T> = { [P in keyof T]-?: T[P] };
        type Readonly<T> = { readonly [P in keyof T]: T[P] };

        interface Person {
            name: string;
            age: number;
        }

        type PartialPerson = Partial<Person>;
    "#;
    let parser = parse(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "Expected no parse errors for mapped types, got: {:?}",
        diags
    );
}

#[test]
fn test_discriminated_unions() {
    let source = r#"
        type Shape =
            | { kind: "circle"; radius: number }
            | { kind: "square"; size: number }
            | { kind: "rectangle"; width: number; height: number };

        function getArea(shape: Shape): number {
            switch (shape.kind) {
                case "circle":
                    return Math.PI * shape.radius ** 2;
                case "square":
                    return shape.size ** 2;
                case "rectangle":
                    return shape.width * shape.height;
            }
        }
    "#;
    let parser = parse(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "Expected no parse errors for discriminated unions, got: {:?}",
        diags
    );
}

#[test]
fn test_template_literal_types_advanced() {
    let source = r#"
        type EventName<T extends string> = `on${Capitalize<T>}`;
        type ClickEvent = EventName<"click">;  // "onClick"

        type PropName = "name" | "age";
        type Getter<T extends string> = `get${Capitalize<T>}`;
        type Getters = Getter<PropName>;  // "getName" | "getAge"
    "#;
    let parser = parse(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "Expected no parse errors for template literal types, got: {:?}",
        diags
    );
}

#[test]
fn test_class_with_decorators() {
    let source = r#"
        function log(target: any, key: string, descriptor: PropertyDescriptor) {
            return descriptor;
        }

        class MyClass {
            @log
            method() {
                return 42;
            }
        }
    "#;
    let parser = parse(source);
    // Decorators should parse without errors
    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "Expected no parse errors for decorators, got: {:?}",
        diags
    );
}

#[test]
fn test_private_class_fields() {
    let source = r#"
        class Counter {
            #count = 0;

            increment() {
                this.#count++;
            }

            get value(): number {
                return this.#count;
            }
        }
    "#;
    let parser = parse(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "Expected no parse errors for private fields, got: {:?}",
        diags
    );
}

#[test]
fn test_using_declaration() {
    // ES2024 using declaration (explicit resource management)
    let source = r#"
        interface Disposable {
            [Symbol.dispose](): void;
        }

        function example() {
            using resource: Disposable = {
                [Symbol.dispose]() {
                    console.log("disposed");
                }
            };
        }
    "#;
    let parser = parse(source);
    // Just verify parsing doesn't crash
    let _ = parser.get_diagnostics();
}

#[test]
fn test_satisfies_operator() {
    let source = r##"
        type Colors = "red" | "green" | "blue";
        const palette = {
            red: [255, 0, 0],
            green: "#00ff00",
            blue: [0, 0, 255]
        } satisfies Record<Colors, string | number[]>;
    "##;
    let parser = parse(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "Expected no parse errors for satisfies, got: {:?}",
        diags
    );
}

#[test]
fn test_assertion_functions() {
    let source = r#"
        function assert(condition: boolean, message?: string): asserts condition {
            if (!condition) {
                throw new Error(message ?? "Assertion failed");
            }
        }

        function assertIsDefined<T>(val: T): asserts val is NonNullable<T> {
            if (val === undefined || val === null) {
                throw new Error("Expected defined value");
            }
        }
    "#;
    let parser = parse(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "Expected no parse errors for assertion functions, got: {:?}",
        diags
    );
}

#[test]
fn test_recursive_types() {
    let source = r#"
        type Json =
            | string
            | number
            | boolean
            | null
            | Json[]
            | { [key: string]: Json };

        type LinkedList<T> = {
            value: T;
            next: LinkedList<T> | null;
        };

        type Tree<T> = {
            value: T;
            children: Tree<T>[];
        };
    "#;
    let parser = parse(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "Expected no parse errors for recursive types, got: {:?}",
        diags
    );
}

#[test]
fn test_namespace_and_module() {
    let source = r#"
        namespace Validation {
            export interface StringValidator {
                isValid(s: string): boolean;
            }

            export const numberRegex = /^[0-9]+$/;

            export class NumberValidator implements StringValidator {
                isValid(s: string): boolean {
                    return numberRegex.test(s);
                }
            }
        }

        // Using the namespace
        const validator: Validation.StringValidator = new Validation.NumberValidator();
    "#;
    let parser = parse(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "Expected no parse errors for namespace, got: {:?}",
        diags
    );
}

#[test]
fn test_index_signatures() {
    let source = r#"
        interface StringArray {
            [index: number]: string;
        }

        interface NumberOrString {
            [key: string]: number | string;
            length: number;
        }

        interface Dictionary<T> {
            [key: string]: T;
        }
    "#;
    let parser = parse(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "Expected no parse errors for index signatures, got: {:?}",
        diags
    );
}

#[test]
fn test_binding_with_destructuring() {
    let source = r#"
        const { a, b: renamed, c = 10 } = { a: 1, b: 2 };
        const [first, second, ...rest] = [1, 2, 3, 4, 5];

        function processPoint({ x, y }: { x: number; y: number }) {
            return x + y;
        }

        function processArray([head, ...tail]: number[]) {
            return head + tail.length;
        }
    "#;
    let (parser, binder) = parse_and_bind(source);

    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "Expected no parse errors, got: {:?}",
        diags
    );
    assert!(
        !binder.symbols.is_empty(),
        "Expected symbols from destructuring"
    );
}

#[test]
#[ignore = "Long-running test"]
fn test_large_file_parsing() {
    // Generate a large file to test parser performance
    let mut source = String::new();
    for i in 0..1000 {
        source.push_str(&format!(
            "const var{} = {};\nfunction fn{}(x: number): number {{ return x * {}; }}\n",
            i, i, i, i
        ));
    }

    let parser = parse(&source);
    let diags = parser.get_diagnostics();
    assert!(diags.is_empty(), "Expected no parse errors for large file");
    assert!(parser.get_node_count() > 5000, "Expected many AST nodes");
}
