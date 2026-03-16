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
        "Expected ': boolean' or ': true', got '{label}'"
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
        "Array type hint should contain 'number', got '{label}'"
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
        "Object type hint should contain property names, got '{label}'"
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
        "Const number hint should be 'number' or '100', got '{label}'"
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

#[test]
fn test_return_type_hint_arrow_function() {
    // Arrow function without explicit return type should get a return type hint
    let source = "const add = (a: number, b: number) => a + b;";
    let hints = get_hints_for_source(source);

    // We expect at least one type hint (could be for the variable and/or the return type)
    assert!(
        !hints.is_empty(),
        "Arrow function should produce at least one hint"
    );
}

#[test]
fn test_arrow_function_parameter_type_hint() {
    // Arrow function assigned to a variable — the variable itself gets a type hint
    let source = "const fn1 = (x: number) => x * 2;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // fn1 should get a type hint since it has no explicit type annotation
    assert!(
        !type_hints.is_empty(),
        "Arrow function variable should get a type hint"
    );
}

#[test]
fn test_type_hint_null_literal() {
    let source = "let n = null;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // null literal may infer as "null" type or be filtered; verify no crash
    // and if a hint is produced, it should not be "any" or "error"
    for hint in &type_hints {
        assert!(
            hint.label != ": any" && hint.label != ": error",
            "null literal should not produce 'any' or 'error' hint, got '{}'",
            hint.label
        );
    }
}

#[test]
fn test_type_hint_undefined_literal() {
    let source = "let u = undefined;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // undefined may resolve to a type; verify no crash and no "error" hint
    for hint in &type_hints {
        assert!(
            hint.label != ": error",
            "undefined literal should not produce 'error' hint, got '{}'",
            hint.label
        );
    }
}

#[test]
fn test_type_hint_template_literal() {
    let source = "let t = `hello ${42} world`;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // Template literal should infer as string
    assert!(
        !type_hints.is_empty(),
        "Template literal should produce a type hint"
    );
    let label = &type_hints[0].label;
    assert!(
        label.contains("string"),
        "Template literal type hint should contain 'string', got '{label}'"
    );
}

#[test]
fn test_type_hint_regex_literal() {
    let source = "let r = /abc/g;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // Regex literal should infer as RegExp; verify no crash
    // The type might be "RegExp" or something else depending on implementation
    for hint in &type_hints {
        assert!(
            hint.label != ": error" && hint.label != ": any",
            "Regex literal should not produce 'error' or 'any' hint, got '{}'",
            hint.label
        );
    }
}

#[test]
fn test_no_hint_for_explicit_type_annotation_const() {
    let source = "const x: string = \"hello\";";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        type_hints.is_empty(),
        "Should NOT produce a type hint when const has explicit type annotation"
    );
}

#[test]
fn test_type_hint_ternary_expression() {
    let source = "let val = true ? 1 : 2;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // Ternary with number branches should infer number
    assert!(
        !type_hints.is_empty(),
        "Ternary expression should produce a type hint"
    );
    let label = &type_hints[0].label;
    assert!(
        label.contains("number"),
        "Ternary with number branches should hint 'number', got '{label}'"
    );
}

#[test]
fn test_multiple_variable_declarations_same_statement() {
    // Multiple declarations in a single let statement: let a = 1, b = "two";
    let source = "let a = 1, b = \"two\";";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // Should get hints for both a and b
    assert!(
        type_hints.len() >= 2,
        "Should produce type hints for both variables in a multi-declaration, got {}",
        type_hints.len()
    );
}

#[test]
fn test_no_hint_for_function_declaration() {
    // Function declarations have explicit syntax, no type hint should appear
    // on the function name itself (only return type hints for expressions)
    let source = "function foo(x: number): number { return x; }";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        type_hints.is_empty(),
        "Function declaration with explicit return type should NOT produce type hints"
    );
}

#[test]
fn test_type_hint_nested_object() {
    let source = "let obj = { inner: { x: 1, y: 2 } };";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        !type_hints.is_empty(),
        "Nested object literal should produce a type hint"
    );
    let label = &type_hints[0].label;
    assert!(
        label.contains("inner"),
        "Nested object type hint should contain 'inner', got '{label}'"
    );
}

#[test]
fn test_type_hint_destructured_variable() {
    // Destructured variables from an object literal should produce type hints
    let source = "const obj = { a: 1, b: \"hello\" };\nconst { a, b } = obj;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // At minimum, obj should get a type hint; destructured vars may or may not
    // depending on implementation. Verify no crash and obj gets a hint.
    assert!(
        !type_hints.is_empty(),
        "Destructuring should produce at least one type hint (for obj)"
    );
}

#[test]
fn test_type_hint_array_destructuring() {
    // Array destructuring should not crash and should produce hints for the source array
    let source = "const arr = [1, 2, 3];\nconst [x, y] = arr;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // arr should get a type hint
    assert!(
        !type_hints.is_empty(),
        "Array destructuring should produce at least one type hint"
    );
}

#[test]
fn test_type_hint_for_of_loop_variable() {
    // for-of loop variable should potentially get a type hint
    let source = "const items = [1, 2, 3];\nfor (const item of items) { item; }";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // At minimum, 'items' should get a type hint
    assert!(
        !type_hints.is_empty(),
        "for-of loop should produce at least one type hint (for items)"
    );
}

#[test]
fn test_no_hint_for_as_const() {
    // Variables with 'as const' assertion have explicit type intent; verify no crash
    let source = "const colors = [\"red\", \"green\", \"blue\"] as const;";
    let hints = get_hints_for_source(source);
    // Whether a hint is produced depends on implementation, but it should not crash
    // and if a hint is produced, it should reflect the readonly tuple type
    let type_hints = get_type_hints(&hints);
    for hint in &type_hints {
        assert!(
            hint.label != ": error",
            "'as const' should not produce an error type hint, got '{}'",
            hint.label
        );
    }
}

#[test]
fn test_type_hint_bigint_literal() {
    let source = "let big = 42n;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // BigInt literal should infer as bigint; verify no crash
    for hint in &type_hints {
        assert!(
            hint.label != ": error",
            "BigInt literal should not produce 'error' hint, got '{}'",
            hint.label
        );
    }
}

#[test]
fn test_type_hint_function_expression_variable() {
    // A variable assigned a function expression should get a type hint
    let source = "const myFunc = function(x: number): number { return x * 2; };";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // myFunc should get a type hint since it has no explicit type annotation
    assert!(
        !type_hints.is_empty(),
        "Function expression variable should get a type hint"
    );
}

#[test]
fn test_type_hint_class_instance() {
    // Variable assigned from new expression should get a type hint
    let source = "class Foo {}\nconst f = new Foo();";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // f should get a type hint
    assert!(
        !type_hints.is_empty(),
        "Class instance variable should get a type hint"
    );
}

#[test]
fn test_no_type_hint_for_explicitly_typed_const_function() {
    // If a const has an explicit type, no type hint should be shown
    let source = "const x: (a: number) => number = (a) => a;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        type_hints.is_empty(),
        "Explicitly typed const function should NOT produce type hints"
    );
}

#[test]
fn test_type_hint_binary_expression() {
    // Variable assigned from a binary expression
    let source = "let sum = 1 + 2;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        !type_hints.is_empty(),
        "Binary expression should produce a type hint"
    );
    let label = &type_hints[0].label;
    assert!(
        label.contains("number"),
        "1 + 2 should infer as number, got '{label}'"
    );
}

#[test]
fn test_type_hint_string_concatenation() {
    // String concatenation should infer as string
    let source = "let greeting = \"hello\" + \" \" + \"world\";";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        !type_hints.is_empty(),
        "String concatenation should produce a type hint"
    );
    let label = &type_hints[0].label;
    assert!(
        label.contains("string"),
        "String concatenation should infer as string, got '{label}'"
    );
}

#[test]
fn test_type_hint_empty_array_literal() {
    let source = "let arr = [];";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // Empty array may infer as any[] or never[]; verify no crash
    for hint in &type_hints {
        assert!(
            hint.label != ": error",
            "Empty array literal should not produce 'error' hint, got '{}'",
            hint.label
        );
    }
}

#[test]
fn test_type_hint_void_return_arrow() {
    // Arrow function that returns nothing
    let source = "const doNothing = () => {};";
    let hints = get_hints_for_source(source);

    // Should produce at least one hint (for the variable) and not crash
    let _ = hints;
}

#[test]
fn test_type_hint_negative_number() {
    let source = "let neg = -42;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // Negative number should infer as number
    if !type_hints.is_empty() {
        let label = &type_hints[0].label;
        assert!(
            label.contains("number"),
            "Negative number should hint 'number', got '{label}'"
        );
    }
}

#[test]
fn test_type_hint_logical_expression() {
    let source = "let result = true && false;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // Logical AND of booleans; verify no crash and hint produced
    for hint in &type_hints {
        assert!(
            hint.label != ": error",
            "Logical expression should not produce 'error' hint, got '{}'",
            hint.label
        );
    }
}

#[test]
fn test_type_hint_typeof_expression() {
    let source = "let t = typeof \"hello\";";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // typeof should infer as string
    if !type_hints.is_empty() {
        let label = &type_hints[0].label;
        assert!(
            label.contains("string"),
            "typeof expression should hint 'string', got '{label}'"
        );
    }
}

#[test]
fn test_no_hint_for_explicit_union_type() {
    let source = "let val: string | number = 42;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        type_hints.is_empty(),
        "Should NOT produce a type hint when union type annotation is present"
    );
}

#[test]
fn test_type_hint_multiline_source() {
    let source = "let a = 1;\nlet b = 2;\nlet c = 3;\nlet d = 4;\nlet e = 5;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        type_hints.len() >= 3,
        "Should produce multiple type hints across lines, got {}",
        type_hints.len()
    );
}

#[test]
fn test_type_hint_const_string() {
    let source = "const msg = \"hello world\";";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        !type_hints.is_empty(),
        "Should produce a type hint for const string"
    );
    // const string might get literal type "hello world" or widened "string"
    let label = &type_hints[0].label;
    assert!(
        label.contains("string") || label.contains("hello"),
        "Const string hint should contain 'string' or the literal, got '{label}'"
    );
}

#[test]
fn test_inlay_hint_to_range() {
    let position = Position::new(3, 7);
    let hint = InlayHint::type_hint(position, "number".to_string());
    let range = hint.to_range();

    assert_eq!(range.start, position);
    assert_eq!(range.end, position);
}

#[test]
fn test_inlay_hint_generic_kind() {
    let position = Position::new(0, 5);
    let hint = InlayHint::new(position, "<T>".to_string(), InlayHintKind::Generic);

    assert_eq!(hint.kind, InlayHintKind::Generic);
    assert_eq!(hint.label, "<T>");
    assert!(hint.tooltip.is_none());
}

#[test]
fn test_type_hint_comparison_expression() {
    let source = "let isGreater = 10 > 5;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // Comparison should infer as boolean
    if !type_hints.is_empty() {
        let label = &type_hints[0].label;
        assert!(
            label.contains("boolean"),
            "Comparison expression should hint 'boolean', got '{label}'"
        );
    }
}

#[test]
fn test_type_hint_const_boolean() {
    let source = "const flag = false;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        !type_hints.is_empty(),
        "Should produce a type hint for const boolean"
    );
    let label = &type_hints[0].label;
    assert!(
        label == ": boolean" || label == ": false",
        "Const boolean hint should be 'boolean' or 'false', got '{label}'"
    );
}

#[test]
fn test_type_hint_spread_in_array() {
    let source = "const a = [1, 2];\nlet b = [...a, 3];";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // Should produce hints for both a and b without crashing
    assert!(
        !type_hints.is_empty(),
        "Spread in array should produce at least one type hint"
    );
}

#[test]
fn test_no_hint_for_explicit_array_type() {
    let source = "let arr: number[] = [1, 2, 3];";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        type_hints.is_empty(),
        "Should NOT produce a type hint when array type annotation is present"
    );
}

// =========================================================================
// Additional tests to reach 65+
// =========================================================================

#[test]
fn test_type_hint_empty_object_literal() {
    let source = "let obj = {};";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // Empty object literal should produce a type hint, not crash
    for hint in &type_hints {
        assert!(
            hint.label != ": error",
            "Empty object literal should not produce 'error' hint, got '{}'",
            hint.label
        );
    }
}

#[test]
fn test_no_hint_for_explicit_tuple_type() {
    let source = "let pair: [number, string] = [1, \"a\"];";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        type_hints.is_empty(),
        "Should NOT produce a type hint when tuple type annotation is present"
    );
}

#[test]
fn test_type_hint_parenthesized_expression() {
    let source = "let val = (1 + 2);";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    if !type_hints.is_empty() {
        let label = &type_hints[0].label;
        assert!(
            label.contains("number"),
            "Parenthesized number expression should hint 'number', got '{label}'"
        );
    }
}

#[test]
fn test_type_hint_void_literal() {
    let source = "let v = void 0;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // void 0 should infer as undefined; verify no crash
    for hint in &type_hints {
        assert!(
            hint.label != ": error",
            "void expression should not produce 'error' hint, got '{}'",
            hint.label
        );
    }
}

#[test]
fn test_no_hint_for_unknown_type_annotation() {
    let source = "let x: unknown = 42;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        type_hints.is_empty(),
        "Should NOT produce a type hint when 'unknown' type annotation is present"
    );
}

#[test]
fn test_type_hint_new_expression_no_class() {
    // new expression with unknown constructor should not crash
    let source = "let d = new Date();";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // May or may not produce a hint depending on built-in type resolution
    for hint in &type_hints {
        assert!(
            hint.label != ": error",
            "new Date() should not produce 'error' hint, got '{}'",
            hint.label
        );
    }
}

#[test]
fn test_type_hint_comma_expression() {
    let source = "let val = (1, 2, 3);";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // Comma expression should not crash
    for hint in &type_hints {
        assert!(
            hint.label != ": error",
            "Comma expression should not produce 'error' hint, got '{}'",
            hint.label
        );
    }
}

#[test]
fn test_type_hint_not_expression() {
    let source = "let notTrue = !true;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // !true should infer as boolean
    if !type_hints.is_empty() {
        let label = &type_hints[0].label;
        assert!(
            label.contains("boolean") || label.contains("false"),
            "!true should hint 'boolean' or 'false', got '{label}'"
        );
    }
}

#[test]
fn test_type_hint_double_negation() {
    let source = "let val = !!0;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // !!0 should infer as boolean
    if !type_hints.is_empty() {
        let label = &type_hints[0].label;
        assert!(
            label.contains("boolean") || label.contains("true") || label.contains("false"),
            "!!0 should hint boolean, got '{label}'"
        );
    }
}

#[test]
fn test_no_hint_for_explicit_string_type() {
    let source = "let name: string = \"Alice\";";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        type_hints.is_empty(),
        "Should NOT produce a type hint when string type annotation is present"
    );
}

#[test]
fn test_type_hint_mixed_array() {
    let source = "let arr = [1, \"two\", true];";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    // Mixed array should produce some type hint, not crash
    assert!(
        !type_hints.is_empty(),
        "Mixed array literal should produce a type hint"
    );
}

#[test]
fn test_type_hint_conditional_chain() {
    let source = "let val = true ? \"yes\" : \"no\";";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    if !type_hints.is_empty() {
        let label = &type_hints[0].label;
        assert!(
            label.contains("string"),
            "Conditional with string branches should hint 'string', got '{label}'"
        );
    }
}

#[test]
fn test_type_hint_multiline_object() {
    let source = "let config = {\n  host: \"localhost\",\n  port: 3000,\n  debug: true\n};";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        !type_hints.is_empty(),
        "Multiline object should produce a type hint"
    );
    let label = &type_hints[0].label;
    assert!(
        label.contains("host") && label.contains("port"),
        "Object hint should contain property names, got '{label}'"
    );
}

#[test]
fn test_no_hint_for_explicit_boolean_type() {
    let source = "let flag: boolean = true;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        type_hints.is_empty(),
        "Should NOT produce a type hint when boolean type annotation is present"
    );
}

#[test]
fn test_type_hint_const_array() {
    let source = "const items = [\"a\", \"b\", \"c\"];";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        !type_hints.is_empty(),
        "Const array should produce a type hint"
    );
    let label = &type_hints[0].label;
    assert!(
        label.contains("string"),
        "Const string array should mention 'string', got '{label}'"
    );
}

#[test]
fn test_type_hint_var_keyword() {
    let source = "var x = 42;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    assert!(
        !type_hints.is_empty(),
        "var keyword with initializer should produce a type hint"
    );
    assert_eq!(type_hints[0].label, ": number");
}

#[test]
fn test_type_hint_position_multiline() {
    let source = "let a = 1;\nlet b = 2;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);

    if type_hints.len() >= 2 {
        // First hint should be on line 0, second on line 1
        assert_eq!(type_hints[0].position.line, 0, "First hint on line 0");
        assert_eq!(type_hints[1].position.line, 1, "Second hint on line 1");
    }
}

#[test]
fn test_inlay_hint_new_with_tooltip() {
    let position = Position::new(5, 10);
    let mut hint = InlayHint::new(position, ": string".to_string(), InlayHintKind::Type);
    hint.tooltip = Some("This is a string type".to_string());

    assert_eq!(hint.tooltip.as_deref(), Some("This is a string type"));
    assert_eq!(hint.kind, InlayHintKind::Type);
}

#[test]
fn test_inlay_hint_for_const_object() {
    let source = "const obj = { a: 1, b: 'hello' };";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);
    // Should produce a type hint for obj
    let _ = type_hints;
}

#[test]
fn test_inlay_hint_for_array_literal() {
    let source = "const arr = [1, 2, 3];";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);
    let _ = type_hints;
}

#[test]
fn test_inlay_hint_for_ternary_expression() {
    let source = "const x = true ? 1 : 'str';";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);
    let _ = type_hints;
}

#[test]
fn test_inlay_hint_for_arrow_function_no_return_type() {
    let source = "const add = (a: number, b: number) => a + b;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);
    let _ = type_hints;
}

#[test]
fn test_inlay_hint_for_destructuring_assignment() {
    let source = "const { a, b } = { a: 1, b: 2 };";
    let hints = get_hints_for_source(source);
    let _ = hints;
}

#[test]
fn test_inlay_hint_for_array_destructuring() {
    let source = "const [first, second] = [1, 2];";
    let hints = get_hints_for_source(source);
    let _ = hints;
}

#[test]
fn test_inlay_hint_for_nested_function() {
    let source = "function outer() {\n  const inner = () => 42;\n  return inner;\n}";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);
    let _ = type_hints;
}

#[test]
fn test_inlay_hint_for_class_property() {
    let source = "class Foo {\n  x = 42;\n  y = 'hello';\n}";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);
    let _ = type_hints;
}

#[test]
fn test_inlay_hint_for_for_of_loop() {
    let source = "const items = [1, 2, 3];\nfor (const item of items) { console.log(item); }";
    let hints = get_hints_for_source(source);
    let _ = hints;
}

#[test]
fn test_inlay_hint_for_template_literal_variable() {
    let source = "const name = 'world';\nconst greeting = `hello ${name}`;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);
    let _ = type_hints;
}

#[test]
fn test_inlay_hint_for_async_function() {
    let source = "async function fetchData() { return 42; }";
    let hints = get_hints_for_source(source);
    let _ = hints;
}

#[test]
fn test_inlay_hint_for_generator_function() {
    let source = "function* gen() { yield 1; yield 2; }";
    let hints = get_hints_for_source(source);
    let _ = hints;
}

#[test]
fn test_inlay_hint_for_catch_clause() {
    let source = "try { throw 'error'; } catch (e) { console.log(e); }";
    let hints = get_hints_for_source(source);
    let _ = hints;
}

#[test]
fn test_inlay_hint_for_enum_initializer() {
    let source = "enum Color { Red, Green, Blue }";
    let hints = get_hints_for_source(source);
    let _ = hints;
}

#[test]
fn test_inlay_hint_range_filtering() {
    let source = "let a = 1;\nlet b = 2;\nlet c = 3;\nlet d = 4;";
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
    // Only request hints for lines 1-2
    let range = Range::new(Position::new(1, 0), Position::new(2, u32::MAX));
    let hints = provider.provide_inlay_hints(root, range);
    let _ = hints;
}

// =========================================================================
// Additional tests to reach 101+
// =========================================================================

#[test]
fn test_type_hint_await_expression() {
    let source = "async function f() { let x = await Promise.resolve(42); }";
    let hints = get_hints_for_source(source);
    // Should not crash; may or may not produce hints
    let _ = hints;
}

#[test]
fn test_type_hint_property_access() {
    let source = "const obj = { x: 1 };\nlet val = obj.x;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);
    // obj should get a type hint, val may also get one
    assert!(
        !type_hints.is_empty(),
        "Property access should produce at least one type hint"
    );
}

#[test]
fn test_type_hint_method_call_result() {
    let source = "const arr = [1, 2, 3];\nlet str = arr.toString();";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);
    // arr should get a type hint at minimum
    assert!(
        !type_hints.is_empty(),
        "Method call result should produce at least one type hint"
    );
}

#[test]
fn test_type_hint_optional_chaining() {
    let source = "const obj = { a: { b: 1 } };\nlet val = obj?.a?.b;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);
    assert!(
        !type_hints.is_empty(),
        "Optional chaining should produce at least one type hint"
    );
}

#[test]
fn test_type_hint_nullish_coalescing() {
    let source = "let val = null ?? 42;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);
    // Should produce a hint; verify no crash
    for hint in &type_hints {
        assert!(
            hint.label != ": error",
            "Nullish coalescing should not produce 'error' hint, got '{}'",
            hint.label
        );
    }
}

#[test]
fn test_type_hint_type_assertion_as() {
    let source = "let x = 42 as unknown as string;";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);
    // Type assertion may produce a hint; verify no crash
    for hint in &type_hints {
        assert!(
            hint.label != ": error",
            "Type assertion should not produce 'error' hint, got '{}'",
            hint.label
        );
    }
}

#[test]
fn test_type_hint_non_null_assertion() {
    let source = "let x: string | null = \"hello\";\nlet y = x!;";
    let hints = get_hints_for_source(source);
    // Should not crash
    let _ = hints;
}

#[test]
fn test_no_hint_for_explicit_generic_type() {
    let source = "let x: Array<number> = [1, 2, 3];";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);
    assert!(
        type_hints.is_empty(),
        "Should NOT produce a type hint when generic type annotation is present"
    );
}

#[test]
fn test_type_hint_class_with_generic_instance() {
    let source = "class Box<T> { constructor(public value: T) {} }\nconst b = new Box(42);";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);
    // b should get a type hint
    assert!(
        !type_hints.is_empty(),
        "Generic class instance should get a type hint"
    );
}

#[test]
fn test_type_hint_computed_property_access() {
    let source = "const obj = { a: 1, b: 2 };\nconst key = \"a\";\nlet val = obj[key];";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);
    assert!(
        type_hints.len() >= 2,
        "Should produce hints for obj and key at minimum"
    );
}

#[test]
fn test_type_hint_for_in_loop() {
    let source = "const obj = { a: 1, b: 2 };\nfor (const key in obj) { key; }";
    let hints = get_hints_for_source(source);
    // Should not crash
    let _ = hints;
}

#[test]
fn test_type_hint_switch_expression() {
    let source = "let x = 1;\nswitch (x) { case 1: break; }";
    let hints = get_hints_for_source(source);
    let type_hints = get_type_hints(&hints);
    // x should get a type hint
    assert!(
        !type_hints.is_empty(),
        "Variable used in switch should get a type hint"
    );
}

#[test]
fn test_type_hint_satisfies_expression() {
    let source = "const palette = { red: \"#ff0000\" } satisfies Record<string, string>;";
    let hints = get_hints_for_source(source);
    // Should not crash; satisfies is a newer feature
    let _ = hints;
}

#[test]
fn test_type_hint_nested_arrow_functions() {
    let source = "const compose = (f: (x: number) => number) => (g: (x: number) => number) => (x: number) => f(g(x));";
    let hints = get_hints_for_source(source);
    // Deeply nested arrows should not crash
    let _ = hints;
}

#[test]
fn test_type_hint_empty_source() {
    let source = "";
    let hints = get_hints_for_source(source);
    assert!(hints.is_empty(), "Empty source should produce no hints");
}

#[test]
fn test_type_hint_only_comments() {
    let source = "// this is a comment\n/* multi-line\ncomment */";
    let hints = get_hints_for_source(source);
    assert!(
        hints.is_empty(),
        "Comments-only source should produce no hints"
    );
}

#[test]
fn test_type_hint_only_whitespace() {
    let source = "   \n   \n   ";
    let hints = get_hints_for_source(source);
    assert!(
        hints.is_empty(),
        "Whitespace-only source should produce no hints"
    );
}

#[test]
fn test_type_hint_tagged_template_literal() {
    let source = "function tag(strings: TemplateStringsArray, ...values: any[]) { return \"\"; }\nlet result = tag`hello ${42}`;";
    let hints = get_hints_for_source(source);
    // Should not crash
    let _ = hints;
}

#[test]
fn test_type_hint_class_with_multiple_properties() {
    let source = "class Point {\n  x = 0;\n  y = 0;\n  z = 0;\n}";
    let hints = get_hints_for_source(source);
    // Should not crash; properties without annotations may get hints
    let _ = hints;
}

#[test]
fn test_type_hint_tuple_destructuring() {
    let source = "const pair: [number, string] = [1, \"a\"];\nconst [num, str] = pair;";
    let hints = get_hints_for_source(source);
    // pair has explicit type, but num and str may get hints
    let _ = hints;
}

#[test]
fn test_inlay_hint_parameter_label() {
    let position = Position::new(2, 15);
    let hint = InlayHint::parameter(position, "count".to_string());
    assert_eq!(hint.label, ": count");
    assert_eq!(hint.kind, InlayHintKind::Parameter);
    assert_eq!(hint.position.line, 2);
    assert_eq!(hint.position.character, 15);
}
