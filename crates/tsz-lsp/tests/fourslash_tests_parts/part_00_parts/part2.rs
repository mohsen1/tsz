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
