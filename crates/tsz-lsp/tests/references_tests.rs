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

// =========================================================================
// Edge case tests for comprehensive coverage
// =========================================================================

#[test]
fn test_find_references_class_name() {
    let source = "class Animal {}\nlet a = new Animal();\nlet b: Animal;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));

    assert!(refs.is_some(), "Should find references for class");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find declaration + usages, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_interface_name() {
    let source = "interface Foo { x: number; }\nlet a: Foo;\nlet b: Foo;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 10));

    assert!(refs.is_some(), "Should find references for interface");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find declaration + usages, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_enum_name() {
    let source = "enum Color { Red, Green }\nlet c: Color;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 5));

    assert!(refs.is_some(), "Should find references for enum");
}

#[test]
fn test_find_references_no_results_for_unknown_position() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at the semicolon
    let refs = find_refs.find_references(root, Position::new(0, 12));

    assert!(
        refs.is_none(),
        "Should not find references for semicolon position"
    );
}

#[test]
fn test_find_references_parameter_in_function() {
    let source = "function foo(param: number) {\n  return param * 2;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 13));

    assert!(
        refs.is_some(),
        "Should find references for function parameter"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find param declaration + usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_in_nested_scope() {
    let source = "const x = 1;\nfunction foo() {\n  const y = x;\n  return y + x;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Find references for 'x' on line 0
    let refs = find_refs.find_references(root, Position::new(0, 6));

    assert!(refs.is_some(), "Should find references for x");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 3,
        "Should find declaration + 2 usages in nested scope, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_type_alias() {
    let source = "type ID = string;\nlet userId: ID;\nlet groupId: ID;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 5));

    assert!(refs.is_some(), "Should find references for type alias");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find declaration + type usages, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_empty_file() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 0));

    assert!(refs.is_none(), "Should not find references in empty file");
}

#[test]
fn test_rename_locations_function() {
    let source = "function greet() {}\ngreet();\ngreet();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let locs = find_refs.find_rename_locations(root, Position::new(0, 9));

    assert!(locs.is_some(), "Should find rename locations for function");
    let locs = locs.unwrap();
    assert!(
        locs.len() >= 3,
        "Should find declaration + 2 calls, got {}",
        locs.len()
    );
}

// =========================================================================
// Additional coverage tests for navigation/references module
// =========================================================================

#[test]
fn test_find_references_type_alias_usage() {
    // Type alias declared once, used in multiple annotation positions
    let source = "type Pair<A, B> = [A, B];\nlet p: Pair<number, string>;\nfunction take(x: Pair<boolean, boolean>) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Pair' declaration (line 0, col 5)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 5));

    assert!(refs.is_some(), "Should find references for type alias Pair");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 3,
        "Should find declaration + 2 type annotation usages, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_generic_type_parameter() {
    // Generic type parameter T used in parameter and return type
    let source = "function identity<T>(value: T): T {\n  return value;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'T' in the type parameter list (line 0, col 18)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 18));

    assert!(
        refs.is_some(),
        "Should find references for generic type parameter T"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find T declaration + usages in param/return type, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_namespace_member() {
    // Namespace with an exported member used outside
    let source = "namespace Shapes {\n  export const PI = 3.14;\n}\nlet x = Shapes.PI;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Shapes' on line 0
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 10));

    assert!(
        refs.is_some(),
        "Should find references for namespace Shapes"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find namespace declaration + qualified usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_enum_member_access() {
    // Enum member referenced via qualified access
    let source =
        "enum Direction {\n  Up,\n  Down,\n}\nlet d = Direction.Up;\nif (d === Direction.Down) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Direction' on line 0, col 5
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 5));

    assert!(refs.is_some(), "Should find references for enum Direction");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find enum declaration + qualified member accesses, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_destructured_variable() {
    // Destructured variable used in multiple places
    let source = "const { alpha, beta } = obj;\nalpha + beta;\nconsole.log(alpha);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'alpha' usage on line 1, col 0
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 0));

    assert!(
        refs.is_some(),
        "Should find references for destructured variable alpha"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 3,
        "Should find binding + 2 usages of alpha, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_rest_parameter() {
    // Rest parameter used inside the function body
    let source = "function sum(...nums: number[]) {\n  return nums.reduce((a, b) => a + b, 0);\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'nums' in the parameter (line 0, col 16)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 16));

    assert!(
        refs.is_some(),
        "Should find references for rest parameter nums"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find rest param declaration + body usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_catch_clause_parameter() {
    // Catch clause parameter used inside the catch block
    let source =
        "try {\n  throw new Error();\n} catch (err) {\n  console.log(err);\n  throw err;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'err' in catch clause (line 2, col 9)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(2, 9));

    assert!(
        refs.is_some(),
        "Should find references for catch clause parameter err"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find catch param + usages in catch block, got {}",
        refs.len()
    );
}

#[test]
fn test_detailed_refs_write_vs_read_locations() {
    // Tests that we correctly distinguish write and read locations:
    // line 0: let x = 1;       (write + definition)
    // line 1: x = 2;           (write, not definition)
    // line 2: console.log(x);  (read)
    // line 3: x++;             (write)
    let source = "let x = 1;\nx = 2;\nconsole.log(x);\nx++;";
    let refs = get_detailed_refs(source, "test.ts", 0, 4);

    assert!(
        refs.len() >= 3,
        "Should find at least 3 references, got {}",
        refs.len()
    );

    // Declaration (line 0) should be write + definition
    let decl = refs.iter().find(|r| r.location.range.start.line == 0);
    assert!(decl.is_some(), "Should have declaration ref on line 0");
    let decl = decl.unwrap();
    assert!(decl.is_write_access, "Declaration should be write access");
    assert!(decl.is_definition, "Declaration should be definition");

    // Assignment (line 1) should be write but not definition
    let assign = refs.iter().find(|r| r.location.range.start.line == 1);
    assert!(assign.is_some(), "Should have assignment ref on line 1");
    let assign = assign.unwrap();
    assert!(assign.is_write_access, "Assignment should be write access");
    assert!(!assign.is_definition, "Assignment should not be definition");

    // Read (line 2) should be neither write nor definition
    let read = refs.iter().find(|r| r.location.range.start.line == 2);
    if let Some(read) = read {
        assert!(
            !read.is_write_access,
            "Read usage should not be write access"
        );
        assert!(!read.is_definition, "Read usage should not be definition");
    }
}

#[test]
fn test_find_references_same_name_different_scopes() {
    // Same variable name 'x' declared in different scopes should NOT
    // cross-reference between scopes.
    let source = "function a() {\n  const x = 1;\n  return x;\n}\nfunction b() {\n  const x = 2;\n  return x;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Find references for 'x' in function a (line 1, col 8)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs_a = find_refs.find_references(root, Position::new(1, 8));

    assert!(
        refs_a.is_some(),
        "Should find references for x in function a"
    );
    let refs_a = refs_a.unwrap();

    // References for x in scope a should only appear on lines 1 and 2
    for loc in &refs_a {
        assert!(
            loc.range.start.line <= 2,
            "Reference for x in function a should not appear on line {}, which is in function b",
            loc.range.start.line
        );
    }

    // Find references for 'x' in function b (line 5, col 8)
    let refs_b = find_refs.find_references(root, Position::new(5, 8));

    assert!(
        refs_b.is_some(),
        "Should find references for x in function b"
    );
    let refs_b = refs_b.unwrap();

    // References for x in scope b should only appear on lines 5 and 6
    for loc in &refs_b {
        assert!(
            loc.range.start.line >= 4,
            "Reference for x in function b should not appear on line {}, which is in function a",
            loc.range.start.line
        );
    }

    // The two sets of references should not overlap
    assert!(
        refs_a.len() <= 3 && refs_b.len() <= 3,
        "Each scoped x should have at most 3 refs (decl + usage + return), got a={}, b={}",
        refs_a.len(),
        refs_b.len()
    );
}

#[test]
fn test_find_references_overloaded_function() {
    // Overloaded function: multiple signatures + implementation
    let source = "function process(x: number): number;\nfunction process(x: string): string;\nfunction process(x: any): any {\n  return x;\n}\nprocess(1);\nprocess(\"hello\");";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'process' on the implementation (line 2, col 9)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(2, 9));

    assert!(
        refs.is_some(),
        "Should find references for overloaded function process"
    );
    let refs = refs.unwrap();
    // Should find at least the implementation + 2 call sites
    assert!(
        refs.len() >= 3,
        "Should find overload signatures/impl + call sites, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_for_in_loop_variable() {
    // for-in loop variable
    let source = "const obj = { a: 1, b: 2 };\nfor (const key in obj) {\n  console.log(key);\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'key' usage inside the loop body (line 2, col 14)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(2, 14));

    assert!(
        refs.is_some(),
        "Should find references for for-in loop variable key"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find loop variable declaration + usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_for_of_loop_variable() {
    // for-of loop variable
    let source = "const arr = [1, 2, 3];\nfor (const item of arr) {\n  console.log(item);\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'item' usage inside the loop body (line 2, col 14)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(2, 14));

    assert!(
        refs.is_some(),
        "Should find references for for-of loop variable item"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find loop variable declaration + usage, got {}",
        refs.len()
    );
}

#[test]
fn test_detailed_refs_postfix_increment_is_write() {
    // x++ should be detected as a write access
    let source = "let counter = 0;\ncounter++;\nconsole.log(counter);";
    let refs = get_detailed_refs(source, "test.ts", 0, 4);

    assert!(
        refs.len() >= 2,
        "Should find at least 2 references, got {}",
        refs.len()
    );

    // The postfix increment (line 1) should be a write access
    let inc_ref = refs.iter().find(|r| r.location.range.start.line == 1);
    if let Some(inc_ref) = inc_ref {
        assert!(
            inc_ref.is_write_access,
            "Postfix increment should be a write access"
        );
        assert!(
            !inc_ref.is_definition,
            "Postfix increment should not be a definition"
        );
    }
}

#[test]
fn test_find_references_array_destructured_variable() {
    // Array destructuring
    let source = "const [first, second] = [1, 2];\nconsole.log(first);\nlet x = second + first;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'first' usage on line 1, col 12
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 12));

    assert!(
        refs.is_some(),
        "Should find references for array-destructured variable first"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 3,
        "Should find array binding + 2 usages of first, got {}",
        refs.len()
    );
}

// =========================================================================
// Additional edge-case tests
// =========================================================================

#[test]
fn test_find_references_class_name_across_usages() {
    let source = "class Widget {}\nconst w = new Widget();\nlet x: Widget;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Widget' declaration (line 0, col 6)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));

    assert!(refs.is_some(), "Should find references for class name");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 3,
        "Should find class declaration + new expression + type annotation, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_interface_name_in_type_position() {
    let source = "interface Config { key: string; }\nfunction init(c: Config) {}\nconst cfg: Config = { key: 'a' };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Config' declaration (line 0, col 10)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 10));

    assert!(refs.is_some(), "Should find references for interface name");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 3,
        "Should find interface decl + 2 type usages, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_namespace_name() {
    let source = "namespace Utils {\n  export function helper() {}\n}\nUtils.helper();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Utils' usage (line 3, col 0)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(3, 0));

    assert!(refs.is_some(), "Should find references for namespace name");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find namespace declaration + usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_empty_file_returns_none() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 0));

    assert!(refs.is_none(), "Empty file should return None");
}

#[test]
fn test_find_references_for_loop_counter() {
    let source = "for (let i = 0; i < 5; i++) {\n  console.log(i);\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'i' declaration (line 0, col 9)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 9));

    assert!(
        refs.is_some(),
        "Should find references for for-loop counter"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 3,
        "Should find declaration + condition + increment + body usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_arrow_function_param() {
    let source = "const double = (n: number) => n * 2;\ndouble(3);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'n' parameter (col 16)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 16));

    assert!(
        refs.is_some(),
        "Should find references for arrow function param"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find param declaration + usage in body, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_nested_function_scoping() {
    let source = "function outer() {\n  const x = 1;\n  function inner() {\n    const x = 2;\n    x;\n  }\n  x;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'x' in outer scope (line 1, col 8)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 8));

    assert!(refs.is_some(), "Should find references for outer x");
    let refs = refs.unwrap();
    // Outer x should have declaration + usage on line 6, but NOT include inner x
    assert!(
        refs.len() >= 2,
        "Should find outer x declaration + usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_type_alias_in_multiple_annotations() {
    let source = "type ID = string;\nlet a: ID;\nlet b: ID;\nfunction process(id: ID) {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'ID' declaration (line 0, col 5)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 5));

    assert!(refs.is_some(), "Should find references for type alias");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 4,
        "Should find type alias decl + 3 type usages, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_const_enum_name() {
    let source = "const enum Fruit { Apple, Banana }\nlet f: Fruit = Fruit.Apple;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'Fruit' declaration (line 0, col 11)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 11));

    assert!(refs.is_some(), "Should find references for const enum");
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find const enum declaration + usages, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_function_used_as_callback() {
    let source = "function handler() {}\nconst arr = [1, 2];\narr.forEach(handler);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);

    // Position at 'handler' declaration (line 0, col 9)
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 9));

    assert!(
        refs.is_some(),
        "Should find references for function used as callback"
    );
    let refs = refs.unwrap();
    assert!(
        refs.len() >= 2,
        "Should find function declaration + callback usage, got {}",
        refs.len()
    );
}

#[test]
fn test_find_references_default_parameter() {
    let source = "function greet(name = 'world') { return name; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 15));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find name param + usage");
    }
}

#[test]
fn test_find_references_computed_property_name() {
    let source = "const key = 'x';\nconst obj = { [key]: 1 };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find key decl + computed usage");
    }
}

#[test]
fn test_find_references_switch_case_variable() {
    let source = "const x = 1;\nswitch(x) { case 0: break; default: x; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2);
    }
}

#[test]
fn test_find_references_class_constructor_param() {
    let source = "class Foo {\n  constructor(public x: number) {}\n  get() { return this.x; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 22));
    let _ = refs;
}

#[test]
fn test_find_references_spread_element() {
    let source = "const arr = [1, 2];\nconst copy = [...arr];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find arr decl + spread usage");
    }
}

#[test]
fn test_find_references_typeof_expression() {
    let source = "const x = 42;\ntype T = typeof x;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    if let Some(r) = refs {
        assert!(!r.is_empty());
    }
}

#[test]
fn test_find_references_optional_chaining_variable() {
    let source = "const obj = { a: 1 };\nconst val = obj?.a;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2);
    }
}

#[test]
fn test_find_references_nullish_coalescing_variable() {
    let source = "const x = null;\nconst y = x ?? 'default';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2);
    }
}

#[test]
fn test_find_references_multiple_declarations_same_name() {
    let source =
        "function foo() { const x = 1; return x; }\nfunction bar() { const x = 2; return x; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    // Position at x in foo
    let refs = find_refs.find_references(root, Position::new(0, 23));
    let _ = refs;
}

#[test]
fn test_find_references_export_assignment() {
    let source = "const value = 42;\nexport default value;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find decl + export usage");
    }
}

#[test]
fn test_find_references_shorthand_property() {
    let source = "const x = 1;\nconst obj = { x };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find decl + shorthand usage");
    }
}

#[test]
fn test_find_references_class_static_property() {
    let source = "class Foo {\n  static count = 0;\n  inc() { Foo.count++; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    let _ = refs;
}

#[test]
fn test_detailed_refs_let_reassignment_is_write() {
    let source = "let x = 1;\nx = 2;";
    let refs = get_detailed_refs(source, "test.ts", 0, 4);
    let writes: Vec<_> = refs.iter().filter(|r| r.is_write_access).collect();
    assert!(!writes.is_empty(), "Reassignment should be a write");
}

#[test]
fn test_detailed_refs_delete_expression() {
    let source = "const obj: any = { x: 1 };\ndelete obj.x;";
    let refs = get_detailed_refs(source, "test.ts", 0, 6);
    assert!(!refs.is_empty());
}

// =========================================================================
// Additional reference tests (batch 4 — edge cases)
// =========================================================================

#[test]
fn test_find_references_single_char_identifier() {
    let source = "const a = 1;\na;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(
            r.len() >= 2,
            "Should find declaration + usage for single-char id"
        );
    }
}

#[test]
fn test_find_references_unicode_identifier() {
    let source = "const \u{00e4}\u{00f6}\u{00fc} = 1;\n\u{00e4}\u{00f6}\u{00fc};";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    let _ = refs;
}

#[test]
fn test_find_references_let_in_block_scope() {
    let source = "{ let y = 10; y; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find block-scoped let refs");
    }
}

#[test]
fn test_find_references_var_in_function() {
    let source = "function f() { var v = 1; return v; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 19));
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find var decl + return usage");
    }
}

#[test]
fn test_find_references_ternary_condition() {
    let source = "const flag = true;\nconst result = flag ? 'yes' : 'no';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(
            r.len() >= 2,
            "Should find flag decl + ternary condition usage"
        );
    }
}

#[test]
fn test_find_references_while_loop_condition() {
    let source = "let running = true;\nwhile (running) { running = false; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 4));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 3, "Should find decl + condition + assignment");
    }
}

#[test]
fn test_find_references_do_while_condition() {
    let source = "let count = 0;\ndo { count++; } while (count < 5);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 4));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2);
    }
}

#[test]
fn test_find_references_nested_destructuring() {
    let source = "const { a: { b } } = { a: { b: 42 } };\nb;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 0));
    let _ = refs;
}

#[test]
fn test_find_references_class_private_field() {
    let source = "class Foo {\n  #secret = 42;\n  get() { return this.#secret; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 2));
    let _ = refs;
}

#[test]
fn test_find_references_async_function_name() {
    let source = "async function fetchData() {}\nawait fetchData();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 15));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find async function decl + call");
    }
}

#[test]
fn test_find_references_generator_function_name() {
    let source = "function* gen() { yield 1; }\nconst it = gen();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 10));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find generator decl + call");
    }
}

#[test]
fn test_find_references_type_parameter_in_function() {
    let source = "function identity<T>(x: T): T { return x; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 18));
    let _ = refs;
}

#[test]
fn test_find_references_type_parameter_in_class() {
    let source = "class Container<T> {\n  value: T;\n  get(): T { return this.value; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 16));
    let _ = refs;
}

#[test]
fn test_find_references_comma_operator() {
    let source = "let x = 0;\n(x++, x);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 4));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2);
    }
}

#[test]
fn test_find_references_logical_assignment() {
    let source = "let x: number | null = null;\nx ??= 42;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 4));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2);
    }
}

#[test]
fn test_find_references_in_arrow_return_expression() {
    let source = "const val = 10;\nconst fn = () => val;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find decl + arrow return usage");
    }
}

#[test]
fn test_find_references_in_object_spread() {
    let source = "const base = { a: 1 };\nconst ext = { ...base, b: 2 };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find decl + object spread usage");
    }
}

#[test]
fn test_find_references_in_array_index() {
    let source = "const idx = 0;\nconst arr = [1, 2, 3];\narr[idx];";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find idx decl + element access usage");
    }
}

#[test]
fn test_find_references_in_if_condition() {
    let source = "const cond = true;\nif (cond) { }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(r.len() >= 2, "Should find decl + if-condition usage");
    }
}

#[test]
fn test_find_references_class_method_name() {
    let source = "class A {\n  run() {}\n}\nnew A().run();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(1, 2));
    let _ = refs;
}

#[test]
fn test_find_references_multiline_string_variable() {
    let source = "const msg = `line1\nline2\nline3`;\nconsole.log(msg);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);
    let line_map = LineMap::build(source);
    let find_refs = FindReferences::new(arena, &binder, &line_map, "test.ts".to_string(), source);
    let refs = find_refs.find_references(root, Position::new(0, 6));
    assert!(refs.is_some());
    if let Some(r) = refs {
        assert!(
            r.len() >= 2,
            "Should find decl + log usage across multiline template"
        );
    }
}

#[test]
fn test_detailed_refs_for_loop_counter_is_write() {
    let source = "for (let i = 0; i < 10; i++) { i; }";
    let refs = get_detailed_refs(source, "test.ts", 0, 9);
    let writes: Vec<_> = refs.iter().filter(|r| r.is_write_access).collect();
    assert!(
        !writes.is_empty(),
        "for-loop init and increment should be writes"
    );
}
