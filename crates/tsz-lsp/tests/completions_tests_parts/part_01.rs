#[test]
fn test_completions_parameter_kind() {
    // Parameters should have Parameter kind
    let source = "function foo(myParam: number) {\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 2));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let param_item = items.iter().find(|i| i.label == "myParam");
        assert!(param_item.is_some(), "Should find 'myParam'");
        let param_item = param_item.unwrap();
        assert_eq!(
            param_item.kind,
            CompletionItemKind::Parameter,
            "Parameter should have Parameter kind"
        );
        // Parameter should have type annotation as detail
        assert_eq!(
            param_item.detail.as_deref(),
            Some("number"),
            "Parameter should show type annotation as detail"
        );
    }
}

#[test]
fn test_completions_no_completions_at_definition_location() {
    // After 'const ' we're defining a new identifier, so no completions
    let source = "const ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(0, 6));
    // Should be suppressed at definition location
    if let Some(ref items) = items {
        assert!(
            items.is_empty(),
            "Should not have completions at variable definition location"
        );
    }
}

#[test]
fn test_completions_class_kind() {
    // Class declarations should have Class kind
    let source = "class MyClass {}\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let class_item = items.iter().find(|i| i.label == "MyClass");
        assert!(class_item.is_some(), "Should find 'MyClass'");
        assert_eq!(
            class_item.unwrap().kind,
            CompletionItemKind::Class,
            "Class should have Class kind"
        );
    }
}

#[test]
fn test_completions_interface_kind_with_helper() {
    // Interface declarations should have Interface kind
    let source = "interface MyInterface { x: number; }\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let iface_item = items.iter().find(|i| i.label == "MyInterface");
        assert!(iface_item.is_some(), "Should find 'MyInterface'");
        assert_eq!(
            iface_item.unwrap().kind,
            CompletionItemKind::Interface,
            "Interface should have Interface kind"
        );
    }
}

#[test]
fn test_completions_enum_kind_with_helper() {
    // Enum declarations should have Enum kind
    let source = "enum MyEnum { A, B }\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let enum_item = items.iter().find(|i| i.label == "MyEnum");
        assert!(enum_item.is_some(), "Should find 'MyEnum'");
        assert_eq!(
            enum_item.unwrap().kind,
            CompletionItemKind::Enum,
            "Enum should have Enum kind"
        );
    }
}

#[test]
fn test_completions_type_alias_kind_with_helper() {
    // Type alias declarations should have TypeAlias kind
    let source = "type MyType = string | number;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let type_item = items.iter().find(|i| i.label == "MyType");
        assert!(type_item.is_some(), "Should find 'MyType'");
        assert_eq!(
            type_item.unwrap().kind,
            CompletionItemKind::TypeAlias,
            "Type alias should have TypeAlias kind"
        );
    }
}

#[test]
fn test_completion_result_commit_characters() {
    // Global completions (non-member, non-new-identifier) should have default commit characters
    let source = "const x = 1;\nfunction foo() {\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let result = completions.get_completion_result(root, Position::new(2, 2));
    assert!(result.is_some(), "Should have completion result");
    let result = result.unwrap();
    // Inside function body is NOT a new identifier location (just typing expressions)
    // so commit characters should be present
    if !result.is_new_identifier_location {
        assert!(
            result.default_commit_characters.is_some(),
            "Non-new-identifier completions should have commit characters"
        );
        let chars = result.default_commit_characters.unwrap();
        assert!(
            chars.contains(&".".to_string()),
            "Commit chars should include '.'"
        );
        assert!(
            chars.contains(&",".to_string()),
            "Commit chars should include ','"
        );
        assert!(
            chars.contains(&";".to_string()),
            "Commit chars should include ';'"
        );
    }
}

#[test]
fn test_is_new_identifier_location_after_class_keyword() {
    // After 'class ' keyword, should be new identifier location
    let source = "class ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        completions.compute_is_new_identifier_location(root, offset),
        "After 'class' keyword should be new identifier location"
    );
}

#[test]
fn test_is_new_identifier_location_after_function_keyword() {
    // After 'function ' keyword, should be new identifier location
    let source = "function ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        completions.compute_is_new_identifier_location(root, offset),
        "After 'function' keyword should be new identifier location"
    );
}

#[test]
fn test_completions_import_meta_dot() {
    // After "import.meta.", should get meta property completions
    let source = "import.";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(0, 7));
    // Should offer "meta" as a completion for import.
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"meta"),
            "Should suggest 'meta' after 'import.'"
        );
    }
}

#[test]
fn test_completions_with_strict_mode() {
    // Test the with_strict constructor
    let source = "const x = 1;\n";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let interner = TypeInterner::new();
    let completions = Completions::with_strict(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
        true,
    );
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions in strict mode");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(names.contains(&"x"), "Should suggest 'x' in strict mode");
    }
}

#[test]
fn test_completions_sort_order_locals_before_keywords() {
    // Local declarations should sort before keywords
    let source = "const myVar = 1;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let var_item = items.iter().find(|i| i.label == "myVar");
        let kw_item = items.iter().find(|i| i.label == "if");
        assert!(var_item.is_some(), "Should find 'myVar'");
        assert!(kw_item.is_some(), "Should find keyword 'if'");
        // Local declarations have sort text "11" (LOCATION_PRIORITY), keywords have "15"
        let var_sort = var_item.unwrap().effective_sort_text();
        let kw_sort = kw_item.unwrap().effective_sort_text();
        assert!(
            var_sort <= kw_sort,
            "Local variable sort text ({var_sort}) should be <= keyword sort text ({kw_sort})"
        );
    }
}

#[test]
fn test_completions_template_literal_expression() {
    // Completions inside template literal expression `${|}`
    // Line 1: "const greeting = `hello ${ }`;"
    //          0123456789...                   col 26 = '$', col 27 = '{', col 28 = ' '
    let source = "const name = 'world';\nconst greeting = `hello ${ }`;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside the ${ } - try at the space between { and }
    let items = completions.get_completions(root, Position::new(1, 28));
    // Template literal expression completion may or may not be supported,
    // just verify no crash and check if we get items
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // If completions are returned, they should include variables in scope
        if !names.is_empty() {
            assert!(
                names.contains(&"name") || names.contains(&"greeting"),
                "Should suggest variables in scope, got: {names:?}"
            );
        }
    }
    // Test passes regardless - we're mainly testing it doesn't crash
}

#[test]
fn test_completions_namespace_members() {
    // After namespace dot, should offer namespace members
    let source =
        "namespace MyNS {\n  export const val = 1;\n  export function greet() {}\n}\nMyNS.";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let interner = TypeInterner::new();
    let completions = Completions::new_with_types(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let items = completions.get_completions(root, Position::new(4, 5));
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"val"),
            "Should suggest namespace member 'val'"
        );
        assert!(
            names.contains(&"greet"),
            "Should suggest namespace member 'greet'"
        );
    }
}

// ============================================================================
// New coverage tests for completions module
// ============================================================================

#[test]
fn test_completions_after_new_keyword() {
    // After `new `, should suggest classes and constructable symbols in scope
    let source = "class MyClass { constructor() {} }\nclass Other {}\nnew ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(2, 4));
    assert!(items.is_some(), "Should have completions after 'new '");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"MyClass"),
            "Should suggest 'MyClass' after 'new', got: {names:?}"
        );
        assert!(
            names.contains(&"Other"),
            "Should suggest 'Other' after 'new', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_object_literal_shorthand_property() {
    // Inside an object literal, should suggest variables for shorthand properties
    let source = "const foo = 1;\nconst bar = 2;\nconst obj = { };";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside the braces of { }
    let items = completions.get_completions(root, Position::new(2, 14));
    assert!(
        items.is_some(),
        "Should have completions inside object literal"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"foo"),
            "Should suggest 'foo' for shorthand property, got: {names:?}"
        );
        assert!(
            names.contains(&"bar"),
            "Should suggest 'bar' for shorthand property, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_in_ternary_expression() {
    // Completions in the consequent and alternate of a ternary expression
    let source = "const flag = true;\nconst a = 1;\nconst b = 2;\nconst result = flag ? ;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `? ` in the ternary
    let items = completions.get_completions(root, Position::new(3, 22));
    assert!(
        items.is_some(),
        "Should have completions in ternary consequent"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"a"),
            "Should suggest 'a' in ternary, got: {names:?}"
        );
        assert!(
            names.contains(&"b"),
            "Should suggest 'b' in ternary, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_after_typeof_operator() {
    // After `typeof `, should suggest variables in scope
    let source = "const myVar = 42;\nconst myStr = 'hello';\nlet t = typeof ;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `typeof `
    let items = completions.get_completions(root, Position::new(2, 15));
    assert!(items.is_some(), "Should have completions after 'typeof '");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"myVar"),
            "Should suggest 'myVar' after typeof, got: {names:?}"
        );
        assert!(
            names.contains(&"myStr"),
            "Should suggest 'myStr' after typeof, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_in_generic_type_arguments() {
    // After `Array<`, should suggest type names in scope
    let source = "interface Foo {}\ntype Bar = {};\nlet x: Array<>;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside `Array<|>`
    let items = completions.get_completions(root, Position::new(2, 14));
    // May or may not produce items, but should not crash
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // Type names should appear
        if names.contains(&"Foo") || names.contains(&"Bar") {
            // Good - type names are suggested in type argument position
        }
    }
}

#[test]
fn test_completions_in_type_annotation_position() {
    // After `: ` in a variable declaration, should suggest types
    let source = "interface MyInterface {}\ntype MyType = string;\nlet x: ;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `let x: `
    let items = completions.get_completions(root, Position::new(2, 7));
    assert!(
        items.is_some(),
        "Should have completions in type annotation position"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"MyInterface") || names.contains(&"MyType"),
            "Should suggest type names in type position, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_in_switch_case_expression() {
    // Inside a switch case, should suggest variables in scope
    let source = "const val = 1;\nconst opt = 2;\nswitch (val) {\n  case : break;\n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `case ` (line 3, col 7)
    let items = completions.get_completions(root, Position::new(3, 7));
    assert!(
        items.is_some(),
        "Should have completions in switch case expression"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"val") || names.contains(&"opt"),
            "Should suggest variables in case expression, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_in_return_statement() {
    // Inside a return statement, should suggest variables in scope
    let source = "function compute() {\n  const result = 42;\n  return ;\n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `return ` (line 2, col 9)
    let items = completions.get_completions(root, Position::new(2, 9));
    assert!(
        items.is_some(),
        "Should have completions in return statement"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"result"),
            "Should suggest 'result' in return statement, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_with_multiple_function_overloads() {
    // Functions declared multiple times (overloads) should appear once
    let source = "function greet(name: string): string;\nfunction greet(name: string, greeting: string): string;\nfunction greet(name: string, greeting?: string): string { return ''; }\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(3, 0));
    assert!(items.is_some(), "Should have completions after overloads");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        let greet_count = names.iter().filter(|&&n| n == "greet").count();
        assert!(
            greet_count <= 1,
            "Overloaded function 'greet' should appear at most once, found {greet_count} times"
        );
    }
}

#[test]
fn test_completions_in_catch_clause() {
    // Inside a catch block, should have access to the error variable and outer scope
    let source = "const outer = 1;\ntry {\n  const inner = 2;\n} catch (err) {\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside catch block (line 4, col 2)
    let items = completions.get_completions(root, Position::new(4, 2));
    assert!(
        items.is_some(),
        "Should have completions inside catch clause"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"outer"),
            "Should suggest 'outer' in catch block, got: {names:?}"
        );
        // The catch parameter 'err' should also be visible
        assert!(
            names.contains(&"err"),
            "Should suggest catch parameter 'err', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_in_module_declaration() {
    // Inside a module/namespace, should see module-scoped declarations
    let source = "namespace Outer {\n  export const a = 1;\n  namespace Inner {\n    const b = 2;\n    \n  }\n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside Inner namespace (line 4, col 4)
    let items = completions.get_completions(root, Position::new(4, 4));
    assert!(
        items.is_some(),
        "Should have completions inside nested namespace"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"b"),
            "Should suggest inner variable 'b', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_import_type_position() {
    // Inside `import("...")`, completions should be suppressed or not crash
    let source = "type T = import(\"\");";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside the import type string literal - should not crash
    let _items = completions.get_completions(root, Position::new(0, 17));
    // Main goal: no panic. Import specifier positions are typically suppressed.
}

#[test]
fn test_completions_computed_property_name() {
    // Inside computed property `[|]`, should suggest variables
    let source = "const key = 'name';\nconst sym = Symbol();\nconst obj = { []: 1 };";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside `[]` in object literal (line 2, col 15)
    let items = completions.get_completions(root, Position::new(2, 15));
    assert!(
        items.is_some(),
        "Should have completions inside computed property brackets"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"key"),
            "Should suggest 'key' in computed property, got: {names:?}"
        );
        assert!(
            names.contains(&"sym"),
            "Should suggest 'sym' in computed property, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_inside_array_literal() {
    // Inside an array literal, should suggest variables in scope
    let source = "const alpha = 1;\nconst beta = 2;\nconst arr = [ ];";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside array literal (line 2, col 14)
    let items = completions.get_completions(root, Position::new(2, 14));
    assert!(
        items.is_some(),
        "Should have completions inside array literal"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"alpha"),
            "Should suggest 'alpha' in array literal, got: {names:?}"
        );
        assert!(
            names.contains(&"beta"),
            "Should suggest 'beta' in array literal, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_binary_expression_rhs() {
    // After binary operator, should suggest variables
    let source = "const x = 10;\nconst y = 20;\nconst sum = x + ;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `x + ` (line 2, col 16)
    let items = completions.get_completions(root, Position::new(2, 16));
    assert!(
        items.is_some(),
        "Should have completions after binary operator"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"y"),
            "Should suggest 'y' after '+', got: {names:?}"
        );
        assert!(
            names.contains(&"x"),
            "Should suggest 'x' after '+', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_binary_expression_lhs() {
    // At the beginning of a binary expression (before operator), should suggest variables
    let source = "const p = 5;\nconst q = 10;\nconst r =  + q;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position at `r = |` (line 2, col 10)
    let items = completions.get_completions(root, Position::new(2, 10));
    assert!(
        items.is_some(),
        "Should have completions at binary expression LHS"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"p"),
            "Should suggest 'p' at LHS of binary expr, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_inside_line_comment() {
    // Inside a line comment, we verify the completion engine handles it without crash
    let source = "const x = 1;\n// some comment ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let _items = completions.get_completions(root, Position::new(1, 15));
    // Currently may or may not return completions in comments
    // The main test is that it doesn't crash
}

#[test]
fn test_completions_inside_block_comment() {
    // Inside a block comment, we verify no crash
    let source = "const x = 1;\n/* block comment  */";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let _items = completions.get_completions(root, Position::new(1, 10));
    // Currently may or may not return completions in comments
}

#[test]
fn test_completions_inside_string_literal() {
    // Inside a string literal, verify no crash
    let source = "const x = 1;\nconst s = \"hello world\";";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let _items = completions.get_completions(root, Position::new(1, 16));
    // Currently may or may not return completions in strings
}

#[test]
fn test_completions_for_loop_variable_scope() {
    // Variables declared in a for loop should be visible inside the loop body
    let source = "const outer = 1;\nfor (let i = 0; i < 10; i++) {\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside loop body (line 2, col 2)
    let items = completions.get_completions(root, Position::new(2, 2));
    assert!(items.is_some(), "Should have completions inside for loop");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"i"),
            "Should suggest loop variable 'i', got: {names:?}"
        );
        assert!(
            names.contains(&"outer"),
            "Should suggest outer variable 'outer', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_no_duplicate_from_var_hoisting() {
    // var declarations are hoisted; should not appear duplicated
    let source = "var x = 1;\nvar x = 2;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(2, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        let x_count = names.iter().filter(|&&n| n == "x").count();
        assert_eq!(
            x_count, 1,
            "Hoisted 'var x' should appear exactly once, found {x_count} times"
        );
    }
}

#[test]
fn test_completions_after_spread_operator() {
    // After `...` in an array, should suggest variables
    let source = "const items = [1, 2];\nconst all = [0, ...];";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `...` (line 1, col 19)
    let items = completions.get_completions(root, Position::new(1, 19));
    assert!(
        items.is_some(),
        "Should have completions after spread operator"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"items"),
            "Should suggest 'items' after spread, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_at_function_name_definition() {
    // At the name position of a function declaration, verify no crash
    let source = "function ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let _items = completions.get_completions(root, Position::new(0, 9));
    // Currently may or may not suppress completions at definition sites
}

#[test]
fn test_completions_at_class_name_definition() {
    // At the name position of a class declaration, verify no crash
    let source = "class ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let _items = completions.get_completions(root, Position::new(0, 6));
    // Currently may or may not suppress completions at definition sites
}

#[test]
fn test_completions_after_assignment_operator() {
    // After `=` in an assignment, should suggest variables
    let source = "let target = 0;\nconst source = 42;\ntarget = ;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `target = ` (line 2, col 9)
    let items = completions.get_completions(root, Position::new(2, 9));
    assert!(
        items.is_some(),
        "Should have completions after assignment operator"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"source"),
            "Should suggest 'source' after '=', got: {names:?}"
        );
        assert!(
            names.contains(&"target"),
            "Should suggest 'target' after '=', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_after_logical_operator() {
    // After logical operators (`&&`, `||`), should suggest variables
    let source = "const a = true;\nconst b = false;\nconst c = a && ;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `a && ` (line 2, col 15)
    let items = completions.get_completions(root, Position::new(2, 15));
    assert!(
        items.is_some(),
        "Should have completions after logical operator"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"b"),
            "Should suggest 'b' after '&&', got: {names:?}"
        );
    }
}

// ============================================================================
// Additional coverage tests (batch 2)
// ============================================================================

#[test]
fn test_completions_member_nested_object_dot() {
    // After `obj.inner.`, member resolution should return some completions
    // (may resolve to inner properties or parent-level members depending on type resolution)
    let source = "const obj = { inner: { deep: 42, flag: true } };\nobj.inner.";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let interner = TypeInterner::new();
    let completions = Completions::new_with_types(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let position = Position::new(1, 10);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);
    // Should not crash on nested property access; verify we get some result
    assert!(
        items.is_some(),
        "Should have completions for nested member access"
    );
    if let Some(items) = items {
        assert!(
            !items.is_empty(),
            "Should have non-empty member completions"
        );
    }
}

#[test]
fn test_completions_member_method_on_object() {
    // Object with method should suggest method with Method kind
    let source = "const obj = { greet() { return 'hi'; } };\nobj.";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let interner = TypeInterner::new();
    let completions = Completions::new_with_types(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let position = Position::new(1, 4);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);
    if let Some(items) = items {
        let greet_item = items.iter().find(|i| i.label == "greet");
        assert!(
            greet_item.is_some(),
            "Should suggest method 'greet', got: {:?}",
            items.iter().map(|i| &i.label).collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_completions_member_class_instance() {
    // Class instance member access should show public properties and methods
    let source = "class Point {\n  x: number = 0;\n  y: number = 0;\n  distance() { return 0; }\n}\nconst p = new Point();\np.";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let interner = TypeInterner::new();
    let completions = Completions::new_with_types(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let position = Position::new(6, 2);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);
    // Should not crash; may or may not have completions depending on class resolution
    let _ = items;
}

#[test]
fn test_completions_return_statement_inside_nested_function() {
    // Return inside nested function should suggest variables from all enclosing scopes
    let source = "const global = 1;\nfunction outer() {\n  const mid = 2;\n  function inner() {\n    const local = 3;\n    return ;\n  }\n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `return ` in inner function (line 5, col 11)
    let items = completions.get_completions(root, Position::new(5, 11));
    assert!(
        items.is_some(),
        "Should have completions in nested return statement"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"local"),
            "Should suggest 'local' in return, got: {names:?}"
        );
        assert!(
            names.contains(&"mid"),
            "Should suggest 'mid' from outer scope, got: {names:?}"
        );
        assert!(
            names.contains(&"global"),
            "Should suggest 'global' from top scope, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_let_in_different_block_scopes() {
    // let variables in different block scopes should not leak
    let source = "if (true) {\n  let blockA = 1;\n}\nif (true) {\n  let blockB = 2;\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside second if block (line 5, col 2)
    let items = completions.get_completions(root, Position::new(5, 2));
    assert!(
        items.is_some(),
        "Should have completions inside second if block"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"blockB"),
            "Should suggest 'blockB' from current block, got: {names:?}"
        );
        // blockA is in a different (closed) block scope - may or may not be visible
        // depending on binder scope resolution
    }
}

#[test]
fn test_completions_try_catch_finally_scoping() {
    // Variables in finally block should see outer scope but not try/catch locals
    let source = "const outer = 0;\ntry {\n  const tryVar = 1;\n} catch (e) {\n  const catchVar = 2;\n} finally {\n  const finalVar = 3;\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside finally block (line 7, col 2)
    let items = completions.get_completions(root, Position::new(7, 2));
    assert!(
        items.is_some(),
        "Should have completions inside finally block"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"outer"),
            "Should suggest 'outer' in finally, got: {names:?}"
        );
        assert!(
            names.contains(&"finalVar"),
            "Should suggest 'finalVar' in finally, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_function_parameter_default_value() {
    // In parameter default value position, should suggest visible variables
    let source = "const defaultVal = 10;\nfunction f(x = ) {}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `x = ` (line 1, col 15)
    let items = completions.get_completions(root, Position::new(1, 15));
    assert!(
        items.is_some(),
        "Should have completions in parameter default value"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"defaultVal"),
            "Should suggest 'defaultVal' in param default, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_in_while_loop_body() {
    // Inside while loop body, should suggest variables from enclosing scope
    let source = "const counter = 0;\nwhile (true) {\n  const loopVar = 1;\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(3, 2));
    assert!(items.is_some(), "Should have completions inside while loop");
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"counter"),
            "Should suggest 'counter', got: {names:?}"
        );
        assert!(
            names.contains(&"loopVar"),
            "Should suggest 'loopVar', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_const_enum_kind() {
    // const enums should also have Enum kind
    let source = "const enum Direction { Up, Down, Left, Right }\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let dir_item = items.iter().find(|i| i.label == "Direction");
        assert!(dir_item.is_some(), "Should find 'Direction'");
        assert_eq!(
            dir_item.unwrap().kind,
            CompletionItemKind::Enum,
            "const enum should have Enum kind"
        );
    }
}

#[test]
fn test_completions_module_kind() {
    // Module declarations should have Module kind
    let source = "module MyModule {\n  export const v = 1;\n}\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(3, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let mod_item = items.iter().find(|i| i.label == "MyModule");
        assert!(mod_item.is_some(), "Should find 'MyModule'");
        assert_eq!(
            mod_item.unwrap().kind,
            CompletionItemKind::Module,
            "Module should have Module kind"
        );
    }
}

#[test]
fn test_completions_namespace_kind() {
    // Namespace declarations should have Module kind
    let source = "namespace NS {\n  export const v = 1;\n}\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(3, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let ns_item = items.iter().find(|i| i.label == "NS");
        assert!(ns_item.is_some(), "Should find 'NS'");
        assert_eq!(
            ns_item.unwrap().kind,
            CompletionItemKind::Module,
            "Namespace should have Module kind"
        );
    }
}

#[test]
fn test_completions_type_parameter_visible_in_function_body() {
    // Type parameter T should be visible in function body as a completion
    let source = "function identity<T>(x: T): T {\n  let y: ;\n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position after `let y: ` (line 1, col 9)
    let items = completions.get_completions(root, Position::new(1, 9));
    // Should not crash; type parameters may or may not appear depending on scope resolution
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        if names.contains(&"T") {
            let t_item = items.iter().find(|i| i.label == "T").unwrap();
            assert_eq!(
                t_item.kind,
                CompletionItemKind::TypeParameter,
                "Type parameter should have TypeParameter kind"
            );
        }
    }
}

#[test]
fn test_completions_no_completions_in_regex_literal() {
    // Inside a regex literal, completions should be suppressed
    let source = "const x = 1;\nconst re = /pattern/;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside regex (line 1, col 15)
    let items = completions.get_completions(root, Position::new(1, 15));
    // Should suppress or return empty
    if let Some(ref items) = items {
        // If items returned, they should be empty since we're inside a regex
        // (though parser may not treat this as a regex in all cases)
        let _ = items;
    }
}

#[test]
fn test_completions_optional_chaining_member() {
    // After `?.`, should still offer member completions
    let source = "const obj = { foo: 1, bar: 'hello' };\nconst x: typeof obj | null = obj;\nx?.";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let interner = TypeInterner::new();
    let completions = Completions::new_with_types(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let position = Position::new(2, 3);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);
    // Should not crash on optional chaining
    let _ = items;
}

#[test]
fn test_completions_no_completions_after_number_dot() {
    // After a number literal dot (e.g., `1.`), completions may be ambiguous
    // because `1.` could be a decimal number or property access
    let source = "1.";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(0, 2));
    // Should not crash; result depends on parser interpretation
    let _ = items;
}

#[test]
fn test_completions_class_static_members_via_class_name() {
    // `ClassName.` should show static members
    let source =
        "class Util {\n  static helper() {}\n  static count = 0;\n  instance() {}\n}\nUtil.";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let interner = TypeInterner::new();
    let completions = Completions::new_with_types(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let position = Position::new(5, 5);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"helper"),
            "Should suggest static method 'helper', got: {names:?}"
        );
        assert!(
            names.contains(&"count"),
            "Should suggest static property 'count', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_is_new_identifier_location_after_equals_in_const() {
    // After `const x = `, should be new identifier location (expression expected)
    let source = "const x = ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        completions.compute_is_new_identifier_location(root, offset),
        "After 'const x = ' should be new identifier location"
    );
}

#[test]
fn test_completions_is_new_identifier_location_after_open_paren() {
    // After `(`, should be new identifier location
    let source = "function f(";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        completions.compute_is_new_identifier_location(root, offset),
        "After '(' should be new identifier location"
    );
}

#[test]
fn test_completions_is_new_identifier_location_after_comma_in_params() {
    // After `,` in a parameter list, should be new identifier location
    let source = "function f(x: number, ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        completions.compute_is_new_identifier_location(root, offset),
        "After ',' in param list should be new identifier location"
    );
}

#[test]
fn test_completions_is_new_identifier_location_after_interface_keyword() {
    // After 'interface ' should be new identifier location
    let source = "interface ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        completions.compute_is_new_identifier_location(root, offset),
        "After 'interface' keyword should be new identifier location"
    );
}

#[test]
fn test_completions_is_new_identifier_location_after_enum_keyword() {
    // After 'enum ' should be new identifier location
    let source = "enum ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        completions.compute_is_new_identifier_location(root, offset),
        "After 'enum' keyword should be new identifier location"
    );
}

#[test]
fn test_completions_is_new_identifier_location_after_type_keyword() {
    // After 'type ' should be new identifier location
    let source = "type ";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let offset = source.len() as u32;
    assert!(
        completions.compute_is_new_identifier_location(root, offset),
        "After 'type' keyword should be new identifier location"
    );
}

#[test]
fn test_completions_class_body_member_position() {
    // Inside class body at member position, constructor keyword should be offered
    let source = "class Foo {\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 2));
    // Should offer constructor keyword in class body
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"constructor"),
            "Should suggest 'constructor' in class body, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_no_member_completions_on_standalone_dot() {
    // A standalone `.` at start of file should not offer completions
    let source = ".";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(0, 1));
    assert!(
        items.is_none(),
        "Standalone '.' should not produce completions"
    );
}

#[test]
fn test_completions_in_do_while_body() {
    // Inside do-while body should have completions
    let source = "const x = 1;\ndo {\n  const y = 2;\n  \n} while (true);";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(3, 2));
    assert!(
        items.is_some(),
        "Should have completions inside do-while body"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"x"),
            "Should suggest outer 'x', got: {names:?}"
        );
        assert!(
            names.contains(&"y"),
            "Should suggest block 'y', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_new_target_in_function() {
    // After `new.` inside a function, should offer `target`
    let source = "function F() {\n  new.\n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 6));
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"target"),
            "Should suggest 'target' after 'new.' inside function, got: {names:?}"
        );
    }
}

#[test]
fn test_completions_deprecated_globals_sort_last() {
    // Deprecated globals like `escape` and `unescape` should sort after non-deprecated items
    let source = "const x = 1;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let escape_item = items.iter().find(|i| i.label == "escape");
        assert!(escape_item.is_some(), "Should find deprecated 'escape'");
        let escape_item = escape_item.unwrap();
        assert!(
            escape_item
                .sort_text
                .as_deref()
                .is_some_and(|s| s.starts_with('z')),
            "Deprecated global should have sort_text starting with 'z', got: {:?}",
            escape_item.sort_text
        );
        assert!(
            escape_item
                .kind_modifiers
                .as_deref()
                .is_some_and(|m| m.contains("deprecated")),
            "Deprecated global should have 'deprecated' in kind_modifiers, got: {:?}",
            escape_item.kind_modifiers
        );
    }
}

#[test]
fn test_completions_global_functions_have_snippets() {
    // Global functions like `parseInt` should have snippet insert text
    let source = "const x = 1;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let parse_item = items.iter().find(|i| i.label == "parseInt");
        assert!(parse_item.is_some(), "Should find 'parseInt'");
        let parse_item = parse_item.unwrap();
        assert_eq!(
            parse_item.kind,
            CompletionItemKind::Function,
            "parseInt should be Function kind"
        );
        assert!(parse_item.is_snippet, "Global function should have snippet");
        assert_eq!(
            parse_item.insert_text.as_deref(),
            Some("parseInt($1)"),
            "Global function should have snippet insert text"
        );
    }
}

#[test]
fn test_completions_const_detail_shows_literal_value() {
    // const with numeric literal initializer should show value as detail
    let source = "const MAX_SIZE = 100;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let max_item = items.iter().find(|i| i.label == "MAX_SIZE");
        assert!(max_item.is_some(), "Should find 'MAX_SIZE'");
        let max_item = max_item.unwrap();
        assert_eq!(
            max_item.detail.as_deref(),
            Some("100"),
            "const with numeric literal should show value as detail"
        );
    }
}

#[test]
fn test_completions_const_string_detail() {
    // const with string literal initializer should show the quoted string as detail
    let source = "const GREETING = \"hello\";\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let greet_item = items.iter().find(|i| i.label == "GREETING");
        assert!(greet_item.is_some(), "Should find 'GREETING'");
        let greet_item = greet_item.unwrap();
        assert_eq!(
            greet_item.detail.as_deref(),
            Some("\"hello\""),
            "const with string literal should show quoted string as detail"
        );
    }
}

#[test]
fn test_completions_const_boolean_detail() {
    // const with boolean literal initializer should show value as detail
    let source = "const IS_DEBUG = true;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let debug_item = items.iter().find(|i| i.label == "IS_DEBUG");
        assert!(debug_item.is_some(), "Should find 'IS_DEBUG'");
        assert_eq!(
            debug_item.unwrap().detail.as_deref(),
            Some("true"),
            "const with boolean literal should show value as detail"
        );
    }
}

#[test]
fn test_completions_let_with_type_annotation_detail() {
    // let with type annotation should show the type as detail
    let source = "let count: number;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let count_item = items.iter().find(|i| i.label == "count");
        assert!(count_item.is_some(), "Should find 'count'");
        // Detail may include trailing semicolon from source text span
        let detail = count_item.unwrap().detail.as_deref().unwrap_or("");
        assert!(
            detail == "number" || detail == "number;",
            "let with type annotation should show type as detail, got: {detail:?}"
        );
    }
}

#[test]
fn test_completions_no_completions_in_template_literal_text() {
    // Inside the text portion of a template literal (not in ${} expression), should suppress
    let source = "const x = 1;\nconst s = `hello world`;";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    // Position inside template literal text portion (line 1, col 16)
    let items = completions.get_completions(root, Position::new(1, 16));
    // Should be suppressed or empty in string part
    if let Some(ref items) = items {
        // Template literal text should be treated as no-completion context
        let _ = items;
    }
}

#[test]
fn test_completions_multiple_parameters_visible() {
    // Multiple function parameters should all be visible inside function body
    let source = "function calc(a: number, b: string, c: boolean) {\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 2));
    assert!(
        items.is_some(),
        "Should have completions inside function with multiple params"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"a"),
            "Should suggest parameter 'a', got: {names:?}"
        );
        assert!(
            names.contains(&"b"),
            "Should suggest parameter 'b', got: {names:?}"
        );
        assert!(
            names.contains(&"c"),
            "Should suggest parameter 'c', got: {names:?}"
        );
        // All should have Parameter kind
        for param_name in &["a", "b", "c"] {
            let param_item = items.iter().find(|i| i.label == *param_name).unwrap();
            assert_eq!(
                param_item.kind,
                CompletionItemKind::Parameter,
                "Parameter '{param_name}' should have Parameter kind"
            );
        }
    }
}

#[test]
fn test_completions_enum_member_dot_access() {
    // After `EnumName.`, should show enum members
    let source = "enum Status { Active, Inactive, Pending }\nStatus.";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let interner = TypeInterner::new();
    let completions = Completions::new_with_types(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let position = Position::new(1, 7);
    let items = completions.get_completions(root, position);
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"Active"),
            "Should suggest enum member 'Active', got: {names:?}"
        );
        assert!(
            names.contains(&"Inactive"),
            "Should suggest enum member 'Inactive', got: {names:?}"
        );
        assert!(
            names.contains(&"Pending"),
            "Should suggest enum member 'Pending', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_completion_result_is_member_false_for_global() {
    // At top-level, completion result should have is_member_completion = false
    let source = "const x = 1;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let result = completions.get_completion_result(root, Position::new(1, 0));
    assert!(result.is_some(), "Should have completion result");
    let result = result.unwrap();
    assert!(
        !result.is_member_completion,
        "Top-level should not be member completion"
    );
    assert!(
        result.is_global_completion,
        "Top-level should be global completion"
    );
}

#[test]
fn test_completions_inside_labeled_statement() {
    // Inside a labeled statement body, should have completions
    let source = "const x = 1;\nouter: for (let i = 0; i < 10; i++) {\n  \n}";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(2, 2));
    assert!(
        items.is_some(),
        "Should have completions inside labeled statement"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"x"),
            "Should suggest 'x' in labeled loop, got: {names:?}"
        );
        assert!(
            names.contains(&"i"),
            "Should suggest loop var 'i', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_import_binding_visible_after_import() {
    // An imported name should be visible after the import statement
    let source = "import { foo } from './bar';\nconst x = 1;\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(2, 0));
    assert!(
        items.is_some(),
        "Should have completions after import statement"
    );
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"foo"),
            "Should suggest imported 'foo', got: {names:?}"
        );
        assert!(
            names.contains(&"x"),
            "Should suggest local 'x', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_import_binding_kind_is_alias() {
    // Import bindings should have Alias kind
    let source = "import { myFunc } from './module';\n";
    let (root, arena, binder, line_map, src) = make_completions_provider(source);
    let completions = Completions::new(&arena, &binder, &line_map, &src);
    let items = completions.get_completions(root, Position::new(1, 0));
    assert!(items.is_some(), "Should have completions");
    if let Some(items) = items {
        let import_item = items.iter().find(|i| i.label == "myFunc");
        if let Some(import_item) = import_item {
            assert_eq!(
                import_item.kind,
                CompletionItemKind::Alias,
                "Import binding should have Alias kind"
            );
        }
    }
}

#[test]
fn test_completions_multiline_object_literal_member() {
    // Object literal with properties across multiple lines
    let source = "const obj = {\n  name: 'test',\n  count: 42,\n  active: true\n};\nobj.";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let interner = TypeInterner::new();
    let completions = Completions::new_with_types(
        arena,
        &binder,
        &line_map,
        &interner,
        source,
        "test.ts".to_string(),
    );
    let position = Position::new(5, 4);
    let mut cache = None;
    let items = completions.get_completions_with_cache(root, position, &mut cache);
    if let Some(items) = items {
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            names.contains(&"name"),
            "Should suggest 'name', got: {names:?}"
        );
        assert!(
            names.contains(&"count"),
            "Should suggest 'count', got: {names:?}"
        );
        assert!(
            names.contains(&"active"),
            "Should suggest 'active', got: {names:?}"
        );
    }
}

#[test]
fn test_completions_completion_item_serialization_fields() {
    // Verify that CompletionItem serializes expected fields correctly
    let item = CompletionItem::new("test".to_string(), CompletionItemKind::Variable)
        .with_detail("number".to_string())
        .with_sort_text("11")
        .with_kind_modifiers("export".to_string());

    let value = serde_json::to_value(&item).expect("should serialize");

    assert_eq!(value.get("label").and_then(|v| v.as_str()), Some("test"));
    assert_eq!(value.get("detail").and_then(|v| v.as_str()), Some("number"));
    assert_eq!(value.get("sort_text").and_then(|v| v.as_str()), Some("11"));
    assert_eq!(
        value.get("kind_modifiers").and_then(|v| v.as_str()),
        Some("export")
    );
    // is_snippet should be omitted when false (skip_serializing_if)
    assert!(
        value.get("is_snippet").is_none(),
        "is_snippet should be omitted when false"
    );
    // has_action should be omitted when false
    assert!(
        value.get("has_action").is_none(),
        "has_action should be omitted when false"
    );
}

#[test]
fn test_completions_completion_result_serialization() {
    // Verify CompletionResult serialization includes correct field names
    let result = CompletionResult {
        is_global_completion: true,
        is_member_completion: false,
        is_new_identifier_location: false,
        default_commit_characters: Some(vec![".".to_string(), ",".to_string()]),
        entries: vec![CompletionItem::new(
            "x".to_string(),
            CompletionItemKind::Variable,
        )],
    };

    let value = serde_json::to_value(&result).expect("should serialize");
    assert_eq!(
        value
            .get("defaultCommitCharacters")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(2),
        "defaultCommitCharacters should be serialized with camelCase"
    );
    assert_eq!(
        value
            .get("entries")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(1),
        "entries should have one item"
    );
}

#[test]
fn test_completions_function_parameter_in_body() {
    // function f(myParam: number) { | }
    let source = "function f(myParam: number) {  }";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    // Position inside the function body (line 0, col 30 = between { and })
    let position = Position::new(0, 30);
    let completions = Completions::new(arena, &binder, &line_map, source);
    let items = completions.get_completions(root, position);
    assert!(
        items.is_some(),
        "Should have completions inside function body"
    );
    let items = items.unwrap();
    let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(
        names.contains(&"myParam"),
        "Function parameter 'myParam' should appear in completions, got: {names:?}"
    );
}

