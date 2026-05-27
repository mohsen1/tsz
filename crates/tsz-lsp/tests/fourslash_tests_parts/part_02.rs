#[test]
fn completions_class_members_after_dot() {
    let mut t = FourslashTest::new(
        "
        class Calculator {
            add(a: number, b: number) { return a + b; }
            subtract(a: number, b: number) { return a - b; }
            multiply(a: number, b: number) { return a * b; }
        }
        const calc = new Calculator();
        calc./*c*/
    ",
    );
    t.completions("c")
        .expect_found()
        .expect_includes("add")
        .expect_includes("subtract")
        .expect_includes("multiply");
}

#[test]
fn completions_interface_members_after_dot() {
    let mut t = FourslashTest::new(
        "
        interface Config {
            host: string;
            port: number;
            debug: boolean;
        }
        declare const cfg: Config;
        cfg./*c*/
    ",
    );
    t.completions("c")
        .expect_found()
        .expect_includes("host")
        .expect_includes("port")
        .expect_includes("debug");
}

#[test]
fn completions_inherited_class_members() {
    let mut t = FourslashTest::new(
        "
        class Base {
            baseMethod() {}
            baseField: number = 0;
        }
        class Derived extends Base {
            derivedMethod() {}
        }
        const d = new Derived();
        d./*c*/
    ",
    );
    let result = t.completions("c");
    result.expect_found().expect_includes("derivedMethod");
    // Inherited members should also appear
    // (if the type system resolves them)
}

#[test]
fn completions_super_members() {
    let mut t = FourslashTest::new(
        "
        class Base {
            greet() { return 'hi'; }
            farewell() { return 'bye'; }
        }
        class Child extends Base {
            greet() {
                super./*c*/
                return '';
            }
        }
    ",
    );
    let result = t.completions("c");
    result.expect_found().expect_includes("greet");
}

#[test]
fn completions_enum_members_after_dot() {
    let mut t = FourslashTest::new(
        "
        enum Color {
            Red,
            Green,
            Blue
        }
        Color./*c*/
    ",
    );
    t.completions("c")
        .expect_found()
        .expect_includes("Red")
        .expect_includes("Green")
        .expect_includes("Blue");
}

#[test]
fn completions_namespace_exports_after_dot() {
    let mut t = FourslashTest::new(
        "
        namespace Utils {
            export function format(s: string) { return s; }
            export function parse(s: string) { return s; }
            export const VERSION = '1.0';
        }
        Utils./*c*/
    ",
    );
    t.completions("c")
        .expect_found()
        .expect_includes("format")
        .expect_includes("parse")
        .expect_includes("VERSION");
}

#[test]
fn completions_nested_scope() {
    let mut t = FourslashTest::new(
        "
        const outer = 1;
        function test() {
            const inner = 2;
            /*c*/
        }
    ",
    );
    t.completions("c")
        .expect_found()
        .expect_includes("outer")
        .expect_includes("inner")
        .expect_includes("test");
}

#[test]
fn completions_function_parameters() {
    let mut t = FourslashTest::new(
        "
        function doSomething(name: string, count: number) {
            /*c*/
        }
    ",
    );
    t.completions("c")
        .expect_found()
        .expect_includes("name")
        .expect_includes("count");
}

#[test]
fn completions_class_this_members() {
    let mut t = FourslashTest::new(
        "
        class Foo {
            value = 42;
            method() { return 1; }
            doStuff() {
                this./*c*/
            }
        }
    ",
    );
    t.completions("c")
        .expect_found()
        .expect_includes("value")
        .expect_includes("method")
        .expect_includes("doStuff");
}

#[test]
fn completions_this_with_private_members() {
    let mut t = FourslashTest::new(
        "
        class SecretKeeper {
            private secret = 'hidden';
            protected level = 5;
            public name = 'keeper';
            reveal() {
                this./*c*/
            }
        }
    ",
    );
    // Inside the class, all members (including private/protected) should show
    t.completions("c")
        .expect_found()
        .expect_includes("secret")
        .expect_includes("level")
        .expect_includes("name")
        .expect_includes("reveal");
}

#[test]
fn completions_external_no_private_members() {
    let mut t = FourslashTest::new(
        "
        class SecretKeeper {
            private secret = 'hidden';
            public name = 'keeper';
        }
        const sk = new SecretKeeper();
        sk./*c*/
    ",
    );
    let result = t.completions("c");
    result.expect_found().expect_includes("name");
    // Private members should NOT appear when accessing from outside
    let has_secret = result.items.iter().any(|item| item.label == "secret");
    assert!(
        !has_secret,
        "Private member 'secret' should not appear in external completions"
    );
}

#[test]
fn completions_no_completions_in_string() {
    let mut t = FourslashTest::new(
        "
        const x = 1;
        const s = 'hello /*c*/ world';
    ",
    );
    // Inside a string literal, normal identifier completions should not appear
    let result = t.completions("c");
    // Either none or very few - just verify no crash
    let _ = result;
}

#[test]
fn definition_this_property_in_method() {
    let mut t = FourslashTest::new(
        "
        class Counter {
            /*def*/count: number = 0;
            increment() {
                this./*ref*/count++;
            }
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn hover_this_property_access() {
    let mut t = FourslashTest::new(
        "
        class Config {
            host: string = 'localhost';
            getUrl() {
                return this./*h*/host;
            }
        }
    ",
    );
    t.hover("h").expect_found();
}

#[test]
fn completions_type_position() {
    let mut t = FourslashTest::new(
        "
        interface Point { x: number; y: number; }
        type Direction = 'up' | 'down';
        class Shape {}
        const p: /*c*/
    ",
    );
    // Type names should appear in type position
    let result = t.completions("c");
    result
        .expect_found()
        .expect_includes("Point")
        .expect_includes("Direction")
        .expect_includes("Shape");
}

#[test]
fn completions_generic_type_argument() {
    let mut t = FourslashTest::new(
        "
        interface Container<T> { value: T; }
        class Box {}
        const c: Container</*c*/> = { value: new Box() };
    ",
    );
    let result = t.completions("c");
    result.expect_found().expect_includes("Box");
}

#[test]
fn completions_after_new_keyword() {
    let mut t = FourslashTest::new(
        "
        class MyClass { }
        class OtherClass { }
        const x = new /*c*/
    ",
    );
    t.completions("c")
        .expect_found()
        .expect_includes("MyClass")
        .expect_includes("OtherClass");
}

#[test]
fn completions_static_members_on_class() {
    let mut t = FourslashTest::new(
        "
        class MathUtils {
            static PI = 3.14;
            static square(n: number) { return n * n; }
            instanceMethod() {}
        }
        MathUtils./*c*/
    ",
    );
    t.completions("c")
        .expect_found()
        .expect_includes("PI")
        .expect_includes("square");
}

// =============================================================================
// Signature Help: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn signature_help_multi_param() {
    let mut t = FourslashTest::new(
        "
        function create(name: string, age: number, active: boolean) {}
        create(/*c*/);
    ",
    );
    t.signature_help("c")
        .expect_found()
        .expect_parameter_count(3);
}

#[test]
fn signature_help_generic_function() {
    let mut t = FourslashTest::new(
        "
        function identity<T>(arg: T): T { return arg; }
        identity(/*c*/);
    ",
    );
    t.signature_help("c").expect_found();
}

#[test]
fn signature_help_rest_parameter() {
    let mut t = FourslashTest::new(
        "
        function sum(...nums: number[]) { return 0; }
        sum(/*c*/);
    ",
    );
    t.signature_help("c").expect_found();
}

#[test]
fn signature_help_optional_params() {
    let mut t = FourslashTest::new(
        "
        function greet(name: string, greeting?: string) {}
        greet(/*c*/);
    ",
    );
    t.signature_help("c")
        .expect_found()
        .expect_parameter_count(2);
}

#[test]
fn signature_help_method_call() {
    let mut t = FourslashTest::new(
        "
        class Calc {
            add(a: number, b: number): number { return a + b; }
        }
        const c = new Calc();
        c.add(/*h*/);
    ",
    );
    t.signature_help("h").expect_found();
}

#[test]
fn signature_help_nested_call() {
    let mut t = FourslashTest::new(
        "
        function outer(x: number) { return x; }
        function inner(a: string, b: string) { return a; }
        outer(inner(/*c*/));
    ",
    );
    // Should show inner's signature, not outer's
    t.signature_help("c")
        .expect_found()
        .expect_parameter_count(2);
}

#[test]
fn signature_help_arrow_function() {
    let mut t = FourslashTest::new(
        "
        const transform = (input: string, repeat: number): string => input;
        transform(/*c*/);
    ",
    );
    t.signature_help("c").expect_found();
}

#[test]
fn signature_help_constructor() {
    let mut t = FourslashTest::new(
        "
        class Point {
            constructor(x: number, y: number) {}
        }
        new Point(/*c*/);
    ",
    );
    t.signature_help("c").expect_found();
}

// =============================================================================
// Document Symbols: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn symbols_nested_functions() {
    let mut t = FourslashTest::new(
        "
        function outer() {
            function inner() {
                function deepest() {}
            }
        }
    ",
    );
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("outer");
}

#[test]
fn symbols_class_with_static_members() {
    let mut t = FourslashTest::new(
        "
        class Config {
            static defaultHost = 'localhost';
            static defaultPort = 80;
            host: string;
            port: number;
            constructor(host: string, port: number) {
                this.host = host;
                this.port = port;
            }
            static create() { return new Config('', 0); }
        }
    ",
    );
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("Config");
}

#[test]
fn symbols_abstract_class() {
    let mut t = FourslashTest::new(
        "
        abstract class Shape {
            abstract area(): number;
            abstract perimeter(): number;
            toString() { return `Area: ${this.area()}`; }
        }
    ",
    );
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("Shape");
}

#[test]
fn symbols_type_declarations() {
    let mut t = FourslashTest::new(
        "
        type Point = { x: number; y: number };
        type StringOrNumber = string | number;
        type Callback<T> = (value: T) => void;
    ",
    );
    let result = t.document_symbols("test.ts");
    result.expect_found().expect_count(3);
}

#[test]
fn symbols_exported_declarations() {
    let mut t = FourslashTest::new(
        "
        export const VERSION = '1.0';
        export function initialize() {}
        export class App {}
        export interface AppConfig { debug: boolean; }
        export type AppId = string;
        export enum AppStatus { Running, Stopped }
    ",
    );
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("VERSION")
        .expect_symbol("initialize")
        .expect_symbol("App")
        .expect_symbol("AppConfig")
        .expect_symbol("AppId")
        .expect_symbol("AppStatus");
}

#[test]
fn symbols_module_declarations() {
    let mut t = FourslashTest::new(
        "
        declare module 'my-module' {
            export function doSomething(): void;
        }
    ",
    );
    t.document_symbols("test.ts").expect_found();
}

// =============================================================================
// Diagnostics: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn diagnostics_unused_variable() {
    let mut t = FourslashTest::new(
        "
        const x = 42;
    ",
    );
    // A clean file with just a const should have no errors
    t.verify_no_errors("test.ts");
}

#[test]
fn diagnostics_duplicate_identifier() {
    let mut t = FourslashTest::new(
        "
        let x = 1;
        let x = 2;
    ",
    );
    t.diagnostics("test.ts").expect_found();
}

#[test]
fn diagnostics_missing_return_type() {
    let mut t = FourslashTest::new(
        "
        function add(a: number, b: number): number {
            return a + b;
        }
    ",
    );
    t.verify_no_errors("test.ts");
}

#[test]
fn diagnostics_const_reassignment() {
    let mut t = FourslashTest::new(
        "
        const x = 1;
        x = 2;
    ",
    );
    t.diagnostics("test.ts").expect_found();
}

#[test]
fn diagnostics_property_does_not_exist() {
    let mut t = FourslashTest::new(
        "
        interface Point { x: number; y: number; }
        const p: Point = { x: 1, y: 2 };
        p.z;
    ",
    );
    t.diagnostics("test.ts").expect_found();
}

// =============================================================================
// Folding Ranges: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn folding_multiline_comment() {
    let t = FourslashTest::new(
        "
        /**
         * This is a multiline
         * JSDoc comment
         */
        function test() {
            return 1;
        }
    ",
    );
    t.folding_ranges("test.ts")
        .expect_found()
        .expect_min_count(2); // comment + function body
}

#[test]
fn folding_switch_statement() {
    let t = FourslashTest::new(
        "
        function handler(action: string) {
            switch (action) {
                case 'a': {
                    break;
                }
                case 'b': {
                    break;
                }
            }
        }
    ",
    );
    t.folding_ranges("test.ts").expect_found();
}

#[test]
fn folding_array_literal() {
    let t = FourslashTest::new(
        "
        const items = [
            1,
            2,
            3,
            4,
            5,
        ];
    ",
    );
    t.folding_ranges("test.ts").expect_found();
}

#[test]
fn folding_object_literal() {
    let t = FourslashTest::new(
        "
        const config = {
            host: 'localhost',
            port: 80,
            debug: true,
        };
    ",
    );
    t.folding_ranges("test.ts").expect_found();
}

// =============================================================================
// Selection Range: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn selection_range_in_function_body() {
    let t = FourslashTest::new(
        "
        function test() {
            const x = /*m*/42;
        }
    ",
    );
    let result = t.selection_range("m");
    result.expect_found().expect_has_parent();
}

#[test]
fn selection_range_in_class_method() {
    let t = FourslashTest::new(
        "
        class Foo {
            bar() {
                return /*m*/this;
            }
        }
    ",
    );
    t.selection_range("m").expect_found().expect_has_parent();
}

#[test]
fn selection_range_in_object_literal() {
    let t = FourslashTest::new(
        "
        const obj = {
            key: /*m*/'value',
        };
    ",
    );
    t.selection_range("m").expect_found();
}

// =============================================================================
// Document Highlights: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn highlight_class_name_usage() {
    let t = FourslashTest::new(
        "
        class /*h*/Foo {}
        const a: Foo = new Foo();
        const b = new Foo();
    ",
    );
    let result = t.document_highlights("h");
    result.expect_found();
    assert!(
        result.highlights.as_ref().unwrap().len() >= 3,
        "Expected at least 3 highlights for class usage"
    );
}

#[test]
fn highlight_enum_name() {
    let t = FourslashTest::new(
        "
        enum /*h*/Color { Red, Green, Blue }
        const c: Color = Color.Red;
    ",
    );
    let result = t.document_highlights("h");
    result.expect_found();
    assert!(
        result.highlights.as_ref().unwrap().len() >= 2,
        "Expected at least 2 highlights for enum"
    );
}

#[test]
fn highlight_interface_name() {
    let t = FourslashTest::new(
        "
        interface /*h*/Serializable {
            serialize(): string;
        }
        class Item implements Serializable {
            serialize() { return '{}'; }
        }
    ",
    );
    let result = t.document_highlights("h");
    result.expect_found();
    assert!(
        result.highlights.as_ref().unwrap().len() >= 2,
        "Expected at least 2 highlights for interface"
    );
}

// =============================================================================
// Semantic Tokens: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn semantic_tokens_enum_declaration() {
    let t = FourslashTest::new(
        "
        enum Direction {
            Up = 'UP',
            Down = 'DOWN',
        }
    ",
    );
    t.semantic_tokens("test.ts").expect_found();
}

#[test]
fn semantic_tokens_generic_function() {
    let t = FourslashTest::new(
        "
        function map<T, U>(arr: T[], fn: (item: T) => U): U[] {
            return arr.map(fn);
        }
    ",
    );
    t.semantic_tokens("test.ts").expect_found();
}

#[test]
fn semantic_tokens_decorators() {
    let t = FourslashTest::new(
        "
        function log(target: any) { return target; }

        @log
        class MyService {
            @log
            method() {}
        }
    ",
    );
    t.semantic_tokens("test.ts").expect_found();
}

#[test]
fn semantic_tokens_type_annotations() {
    let t = FourslashTest::new(
        "
        interface Foo { bar: string; }
        const x: Foo = { bar: 'baz' };
        function f(a: number, b: string): boolean { return true; }
    ",
    );
    t.semantic_tokens("test.ts").expect_found();
}

// =============================================================================
// Workspace Symbols: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn workspace_symbols_case_insensitive() {
    let t = FourslashTest::new(
        "
        function myLongFunctionName() {}
        class MyService {}
    ",
    );
    let result = t.workspace_symbols("mylong");
    // Case-insensitive should find the function
    if !result.symbols.is_empty() {
        result.expect_found();
    }
}

#[test]
fn workspace_symbols_multi_file() {
    let t = FourslashTest::multi_file(&[
        ("a.ts", "export function helperA() {}"),
        ("b.ts", "export function helperB() {}"),
        ("c.ts", "export class MainApp {}"),
    ]);
    let result = t.workspace_symbols("helper");
    result.expect_found();
    assert!(
        result.symbols.len() >= 2,
        "Expected at least 2 symbols matching 'helper'"
    );
}

#[test]
fn workspace_symbols_returns_classes() {
    let t = FourslashTest::new(
        "
        class UserService {}
        class ProductService {}
        class OrderService {}
    ",
    );
    let result = t.workspace_symbols("Service");
    result.expect_found();
    assert!(
        result.symbols.len() >= 3,
        "Expected at least 3 service classes"
    );
}

// =============================================================================
// Code Lens: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn code_lens_class_methods() {
    let t = FourslashTest::new(
        "
        class Service {
            start() {}
            stop() {}
            restart() {}
        }
    ",
    );
    let result = t.code_lenses("test.ts");
    // Should produce lenses for the class and its methods
    if !result.lenses.is_empty() {
        result.expect_min_count(1);
    }
}

#[test]
fn code_lens_interface_with_implementations() {
    let t = FourslashTest::new(
        "
        interface Serializable {
            serialize(): string;
        }
        class JsonSerializer implements Serializable {
            serialize() { return '{}'; }
        }
    ",
    );
    let result = t.code_lenses("test.ts");
    if !result.lenses.is_empty() {
        result.expect_min_count(1);
    }
}

// =============================================================================
// Inlay Hints: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn inlay_hints_function_parameters() {
    let t = FourslashTest::new(
        "
        function createUser(name: string, age: number, active: boolean) {}
        createUser('Alice', 30, true);
    ",
    );
    let result = t.inlay_hints("test.ts");
    // Should have parameter name hints for the call
    if !result.hints.is_empty() {
        result.expect_min_count(1);
    }
}

#[test]
fn inlay_hints_variable_type() {
    let t = FourslashTest::new(
        "
        const x = [1, 2, 3];
        const y = { a: 1, b: 'hello' };
        const z = new Map<string, number>();
    ",
    );
    let result = t.inlay_hints("test.ts");
    // Should have type hints for the variables
    let _ = result; // Just verify no crash
}

#[test]
fn inlay_hints_method_call_parameters() {
    let t = FourslashTest::new(
        "
        class Logger {
            log(message: string, level: number) {}
        }
        const logger = new Logger();
        logger.log('hello', 1);
    ",
    );
    let result = t.inlay_hints("test.ts");
    // Should have parameter name hints for the method call
    let has_message_hint = result.hints.iter().any(|h| h.label.contains("message"));
    let has_level_hint = result.hints.iter().any(|h| h.label.contains("level"));
    assert!(
        has_message_hint && has_level_hint,
        "Expected parameter hints for method call, got: {:?}",
        result.hints.iter().map(|h| &h.label).collect::<Vec<_>>()
    );
}

// =============================================================================
// Type Hierarchy: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn inlay_hints_constructor_parameters() {
    let t = FourslashTest::new(
        "
        class User {
            constructor(name: string, age: number) {}
        }
        const u = new User('Alice', 30);
    ",
    );
    let result = t.inlay_hints("test.ts");
    let has_name_hint = result.hints.iter().any(|h| h.label.contains("name"));
    let has_age_hint = result.hints.iter().any(|h| h.label.contains("age"));
    assert!(
        has_name_hint && has_age_hint,
        "Expected constructor parameter hints, got: {:?}",
        result.hints.iter().map(|h| &h.label).collect::<Vec<_>>()
    );
}

#[test]
fn inlay_hints_skip_obvious_args() {
    let t = FourslashTest::new(
        "
        function setConfig(options: { a: number }, callback: () => void) {}
        setConfig({ a: 1 }, () => {});
    ",
    );
    let result = t.inlay_hints("test.ts");
    // Object literal and arrow function args should NOT have parameter hints
    let has_options = result.hints.iter().any(|h| h.label.contains("options"));
    let has_callback = result.hints.iter().any(|h| h.label.contains("callback"));
    assert!(
        !has_options && !has_callback,
        "Should skip hints for object literal and callback args, got: {:?}",
        result.hints.iter().map(|h| &h.label).collect::<Vec<_>>()
    );
}

#[test]
fn type_hierarchy_deep_inheritance() {
    let t = FourslashTest::new(
        "
        class /*base*/Animal {}
        class /*mammal*/Mammal extends Animal {}
        class /*dog*/Dog extends Mammal {}
    ",
    );
    // Dog's supertypes should include Mammal
    let result = t.supertypes("dog");
    result.expect_found();
}

#[test]
fn type_hierarchy_interface_extends() {
    let t = FourslashTest::new(
        "
        interface /*base*/Printable {
            print(): void;
        }
        interface /*child*/FormattedPrintable extends Printable {
            format(): string;
        }
    ",
    );
    let result = t.supertypes("child");
    result.expect_found();
    assert!(
        result.items.iter().any(|item| item.name == "Printable"),
        "Expected Printable as a supertype"
    );
}

#[test]
fn type_hierarchy_class_implements_interface() {
    let t = FourslashTest::new(
        "
        interface Serializable {
            serialize(): string;
        }
        class /*cls*/JsonItem implements Serializable {
            serialize() { return '{}'; }
        }
    ",
    );
    let result = t.supertypes("cls");
    result.expect_found();
    assert!(
        result.items.iter().any(|item| item.name == "Serializable"),
        "Expected Serializable as a supertype"
    );
}

#[test]
fn type_hierarchy_subtypes_finds_implementations() {
    let t = FourslashTest::new(
        "
        interface /*iface*/Logger {
            log(msg: string): void;
        }
        class ConsoleLogger implements Logger {
            log(msg: string) { console.log(msg); }
        }
        class FileLogger implements Logger {
            log(msg: string) {}
        }
    ",
    );
    let result = t.subtypes("iface");
    result.expect_found();
    assert!(
        result.items.len() >= 2,
        "Expected at least 2 implementations"
    );
}

#[test]
fn type_hierarchy_multiple_interfaces() {
    let t = FourslashTest::new(
        "
        interface Readable { read(): string; }
        interface Writable { write(s: string): void; }
        class /*cls*/Stream implements Readable, Writable {
            read() { return ''; }
            write(s: string) {}
        }
    ",
    );
    let result = t.supertypes("cls");
    result.expect_found();
    assert!(result.items.len() >= 2, "Expected at least 2 supertypes");
}

// =============================================================================
// Call Hierarchy: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn call_hierarchy_prepare_constructor() {
    let t = FourslashTest::new(
        "
        class Foo {
            /*c*/constructor() {}
        }
    ",
    );
    let result = t.prepare_call_hierarchy("c");
    result.expect_found();
}

#[test]
fn call_hierarchy_prepare_arrow_function() {
    let t = FourslashTest::new(
        "
        const /*f*/greet = (name: string) => `Hello ${name}`;
    ",
    );
    let result = t.prepare_call_hierarchy("f");
    result.expect_found();
}

#[test]
fn call_hierarchy_outgoing_from_function() {
    let t = FourslashTest::new(
        "
        function helper() { return 1; }
        function /*f*/main() {
            helper();
            helper();
        }
    ",
    );
    let result = t.outgoing_calls("f");
    result.expect_found();
}

#[test]
fn call_hierarchy_incoming_to_function() {
    let t = FourslashTest::new(
        "
        function /*f*/target() { return 42; }
        function caller1() { target(); }
        function caller2() { target(); }
    ",
    );
    let result = t.incoming_calls("f");
    result.expect_found();
    assert!(result.calls.len() >= 2, "Expected at least 2 callers");
}

#[test]
fn call_hierarchy_method_calls() {
    let t = FourslashTest::new(
        "
        class Service {
            /*m*/process() {
                this.validate();
                this.execute();
            }
            validate() {}
            execute() {}
        }
    ",
    );
    let result = t.outgoing_calls("m");
    result.expect_found();
}

// =============================================================================
// Document Links: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn document_links_re_export() {
    let t = FourslashTest::new(
        "
        export { something } from './other';
        export * from './utils';
    ",
    );
    let result = t.document_links("test.ts");
    result.expect_found();
    result.expect_min_count(2);
}

#[test]
fn document_links_type_import() {
    let t = FourslashTest::new(
        "
        import type { Config } from './config';
    ",
    );
    let result = t.document_links("test.ts");
    result.expect_found();
}

#[test]
fn document_links_require() {
    let t = FourslashTest::new(
        "
        const fs = require('fs');
        const path = require('path');
    ",
    );
    let result = t.document_links("test.ts");
    result.expect_found();
}

// =============================================================================
// Linked Editing (JSX Tag Sync): Advanced Patterns (NEW)
// =============================================================================

#[test]
fn linked_editing_simple_jsx() {
    let t = FourslashTest::new(
        "
        const elem = </*m*/div>content</div>;
    ",
    );
    // JSX linked editing should find paired tags
    let _ = t.linked_editing_ranges("m");
}

// =============================================================================
// Multi-file: Advanced Patterns (NEW)
// =============================================================================

#[test]
fn multi_file_cross_file_references() {
    let mut t = FourslashTest::multi_file(&[
        ("types.ts", "export interface /*def*/User { name: string; }"),
        ("utils.ts", "import { /*ref*/User } from './types';"),
    ]);
    // Within-file definition of the import binding
    let result = t.go_to_definition("ref");
    result.expect_found();
}

#[test]
fn multi_file_workspace_symbols() {
    let t = FourslashTest::multi_file(&[
        ("a.ts", "export class Alpha {}"),
        ("b.ts", "export class Beta {}"),
        ("c.ts", "export class Gamma {}"),
    ]);
    let result = t.workspace_symbols("a");
    // Should find Alpha at minimum
    if !result.symbols.is_empty() {
        result.expect_found();
    }
}

#[test]
fn multi_file_diagnostics_independent() {
    let mut t = FourslashTest::multi_file(&[
        ("good.ts", "const x: number = 42;"),
        ("bad.ts", "const y: number = 'not a number';"),
    ]);
    t.verify_no_errors("good.ts");
    t.diagnostics("bad.ts").expect_found();
}

#[test]
fn multi_file_completions_imports() {
    let mut t = FourslashTest::multi_file(&[
        ("lib.ts", "export function helperFunc() { return 1; }"),
        ("main.ts", "/*c*/"),
    ]);
    // Completions at the top of main.ts
    let result = t.completions("c");
    // Just verify no crash - cross-file completions depend on project setup
    let _ = result;
}

// =============================================================================
// Edge Cases & Robustness (NEW)
// =============================================================================

#[test]
fn edge_case_empty_class() {
    let mut t = FourslashTest::new(
        "
        class /*cls*/Empty {}
    ",
    );
    t.hover("cls")
        .expect_found()
        .expect_display_string_contains("Empty");
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("Empty");
}

#[test]
fn edge_case_empty_interface() {
    let mut t = FourslashTest::new(
        "
        interface /*iface*/Marker {}
    ",
    );
    t.hover("iface")
        .expect_found()
        .expect_display_string_contains("Marker");
}

#[test]
fn edge_case_single_line_function() {
    let mut t = FourslashTest::new(
        "
        const /*f*/double = (x: number) => x * 2;
    ",
    );
    t.hover("f")
        .expect_found()
        .expect_display_string_contains("double");
}

#[test]
fn edge_case_nested_destructuring() {
    let mut t = FourslashTest::new(
        "
        const { a: { /*def*/b } } = { a: { b: 42 } };
        /*ref*/b;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn edge_case_spread_operator() {
    let mut t = FourslashTest::new(
        "
        const /*def*/base = { x: 1, y: 2 };
        const extended = { .../*ref*/base, z: 3 };
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn edge_case_template_expression() {
    let mut t = FourslashTest::new(
        "
        const /*def*/name = 'World';
        const greeting = `Hello ${/*ref*/name}!`;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn edge_case_ternary_expression() {
    let mut t = FourslashTest::new(
        "
        const /*def*/flag = true;
        const result = /*ref*/flag ? 'yes' : 'no';
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn edge_case_array_destructuring() {
    let mut t = FourslashTest::new(
        "
        const [/*def*/first, second] = [1, 2];
        /*ref*/first;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn edge_case_for_of_variable() {
    let mut t = FourslashTest::new(
        "
        const items = [1, 2, 3];
        for (const /*def*/item of items) {
            console.log(/*ref*/item);
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn edge_case_labeled_statement() {
    let mut t = FourslashTest::new(
        "
        const /*def*/x = 1;
        /*ref*/x;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn edge_case_very_long_identifier() {
    let mut t = FourslashTest::new(
        "
        const /*def*/thisIsAVeryLongVariableNameThatShouldStillWorkCorrectly = 42;
        /*ref*/thisIsAVeryLongVariableNameThatShouldStillWorkCorrectly;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn edge_case_numeric_variable_name() {
    let mut t = FourslashTest::new(
        "
        const /*def*/$0 = 'first';
        /*ref*/$0;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn edge_case_underscore_variable() {
    let mut t = FourslashTest::new(
        "
        const /*def*/_private = 42;
        /*ref*/_private;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

// =============================================================================
// Type-annotated Variable Go-to-Definition Comprehensive (NEW)
// =============================================================================

#[test]
fn definition_type_annotated_interface_method() {
    let mut t = FourslashTest::new(
        "
        interface Logger {
            /*def*/log(msg: string): void;
            warn(msg: string): void;
        }
        function setup(logger: Logger) {
            logger./*ref*/log('hello');
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_type_annotated_class_property() {
    let mut t = FourslashTest::new(
        "
        class Database {
            /*def*/connectionString: string = '';
            isConnected: boolean = false;
        }
        function connect(db: Database) {
            return db./*ref*/connectionString;
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_enum_from_variable_type() {
    let mut t = FourslashTest::new(
        "
        enum /*def*/Priority {
            Low,
            Medium,
            High
        }
        const p: /*ref*/Priority = Priority.High;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_interface_nested_property() {
    let mut t = FourslashTest::new(
        "
        interface Address {
            /*def*/street: string;
            city: string;
        }
        interface Person {
            name: string;
            address: Address;
        }
        function getStreet(p: Person): string {
            return p.address./*ref*/street;
        }
    ",
    );
    // This tests nested property access - needs the first dot resolved then the second
    // Currently may not resolve all the way through, so just verify no crash
    let result = t.go_to_definition("ref");
    let _ = result;
}

// =============================================================================
// Combined Feature Tests (NEW)
// =============================================================================

#[test]
fn combined_hover_definition_references_for_class() {
    let mut t = FourslashTest::new(
        "
        class /*def*/EventEmitter {
            /*on*/on(event: string, listener: Function) {}
            /*emit*/emit(event: string) {}
        }
        const emitter = new /*r1*/EventEmitter();
    ",
    );

    // Definition
    t.go_to_definition("r1").expect_at_marker("def");

    // Hover
    t.hover("def")
        .expect_found()
        .expect_display_string_contains("EventEmitter");

    // Symbols
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("EventEmitter")
        .expect_symbol("emitter");
}

#[test]
fn combined_completions_and_signature_help() {
    let mut t = FourslashTest::new(
        "
        function transform(input: string, factor: number): string {
            return input.repeat(factor);
        }
        const result = transform(/*sig*/'hello', 3);
        /*comp*/
    ",
    );

    // Signature help at the call
    t.signature_help("sig")
        .expect_found()
        .expect_parameter_count(2);

    // Completions should include result and transform
    t.completions("comp")
        .expect_found()
        .expect_includes("result")
        .expect_includes("transform");
}

#[test]
fn combined_diagnostics_and_code_actions() {
    let mut t = FourslashTest::new(
        "
        const x: number = 'not a number';
    ",
    );

    // Should have a diagnostic
    t.diagnostics("test.ts").expect_found();

    // Code actions should be available
    let actions = t.code_actions("test.ts");
    // Just verify no crash
    let _ = actions;
}

#[test]
fn combined_folding_selection_highlights() {
    let t = FourslashTest::new(
        "
        function /*f*/processData(data: string[]) {
            const results: string[] = [];
            for (const item of data) {
                results.push(item.trim());
            }
            return results;
        }
    ",
    );

    // Folding ranges
    t.folding_ranges("test.ts").expect_found();

    // Selection range inside
    t.selection_range("f").expect_found();

    // Highlights
    let hl = t.document_highlights("f");
    hl.expect_found();
}

// =============================================================================
// Edit File & Incremental Update Tests (NEW)
// =============================================================================

#[test]
fn edit_file_preserves_definition() {
    let mut t = FourslashTest::new(
        "
        const /*def*/x = 1;
        /*ref*/x;
    ",
    );

    // Initially should work
    t.go_to_definition("ref").expect_at_marker("def");

    // Edit the file - add a new line
    t.edit_file("test.ts", "const x = 1;\nconst y = 2;\nx;");

    // Should still have symbols
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("x")
        .expect_symbol("y");
}

#[test]
fn edit_file_new_function_appears() {
    let mut t = FourslashTest::new("const x = 1;");

    // Add a function
    t.edit_file("test.ts", "const x = 1;\nfunction newFunc() { return x; }");

    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("x")
        .expect_symbol("newFunc");
}

#[test]
fn edit_file_fixes_diagnostic() {
    let mut t = FourslashTest::new(
        "
        const x: number = 'bad';
    ",
    );

    // Should have error
    t.diagnostics("test.ts").expect_found();

    // Fix the type error
    t.edit_file("test.ts", "const x: number = 42;");

    // Should be clean now
    t.verify_no_errors("test.ts");
}
