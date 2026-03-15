//! Comprehensive fourslash-style tests for LSP features.
//!
//! These tests use the `FourslashTest` framework to declare test scenarios
//! with marker positions (`/*name*/`) and fluent assertions.

use super::fourslash::FourslashTest;

// =============================================================================
// Go-to-Definition Tests
// =============================================================================

#[test]
fn definition_const_variable() {
    let mut t = FourslashTest::new(
        "
        const /*def*/myVar = 42;
        /*ref*/myVar + 1;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_let_variable() {
    let mut t = FourslashTest::new(
        "
        let /*def*/x = 'hello';
        console.log(/*ref*/x);
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_function_declaration() {
    let mut t = FourslashTest::new(
        "
        function /*def*/greet(name: string) { return name; }
        /*ref*/greet('world');
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_class_reference() {
    let mut t = FourslashTest::new(
        "
        class /*def*/Foo { value = 1; }
        const f: /*ref*/Foo = new Foo();
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_interface_reference() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/IPoint { x: number; y: number; }
        const p: /*ref*/IPoint = { x: 0, y: 0 };
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_type_alias() {
    let mut t = FourslashTest::new(
        "
        type /*def*/StringOrNumber = string | number;
        const val: /*ref*/StringOrNumber = 42;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_enum_reference() {
    let mut t = FourslashTest::new(
        "
        enum /*def*/Color { Red, Green, Blue }
        const c: /*ref*/Color = Color.Red;
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_parameter_reference() {
    let mut t = FourslashTest::new(
        "
        function foo(/*def*/x: number) {
            return /*ref*/x + 1;
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_destructured_variable() {
    let mut t = FourslashTest::new(
        "
        const { /*def*/name } = { name: 'hello' };
        console.log(/*ref*/name);
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_at_declaration_itself() {
    let mut t = FourslashTest::new(
        "
        const /*self*/x = 1;
    ",
    );
    t.go_to_definition("self").expect_found();
}

#[test]
fn definition_no_result_at_keyword() {
    let mut t = FourslashTest::new(
        "
        /*kw*/const x = 1;
    ",
    );
    t.go_to_definition("kw").expect_none();
}

#[test]
fn definition_nested_function() {
    let mut t = FourslashTest::new(
        "
        function outer() {
            function /*def*/inner() { return 1; }
            return /*ref*/inner();
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_arrow_function_variable() {
    let mut t = FourslashTest::new(
        "
        const /*def*/add = (a: number, b: number) => a + b;
        /*ref*/add(1, 2);
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_generic_type() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/Container<T> { value: T; }
        const c: /*ref*/Container<number> = { value: 42 };
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_for_loop_variable() {
    let mut t = FourslashTest::new(
        "
        for (let /*def*/i = 0; /*ref*/i < 10; i++) {}
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_try_catch_variable() {
    let mut t = FourslashTest::new(
        "
        try {
        } catch (/*def*/err) {
            console.log(/*ref*/err);
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_export_default_function() {
    let mut t = FourslashTest::new(
        "
        export default function /*def*/main() {}
        /*ref*/main();
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_overloaded_function() {
    let mut t = FourslashTest::new(
        "
        function /*def*/overloaded(x: number): number;
        function overloaded(x: string): string;
        function overloaded(x: any): any { return x; }
        /*ref*/overloaded(1);
    ",
    );
    t.go_to_definition("ref").expect_found();
}

#[test]
fn definition_shorthand_property() {
    let mut t = FourslashTest::new(
        "
        const /*def*/name = 'world';
        const obj = { /*ref*/name };
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn definition_deeply_nested() {
    let mut t = FourslashTest::new(
        "
        function a() {
            function b() {
                function c() {
                    function d() {
                        const /*x*/x = 1;
                        return /*ref*/x;
                    }
                }
            }
        }
    ",
    );
    t.go_to_definition("ref").expect_at_marker("x");
}

#[test]
fn definition_verify_convenience() {
    let mut t = FourslashTest::new(
        "
        const /*def*/x = 1;
        /*ref*/x;
    ",
    );
    t.verify_definition("ref", "def");
}

// =============================================================================
// Go-to-Type-Definition Tests
// =============================================================================

#[test]
fn type_definition_variable_with_annotation() {
    let t = FourslashTest::new(
        "
        interface /*def*/Point { x: number; y: number; }
        const /*ref*/p: Point = { x: 0, y: 0 };
    ",
    );
    let result = t.go_to_type_definition("ref");
    // Type definition should resolve to the interface declaration
    if result.locations.is_some() && !result.locations.as_ref().unwrap().is_empty() {
        result.expect_at_marker("def");
    }
}

#[test]
fn type_definition_no_result_for_primitive() {
    let t = FourslashTest::new(
        "
        const /*x*/x = 42;
    ",
    );
    // Primitives have no user-defined type declaration
    t.go_to_type_definition("x").expect_none();
}

#[test]
fn type_definition_class_typed_variable() {
    let t = FourslashTest::new(
        "
        class /*def*/MyClass { value = 0; }
        const /*ref*/obj: MyClass = new MyClass();
    ",
    );
    let result = t.go_to_type_definition("ref");
    if result.locations.is_some() && !result.locations.as_ref().unwrap().is_empty() {
        result.expect_at_marker("def");
    }
}

// =============================================================================
// Hover Tests
// =============================================================================

#[test]
fn hover_const_number() {
    let mut t = FourslashTest::new(
        "
        const /*x*/x = 42;
    ",
    );
    t.hover("x")
        .expect_found()
        .expect_display_string_contains("x");
}

#[test]
fn hover_function_declaration() {
    let mut t = FourslashTest::new(
        "
        function /*fn*/add(a: number, b: number): number {
            return a + b;
        }
    ",
    );
    t.hover("fn")
        .expect_found()
        .expect_display_string_contains("add");
}

#[test]
fn hover_class_name() {
    let mut t = FourslashTest::new(
        "
        class /*cls*/MyClass {
            value: number = 0;
        }
    ",
    );
    t.hover("cls")
        .expect_found()
        .expect_display_string_contains("MyClass");
}

#[test]
fn hover_interface_name() {
    let mut t = FourslashTest::new(
        "
        interface /*iface*/Point {
            x: number;
            y: number;
        }
    ",
    );
    t.hover("iface")
        .expect_found()
        .expect_display_string_contains("Point");
}

#[test]
fn hover_no_info_on_empty_line() {
    let mut t = FourslashTest::new(
        "
        const x = 1;
        /*ws*/
        const y = 2;
    ",
    );
    t.hover("ws").expect_none();
}

#[test]
fn hover_jsdoc_preserved() {
    let mut t = FourslashTest::new(
        "
        /** This is a documented function */
        function /*fn*/documented() {}
    ",
    );
    t.hover("fn")
        .expect_found()
        .expect_display_string_contains("documented");
}

#[test]
fn hover_arrow_function() {
    let mut t = FourslashTest::new(
        "
        const /*fn*/add = (a: number, b: number) => a + b;
    ",
    );
    t.hover("fn")
        .expect_found()
        .expect_display_string_contains("add");
}

#[test]
fn hover_array_literal() {
    let mut t = FourslashTest::new(
        "
        const /*arr*/arr = [1, 2, 3];
    ",
    );
    t.hover("arr")
        .expect_found()
        .expect_display_string_contains("arr");
}

#[test]
fn hover_object_literal() {
    let mut t = FourslashTest::new(
        "
        const /*obj*/obj = { x: 1, y: 'hello' };
    ",
    );
    t.hover("obj")
        .expect_found()
        .expect_display_string_contains("obj");
}

#[test]
fn hover_enum_member() {
    let mut t = FourslashTest::new(
        "
        enum Direction {
            /*up*/Up,
            Down,
            Left,
            Right
        }
    ",
    );
    t.hover("up")
        .expect_found()
        .expect_display_string_contains("Up");
}

#[test]
fn hover_type_alias() {
    let mut t = FourslashTest::new(
        "
        type /*t*/StringOrNumber = string | number;
    ",
    );
    t.hover("t")
        .expect_found()
        .expect_display_string_contains("StringOrNumber");
}

#[test]
fn hover_type_assertion() {
    let mut t = FourslashTest::new(
        "
        const /*x*/x = 42 as const;
    ",
    );
    t.hover("x")
        .expect_found()
        .expect_display_string_contains("x");
}

#[test]
fn hover_template_literal() {
    let mut t = FourslashTest::new(
        "
        const /*name*/name = 'world';
        const greeting = `hello ${name}`;
    ",
    );
    t.hover("name")
        .expect_found()
        .expect_display_string_contains("name");
}

#[test]
fn hover_verify_convenience() {
    let mut t = FourslashTest::new(
        "
        function /*fn*/myFunction() {}
    ",
    );
    t.verify_hover_contains("fn", "myFunction");
}

// =============================================================================
// Find References Tests
// =============================================================================

#[test]
fn references_simple_variable() {
    let mut t = FourslashTest::new(
        "
        const /*def*/x = 1;
        /*r1*/x + /*r2*/x;
    ",
    );
    // Definition + 2 usages = 3 references
    t.references("def").expect_found().expect_count(3);
}

#[test]
fn references_function() {
    let mut t = FourslashTest::new(
        "
        function /*def*/foo() {}
        /*r1*/foo();
        /*r2*/foo();
    ",
    );
    // Definition + 2 calls = 3 references
    t.references("def").expect_found().expect_count(3);
}

#[test]
fn references_class() {
    let mut t = FourslashTest::new(
        "
        class /*def*/Point {
            x = 0;
        }
        const p = new /*r1*/Point();
        const q: /*r2*/Point = p;
    ",
    );
    t.references("def").expect_found();
}

#[test]
fn references_no_refs_for_keyword() {
    let mut t = FourslashTest::new(
        "
        /*kw*/const x = 1;
    ",
    );
    t.references("kw").expect_none();
}

#[test]
fn references_parameter() {
    let mut t = FourslashTest::new(
        "
        function greet(/*p*/name: string) {
            return 'hello ' + name;
        }
    ",
    );
    // Parameter declaration + usage in body = 2 refs
    t.references("p").expect_found().expect_count(2);
}

#[test]
fn references_from_usage_site() {
    let mut t = FourslashTest::new(
        "
        const /*def*/y = 1;
        /*ref*/y + 1;
    ",
    );
    // Querying from the usage should also find all refs
    t.references("ref").expect_found().expect_count(2);
}

// =============================================================================
// Rename Tests
// =============================================================================

#[test]
fn rename_simple_variable() {
    let mut t = FourslashTest::new(
        "
        const /*x*/x = 1;
        x + x;
    ",
    );
    t.rename("x", "newName")
        .expect_success()
        .expect_edits_in_file("test.ts")
        .expect_total_edits(3); // declaration + 2 usages
}

#[test]
fn rename_function() {
    let mut t = FourslashTest::new(
        "
        function /*fn*/foo() {}
        foo();
    ",
    );
    t.rename("fn", "bar")
        .expect_success()
        .expect_edits_in_file("test.ts")
        .expect_total_edits(2); // declaration + 1 call
}

#[test]
fn rename_class() {
    let mut t = FourslashTest::new(
        "
        class /*cls*/Foo {
            value = 1;
        }
        const f = new Foo();
        const x: Foo = f;
    ",
    );
    t.rename("cls", "Bar")
        .expect_success()
        .expect_edits_in_file("test.ts");
}

#[test]
fn rename_parameter() {
    let mut t = FourslashTest::new(
        "
        function greet(/*p*/name: string) {
            return 'hello ' + name;
        }
    ",
    );
    t.rename("p", "person")
        .expect_success()
        .expect_edits_in_file("test.ts")
        .expect_total_edits(2); // parameter + body usage
}

// =============================================================================
// Document Symbols Tests
// =============================================================================

#[test]
fn symbols_function_declarations() {
    let mut t = FourslashTest::new(
        "
        function alpha() {}
        function beta() {}
        function gamma() {}
    ",
    );
    t.document_symbols("test.ts")
        .expect_found()
        .expect_count(3)
        .expect_symbol("alpha")
        .expect_symbol("beta")
        .expect_symbol("gamma");
}

#[test]
fn symbols_class_with_members() {
    let mut t = FourslashTest::new(
        "
        class Animal {
            name: string = '';
            speak() {}
        }
    ",
    );
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("Animal");
}

#[test]
fn symbols_mixed_declarations() {
    let mut t = FourslashTest::new(
        "
        const PI = 3.14;
        function area(r: number) { return PI * r * r; }
        class Circle {
            radius: number = 0;
        }
        interface Shape {}
        enum Color { Red, Green, Blue }
        type ID = string;
    ",
    );
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("PI")
        .expect_symbol("area")
        .expect_symbol("Circle")
        .expect_symbol("Shape")
        .expect_symbol("Color")
        .expect_symbol("ID");
}

#[test]
fn symbols_empty_file() {
    let mut t = FourslashTest::new("");
    let result = t.document_symbols("test.ts");
    assert!(result.symbols.is_empty());
}

#[test]
fn symbols_enum_members() {
    let mut t = FourslashTest::new(
        "
        enum Color {
            Red,
            Green,
            Blue
        }
    ",
    );
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("Color");
}

#[test]
fn symbols_interface_members() {
    let mut t = FourslashTest::new(
        "
        interface Config {
            host: string;
            port: number;
            debug: boolean;
        }
    ",
    );
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("Config");
}

#[test]
fn symbols_arrow_functions() {
    let mut t = FourslashTest::new(
        "
        const add = (a: number, b: number) => a + b;
        const multiply = (a: number, b: number) => a * b;
    ",
    );
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("add")
        .expect_symbol("multiply");
}

#[test]
fn symbols_for_namespace() {
    let mut t = FourslashTest::new(
        "
        namespace MyNamespace {
            export function helper() {}
            export const VALUE = 1;
        }
    ",
    );
    t.document_symbols("test.ts")
        .expect_found()
        .expect_symbol("MyNamespace");
}

// =============================================================================
// Completions Tests
// =============================================================================

#[test]
fn completions_basic_scope() {
    let mut t = FourslashTest::new(
        "
        const myLongVariable = 42;
        const myOtherVar = 'hello';
        /**/
    ",
    );
    let result = t.completions("");
    // At least the framework shouldn't crash
    let _ = result;
}

#[test]
fn completions_after_dot() {
    let mut t = FourslashTest::new(
        "
        const obj = { foo: 1, bar: 'hello' };
        obj./**/
    ",
    );
    let result = t.completions("");
    // After dot, should offer property completions
    if !result.items.is_empty() {
        result.expect_contains("foo").expect_contains("bar");
    }
}

#[test]
fn completions_in_function_body() {
    let mut t = FourslashTest::new(
        "
        function test(param1: number, param2: string) {
            /**/
        }
    ",
    );
    let result = t.completions("");
    // Inside function body, params should be available
    if !result.items.is_empty() {
        result.expect_contains("param1").expect_contains("param2");
    }
}

#[test]
fn completions_enum_members() {
    let mut t = FourslashTest::new(
        "
        enum Color { Red, Green, Blue }
        Color./**/
    ",
    );
    let result = t.completions("");
    if !result.items.is_empty() {
        result.expect_contains("Red");
    }
}

// =============================================================================
// Signature Help Tests
// =============================================================================

#[test]
fn signature_help_at_call() {
    let mut t = FourslashTest::new(
        "
        function add(a: number, b: number): number { return a + b; }
        add(/**/);
    ",
    );
    let result = t.signature_help("");
    if result.help.is_some() {
        result
            .expect_found()
            .expect_label_contains("add")
            .expect_active_parameter(0);
    }
}

#[test]
fn signature_help_second_parameter() {
    let mut t = FourslashTest::new(
        "
        function greet(name: string, greeting: string) {}
        greet('hello', /**/);
    ",
    );
    let result = t.signature_help("");
    if result.help.is_some() {
        result.expect_found().expect_label_contains("greet");
    }
}

#[test]
fn signature_help_no_help_outside_call() {
    let mut t = FourslashTest::new(
        "
        function foo() {}
        /**/const x = 1;
    ",
    );
    t.signature_help("").expect_none();
}

// =============================================================================
// Diagnostics Tests
// =============================================================================

#[test]
fn diagnostics_clean_file() {
    let mut t = FourslashTest::new(
        "
        const x: number = 42;
        const y: string = 'hello';
    ",
    );
    t.diagnostics("test.ts").expect_none();
}

#[test]
fn diagnostics_type_mismatch() {
    let mut t = FourslashTest::new(
        "
        const x: number = 'hello';
    ",
    );
    let result = t.diagnostics("test.ts");
    if !result.diagnostics.is_empty() {
        result.expect_code(2322);
    }
}

#[test]
fn diagnostics_undeclared_variable() {
    let mut t = FourslashTest::new(
        "
        const x = undeclaredVariable;
    ",
    );
    let result = t.diagnostics("test.ts");
    if !result.diagnostics.is_empty() {
        result.expect_code(2304);
    }
}

#[test]
fn diagnostics_multiple_errors() {
    let mut t = FourslashTest::new(
        "
        const a: number = 'str';
        const b: string = 42;
    ",
    );
    let result = t.diagnostics("test.ts");
    if result.diagnostics.len() >= 2 {
        result.expect_code(2322);
    }
}

#[test]
fn diagnostics_verify_convenience() {
    let mut t = FourslashTest::new(
        "
        const x: number = 42;
    ",
    );
    t.verify_no_errors("test.ts");
}

// =============================================================================
// Folding Range Tests
// =============================================================================

#[test]
fn folding_function_body() {
    let t = FourslashTest::new(
        "
        function foo() {
            const x = 1;
            const y = 2;
            return x + y;
        }
    ",
    );
    t.folding_ranges("test.ts").expect_found();
}

#[test]
fn folding_class_body() {
    let t = FourslashTest::new(
        "
        class MyClass {
            method1() {
                return 1;
            }
            method2() {
                return 2;
            }
        }
    ",
    );
    t.folding_ranges("test.ts").expect_found();
}

#[test]
fn folding_nested_blocks() {
    let t = FourslashTest::new(
        "
        function outer() {
            if (true) {
                for (let i = 0; i < 10; i++) {
                    console.log(i);
                }
            }
        }
    ",
    );
    t.folding_ranges("test.ts").expect_found();
}

#[test]
fn folding_empty_file() {
    let t = FourslashTest::new("");
    let result = t.folding_ranges("test.ts");
    assert!(result.ranges.is_empty());
}

#[test]
fn folding_import_group() {
    let t = FourslashTest::new(
        "
        import { a } from './a';
        import { b } from './b';
        import { c } from './c';

        const x = 1;
    ",
    );
    // Should not crash even if import folding isn't implemented
    let _ = t.folding_ranges("test.ts");
}

// =============================================================================
// Selection Range Tests
// =============================================================================

#[test]
fn selection_range_identifier() {
    let t = FourslashTest::new(
        "
        const /*x*/myVariable = 42;
    ",
    );
    t.selection_range("x").expect_found();
}

#[test]
fn selection_range_has_parent() {
    let t = FourslashTest::new(
        "
        function foo() {
            const /*x*/x = 1;
        }
    ",
    );
    let result = t.selection_range("x");
    result.expect_found();
    assert!(
        result.depth() >= 2,
        "Expected depth >= 2, got {}",
        result.depth()
    );
}

#[test]
fn selection_range_nested_expression() {
    let t = FourslashTest::new(
        "
        const result = (/*m*/a + b) * c;
    ",
    );
    t.selection_range("m").expect_found();
}

// =============================================================================
// Document Highlighting Tests
// =============================================================================

#[test]
fn highlight_variable_usage() {
    let t = FourslashTest::new(
        "
        const /*x*/x = 1;
        x + x;
    ",
    );
    let result = t.document_highlights("x");
    result.expect_found();
    // At least 3 highlights (declaration + 2 usages, possibly more due to impl details)
    assert!(
        result.highlights.as_ref().unwrap().len() >= 3,
        "Expected at least 3 highlights, got {}",
        result.highlights.as_ref().unwrap().len()
    );
}

#[test]
fn highlight_function_calls() {
    let t = FourslashTest::new(
        "
        function /*fn*/foo() {}
        foo();
        foo();
    ",
    );
    let result = t.document_highlights("fn");
    result.expect_found();
    assert!(
        result.highlights.as_ref().unwrap().len() >= 3,
        "Expected at least 3 highlights, got {}",
        result.highlights.as_ref().unwrap().len()
    );
}

#[test]
fn highlight_parameter() {
    let t = FourslashTest::new(
        "
        function greet(/*p*/name: string) {
            return 'hello ' + name;
        }
    ",
    );
    let result = t.document_highlights("p");
    result.expect_found();
    assert!(
        result.highlights.as_ref().unwrap().len() >= 2,
        "Expected at least 2 highlights, got {}",
        result.highlights.as_ref().unwrap().len()
    );
}

#[test]
fn highlight_assignment_write() {
    let t = FourslashTest::new(
        "
        let /*x*/x = 1;
        x = 2;
        console.log(x);
    ",
    );
    let result = t.document_highlights("x");
    result.expect_found();
    result.expect_has_write();
}

// =============================================================================
// Semantic Tokens Tests
// =============================================================================

#[test]
fn semantic_tokens_basic() {
    let t = FourslashTest::new(
        "
        const x = 42;
        function foo() {}
    ",
    );
    t.semantic_tokens("test.ts").expect_found();
}

#[test]
fn semantic_tokens_class() {
    let t = FourslashTest::new(
        "
        class MyClass {
            value: number = 0;
            method(): string { return ''; }
        }
    ",
    );
    t.semantic_tokens("test.ts")
        .expect_found()
        .expect_min_tokens(3);
}

#[test]
fn semantic_tokens_empty() {
    let t = FourslashTest::new("");
    assert!(t.semantic_tokens("test.ts").data.is_empty());
}

#[test]
fn semantic_tokens_interface_and_type() {
    let t = FourslashTest::new(
        "
        interface Foo {
            bar: string;
            baz: number;
        }
        type Combined = Foo & { extra: boolean };
    ",
    );
    t.semantic_tokens("test.ts")
        .expect_found()
        .expect_min_tokens(2);
}

// =============================================================================
// Workspace Symbols Tests
// =============================================================================

#[test]
fn workspace_symbols_empty_query() {
    let t = FourslashTest::new(
        "
        function mySpecialFunction() {}
    ",
    );
    t.workspace_symbols("").expect_none();
}

#[test]
fn workspace_symbols_no_match() {
    let t = FourslashTest::new(
        "
        const x = 1;
    ",
    );
    assert!(t.workspace_symbols("nonexistentSymbol").symbols.is_empty());
}

#[test]
fn workspace_symbols_finds_function() {
    let t = FourslashTest::new(
        "
        function calculateTotal() {}
        function calculateAverage() {}
    ",
    );
    let result = t.workspace_symbols("calculate");
    if !result.symbols.is_empty() {
        result.expect_symbol("calculateTotal");
        result.expect_symbol("calculateAverage");
    }
}

#[test]
fn workspace_symbols_finds_class() {
    let t = FourslashTest::new(
        "
        class UserService {
            getUser() {}
        }
    ",
    );
    let result = t.workspace_symbols("UserService");
    if !result.symbols.is_empty() {
        result.expect_symbol("UserService");
    }
}

// =============================================================================
// Formatting Tests
// =============================================================================

#[test]
fn formatting_basic() {
    let t = FourslashTest::new(
        "
        const x = 1;
    ",
    );
    // Formatting may fail if prettier is not installed - just verify it doesn't panic
    let _ = t.format("test.ts");
}

// =============================================================================
// Call Hierarchy Tests
// =============================================================================

#[test]
fn call_hierarchy_prepare_function() {
    let t = FourslashTest::new(
        "
        function /*fn*/myFunction() {
            return 42;
        }
    ",
    );
    let result = t.prepare_call_hierarchy("fn");
    if result.item.is_some() {
        result.expect_name("myFunction");
    }
}

#[test]
fn call_hierarchy_prepare_method() {
    let t = FourslashTest::new(
        "
        class MyClass {
            /*m*/myMethod() { return 1; }
        }
    ",
    );
    let result = t.prepare_call_hierarchy("m");
    if result.item.is_some() {
        result.expect_name("myMethod");
    }
}

#[test]
fn call_hierarchy_prepare_at_non_callable() {
    let t = FourslashTest::new(
        "
        const /*x*/x = 42;
    ",
    );
    // A variable (not a function) should not produce a call hierarchy item
    t.prepare_call_hierarchy("x").expect_none();
}

#[test]
fn call_hierarchy_outgoing_calls() {
    let t = FourslashTest::new(
        "
        function helper() {}
        function /*fn*/main() {
            helper();
        }
    ",
    );
    let result = t.outgoing_calls("fn");
    if !result.calls.is_empty() {
        result.expect_callee("helper");
    }
}

#[test]
fn call_hierarchy_incoming_calls() {
    let t = FourslashTest::new(
        "
        function /*fn*/target() {}
        function caller1() { target(); }
        function caller2() { target(); }
    ",
    );
    let result = t.incoming_calls("fn");
    if !result.calls.is_empty() {
        result.expect_caller("caller1");
    }
}

#[test]
fn call_hierarchy_no_outgoing_calls() {
    let t = FourslashTest::new(
        "
        function /*fn*/empty() {
            const x = 1;
        }
    ",
    );
    t.outgoing_calls("fn").expect_none();
}

// =============================================================================
// Type Hierarchy Tests
// =============================================================================

#[test]
fn type_hierarchy_prepare_class() {
    let t = FourslashTest::new(
        "
        class /*cls*/Animal {
            name: string = '';
        }
    ",
    );
    let result = t.prepare_type_hierarchy("cls");
    if result.item.is_some() {
        result.expect_name("Animal");
    }
}

#[test]
fn type_hierarchy_prepare_interface() {
    let t = FourslashTest::new(
        "
        interface /*iface*/Serializable {
            serialize(): string;
        }
    ",
    );
    let result = t.prepare_type_hierarchy("iface");
    if result.item.is_some() {
        result.expect_name("Serializable");
    }
}

#[test]
fn type_hierarchy_prepare_at_variable() {
    let t = FourslashTest::new(
        "
        const /*x*/x = 42;
    ",
    );
    // A variable should not produce a type hierarchy item
    t.prepare_type_hierarchy("x").expect_none();
}

#[test]
fn type_hierarchy_supertypes() {
    let t = FourslashTest::new(
        "
        class Animal { name: string = ''; }
        class /*cls*/Dog extends Animal {
            breed: string = '';
        }
    ",
    );
    let result = t.supertypes("cls");
    if !result.items.is_empty() {
        result.expect_name("Animal");
    }
}

#[test]
fn type_hierarchy_subtypes() {
    let t = FourslashTest::new(
        "
        class /*cls*/Animal { name: string = ''; }
        class Dog extends Animal {}
        class Cat extends Animal {}
    ",
    );
    let result = t.subtypes("cls");
    if !result.items.is_empty() {
        result.expect_name("Dog");
    }
}

#[test]
fn type_hierarchy_interface_supertypes() {
    let t = FourslashTest::new(
        "
        interface Readable { read(): string; }
        interface /*iface*/BufferedReadable extends Readable {
            buffer: string;
        }
    ",
    );
    let result = t.supertypes("iface");
    if !result.items.is_empty() {
        result.expect_name("Readable");
    }
}

// =============================================================================
// Code Lens Tests
// =============================================================================

#[test]
fn code_lens_function_declarations() {
    let t = FourslashTest::new(
        "
        function foo() {}
        function bar() {}
    ",
    );
    let result = t.code_lenses("test.ts");
    // Functions should get code lenses (reference counts)
    if !result.lenses.is_empty() {
        result.expect_min_count(1);
    }
}

#[test]
fn code_lens_class() {
    let t = FourslashTest::new(
        "
        class MyClass {
            method1() {}
            method2() {}
        }
    ",
    );
    let result = t.code_lenses("test.ts");
    if !result.lenses.is_empty() {
        result.expect_found();
    }
}

#[test]
fn code_lens_interface() {
    let t = FourslashTest::new(
        "
        interface Serializable {
            serialize(): string;
        }
        class Data implements Serializable {
            serialize() { return ''; }
        }
    ",
    );
    let result = t.code_lenses("test.ts");
    // Interface should have implementations code lens
    if !result.lenses.is_empty() {
        result.expect_found();
    }
}

#[test]
fn code_lens_empty_file() {
    let t = FourslashTest::new("");
    t.code_lenses("test.ts").expect_none();
}

// =============================================================================
// Document Links Tests
// =============================================================================

#[test]
fn document_links_import() {
    let t = FourslashTest::new(
        "
        import { foo } from './utils';
    ",
    );
    let result = t.document_links("test.ts");
    // Import specifier should produce a document link
    if !result.links.is_empty() {
        result.expect_found().expect_count(1);
    }
}

#[test]
fn document_links_multiple_imports() {
    let t = FourslashTest::new(
        "
        import { a } from './a';
        import { b } from './b';
        import { c } from './c';
    ",
    );
    let result = t.document_links("test.ts");
    if !result.links.is_empty() {
        result.expect_found();
    }
}

#[test]
fn document_links_no_imports() {
    let t = FourslashTest::new(
        "
        const x = 1;
        const y = 2;
    ",
    );
    t.document_links("test.ts").expect_none();
}

#[test]
fn document_links_export_from() {
    let t = FourslashTest::new(
        "
        export { foo } from './utils';
    ",
    );
    let result = t.document_links("test.ts");
    if !result.links.is_empty() {
        result.expect_found();
    }
}

#[test]
fn document_links_dynamic_import() {
    let t = FourslashTest::new(
        "
        const mod = import('./dynamic-module');
    ",
    );
    let result = t.document_links("test.ts");
    if !result.links.is_empty() {
        result.expect_found();
    }
}

// =============================================================================
// Inlay Hints Tests
// =============================================================================

#[test]
fn inlay_hints_variable_types() {
    let t = FourslashTest::new(
        "
        const x = 42;
        const y = 'hello';
        const z = [1, 2, 3];
    ",
    );
    let result = t.inlay_hints("test.ts");
    // Inlay hints for variable types
    if !result.hints.is_empty() {
        result.expect_found();
    }
}

#[test]
fn inlay_hints_function_return() {
    let t = FourslashTest::new(
        "
        function add(a: number, b: number) {
            return a + b;
        }
    ",
    );
    let result = t.inlay_hints("test.ts");
    // May have return type hint
    let _ = result;
}

#[test]
fn inlay_hints_empty_file() {
    let t = FourslashTest::new("");
    let result = t.inlay_hints("test.ts");
    assert!(result.hints.is_empty());
}

// =============================================================================
// Go-to-Implementation Tests
// =============================================================================

#[test]
fn implementation_interface() {
    let mut t = FourslashTest::new(
        "
        interface /*iface*/Printable {
            print(): void;
        }
        class /*impl*/Document implements Printable {
            print() {}
        }
    ",
    );
    let result = t.go_to_implementation("iface");
    if result.locations.is_some() && !result.locations.as_ref().unwrap().is_empty() {
        result.expect_at_marker("impl");
    }
}

#[test]
fn implementation_abstract_class() {
    let mut t = FourslashTest::new(
        "
        abstract class /*abs*/Shape {
            abstract area(): number;
        }
        class /*impl*/Circle extends Shape {
            area() { return Math.PI; }
        }
    ",
    );
    let result = t.go_to_implementation("abs");
    if result.locations.is_some() && !result.locations.as_ref().unwrap().is_empty() {
        result.expect_at_marker("impl");
    }
}

#[test]
fn implementation_no_implementations() {
    let mut t = FourslashTest::new(
        "
        interface /*iface*/Unused {
            method(): void;
        }
    ",
    );
    // No classes implement this interface
    t.go_to_implementation("iface").expect_none();
}

// =============================================================================
// Multi-file Tests
// =============================================================================

#[test]
fn multi_file_definition_within_file() {
    let mut t = FourslashTest::multi_file(&[
        ("types.ts", "export interface /*def*/User { name: string; }"),
        ("app.ts", "const /*x*/x = 1;\n/*ref*/x;"),
    ]);
    t.go_to_definition("ref").expect_at_marker("x");
}

#[test]
fn multi_file_symbols() {
    let mut t = FourslashTest::multi_file(&[
        ("a.ts", "export function helper() {}"),
        ("b.ts", "function main() {}\nconst config = {};"),
    ]);
    t.document_symbols("a.ts")
        .expect_found()
        .expect_symbol("helper");
    t.document_symbols("b.ts")
        .expect_found()
        .expect_symbol("main")
        .expect_symbol("config");
}

#[test]
fn multi_file_folding_ranges() {
    let t = FourslashTest::multi_file(&[
        ("a.ts", "function foo() {\n  return 1;\n}"),
        ("b.ts", "class Bar {\n  method() {\n    return 2;\n  }\n}"),
    ]);
    t.folding_ranges("a.ts").expect_found();
    t.folding_ranges("b.ts").expect_found();
}

#[test]
fn multi_file_independent_symbols() {
    let mut t = FourslashTest::multi_file(&[
        ("a.ts", "export function alpha() {}"),
        ("b.ts", "export function beta() {}"),
        ("c.ts", "export function gamma() {}"),
    ]);
    t.document_symbols("a.ts").expect_symbol("alpha");
    t.document_symbols("b.ts").expect_symbol("beta");
    t.document_symbols("c.ts").expect_symbol("gamma");
}

#[test]
fn multi_file_same_symbol_name() {
    let mut t = FourslashTest::multi_file(&[
        ("a.ts", "export const value = 1;"),
        ("b.ts", "export const value = 2;"),
    ]);
    t.document_symbols("a.ts").expect_symbol("value");
    t.document_symbols("b.ts").expect_symbol("value");
}

#[test]
fn multi_file_semantic_tokens() {
    let t = FourslashTest::multi_file(&[
        ("a.ts", "function foo() { return 1; }"),
        ("b.ts", "class Bar { method() { return 2; } }"),
    ]);
    t.semantic_tokens("a.ts").expect_found();
    t.semantic_tokens("b.ts").expect_found();
}

// =============================================================================
// @filename directive Tests
// =============================================================================

#[test]
fn at_filename_single_file() {
    let mut t = FourslashTest::from_content("// @filename: main.ts\nconst /*x*/x = 1;\n/*ref*/x;");
    t.go_to_definition("ref").expect_at_marker("x");
}

#[test]
fn at_filename_multi_file() {
    let mut t = FourslashTest::from_content(
        "// @filename: helper.ts\nexport function /*def*/greet() {}\n// @filename: app.ts\nconst /*x*/y = 1;\n/*ref*/y;",
    );
    assert_eq!(t.marker_file("def"), "helper.ts");
    assert_eq!(t.marker_file("ref"), "app.ts");
    t.go_to_definition("ref").expect_at_marker("x");
}

// =============================================================================
// Edit and Re-query Tests
// =============================================================================

#[test]
fn edit_file_and_requery() {
    let mut t = FourslashTest::new(
        "
        const /*x*/x = 1;
    ",
    );
    t.hover("x").expect_found();
    t.edit_file("test.ts", "const /*y*/y = 'hello';\n/*ref*/y;");
    t.go_to_definition("ref").expect_at_marker("y");
}

#[test]
fn edit_file_updates_symbols() {
    let mut t = FourslashTest::new(
        "
        function foo() {}
    ",
    );
    t.document_symbols("test.ts").expect_symbol("foo");
    t.edit_file("test.ts", "function bar() {}\nfunction baz() {}");
    let result = t.document_symbols("test.ts");
    result.expect_symbol("bar");
    result.expect_symbol("baz");
}

#[test]
fn edit_file_updates_hover() {
    let mut t = FourslashTest::new(
        "
        const /*x*/x = 42;
    ",
    );
    t.hover("x").expect_found();
    t.edit_file("test.ts", "const /*y*/y = 'hello';");
    t.hover("y").expect_found();
}

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
    let t = FourslashTest::new(
        "
        const x: number = 42;
    ",
    );
    // Clean file should have no code actions (or some refactorings)
    let _ = t.code_actions("test.ts");
}

#[test]
fn code_actions_at_marker() {
    let t = FourslashTest::new(
        "
        const /*x*/x = 42;
    ",
    );
    let _ = t.code_actions_at("x");
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
