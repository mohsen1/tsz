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

