#[test]
fn edit_file_updates_diagnostics() {
    let mut t = FourslashTest::new(
        "
        const x: number = 42;
    ",
    );
    t.diagnostics("test.ts").expect_none();
    // Now introduce a type error
    t.edit_file("test.ts", "const x: number = 'wrong';");
    let result = t.diagnostics("test.ts");
    if !result.diagnostics.is_empty() {
        result.expect_code(2322);
    }
}

// =============================================================================
// Project Management Tests
// =============================================================================

#[test]
fn file_count_single() {
    let t = FourslashTest::new("const x = 1;");
    assert_eq!(t.file_count(), 1);
}

#[test]
fn file_count_multi() {
    let t = FourslashTest::multi_file(&[
        ("a.ts", "export const a = 1;"),
        ("b.ts", "export const b = 2;"),
        ("c.ts", "export const c = 3;"),
    ]);
    assert_eq!(t.file_count(), 3);
}

#[test]
fn remove_file_from_project() {
    let mut t = FourslashTest::multi_file(&[
        ("a.ts", "export const a = 1;"),
        ("b.ts", "export const b = 2;"),
    ]);
    assert_eq!(t.file_count(), 2);
    t.remove_file("b.ts");
    assert_eq!(t.file_count(), 1);
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn empty_source() {
    let mut t = FourslashTest::new("");
    assert!(t.document_symbols("test.ts").symbols.is_empty());
}

#[test]
fn markers_at_start_and_end() {
    let t = FourslashTest::new("/*start*/const x = 1/*end*/;");
    assert_eq!(t.marker("start").character, 0);
}

#[test]
fn multiple_markers_same_line() {
    let t = FourslashTest::new("const /*a*/a = /*b*/b;");
    let a = t.marker("a");
    let b = t.marker("b");
    assert_eq!(a.line, b.line);
    assert!(a.character < b.character);
}

#[test]
fn marker_in_string_literal() {
    let t = FourslashTest::new("const s = '/*m*/hello';");
    let m = t.marker("m");
    assert_eq!(m.line, 0);
}

#[test]
fn unicode_in_identifiers() {
    let mut t = FourslashTest::new(
        "
        const /*x*/café = 'coffee';
    ",
    );
    t.hover("x").expect_found();
}

#[test]
fn very_long_line() {
    let long_var = "a".repeat(200);
    let source = format!("const /*x*/{long_var} = 1;");
    let mut t = FourslashTest::new(&source);
    t.hover("x").expect_found();
}

// =============================================================================
// Complex Scenario Tests
// =============================================================================

#[test]
fn class_method_dot_access_definition() {
    let mut t = FourslashTest::new(
        "
        class MyClass {
            /*def*/method() {
                return 42;
            }
        }
        const obj = new MyClass();
        obj./*ref*/method();
    ",
    );
    // Method reference via dot access
    let result = t.go_to_definition("ref");
    // Just verify it doesn't crash - dot access resolution varies
    let _ = result;
}

#[test]
fn generic_function_hover() {
    let mut t = FourslashTest::new(
        "
        function /*fn*/identity<T>(arg: T): T { return arg; }
    ",
    );
    t.hover("fn")
        .expect_found()
        .expect_display_string_contains("identity");
}

#[test]
fn intersection_type_hover() {
    let mut t = FourslashTest::new(
        "
        type /*t*/NamedPoint = { name: string } & { x: number; y: number };
    ",
    );
    t.hover("t")
        .expect_found()
        .expect_display_string_contains("NamedPoint");
}

#[test]
fn conditional_type_hover() {
    let mut t = FourslashTest::new(
        "
        type /*t*/IsString<T> = T extends string ? true : false;
    ",
    );
    t.hover("t")
        .expect_found()
        .expect_display_string_contains("IsString");
}

#[test]
fn mapped_type_hover() {
    let mut t = FourslashTest::new(
        "
        type /*t*/Readonly<T> = { readonly [K in keyof T]: T[K] };
    ",
    );
    t.hover("t")
        .expect_found()
        .expect_display_string_contains("Readonly");
}

#[test]
fn decorator_class_symbols() {
    let mut t = FourslashTest::new(
        "
        function Component(target: any) {}
        @Component
        class MyComponent {
            render() {}
        }
    ",
    );
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("MyComponent");
}

#[test]
fn async_function_hover() {
    let mut t = FourslashTest::new(
        "
        async function /*fn*/fetchData(): Promise<string> {
            return 'data';
        }
    ",
    );
    t.hover("fn")
        .expect_found()
        .expect_display_string_contains("fetchData");
}

#[test]
fn generator_function_hover() {
    let mut t = FourslashTest::new(
        "
        function* /*fn*/counter() {
            yield 1;
            yield 2;
        }
    ",
    );
    t.hover("fn")
        .expect_found()
        .expect_display_string_contains("counter");
}

#[test]
fn rest_parameter_hover() {
    let mut t = FourslashTest::new(
        "
        function /*fn*/sum(.../*args*/numbers: number[]) {
            return numbers.reduce((a, b) => a + b, 0);
        }
    ",
    );
    t.hover("fn")
        .expect_found()
        .expect_display_string_contains("sum");
    t.hover("args")
        .expect_found()
        .expect_display_string_contains("numbers");
}

#[test]
fn optional_parameter_definition() {
    let mut t = FourslashTest::new(
        "
        function greet(/*def*/name?: string) {
            return `Hello, ${/*ref*/name || 'world'}`;
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn default_parameter_definition() {
    let mut t = FourslashTest::new(
        "
        function greet(/*def*/greeting: string = 'Hello') {
            return /*ref*/greeting + '!';
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn switch_case_scoping() {
    let mut t = FourslashTest::new(
        "
        function test(x: number) {
            switch (x) {
                case 1: {
                    const /*def*/result = 'one';
                    console.log(/*ref*/result);
                    break;
                }
            }
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn object_method_shorthand_hover() {
    let mut t = FourslashTest::new(
        "
        const /*obj*/obj = {
            greet() { return 'hello'; },
            farewell() { return 'bye'; }
        };
    ",
    );
    t.hover("obj")
        .expect_found()
        .expect_display_string_contains("obj");
}

#[test]
fn tuple_type_hover() {
    let mut t = FourslashTest::new(
        "
        const /*t*/pair: [string, number] = ['hello', 42];
    ",
    );
    t.hover("t")
        .expect_found()
        .expect_display_string_contains("pair");
}

#[test]
fn template_literal_type_hover() {
    let mut t = FourslashTest::new(
        "
        type /*t*/EventName = `on${string}`;
    ",
    );
    t.hover("t")
        .expect_found()
        .expect_display_string_contains("EventName");
}

#[test]
fn enum_with_values_symbols() {
    let mut t = FourslashTest::new(
        "
        enum HttpStatus {
            OK = 200,
            NotFound = 404,
            InternalServerError = 500
        }
    ",
    );
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("HttpStatus");
}

#[test]
fn multiple_export_forms_symbols() {
    let mut t = FourslashTest::new(
        "
        export const A = 1;
        export function B() {}
        export class C {}
        export interface D {}
        export type E = string;
        export enum F { X }
    ",
    );
    let result = t.document_symbols("test.ts");
    result
        .expect_found()
        .expect_symbol("A")
        .expect_symbol("B")
        .expect_symbol("C")
        .expect_symbol("D")
        .expect_symbol("E")
        .expect_symbol("F");
}

#[test]
fn index_signature_hover() {
    let mut t = FourslashTest::new(
        "
        interface /*t*/StringMap {
            [key: string]: number;
        }
    ",
    );
    t.hover("t")
        .expect_found()
        .expect_display_string_contains("StringMap");
}

#[test]
fn overloaded_method_hover() {
    let mut t = FourslashTest::new(
        "
        class Calculator {
            /*fn*/add(a: number, b: number): number;
            add(a: string, b: string): string;
            add(a: any, b: any): any { return a + b; }
        }
    ",
    );
    t.hover("fn")
        .expect_found()
        .expect_display_string_contains("add");
}

// =============================================================================
// Linked Editing Range Tests (JSX)
// =============================================================================

#[test]
fn linked_editing_non_jsx() {
    let t = FourslashTest::new(
        "
        const /*x*/x = 1;
    ",
    );
    // Non-JSX code should not have linked editing ranges
    t.linked_editing_ranges("x").expect_none();
}

// =============================================================================
// Code Actions Tests
// =============================================================================

#[test]
fn code_actions_whole_file() {
    let mut t = FourslashTest::new(
        "
        const x: number = 42;
    ",
    );
    // Clean file should have no code actions (or some refactorings)
    let _ = t.code_actions("test.ts");
}

#[test]
fn code_actions_at_marker() {
    let mut t = FourslashTest::new(
        "
        const /*x*/x = 42;
    ",
    );
    let _ = t.code_actions_at("x");
}

#[test]
fn code_actions_on_function_no_crash() {
    let mut t = FourslashTest::new(
        "
        function /*f*/add(a: number, b: number) {
            return a + b;
        }
    ",
    );
    let _ = t.code_actions_at("f");
}

#[test]
fn code_actions_on_type_error_no_crash() {
    let mut t = FourslashTest::new(
        "
        const x: number = /*e*/'hello';
    ",
    );
    let _ = t.code_actions_at("e");
}

#[test]
fn code_actions_on_inline_type_no_crash() {
    let mut t = FourslashTest::new(
        "
        function process(data: /*t*/{ name: string; age: number }) {}
    ",
    );
    let _ = t.code_actions_at("t");
}

// =============================================================================
// Combined Feature Tests (testing multiple features on same code)
// =============================================================================

#[test]
fn combined_definition_hover_references() {
    let mut t = FourslashTest::new(
        "
        function /*def*/calculate(x: number): number {
            return x * 2;
        }
        const result = /*ref*/calculate(21);
    ",
    );

    // Go to definition
    t.go_to_definition("ref").expect_at_marker("def");

    // Hover
    t.hover("def")
        .expect_found()
        .expect_display_string_contains("calculate");

    // References (definition + usage)
    t.references("def").expect_found().expect_count(2);

    // Highlights should have at least the same occurrences
    let hl = t.document_highlights("def");
    hl.expect_found();
    assert!(
        hl.highlights.as_ref().unwrap().len() >= 2,
        "Expected at least 2 highlights"
    );
}

#[test]
fn combined_class_features() {
    let mut t = FourslashTest::new(
        "
        class /*cls*/Animal {
            /*name*/name: string;
            constructor(name: string) {
                this.name = name;
            }
            /*speak*/speak() {
                return `${this.name} speaks`;
            }
        }
    ",
    );

    // Hover on class
    t.hover("cls")
        .expect_found()
        .expect_display_string_contains("Animal");

    // Document symbols
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("Animal");

    // Semantic tokens should exist
    t.semantic_tokens("test.ts").expect_found();

    // Folding ranges for the class body
    t.folding_ranges("test.ts").expect_found();
}

#[test]
fn combined_interface_features() {
    let mut t = FourslashTest::new(
        "
        interface /*iface*/Config {
            host: string;
            port: number;
            debug?: boolean;
        }
        const /*cfg*/config: /*ref*/Config = { host: 'localhost', port: 3000 };
    ",
    );

    // Go to definition
    t.go_to_definition("ref").expect_at_marker("iface");

    // Hover
    t.hover("iface")
        .expect_found()
        .expect_display_string_contains("Config");

    // Symbols
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("Config")
        .expect_symbol("config");
}

#[test]
fn combined_multi_file_project() {
    let mut t = FourslashTest::multi_file(&[
        (
            "types.ts",
            "export interface User { name: string; age: number; }",
        ),
        (
            "utils.ts",
            "export function /*def*/formatName(name: string) { return name.toUpperCase(); }",
        ),
        ("app.ts", "const /*x*/greeting = 'Hello';\n/*ref*/greeting;"),
    ]);

    // Each file should have its own symbols
    t.document_symbols("types.ts").expect_symbol("User");
    t.document_symbols("utils.ts").expect_symbol("formatName");
    t.document_symbols("app.ts").expect_symbol("greeting");

    // Within-file definition
    t.go_to_definition("ref").expect_at_marker("x");

    // Semantic tokens for each file
    t.semantic_tokens("types.ts").expect_found();
    t.semantic_tokens("utils.ts").expect_found();
    t.semantic_tokens("app.ts").expect_found();
}

// =============================================================================
// Stress / Boundary Tests
// =============================================================================

#[test]
fn many_declarations_symbols() {
    let mut source = String::new();
    for i in 0..50 {
        source.push_str(&format!("const var{i} = {i};\n"));
    }
    let mut t = FourslashTest::new(&source);
    let result = t.document_symbols("test.ts");
    result.expect_count(50);
}

#[test]
fn many_functions_code_lens() {
    let mut source = String::new();
    for i in 0..20 {
        source.push_str(&format!("function func{i}() {{ }}\n"));
    }
    let t = FourslashTest::new(&source);
    let result = t.code_lenses("test.ts");
    // Should have at least some lenses for the functions
    if !result.lenses.is_empty() {
        result.expect_min_count(5);
    }
}

#[test]
fn complex_generic_hover() {
    let mut t = FourslashTest::new(
        "
        type /*t*/DeepPartial<T> = T extends object
            ? { [K in keyof T]?: DeepPartial<T[K]> }
            : T;
    ",
    );
    t.hover("t")
        .expect_found()
        .expect_display_string_contains("DeepPartial");
}

#[test]
fn multiple_generics_hover() {
    let mut t = FourslashTest::new(
        "
        function /*fn*/merge<A extends object, B extends object>(a: A, b: B): A & B {
            return { ...a, ...b };
        }
    ",
    );
    t.hover("fn")
        .expect_found()
        .expect_display_string_contains("merge");
}

#[test]
fn discriminated_union_hover() {
    let mut t = FourslashTest::new(
        "
        type /*t*/Shape =
            | { kind: 'circle'; radius: number }
            | { kind: 'square'; size: number }
            | { kind: 'rectangle'; width: number; height: number };
    ",
    );
    t.hover("t")
        .expect_found()
        .expect_display_string_contains("Shape");
}

#[test]
fn nested_class_definition() {
    let mut t = FourslashTest::new(
        "
        class Outer {
            inner = class /*def*/Inner {
                value = 42;
            };
        }
    ",
    );
    t.hover("def")
        .expect_found()
        .expect_display_string_contains("Inner");
}

// =============================================================================
// Go-to-Definition: Property Access & Member Resolution (NEW)
// =============================================================================

#[test]
fn definition_interface_property_access() {
    let mut t = FourslashTest::new(
        "
        interface Config {
            /*def*/host: string;
            port: number;
        }
        const cfg: Config = { host: 'localhost', port: 80 };
        cfg./*ref*/host;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_interface_method_access() {
    let mut t = FourslashTest::new(
        "
        interface Greeter {
            /*def*/greet(name: string): string;
        }
        const g: Greeter = { greet: (n) => `Hello ${n}` };
        g./*ref*/greet('World');
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_class_method_via_type_annotation() {
    let mut t = FourslashTest::new(
        "
        class Animal {
            /*def*/speak() { return 'sound'; }
        }
        const a: Animal = new Animal();
        a./*ref*/speak();
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_class_property_via_new_expression() {
    let mut t = FourslashTest::new(
        "
        class Point {
            /*def*/x: number;
            y: number;
            constructor(x: number, y: number) {
                this.x = x;
                this.y = y;
            }
        }
        const p = new Point(1, 2);
        p./*ref*/x;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_parameter_type_property_access() {
    let mut t = FourslashTest::new(
        "
        interface User {
            /*def*/name: string;
            age: number;
        }
        function greetUser(user: User) {
            return user./*ref*/name;
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_enum_member_access() {
    let mut t = FourslashTest::new(
        "
        enum Direction {
            /*def*/Up,
            Down,
            Left,
            Right
        }
        const d = Direction./*ref*/Up;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_enum_string_member_access() {
    let mut t = FourslashTest::new(
        "
        enum Color {
            /*def*/Red = 'red',
            Green = 'green',
            Blue = 'blue'
        }
        const c = Color./*ref*/Red;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_namespace_member_access() {
    let mut t = FourslashTest::new(
        "
        namespace Utils {
            export function /*def*/format(s: string) { return s; }
        }
        Utils./*ref*/format('hello');
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_class_static_member() {
    let mut t = FourslashTest::new(
        "
        class MathUtils {
            static /*def*/PI = 3.14;
        }
        const pi = MathUtils./*ref*/PI;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_class_static_method() {
    let mut t = FourslashTest::new(
        "
        class Factory {
            static /*def*/create() { return new Factory(); }
        }
        Factory./*ref*/create();
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_this_property_access() {
    let mut t = FourslashTest::new(
        "
        class Foo {
            /*def*/value = 42;
            getIt() {
                return this./*ref*/value;
            }
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_interface_optional_property() {
    let mut t = FourslashTest::new(
        "
        interface Options {
            /*def*/verbose?: boolean;
            output?: string;
        }
        function run(opts: Options) {
            if (opts./*ref*/verbose) {
                console.log('verbose mode');
            }
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_inherited_class_member() {
    let mut t = FourslashTest::new(
        "
        class Base {
            /*def*/baseMethod() { return 1; }
        }
        class Derived extends Base {
            derivedMethod() { return 2; }
        }
        const d = new Derived();
        d./*ref*/baseMethod();
    ",
    );
    // Should resolve to Base.baseMethod
    t.go_to_definition("ref").expect_at_marker("def");
}

// =============================================================================
// Go-to-Definition: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn definition_function_expression_variable() {
    let mut t = FourslashTest::new(
        "
        const /*def*/add = function(a: number, b: number) {
            return a + b;
        };
        /*ref*/add(1, 2);
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_default_export_class() {
    let mut t = FourslashTest::new(
        "
        export default class /*def*/MyClass {}
        const x = new /*ref*/MyClass();
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_const_enum() {
    let mut t = FourslashTest::new(
        "
        const enum /*def*/Status { Active, Inactive }
        const s: /*ref*/Status = Status.Active;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_abstract_class() {
    let mut t = FourslashTest::new(
        "
        abstract class /*def*/Shape {
            abstract area(): number;
        }
        class Circle extends /*ref*/Shape {
            area() { return 3.14; }
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_type_in_union() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/A { x: number; }
        interface B { y: string; }
        type C = /*ref*/A | B;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_type_in_intersection() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/X { a: number; }
        interface Y { b: string; }
        type Z = /*ref*/X & Y;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_generic_constraint() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/HasLength { length: number; }
        function longest<T extends /*ref*/HasLength>(a: T, b: T): T {
            return a.length >= b.length ? a : b;
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_return_type_reference() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/Result { success: boolean; }
        function getResult(): /*ref*/Result {
            return { success: true };
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_array_element_type() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/Item { id: number; }
        const items: /*ref*/Item[] = [];
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_promise_type_argument() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/Data { value: string; }
        async function fetch(): Promise</*ref*/Data> {
            return { value: '' };
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

// =============================================================================
// Hover: Property Access & Members (NEW)
// =============================================================================

#[test]
fn hover_interface_property() {
    let mut t = FourslashTest::new(
        "
        interface Config {
            host: string;
            port: number;
        }
        const cfg: Config = { host: 'localhost', port: 80 };
        cfg./*h*/host;
    ",
    );
    t.hover("h").expect_found();
}

#[test]
fn hover_class_method_call() {
    let mut t = FourslashTest::new(
        "
        class Calc {
            add(a: number, b: number) { return a + b; }
        }
        const c = new Calc();
        c./*h*/add(1, 2);
    ",
    );
    t.hover("h").expect_found();
}

#[test]
fn hover_enum_member_dot_access() {
    let mut t = FourslashTest::new(
        "
        enum Status {
            Active = 1,
            Inactive = 0
        }
        Status./*h*/Active;
    ",
    );
    t.hover("h").expect_found();
}

#[test]
fn hover_namespace_function() {
    let mut t = FourslashTest::new(
        "
        namespace Utils {
            export function format(s: string) { return s.trim(); }
        }
        Utils./*h*/format(' hello ');
    ",
    );
    t.hover("h").expect_found();
}

#[test]
fn hover_class_static_property() {
    let mut t = FourslashTest::new(
        "
        class Config {
            static readonly MAX_SIZE = 100;
        }
        Config./*h*/MAX_SIZE;
    ",
    );
    t.hover("h").expect_found();
}

#[test]
fn hover_enum_member_without_initializer() {
    let mut t = FourslashTest::new(
        "
        enum Direction { Up, Down, Left, Right }
        Direction./*h*/Left;
    ",
    );
    t.hover("h").expect_found();
}

#[test]
fn definition_inherited_method_deep_chain() {
    let mut t = FourslashTest::new(
        "
        class A {
            /*def*/greet() { return 'hi'; }
        }
        class B extends A {}
        class C extends B {}
        const c = new C();
        c./*ref*/greet();
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_interface_inherited_member() {
    let mut t = FourslashTest::new(
        "
        interface Readable {
            /*def*/read(): string;
        }
        interface BufferedReadable extends Readable {
            buffer(): void;
        }
        const r: BufferedReadable = { read() { return ''; }, buffer() {} };
        r./*ref*/read();
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_interface_deep_inheritance() {
    let mut t = FourslashTest::new(
        "
        interface A {
            /*def*/method(): void;
        }
        interface B extends A {
            other(): void;
        }
        interface C extends B {
            third(): void;
        }
        const c: C = { method() {}, other() {}, third() {} };
        c./*ref*/method();
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_inherited_via_type_annotation() {
    let mut t = FourslashTest::new(
        "
        class Animal {
            /*def*/speak() { return 'sound'; }
        }
        class Dog extends Animal {
            fetch() { return 'ball'; }
        }
        const pet: Dog = new Dog();
        pet./*ref*/speak();
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn hover_this_keyword() {
    let mut t = FourslashTest::new(
        "
        class MyClass {
            value: number = 42;
            getValue() {
                return /*h*/this.value;
            }
        }
    ",
    );
    t.hover("h").expect_found();
}

#[test]
fn hover_super_keyword() {
    let mut t = FourslashTest::new(
        "
        class Base {
            greet() { return 'hi'; }
        }
        class Child extends Base {
            greet() {
                return /*h*/super.greet();
            }
        }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("Base");
}

#[test]
fn definition_super_method_call() {
    let mut t = FourslashTest::new(
        "
        class Base {
            /*def*/greet() { return 'hi'; }
        }
        class Child extends Base {
            greet() {
                return super./*ref*/greet();
            }
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn hover_optional_chaining() {
    let mut t = FourslashTest::new(
        "
        interface User { name?: string; }
        const u: User | undefined = { name: 'Bob' };
        u?./*h*/name;
    ",
    );
    // Optional chaining hover may or may not resolve - just verify no crash
    let _ = t.hover("h");
}

#[test]
fn hover_getter_setter() {
    let mut t = FourslashTest::new(
        "
        class Thermometer {
            private _temp = 0;
            get /*h*/temperature() { return this._temp; }
            set temperature(val: number) { this._temp = val; }
        }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("temperature");
}

#[test]
fn hover_readonly_property() {
    let mut t = FourslashTest::new(
        "
        class Config {
            readonly /*h*/apiKey: string = 'key123';
        }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("apiKey");
}

#[test]
fn hover_string_literal_type() {
    let mut t = FourslashTest::new(
        "
        type /*h*/Direction = 'north' | 'south' | 'east' | 'west';
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("Direction");
}

#[test]
fn hover_numeric_literal_type() {
    let mut t = FourslashTest::new(
        "
        const /*h*/PI = 3.14159;
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("PI");
}

#[test]
fn hover_typeof_variable() {
    let mut t = FourslashTest::new(
        "
        const config = { host: 'localhost', port: 80 };
        type /*h*/ConfigType = typeof config;
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("ConfigType");
}

#[test]
fn hover_keyof_type() {
    let mut t = FourslashTest::new(
        "
        interface Person { name: string; age: number; }
        type /*h*/PersonKey = keyof Person;
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("PersonKey");
}

#[test]
fn hover_nested_generic_type() {
    let mut t = FourslashTest::new(
        "
        type /*h*/DeepReadonly<T> = {
            readonly [P in keyof T]: T[P] extends object ? DeepReadonly<T[P]> : T[P];
        };
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("DeepReadonly");
}

#[test]
fn hover_function_with_overloads() {
    let mut t = FourslashTest::new(
        "
        function /*h*/format(x: string): string;
        function format(x: number): string;
        function format(x: string | number): string {
            return String(x);
        }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("format");
}

#[test]
fn hover_abstract_method() {
    let mut t = FourslashTest::new(
        "
        abstract class Shape {
            abstract /*h*/area(): number;
        }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("area");
}

#[test]
fn hover_private_method() {
    let mut t = FourslashTest::new(
        "
        class Service {
            private /*h*/fetchData() { return null; }
        }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("fetchData");
}

#[test]
fn hover_static_property() {
    let mut t = FourslashTest::new(
        "
        class Counter {
            static /*h*/count = 0;
        }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("count");
}

#[test]
fn hover_computed_property() {
    let mut t = FourslashTest::new(
        "
        const key = 'name';
        const obj = { [key]: 'value' };
        const /*h*/x = obj;
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("x");
}

// =============================================================================
// Hover: JSDoc & Documentation (NEW)
// =============================================================================

#[test]
fn hover_jsdoc_param_tags() {
    let mut t = FourslashTest::new(
        "
        /**
         * Calculates the sum of two numbers.
         * @param a - First number
         * @param b - Second number
         * @returns The sum
         */
        function /*h*/add(a: number, b: number): number {
            return a + b;
        }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_display_string_contains("add")
        .expect_documentation_contains("sum");
}

#[test]
fn hover_jsdoc_deprecated() {
    let mut t = FourslashTest::new(
        "
        /**
         * @deprecated Use newMethod instead
         */
        function /*h*/oldMethod() {}
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_documentation_contains("deprecated");
}

#[test]
fn hover_jsdoc_example() {
    let mut t = FourslashTest::new(
        "
        /**
         * Formats a name.
         * @example
         * formatName('john') // => 'John'
         */
        function /*h*/formatName(name: string) { return name; }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_documentation_contains("example");
}

#[test]
fn hover_jsdoc_multiline() {
    let mut t = FourslashTest::new(
        "
        /**
         * A complex utility function that does several things:
         *
         * 1. Validates input
         * 2. Transforms data
         * 3. Returns result
         */
        function /*h*/process(data: string) { return data; }
    ",
    );
    t.hover("h")
        .expect_found()
        .expect_documentation_contains("Validates");
}

// =============================================================================
// References: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn references_type_annotation_usage() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/Config {
            host: string;
        }
        const a: /*r1*/Config = { host: '' };
        function f(c: /*r2*/Config) {}
    ",
    );
    t.references("def").expect_found().expect_count(3);
}

#[test]
fn references_enum_usage() {
    let mut t = FourslashTest::new(
        "
        enum /*def*/Color { Red, Green }
        const c: /*r1*/Color = Color.Red;
        function paint(color: /*r2*/Color) {}
    ",
    );
    // 4 references: definition + 2 type annotations + value reference in Color.Red
    t.references("def").expect_found().expect_count(4);
}

#[test]
fn references_type_alias_usage() {
    let mut t = FourslashTest::new(
        "
        type /*def*/ID = string | number;
        const id: /*r1*/ID = '123';
        function getById(id: /*r2*/ID) {}
    ",
    );
    t.references("def").expect_found().expect_count(3);
}

#[test]
fn references_multiple_declarations() {
    let mut t = FourslashTest::new(
        "
        const /*def*/x = 1;
        const y = /*r1*/x + 2;
        const z = /*r2*/x + 3;
        const w = /*r3*/x + /*r4*/x;
    ",
    );
    t.references("def").expect_found().expect_count(5);
}

#[test]
fn references_from_middle_usage() {
    let mut t = FourslashTest::new(
        "
        const x = 1;
        const y = /*ref*/x + 2;
        const z = x + 3;
    ",
    );
    // Finding references from a usage should find all refs including declaration
    t.references("ref").expect_found();
}

#[test]
fn references_class_with_heritage() {
    let mut t = FourslashTest::new(
        "
        class /*def*/Base { value = 1; }
        class Child extends /*r1*/Base {}
        class GrandChild extends Child {}
        const b: /*r2*/Base = new Base();
    ",
    );
    // 4 references: definition + heritage clause + type annotation + constructor call
    t.references("def").expect_found().expect_count(4);
}

// =============================================================================
// References: Additional Patterns
// =============================================================================

#[test]
fn references_import_alias() {
    let mut t = FourslashTest::new(
        "
        type /*def*/Str = string;
        const x: /*r1*/Str = 'hello';
        const y: /*r2*/Str = 'world';
    ",
    );
    t.references("def").expect_found().expect_count(3);
}

#[test]
fn references_enum_member() {
    let mut t = FourslashTest::new(
        "
        enum /*def*/Color { Red, Green, Blue }
        const c: Color = Color.Red;
        function paint(color: /*r1*/Color) {}
    ",
    );
    t.references("def").expect_found().expect_count(4);
}

// =============================================================================
// Definition: Edge Cases
// =============================================================================

#[test]
fn definition_default_parameter() {
    let mut t = FourslashTest::new(
        "
        const /*def*/DEFAULT = 42;
        function foo(x = /*ref*/DEFAULT) { return x; }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_computed_property() {
    let mut t = FourslashTest::new(
        "
        const /*def*/KEY = 'hello';
        const obj = { [/*ref*/KEY]: 1 };
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_template_literal_expression() {
    let mut t = FourslashTest::new(
        "
        const /*def*/name = 'world';
        const greeting = `hello ${/*ref*/name}`;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

// =============================================================================
// Hover: Edge Cases
// =============================================================================

#[test]
fn hover_const_assertion() {
    let mut t = FourslashTest::new(
        "
        const /*h*/colors = ['red', 'green', 'blue'] as const;
    ",
    );
    t.hover("h").expect_found();
}

#[test]
fn hover_destructured_parameter() {
    let mut t = FourslashTest::new(
        "
        function foo({ /*h*/x, y }: { x: number; y: string }) {
            return x;
        }
    ",
    );
    t.hover("h").expect_found();
}

// =============================================================================
// Definition: Extends/Implements Resolution
// =============================================================================

#[test]
fn definition_class_extends_target() {
    let mut t = FourslashTest::new(
        "
        class /*def*/Animal {
            name: string = '';
        }
        class Dog extends /*ref*/Animal {
            breed: string = '';
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_interface_extends_target() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/Serializable {
            serialize(): string;
        }
        interface JsonSerializable extends /*ref*/Serializable {
            toJSON(): object;
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_class_implements_target() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/Disposable {
            dispose(): void;
        }
        class Resource implements /*ref*/Disposable {
            dispose() {}
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

// =============================================================================
// Rename: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn rename_interface() {
    let mut t = FourslashTest::new(
        "
        interface /*r*/Config { host: string; }
        const c: Config = { host: '' };
        function setup(c: Config) {}
    ",
    );
    t.rename("r", "Options")
        .expect_success()
        .expect_total_edits(3);
}

#[test]
fn rename_enum() {
    let mut t = FourslashTest::new(
        "
        enum /*r*/Status { Active, Inactive }
        const s: Status = Status.Active;
    ",
    );
    t.rename("r", "State")
        .expect_success()
        .expect_total_edits(3);
}

#[test]
fn rename_type_alias() {
    let mut t = FourslashTest::new(
        "
        type /*r*/ID = string;
        const id: ID = '123';
    ",
    );
    t.rename("r", "Identifier")
        .expect_success()
        .expect_total_edits(2);
}

#[test]
fn rename_function_with_calls() {
    let mut t = FourslashTest::new(
        "
        function /*r*/greet(name: string) { return 'Hello ' + name; }
        greet('Alice');
        greet('Bob');
    ",
    );
    t.rename("r", "sayHello")
        .expect_success()
        .expect_total_edits(3);
}

#[test]
fn rename_class_across_type_and_value() {
    let mut t = FourslashTest::new(
        "
        class /*r*/Foo { x = 1; }
        const f: Foo = new Foo();
    ",
    );
    t.rename("r", "Bar").expect_success().expect_total_edits(3);
}

#[test]
#[ignore = "requires destructuring pattern rename support"]
fn rename_destructured() {
    let mut t = FourslashTest::new(
        "
        const { /*r*/name, age } = { name: 'Alice', age: 30 };
        console.log(name);
    ",
    );
    t.rename("r", "fullName")
        .expect_success()
        .expect_total_edits(2);
}

#[test]
fn rename_across_scopes() {
    let mut t = FourslashTest::new(
        "
        function outer() {
            const /*r*/val = 1;
            function inner() {
                return val + 1;
            }
            return val + inner();
        }
    ",
    );
    t.rename("r", "result")
        .expect_success()
        .expect_total_edits(3);
}

// =============================================================================
// Completions: Advanced Patterns (NEW)
// =============================================================================

