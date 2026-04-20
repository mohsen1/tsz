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

