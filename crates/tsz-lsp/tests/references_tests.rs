use super::*;
use tsz_binder::BinderState;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;

#[test]
fn test_find_references_simple() {
    // const x = 1;
    // x + x;
    let source = "const x = 1;\nx + x;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the first 'x' in "x + x" (line 1, column 0)
    let position = Position::new(1, 0);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(references.is_some(), "Should find references for x");

    if let Some(refs) = references {
        // Should find at least the declaration and two usages
        assert!(
            refs.len() >= 2,
            "Should find at least 2 references (declaration + usages)"
        );
    }
}

#[test]
fn test_find_references_for_symbol() {
    let source = "const x = 1;\nx + x;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let symbol_id = binder.file_locals.get("x").expect("Expected symbol for x");

    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references_for_symbol(root, symbol_id);

    assert!(references.is_some(), "Should find references for x");
    if let Some(refs) = references {
        assert!(
            refs.len() >= 2,
            "Should find at least 2 references (declaration + usages)"
        );
    }
}

#[test]
fn test_find_references_not_found() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position outside any identifier
    let position = Position::new(0, 11); // At the semicolon

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    // Should not find references
    assert!(
        references.is_none(),
        "Should not find references at semicolon"
    );
}

#[test]
fn test_find_references_template_expression() {
    let source = "const name = \"Ada\";\nconst msg = `hi ${name}`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'name' inside the template expression (line 1)
    let position = Position::new(1, 18);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references in template expression"
    );
    let refs = references.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find declaration and template usage"
    );
}

#[test]
fn test_find_references_jsx_expression() {
    let source = "const name = \"Ada\";\nconst el = <div>{name}</div>;";
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'name' inside JSX expression (line 1)
    let position = Position::new(1, 17);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.tsx".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references in JSX expression"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and JSX usage");
}

#[test]
fn test_find_references_await_expression() {
    let source = "const value = 1;\nasync function run() {\n  await value;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' inside await (line 2)
    let position = Position::new(2, 8);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references in await expression"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and await usage");
}

#[test]
fn test_find_references_tagged_template_expression() {
    let source =
        "const tag = (strings: TemplateStringsArray) => strings[0];\nconst msg = tag`hello`;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'tag' inside tagged template (line 1)
    let position = Position::new(1, 16);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references in tagged template"
    );
    let refs = references.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find declaration and tagged template usage"
    );
}

#[test]
fn test_find_references_as_expression() {
    let source = "const value = 1;\nconst result = value as number;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' inside the as-expression (line 1)
    let position = Position::new(1, 15);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references in as expression"
    );
    let refs = references.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find declaration and as-expression usage"
    );
}

#[test]
fn test_find_references_binding_pattern() {
    let source = "const { foo } = obj;\nfoo;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'foo' usage (line 1)
    let position = Position::new(1, 0);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for binding pattern name"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_binding_pattern_initializer() {
    let source = "const value = 1;\nconst { foo = value } = obj;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' inside the initializer (line 1)
    let position = Position::new(1, 14);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references in binding pattern initializer"
    );
    let refs = references.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find declaration and initializer usage"
    );
}

#[test]
fn test_find_references_parameter_binding_pattern() {
    let source = "function demo({ foo }: { foo: number }) {\n  return foo;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'foo' usage in the return (line 1)
    let position = Position::new(1, 9);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for parameter binding name"
    );
    let refs = references.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find parameter declaration and usage"
    );
}

#[test]
fn test_find_references_parameter_array_binding() {
    let source = "function demo([foo]: number[]) {\n  return foo;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'foo' usage in the return (line 1)
    let position = Position::new(1, 9);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for array binding name"
    );
    let refs = references.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find parameter declaration and usage"
    );
}

#[test]
fn test_find_references_nested_arrow_in_switch_case() {
    let source = "switch (state) {\n  case (() => {\n    const value = 1;\n    return value;\n  })():\n    break;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage (line 3)
    let position = Position::new(3, 11);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for switch case locals"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_nested_arrow_in_if_condition() {
    let source = "if ((() => {\n  const value = 1;\n  return value;\n})()) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage (line 2)
    let position = Position::new(2, 9);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for nested arrow locals in condition"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_export_default_expression() {
    let source = "export default (() => {\n  const value = 1;\n  return value;\n})();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage (line 2)
    let position = Position::new(2, 9);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for export default expression locals"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_labeled_statement_local() {
    let source = "label: {\n  const value = 1;\n  value;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage (line 2)
    let position = Position::new(2, 2);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for labeled statement locals"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_with_statement_local() {
    let source = "with (obj) {\n  const value = 1;\n  value;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage (line 2)
    let position = Position::new(2, 2);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for with statement locals"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_var_hoisted_in_nested_block() {
    let source = "function demo() {\n  value;\n  if (cond) {\n    var value = 1;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage before the declaration (line 1)
    let position = Position::new(1, 2);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for hoisted var"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_decorator_reference() {
    let source = "const deco = () => {};\n@deco\nclass Foo {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'deco' usage in the decorator (line 1)
    let position = Position::new(1, 1);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for decorator usage"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_class_method_local() {
    let source = "class Foo {\n  method() {\n    const value = 1;\n    return value;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage (line 3)
    let position = Position::new(3, 11);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for method local"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_class_self_reference() {
    let source = "class Foo {\n  method() {\n    return Foo;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'Foo' usage inside the method (line 2)
    let position = Position::new(2, 11);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for class self name"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_class_expression_name() {
    let source = "const Foo = class Bar {\n  method() {\n    return Bar;\n  }\n};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'Bar' usage inside the method (line 2)
    let position = Position::new(2, 11);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for class expression name"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

#[test]
fn test_find_references_class_static_block_local() {
    let source = "class Foo {\n  static {\n    const value = 1;\n    value;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    // Position at the 'value' usage inside the static block (line 3)
    let position = Position::new(3, 4);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let references = find_refs.find_references(root, position);

    assert!(
        references.is_some(),
        "Should find references for static block locals"
    );
    let refs = references.unwrap();
    assert!(refs.len() >= 2, "Should find declaration and usage");
}

// =========================================================================
// Tests for ReferenceInfo: isWriteAccess, isDefinition, lineText
// =========================================================================

/// Helper to get detailed references for a symbol at a given position.
fn get_detailed_refs(source: &str, file_name: &str, line: u32, col: u32) -> Vec<ReferenceInfo> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let position = Position::new(line, col);

    let find_refs = FindReferences::new(arena, &binder, &line_map, file_name.to_string(), source);
    find_refs
        .find_references_detailed(root, position)
        .unwrap_or_default()
}

#[test]
fn test_detailed_refs_const_declaration_is_write_and_definition() {
    // `const x = 1; x + x;`
    // The declaration of x should be isWriteAccess=true, isDefinition=true
    // The usages of x should be isWriteAccess=false, isDefinition=false
    let source = "const x = 1;\nx + x;";
    let refs = get_detailed_refs(source, "test.ts", 1, 0);

    assert!(
        refs.len() >= 2,
        "Should find at least 2 references, got {}",
        refs.len()
    );

    // Find the declaration ref (on line 0, which is "const x = 1;")
    let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
    assert!(
        decl_ref.is_some(),
        "Should have a ref on line 0 (declaration)"
    );
    let decl_ref = decl_ref.unwrap();
    assert!(
        decl_ref.is_write_access,
        "Declaration should be a write access"
    );
    assert!(decl_ref.is_definition, "Declaration should be a definition");

    // Find a usage ref (on line 1, which is "x + x;")
    let usage_refs: Vec<_> = refs
        .iter()
        .filter(|r| r.location.range.start.line == 1)
        .collect();
    assert!(
        !usage_refs.is_empty(),
        "Should have at least one usage ref on line 1"
    );
    for ur in &usage_refs {
        assert!(
            !ur.is_write_access,
            "Read-only usage should not be a write access"
        );
        assert!(!ur.is_definition, "Usage should not be a definition");
    }
}

#[test]
fn test_detailed_refs_assignment_is_write_access() {
    // `let x = 1; x = 2;`
    // The assignment `x = 2` should be isWriteAccess=true, isDefinition=false
    let source = "let x = 1;\nx = 2;";
    let refs = get_detailed_refs(source, "test.ts", 0, 4);

    assert!(
        refs.len() >= 2,
        "Should find at least 2 references, got {}",
        refs.len()
    );

    // The ref on line 1 ("x = 2;") is an assignment - should be write
    let assign_ref = refs.iter().find(|r| r.location.range.start.line == 1);
    assert!(
        assign_ref.is_some(),
        "Should have a ref on line 1 (assignment)"
    );
    let assign_ref = assign_ref.unwrap();
    assert!(
        assign_ref.is_write_access,
        "Assignment target should be a write access"
    );
    assert!(!assign_ref.is_definition, "Assignment is not a definition");
}

#[test]
fn test_detailed_refs_compound_assignment_is_write_access() {
    // `let x = 0; x += 1;`
    // The compound assignment `x += 1` should be isWriteAccess=true
    let source = "let x = 0;\nx += 1;";
    let refs = get_detailed_refs(source, "test.ts", 0, 4);

    assert!(
        refs.len() >= 2,
        "Should find at least 2 references, got {}",
        refs.len()
    );

    let compound_ref = refs.iter().find(|r| r.location.range.start.line == 1);
    assert!(
        compound_ref.is_some(),
        "Should have a ref on line 1 (compound assignment)"
    );
    let compound_ref = compound_ref.unwrap();
    assert!(
        compound_ref.is_write_access,
        "Compound assignment target should be a write access"
    );
    assert!(
        !compound_ref.is_definition,
        "Compound assignment is not a definition"
    );
}

#[test]
fn test_detailed_refs_function_declaration_is_definition() {
    // `function foo() {} foo();`
    // The function name at declaration is isDefinition=true, isWriteAccess=true
    // The call site is isDefinition=false, isWriteAccess=false
    let source = "function foo() {}\nfoo();";
    let refs = get_detailed_refs(source, "test.ts", 1, 0);

    assert!(
        refs.len() >= 2,
        "Should find at least 2 references, got {}",
        refs.len()
    );

    // The declaration on line 0
    let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
    assert!(
        decl_ref.is_some(),
        "Should have a ref on line 0 (declaration)"
    );
    let decl_ref = decl_ref.unwrap();
    assert!(
        decl_ref.is_definition,
        "Function declaration name should be a definition"
    );
    assert!(
        decl_ref.is_write_access,
        "Function declaration name should be a write access"
    );

    // The call on line 1
    let call_ref = refs.iter().find(|r| r.location.range.start.line == 1);
    assert!(call_ref.is_some(), "Should have a ref on line 1 (call)");
    let call_ref = call_ref.unwrap();
    assert!(
        !call_ref.is_definition,
        "Function call should not be a definition"
    );
    assert!(
        !call_ref.is_write_access,
        "Function call should not be a write access"
    );
}

#[test]
fn test_detailed_refs_class_declaration_is_definition() {
    // `class Foo {} new Foo();`
    let source = "class Foo {}\nnew Foo();";
    let refs = get_detailed_refs(source, "test.ts", 0, 6);

    assert!(
        refs.len() >= 2,
        "Should find at least 2 references, got {}",
        refs.len()
    );

    let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
    assert!(decl_ref.is_some(), "Should have declaration ref");
    let decl_ref = decl_ref.unwrap();
    assert!(
        decl_ref.is_definition,
        "Class declaration should be a definition"
    );
    assert!(
        decl_ref.is_write_access,
        "Class declaration should be a write access"
    );

    let usage_ref = refs.iter().find(|r| r.location.range.start.line == 1);
    assert!(usage_ref.is_some(), "Should have usage ref");
    let usage_ref = usage_ref.unwrap();
    assert!(
        !usage_ref.is_definition,
        "new Foo() should not be a definition"
    );
    assert!(
        !usage_ref.is_write_access,
        "new Foo() should not be a write access"
    );
}

#[test]
fn test_detailed_refs_parameter_is_write_and_definition() {
    // `function foo(x: number) { return x; }`
    // Parameter x declaration is isWriteAccess=true, isDefinition=true
    // Usage of x in body is isWriteAccess=false, isDefinition=false
    let source = "function foo(x: number) {\n  return x;\n}";
    let refs = get_detailed_refs(source, "test.ts", 1, 9);

    assert!(
        refs.len() >= 2,
        "Should find at least 2 references, got {}",
        refs.len()
    );

    // The parameter declaration (line 0)
    let param_ref = refs.iter().find(|r| r.location.range.start.line == 0);
    assert!(param_ref.is_some(), "Should have param ref on line 0");
    let param_ref = param_ref.unwrap();
    assert!(
        param_ref.is_definition,
        "Parameter declaration should be a definition"
    );
    assert!(
        param_ref.is_write_access,
        "Parameter declaration should be a write access"
    );

    // The usage in the body (line 1)
    let body_ref = refs.iter().find(|r| r.location.range.start.line == 1);
    assert!(body_ref.is_some(), "Should have body ref on line 1");
    let body_ref = body_ref.unwrap();
    assert!(
        !body_ref.is_definition,
        "Parameter usage should not be a definition"
    );
    assert!(
        !body_ref.is_write_access,
        "Parameter read should not be a write access"
    );
}

#[test]
fn test_detailed_refs_line_text_is_correct() {
    // Verify lineText contains the correct line content
    let source = "const x = 1;\nconsole.log(x);";
    let refs = get_detailed_refs(source, "test.ts", 0, 6);

    assert!(
        refs.len() >= 2,
        "Should find at least 2 references, got {}",
        refs.len()
    );

    let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
    assert!(decl_ref.is_some(), "Should have ref on line 0");
    assert_eq!(
        decl_ref.unwrap().line_text,
        "const x = 1;",
        "lineText should be the full line content"
    );

    let usage_ref = refs.iter().find(|r| r.location.range.start.line == 1);
    assert!(usage_ref.is_some(), "Should have ref on line 1");
    assert_eq!(
        usage_ref.unwrap().line_text,
        "console.log(x);",
        "lineText should be the full line content"
    );
}

#[test]
fn test_detailed_refs_interface_declaration_is_definition() {
    // `interface Foo { x: number; } let a: Foo;`
    let source = "interface Foo {\n  x: number;\n}\nlet a: Foo;";
    let refs = get_detailed_refs(source, "test.ts", 0, 10);

    assert!(!refs.is_empty(), "Should find at least 1 reference");

    let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
    assert!(decl_ref.is_some(), "Should have declaration ref");
    let decl_ref = decl_ref.unwrap();
    assert!(
        decl_ref.is_definition,
        "Interface declaration should be a definition"
    );
    assert!(
        decl_ref.is_write_access,
        "Interface declaration should be a write access"
    );
}

#[test]
fn test_detailed_refs_enum_declaration_is_definition() {
    // `enum Color { Red } let c = Color.Red;`
    let source = "enum Color {\n  Red\n}\nlet c = Color.Red;";
    let refs = get_detailed_refs(source, "test.ts", 0, 5);

    assert!(!refs.is_empty(), "Should find at least 1 reference");

    let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
    assert!(decl_ref.is_some(), "Should have declaration ref on line 0");
    let decl_ref = decl_ref.unwrap();
    assert!(
        decl_ref.is_definition,
        "Enum declaration should be a definition"
    );
    assert!(
        decl_ref.is_write_access,
        "Enum declaration should be a write access"
    );
}

#[test]
fn test_detailed_refs_type_alias_is_definition() {
    // `type Foo = number; let x: Foo;`
    let source = "type Foo = number;\nlet x: Foo;";
    let refs = get_detailed_refs(source, "test.ts", 0, 5);

    assert!(!refs.is_empty(), "Should find at least 1 reference");

    let decl_ref = refs.iter().find(|r| r.location.range.start.line == 0);
    assert!(decl_ref.is_some(), "Should have declaration ref on line 0");
    let decl_ref = decl_ref.unwrap();
    assert!(
        decl_ref.is_definition,
        "Type alias declaration should be a definition"
    );
    assert!(
        decl_ref.is_write_access,
        "Type alias declaration should be a write access"
    );
}

#[test]
fn test_detailed_refs_read_in_expression_not_write() {
    // `let x = 1; let y = x + 2;`
    // x in the expression `x + 2` should be isWriteAccess=false
    let source = "let x = 1;\nlet y = x + 2;";
    let refs = get_detailed_refs(source, "test.ts", 0, 4);

    assert!(
        refs.len() >= 2,
        "Should find at least 2 references, got {}",
        refs.len()
    );

    let expr_ref = refs
        .iter()
        .find(|r| r.location.range.start.line == 1 && !r.is_definition);
    assert!(expr_ref.is_some(), "Should have a read usage ref on line 1");
    let expr_ref = expr_ref.unwrap();
    assert!(
        !expr_ref.is_write_access,
        "Read in expression should not be write access"
    );
}

// =========================================================================
// Tests for find_rename_locations
// =========================================================================

#[test]
fn test_rename_locations_simple() {
    let source = "const x = 1;\nx + x;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let position = Position::new(1, 0);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let locations = find_refs.find_rename_locations(root, position);

    assert!(locations.is_some(), "Should find rename locations for x");
    let locs = locations.unwrap();
    assert!(
        locs.len() >= 2,
        "Should find at least 2 rename locations (declaration + usages)"
    );

    // Each location should have a line_text
    for loc in &locs {
        assert!(
            !loc.line_text.is_empty(),
            "Rename location should have non-empty line_text"
        );
    }
}
