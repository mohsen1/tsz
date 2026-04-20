#[test]
fn code_actions_at_marker() {
    let t = FourslashTest::new(
        "
        const /*x*/x = 42;
    ",
    );
    let _ = t.code_actions_at("x");
}

#[test]
fn code_actions_on_function_no_crash() {
    let t = FourslashTest::new(
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
    let t = FourslashTest::new(
        "
        const x: number = /*e*/'hello';
    ",
    );
    let _ = t.code_actions_at("e");
}

#[test]
fn code_actions_on_inline_type_no_crash() {
    let t = FourslashTest::new(
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

