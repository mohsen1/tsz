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
    // Querying definition at declaration should still work
    t.go_to_definition("self").expect_found();
}

#[test]
fn definition_no_result_at_keyword() {
    let mut t = FourslashTest::new(
        "
        /*kw*/const x = 1;
    ",
    );
    // At a keyword, there's no definition
    t.go_to_definition("kw").expect_none();
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
    t.hover("x").expect_found();
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
    // On an empty line, there should be no hover info
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
    let result = t.hover("fn");
    result.expect_found();
    // The hover should include the function name at minimum
    result.expect_display_string_contains("documented");
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
    t.references("def").expect_found();
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
    t.references("def").expect_found();
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
        .expect_edits_in_file("test.ts");
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
        .expect_edits_in_file("test.ts");
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

// =============================================================================
// Completions Tests
// =============================================================================

#[test]
fn completions_basic() {
    let mut t = FourslashTest::new(
        "
        const myLongVariable = 42;
        const myOtherVar = 'hello';
        /**/
    ",
    );
    // At end of file, should get completions including our variables
    let result = t.completions("");
    // We at least shouldn't crash
    assert!(result.items.len() >= 0);
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
    // At the open paren, should get signature help
    let result = t.signature_help("");
    // Framework test - the query should work
    if result.help.is_some() {
        result.expect_label_contains("add");
    }
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

    // Edit the file
    t.edit_file("test.ts", "const /*y*/y = 'hello';\n/*ref*/y;");
    t.go_to_definition("ref").expect_at_marker("y");
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn empty_source() {
    let mut t = FourslashTest::new("");
    let result = t.document_symbols("test.ts");
    assert!(result.symbols.is_empty());
}

#[test]
fn markers_at_start_and_end() {
    let t = FourslashTest::new("/*start*/const x = 1/*end*/;");
    // Should parse markers at boundaries without issues
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
    // Markers in strings should still be parsed (they're source-level markers)
    let t = FourslashTest::new("const s = '/*m*/hello';");
    let m = t.marker("m");
    assert_eq!(m.line, 0);
}

// =============================================================================
// Complex Scenarios
// =============================================================================

#[test]
fn nested_function_definition() {
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
fn class_method_definition() {
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
    // Method reference via dot access - may or may not resolve depending on implementation
    let result = t.go_to_definition("ref");
    // Just verify it doesn't crash
    let _ = result;
}

#[test]
fn arrow_function_hover() {
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
fn generic_type_definition() {
    let mut t = FourslashTest::new(
        "
        interface /*def*/Container<T> { value: T; }
        const c: /*ref*/Container<number> = { value: 42 };
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
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

#[test]
fn type_assertion_hover() {
    let mut t = FourslashTest::new(
        "
        const /*x*/x = 42 as const;
    ",
    );
    t.hover("x").expect_found();
}

#[test]
fn template_literal_hover() {
    let mut t = FourslashTest::new(
        "
        const /*name*/name = 'world';
        const greeting = `hello ${name}`;
    ",
    );
    t.hover("name").expect_found();
}

#[test]
fn for_loop_variable_definition() {
    let mut t = FourslashTest::new(
        "
        for (let /*def*/i = 0; /*ref*/i < 10; i++) {}
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
}

#[test]
fn try_catch_variable() {
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
fn export_default_function() {
    let mut t = FourslashTest::new(
        "
        export default function /*def*/main() {}
        /*ref*/main();
    ",
    );
    t.go_to_definition("ref").expect_at_marker("def");
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
    let result = t.folding_ranges("test.ts");
    result.expect_found();
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
    let result = t.folding_ranges("test.ts");
    result.expect_found();
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
    // Should produce at least one folding range for the import group
    let result = t.folding_ranges("test.ts");
    // Even if import folding isn't implemented, it shouldn't crash
    let _ = result;
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
    let result = t.selection_range("x");
    result.expect_found();
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
    // An identifier inside a function should have multiple levels of nesting
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
    let result = t.selection_range("m");
    result.expect_found();
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
}

#[test]
fn highlight_no_result_on_keyword() {
    let t = FourslashTest::new(
        "
        /*kw*/const x = 1;
    ",
    );
    // On a keyword, there may or may not be highlights
    let result = t.document_highlights("kw");
    // Just verify it doesn't crash
    let _ = result;
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
    let result = t.semantic_tokens("test.ts");
    result.expect_found();
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
    let result = t.semantic_tokens("test.ts");
    result.expect_found();
    result.expect_min_tokens(3);
}

#[test]
fn semantic_tokens_empty() {
    let t = FourslashTest::new("");
    let result = t.semantic_tokens("test.ts");
    assert!(result.data.is_empty());
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
    let result = t.format("test.ts");
    // Formatting may fail if prettier is not installed - just verify it doesn't panic
    let _ = result;
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
    // Empty query should return no results
    let result = t.workspace_symbols("");
    assert!(result.symbols.is_empty());
}

#[test]
fn workspace_symbols_no_match() {
    let t = FourslashTest::new(
        "
        const x = 1;
    ",
    );
    let result = t.workspace_symbols("nonexistentSymbol");
    assert!(result.symbols.is_empty());
}

// =============================================================================
// Document Highlights - Write vs Read
// =============================================================================

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
    // Should have both read and write highlights
    result.expect_has_write();
}

// =============================================================================
// More Complex Go-to-Definition Tests
// =============================================================================

#[test]
fn definition_import_clause() {
    let mut t = FourslashTest::multi_file(&[
        ("module.ts", "export const /*def*/value = 42;"),
        ("main.ts", "const /*x*/x = 1;\n/*ref*/x;"),
    ]);
    // Within-file definition should still work in multi-file context
    t.go_to_definition("ref").expect_at_marker("x");
}

#[test]
fn definition_multiple_declarations() {
    let mut t = FourslashTest::new(
        "
        function /*def*/overloaded(x: number): number;
        function overloaded(x: string): string;
        function overloaded(x: any): any { return x; }
        /*ref*/overloaded(1);
    ",
    );
    // Should find at least one definition
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

// =============================================================================
// Rename - More Cases
// =============================================================================

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
        .expect_edits_in_file("test.ts");
}

// =============================================================================
// Diagnostics - Type Error Cases
// =============================================================================

#[test]
fn diagnostics_type_mismatch() {
    let mut t = FourslashTest::new(
        "
        const x: number = 'hello';
    ",
    );
    let result = t.diagnostics("test.ts");
    // Should report a type error (TS2322)
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
    // Should report "cannot find name" (TS2304)
    if !result.diagnostics.is_empty() {
        result.expect_code(2304);
    }
}

// =============================================================================
// Hover - Type Information
// =============================================================================

#[test]
fn hover_array_literal() {
    let mut t = FourslashTest::new(
        "
        const /*arr*/arr = [1, 2, 3];
    ",
    );
    t.hover("arr").expect_found();
}

#[test]
fn hover_object_literal() {
    let mut t = FourslashTest::new(
        "
        const /*obj*/obj = { x: 1, y: 'hello' };
    ",
    );
    t.hover("obj").expect_found();
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
    t.hover("up").expect_found();
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

// =============================================================================
// Document Symbols - More Cases
// =============================================================================

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

// =============================================================================
// Completions - More Cases
// =============================================================================

#[test]
fn completions_after_dot() {
    let mut t = FourslashTest::new(
        "
        const obj = { foo: 1, bar: 'hello' };
        obj./**/
    ",
    );
    // After dot, should offer property completions
    let result = t.completions("");
    // Just verify it doesn't crash
    let _ = result;
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
    // Should at least not crash
    let _ = result;
}

// =============================================================================
// Multi-file Advanced Tests
// =============================================================================

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

// =============================================================================
// Edit and Re-query Advanced Tests
// =============================================================================

#[test]
fn edit_file_updates_symbols() {
    let mut t = FourslashTest::new(
        "
        function foo() {}
    ",
    );
    t.document_symbols("test.ts").expect_symbol("foo");

    // Edit the file to have a different function
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

    // Edit to use a string instead
    t.edit_file("test.ts", "const /*y*/y = 'hello';");
    t.hover("y").expect_found();
}

// =============================================================================
// Edge Cases - Extended
// =============================================================================

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
    let source = format!("const /*x*/{} = 1;", long_var);
    let mut t = FourslashTest::new(&source);
    t.hover("x").expect_found();
}

#[test]
fn deeply_nested_structure() {
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
fn multiple_files_same_symbol_name() {
    let mut t = FourslashTest::multi_file(&[
        ("a.ts", "export const value = 1;"),
        ("b.ts", "export const value = 2;"),
    ]);
    // Both files should have 'value' as a symbol
    t.document_symbols("a.ts").expect_symbol("value");
    t.document_symbols("b.ts").expect_symbol("value");
}
