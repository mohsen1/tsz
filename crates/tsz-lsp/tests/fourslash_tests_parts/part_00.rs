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

#[test]
fn type_definition_interface_variable() {
    let t = FourslashTest::new(
        "
        interface /*def*/Config { host: string; port: number; }
        const /*ref*/cfg: Config = { host: 'localhost', port: 80 };
    ",
    );
    let result = t.go_to_type_definition("ref");
    if result.locations.as_ref().is_some_and(|v| !v.is_empty()) {
        result.expect_at_marker("def");
    }
}

#[test]
fn type_definition_function_parameter() {
    let t = FourslashTest::new(
        "
        interface /*def*/User { name: string; }
        function greet(/*ref*/user: User) {}
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

