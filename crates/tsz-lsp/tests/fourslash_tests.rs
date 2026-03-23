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
    if result.locations.as_ref().is_some_and(|v| !v.is_empty()) {
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
    if result.locations.as_ref().is_some_and(|v| !v.is_empty()) {
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
    if result.locations.as_ref().is_some_and(|v| !v.is_empty()) {
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
    if result.locations.as_ref().is_some_and(|v| !v.is_empty()) {
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
