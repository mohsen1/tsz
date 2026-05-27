#[test]
fn test_selection_range_arrow_with_expression_body() {
    let source = "const double = (x: number) => x * 2;";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'x' in expression body (column 30)
    let pos = Position::new(0, 30);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in arrow function expression body"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection for arrow body, got {depth}"
    );
}

#[test]
fn test_selection_range_multiline_object_literal() {
    let source = "const config = {\n  host: 'localhost',\n  port: 8080,\n  debug: true\n};";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'port' (line 2, column 2)
    let pos = Position::new(2, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for property in multiline object"
    );

    // Should eventually expand to include the whole object
    let mut current = result.as_ref();
    let mut found_object = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 4 {
            found_object = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(
        found_object,
        "Selection should expand to include entire object literal"
    );
}

#[test]
fn test_selection_range_tuple_type() {
    let source = "let pair: [string, number] = ['a', 1];";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'number' in tuple type (column 19)
    let pos = Position::new(0, 19);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for tuple type element"
    );
}

#[test]
fn test_selection_range_satisfies_expression() {
    let source = "const x = { a: 1 } satisfies Record<string, number>;";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'Record' (column 30)
    let pos = Position::new(0, 30);
    let result = provider.get_selection_range(pos);

    // Should not panic; parser may or may not support `satisfies`
    let _ = result;
}

// =========================================================================
// Additional selection range tests to reach 80+ (batch 3)
// =========================================================================

#[test]
fn test_selection_range_enum_member_expand() {
    let source = "enum Color {\n  Red,\n  Green,\n  Blue\n}";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'Green' (line 2, column 2)
    let pos = Position::new(2, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for enum member"
    );

    let mut current = result.as_ref();
    let mut found_enum = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 4 {
            found_enum = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(found_enum, "Selection should expand to include entire enum");
}

#[test]
fn test_selection_range_namespace_body() {
    let source = "namespace App {\n  export const x = 1;\n}";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'x' (line 1, column 16)
    let pos = Position::new(1, 16);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range inside namespace"
    );

    let mut current = result.as_ref();
    let mut found_ns = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 2 {
            found_ns = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(found_ns, "Selection should expand to include namespace");
}

#[test]
fn test_selection_range_interface_body() {
    let source = "interface Config {\n  host: string;\n  port: number;\n}";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'port' (line 2, column 2)
    let pos = Position::new(2, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for interface property"
    );

    let mut current = result.as_ref();
    let mut found_iface = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 3 {
            found_iface = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(found_iface, "Selection should expand to include interface");
}

#[test]
fn test_selection_range_class_static_method() {
    let source = "class Util {\n  static create() {\n    return new Util();\n  }\n}";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'Util' in new expression (line 2, column 15)
    let pos = Position::new(2, 15);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for static method body"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 4,
        "Should have deep nesting inside class static method, got {depth}"
    );
}

#[test]
fn test_selection_range_template_literal_expression() {
    let source = "const name = 'world';\nconst msg = `hello ${name}`;";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position inside the template literal (line 1, column 20)
    let pos = Position::new(1, 20);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in template literal"
    );
}

#[test]
fn test_selection_range_class_extends() {
    let source = "class Base {}\nclass Child extends Base {\n  method() {}\n}";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'method' (line 2, column 2)
    let pos = Position::new(2, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in child class"
    );

    let mut current = result.as_ref();
    let mut found_child = false;
    while let Some(sel) = current {
        if sel.range.start.line == 1 && sel.range.end.line == 3 {
            found_child = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(
        found_child,
        "Selection should expand to include Child class"
    );
}

#[test]
fn test_selection_range_deeply_nested_blocks() {
    let source = "function f() {\n  if (a) {\n    if (b) {\n      if (c) {\n        doIt();\n      }\n    }\n  }\n}";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'doIt' (line 4, column 8)
    let pos = Position::new(4, 8);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in deeply nested blocks"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 5,
        "Should have deep nesting for nested if blocks, got {depth}"
    );
}

#[test]
fn test_selection_range_multiline_interface() {
    let source = "interface API {\n  get(url: string): void;\n  post(url: string, body: any): void;\n  put(url: string, body: any): void;\n  del(url: string): void;\n}";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'post' (line 2, column 2)
    let pos = Position::new(2, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for interface method"
    );

    let mut current = result.as_ref();
    let mut found_iface = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 5 {
            found_iface = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(
        found_iface,
        "Selection should expand to include entire interface"
    );
}

#[test]
fn test_selection_range_switch_case_body() {
    let source = "switch (x) {\n  case 1:\n    console.log('one');\n    break;\n  default:\n    console.log('other');\n}";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'one' in case body (line 2, column 17)
    let pos = Position::new(2, 17);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection range in case body");

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 3,
        "Should have nested selection in switch case, got {depth}"
    );
}

#[test]
fn test_selection_range_class_with_multiple_members() {
    let source = "class Foo {\n  a: number;\n  b: string;\n  c(): void {}\n  d(): boolean { return true; }\n}";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'c' method (line 3, column 2)
    let pos = Position::new(3, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for class method"
    );

    let mut current = result.as_ref();
    let mut found_class = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 5 {
            found_class = true;
            break;
        }
        current = sel.parent.as_deref();
    }

    assert!(
        found_class,
        "Selection should expand to include entire class"
    );
}

#[test]
fn test_selection_range_object_method_shorthand() {
    let source = "const obj = {\n  greet() {\n    return 'hi';\n  }\n};";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'hi' (line 2, column 12)
    let pos = Position::new(2, 12);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in object method"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 3,
        "Should have nested selection in object method, got {depth}"
    );
}

#[test]
fn test_selection_range_arrow_with_block_body() {
    let source = "const fn = (x: number) => {\n  const y = x * 2;\n  return y;\n};";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'y' in return statement (line 2, column 9)
    let pos = Position::new(2, 9);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in arrow block body"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 3,
        "Should have nested selection in arrow block body, got {depth}"
    );
}

#[test]
fn test_selection_range_boolean_expression() {
    let source = "const result = a && b || c && d;";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'b' (column 20)
    let pos = Position::new(0, 20);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in boolean expression"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection in boolean expression, got {depth}"
    );
}

#[test]
fn test_selection_range_type_assertion_angle_bracket() {
    let source = "const x = <string>'hello';";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'hello' (column 19)
    let pos = Position::new(0, 19);
    let result = provider.get_selection_range(pos);

    // Should not panic
    let _ = result;
}

#[test]
fn test_selection_range_object_destructuring() {
    let source = "const { a, b, c } = obj;";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'b' (column 11)
    let pos = Position::new(0, 11);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range in object destructuring"
    );

    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }

    assert!(
        depth >= 2,
        "Should have nested selection for destructuring, got {depth}"
    );
}

#[test]
fn test_selection_range_as_expression() {
    let source = "const x = someValue as string;";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);

    // Position at 'someValue' (column 10)
    let pos = Position::new(0, 10);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for as expression"
    );
}

// =========================================================================
// Additional selection range tests (batch 4 — edge cases)
// =========================================================================

#[test]
fn test_selection_range_single_identifier_file() {
    let source = "x";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(0, 0);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range for single identifier"
    );
}

#[test]
fn test_selection_range_whitespace_before_code() {
    let source = "  \n  \nconst x = 1;";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(2, 6);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection range after leading whitespace"
    );
}

#[test]
fn test_selection_range_unicode_identifier() {
    let source = "const \u{00e4}\u{00f6}\u{00fc} = 42;";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(0, 6);
    let result = provider.get_selection_range(pos);

    let _ = result;
}

#[test]
fn test_selection_range_nested_arrow_functions() {
    let source = "const f = (x: number) => (y: number) => x + y;";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'y' in x + y (column 45)
    let pos = Position::new(0, 45);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection in nested arrow");
    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }
    assert!(
        depth >= 3,
        "Nested arrows should produce deep chain, got {depth}"
    );
}

#[test]
fn test_selection_range_async_arrow_function() {
    let source = "const f = async () => { return 1; };";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(0, 31);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection in async arrow body"
    );
}

#[test]
fn test_selection_range_generator_function() {
    let source = "function* gen() {\n  yield 1;\n  yield 2;\n}";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'yield' (line 1, column 2)
    let pos = Position::new(1, 2);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection in generator body");
}

#[test]
fn test_selection_range_class_private_field() {
    let source = "class Foo {\n  #bar = 42;\n}";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(1, 2);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection for private field");
}

#[test]
fn test_selection_range_class_accessor() {
    let source = "class Foo {\n  get val() { return 1; }\n  set val(v: number) {}\n}";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'val' in getter (line 1, column 6)
    let pos = Position::new(1, 6);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection for getter");
}

#[test]
fn test_selection_range_const_enum() {
    let source = "const enum Dir {\n  Up,\n  Down,\n  Left,\n  Right\n}";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(2, 2);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection for const enum member"
    );
    let mut current = result.as_ref();
    let mut found_enum = false;
    while let Some(sel) = current {
        if sel.range.start.line == 0 && sel.range.end.line == 5 {
            found_enum = true;
            break;
        }
        current = sel.parent.as_deref();
    }
    assert!(found_enum, "Selection should expand to entire const enum");
}

#[test]
fn test_selection_range_index_type_query() {
    let source = "type Keys = keyof { a: 1; b: 2 };";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(0, 12);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection for keyof expression"
    );
}

#[test]
fn test_selection_range_infer_type() {
    let source = "type Ret<T> = T extends (...args: any) => infer R ? R : never;";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'infer' (column 42)
    let pos = Position::new(0, 42);
    let result = provider.get_selection_range(pos);

    let _ = result;
}

#[test]
fn test_selection_range_nested_ternary_deep() {
    let source = "const x = a ? b : c ? d : e ? f : g;";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'f' (column 34 area)
    let pos = Position::new(0, 34);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection in deeply nested ternary"
    );
    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }
    assert!(depth >= 2, "Should have nested selections, got {depth}");
}

#[test]
fn test_selection_range_rest_parameter() {
    let source = "function sum(...nums: number[]) { return 0; }";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'nums' (column 16)
    let pos = Position::new(0, 16);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection for rest parameter");
}

#[test]
fn test_selection_range_optional_parameter() {
    let source = "function greet(name?: string) {}";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(0, 15);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection for optional parameter"
    );
}

#[test]
fn test_selection_range_abstract_class_method() {
    let source = "abstract class Shape {\n  abstract area(): number;\n}";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(1, 11);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection for abstract method"
    );
}

#[test]
fn test_selection_range_multiline_chain() {
    let source = "promise\n  .then(x => x)\n  .catch(e => e)\n  .finally(() => {});";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'catch' (line 2, column 3)
    let pos = Position::new(2, 3);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection in chained call");
}

#[test]
fn test_selection_range_array_destructuring_with_rest() {
    let source = "const [first, ...rest] = [1, 2, 3];";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'rest' (column 17)
    let pos = Position::new(0, 17);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection for rest in array destructuring"
    );
}

#[test]
fn test_selection_range_nested_object_type() {
    let source = "type Config = {\n  db: {\n    host: string;\n    port: number;\n  };\n};";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'host' (line 2, column 4)
    let pos = Position::new(2, 4);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection in nested object type"
    );
    let mut depth = 0;
    let mut current = result.as_ref();
    while let Some(sel) = current {
        depth += 1;
        current = sel.parent.as_deref();
    }
    assert!(
        depth >= 3,
        "Should have deep nesting for nested type, got {depth}"
    );
}

#[test]
fn test_selection_range_throw_statement() {
    let source = "function fail() {\n  throw new Error('oops');\n}";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'Error' (line 1, column 12)
    let pos = Position::new(1, 12);
    let result = provider.get_selection_range(pos);

    assert!(result.is_some(), "Should find selection in throw statement");
}

#[test]
fn test_selection_range_regex_literal() {
    let source = "const pattern = /hello\\s+world/gi;";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    let pos = Position::new(0, 20);
    let result = provider.get_selection_range(pos);

    let _ = result;
}

#[test]
fn test_selection_range_multiple_variable_declarations() {
    let source = "let a = 1, b = 2, c = 3;";
    let (parser, _root) = parse_test_source(source);
    let arena = parser.get_arena();
    let line_map = LineMap::build(source);

    let provider = SelectionRangeProvider::new(arena, &line_map, source);
    // Position at 'b' (column 11)
    let pos = Position::new(0, 11);
    let result = provider.get_selection_range(pos);

    assert!(
        result.is_some(),
        "Should find selection for middle variable declaration"
    );
}
