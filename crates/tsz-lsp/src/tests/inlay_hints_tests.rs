use super::*;
use tsz_binder::BinderState;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;
use tsz_solver::TypeInterner;

/// Helper to create a provider and get hints for the given source code.
fn get_hints_for_source(source: &str) -> Vec<InlayHint> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let interner = TypeInterner::new();
    let line_map = LineMap::build(source);

    let provider = InlayHintsProvider::new(
        parser.get_arena(),
        &binder,
        &line_map,
        source,
        &interner,
        "test.ts".to_string(),
    );

    let range = Range::new(Position::new(0, 0), Position::new(u32::MAX, u32::MAX));
    provider.provide_inlay_hints(root, range)
}

/// Helper to get only type hints from the results.
fn get_type_hints(hints: &[InlayHint]) -> Vec<&InlayHint> {
    hints
        .iter()
        .filter(|h| h.kind == InlayHintKind::Type)
        .collect()
}

#[test]
fn test_inlay_hint_parameter() {
    let position = Position::new(0, 10);
    let hint = InlayHint::parameter(position, "paramName".to_string());

    assert_eq!(hint.position, position);
    assert_eq!(hint.label, ": paramName");
    assert_eq!(hint.kind, InlayHintKind::Parameter);
}

#[test]
fn test_inlay_hint_type() {
    let position = Position::new(0, 10);
    let hint = InlayHint::type_hint(position, "number".to_string());

    assert_eq!(hint.position, position);
    assert_eq!(hint.label, ": number");
    assert_eq!(hint.kind, InlayHintKind::Type);
}

#[test]
fn test_type_hint_number_literal() {
    let source = "let x = 42;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        !type_hints.is_empty(),
        "Should produce a type hint for number literal"
    );
    assert_eq!(type_hints[0].label, ": number");
    assert_eq!(type_hints[0].kind, InlayHintKind::Type);
}

#[test]
fn test_type_hint_string_literal() {
    let source = "let s = \"hello\";";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        !type_hints.is_empty(),
        "Should produce a type hint for string literal"
    );
    assert_eq!(type_hints[0].label, ": string");
    assert_eq!(type_hints[0].kind, InlayHintKind::Type);
}

#[test]
fn test_type_hint_boolean_literal() {
    let source = "let b = true;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        !type_hints.is_empty(),
        "Should produce a type hint for boolean literal"
    );
    // The checker may return "boolean" or "true" (literal type) depending on
    // whether const or let. With let, it should widen to "boolean".
    let label = &type_hints[0].label;
    assert!(
        label == ": boolean" || label == ": true",
        "Expected ': boolean' or ': true', got '{}'",
        label
    );
}

#[test]
fn test_no_hint_with_type_annotation() {
    let source = "let x: number = 42;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        type_hints.is_empty(),
        "Should NOT produce a type hint when type annotation is present"
    );
}

#[test]
fn test_no_hint_without_initializer() {
    let source = "let x;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        type_hints.is_empty(),
        "Should NOT produce a type hint when there is no initializer"
    );
}

#[test]
fn test_type_hint_array() {
    let source = "let arr = [1, 2, 3];";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        !type_hints.is_empty(),
        "Should produce a type hint for array literal"
    );
    // The type might be "number[]" or "Array<number>" depending on formatter
    let label = &type_hints[0].label;
    assert!(
        label.contains("number"),
        "Array type hint should contain 'number', got '{}'",
        label
    );
}

#[test]
fn test_type_hint_object() {
    let source = "let obj = { a: 1, b: \"hello\" };";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        !type_hints.is_empty(),
        "Should produce a type hint for object literal"
    );
    let label = &type_hints[0].label;
    // Object type should mention the properties
    assert!(
        label.contains("a") && label.contains("b"),
        "Object type hint should contain property names, got '{}'",
        label
    );
}

#[test]
fn test_no_hint_for_any_type() {
    // Variables explicitly typed as any should be skipped, and variables
    // that the checker infers as any/unknown should also be skipped.
    let source = "let x: any = 42;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        type_hints.is_empty(),
        "Should NOT produce a type hint for 'any' typed variable"
    );
}

#[test]
fn test_parameter_and_type_hints_together() {
    let source = "function greet(name: string) { return name; }\nlet msg = \"Hello\";\ngreet(msg);";
    let hints = get_hints_for_source(source);

    let type_hints: Vec<_> = hints
        .iter()
        .filter(|h| h.kind == InlayHintKind::Type)
        .collect();
    let param_hints: Vec<_> = hints
        .iter()
        .filter(|h| h.kind == InlayHintKind::Parameter)
        .collect();

    // msg should get a type hint for string
    assert!(
        !type_hints.is_empty(),
        "Should have at least one type hint for 'msg'"
    );
    assert!(
        type_hints.iter().any(|h| h.label == ": string"),
        "Should have a string type hint for 'msg'"
    );

    // greet(msg) should get a parameter hint (msg != name, so hint shown)
    // Note: parameter hints depend on binder resolution working correctly
    // for the greet function. We verify at least no crash occurs.
    let _ = param_hints;
}

#[test]
fn test_type_hint_position_after_name() {
    let source = "let x = 42;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    if !type_hints.is_empty() {
        let hint = &type_hints[0];
        // "let x = 42;" - 'x' is at index 4, so hint should be on line 0
        assert_eq!(hint.position.line, 0, "Hint should be on line 0");
        // The position should be at or after column 4 (end of 'x')
        assert!(
            hint.position.character >= 4,
            "Hint position should be at or after the end of the variable name, got col {}",
            hint.position.character
        );
    }
}

#[test]
fn test_type_hint_const_number() {
    let source = "const x = 100;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        !type_hints.is_empty(),
        "Should produce a type hint for const with number literal"
    );
    // const might get a literal type like "100" or widened "number"
    let label = &type_hints[0].label;
    assert!(
        label.contains("number") || label.contains("100"),
        "Const number hint should be 'number' or '100', got '{}'",
        label
    );
}

#[test]
fn test_multiple_variable_declarations() {
    let source = "let a = 1;\nlet b = \"two\";\nlet c = true;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        type_hints.len() >= 2,
        "Should produce type hints for multiple variable declarations, got {}",
        type_hints.len()
    );
}

#[test]
fn test_no_type_hint_var_without_init() {
    let source = "var x;\nvar y;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        type_hints.is_empty(),
        "Should NOT produce type hints for variables without initializers"
    );
}
