use super::*;
use tsz_binder::BinderState;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;
use tsz_solver::TypeInterner;

/// Helper to set up hover infrastructure and get hover info at a position.
fn get_hover_at(source: &str, line: u32, col: u32) -> Option<HoverInfo> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let interner = TypeInterner::new();
    let line_map = LineMap::build(source);

    let provider = HoverProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );

    let pos = Position::new(line, col);
    let mut cache = None;
    provider.get_hover(root, pos, &mut cache)
}

#[test]
fn test_hover_variable_type() {
    let source = "/** The answer */\nconst x = 42;\nx;";
    let info = get_hover_at(source, 2, 0);
    assert!(info.is_some(), "Should find hover info");
    if let Some(info) = info {
        assert!(!info.contents.is_empty(), "Should have contents");
        assert!(
            info.contents[0].contains("x"),
            "Should contain variable name"
        );
        assert!(info.range.is_some(), "Should have range");
    }
}

#[test]
fn test_hover_at_eof_identifier() {
    let source = "/** The answer */\nconst x = 42;\nx";
    let info = get_hover_at(source, 2, 1);
    assert!(info.is_some(), "Should find hover info at EOF");
    if let Some(info) = info {
        assert!(
            info.contents
                .iter()
                .any(|content| content.contains("The answer"))
        );
    }
}

#[test]
fn test_hover_incomplete_member_access() {
    let source = "const foo = 1;\nfoo.";
    let info = get_hover_at(source, 1, 4);
    assert!(
        info.is_some(),
        "Should find hover info after incomplete member access"
    );
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("foo"),
            "Should use base identifier for hover"
        );
    }
}

#[test]
fn test_hover_jsdoc_summary_and_params() {
    let source = "/**\n * Adds two numbers.\n * @param a First number.\n * @param b Second number.\n */\nfunction add(a: number, b: number): number { return a + b; }\nadd(1, 2);";
    let info = get_hover_at(source, 6, 0).expect("Expected hover info");
    let doc = info
        .contents
        .iter()
        .find(|c| c.contains("Adds two numbers."))
        .cloned()
        .unwrap_or_default();
    assert!(doc.contains("Adds two numbers."));
    assert!(doc.contains("Parameters:"));
    assert!(doc.contains("`a` First number."));
    assert!(doc.contains("`b` Second number."));
}

#[test]
fn test_hover_no_symbol() {
    let source = "const x = 42;";
    let info = get_hover_at(source, 0, 13);
    assert!(info.is_none(), "Should not find hover info at semicolon");
}

#[test]
fn test_hover_function() {
    let source = "function foo() { return 1; }\nfoo();";
    let info = get_hover_at(source, 1, 0);
    assert!(info.is_some(), "Should find hover info for function");
    if let Some(info) = info {
        assert!(
            info.contents[0].contains("foo"),
            "Should contain function name"
        );
    }
}

// =========================================================================
// New tests for tsserver-compatible quickinfo format
// =========================================================================

#[test]
fn test_hover_const_variable_display_string() {
    let source = "const x = 42;\nx;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover info");
    assert!(
        info.display_string.starts_with("const ") || info.display_string.starts_with("let "),
        "Variable display_string should start with const or let keyword, got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("x"),
        "display_string should contain variable name 'x', got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains(':'),
        "display_string should contain colon for type annotation, got: {}",
        info.display_string
    );
    assert!(
        info.kind == "const" || info.kind == "let",
        "Kind should be 'const' or 'let' for block-scoped variable, got: {}",
        info.kind
    );
}

#[test]
fn test_hover_let_variable_display_string() {
    let source = "let y = \"hello\";\ny;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover info");
    assert!(
        info.display_string.starts_with("let "),
        "Let variable display_string should start with 'let ', got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("y"),
        "display_string should contain variable name 'y', got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "let",
        "Kind should be 'let' for let variable, got: {}",
        info.kind
    );
}

#[test]
fn test_hover_var_variable_display_string() {
    let source = "var z = true;\nz;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover info");
    assert!(
        info.display_string.starts_with("var "),
        "Var variable display_string should start with 'var ', got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("z"),
        "display_string should contain variable name 'z', got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "var",
        "Kind should be 'var' for var variable, got: {}",
        info.kind
    );
}

#[test]
fn test_hover_function_display_string() {
    let source = "function greet(name: string): void {}\ngreet(\"hi\");";
    let info = get_hover_at(source, 1, 0).expect("Should find hover info");
    assert!(
        info.display_string.starts_with("function "),
        "Function display_string should start with 'function ', got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("greet"),
        "display_string should contain function name 'greet', got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "function",
        "Kind should be 'function', got: {}",
        info.kind
    );
}

#[test]
fn test_hover_class_display_string() {
    let source = "class MyClass { x: number = 0; }\nlet c = new MyClass();";
    let info = get_hover_at(source, 0, 6).expect("Should find hover info for class");
    assert!(
        info.display_string.starts_with("class "),
        "Class display_string should start with 'class ', got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("MyClass"),
        "display_string should contain class name, got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "class",
        "Kind should be 'class', got: {}",
        info.kind
    );
}

#[test]
fn test_hover_interface_display_string() {
    let source = "interface IPoint { x: number; y: number; }\nlet p: IPoint;";
    let info = get_hover_at(source, 0, 10).expect("Should find hover info for interface");
    assert!(
        info.display_string.starts_with("interface "),
        "Interface display_string should start with 'interface ', got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("IPoint"),
        "display_string should contain interface name, got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "interface",
        "Kind should be 'interface', got: {}",
        info.kind
    );
}

#[test]
fn test_hover_enum_display_string() {
    let source = "enum Color { Red, Green, Blue }\nlet c: Color;";
    let info = get_hover_at(source, 0, 5).expect("Should find hover info for enum");
    assert!(
        info.display_string.starts_with("enum "),
        "Enum display_string should start with 'enum ', got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("Color"),
        "display_string should contain enum name, got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "enum",
        "Kind should be 'enum', got: {}",
        info.kind
    );
}

#[test]
fn test_hover_kind_field_populated() {
    let source = "const a = 1;\nlet b = 2;\nfunction f() {}\nclass C {}\ninterface I {}\na; b;";
    let info_a = get_hover_at(source, 5, 0).expect("Should find hover info for a");
    assert!(
        !info_a.kind.is_empty(),
        "Kind should not be empty for const variable"
    );
    let info_b = get_hover_at(source, 5, 3).expect("Should find hover info for b");
    assert!(
        !info_b.kind.is_empty(),
        "Kind should not be empty for let variable"
    );
}

#[test]
fn test_hover_documentation_field_with_jsdoc() {
    let source = "/** My variable */\nconst x = 42;\nx;";
    let info = get_hover_at(source, 2, 0).expect("Should find hover info");
    assert!(
        info.documentation.contains("My variable"),
        "documentation field should contain JSDoc summary, got: '{}'",
        info.documentation
    );
}

#[test]
fn test_hover_documentation_field_empty_without_jsdoc() {
    let source = "const x = 42;\nx;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover info");
    assert!(
        info.documentation.is_empty(),
        "documentation field should be empty without JSDoc, got: '{}'",
        info.documentation
    );
}

#[test]
fn test_hover_display_string_in_code_block() {
    let source = "const x = 42;\nx;";
    let info = get_hover_at(source, 1, 0).expect("Should find hover info");
    assert!(
        info.contents[0].contains(&info.display_string),
        "Code block should contain the display_string. Code block: '{}', display_string: '{}'",
        info.contents[0],
        info.display_string
    );
}

#[test]
fn test_hover_type_alias_display_string() {
    let source = "type MyStr = string;\nlet s: MyStr;";
    let info = get_hover_at(source, 0, 5).expect("Should find hover info for type alias");
    assert!(
        info.display_string.starts_with("type "),
        "Type alias display_string should start with 'type ', got: {}",
        info.display_string
    );
    assert!(
        info.display_string.contains("MyStr"),
        "display_string should contain type alias name, got: {}",
        info.display_string
    );
    assert_eq!(
        info.kind, "type",
        "Kind should be 'type', got: {}",
        info.kind
    );
}
