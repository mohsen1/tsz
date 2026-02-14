use super::*;
use tsz_binder::BinderState;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;

#[test]
fn test_code_lens_function() {
    let source = "function foo() {\n  return 1;\n}\nfoo();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have at least one lens for the function
    assert!(!lenses.is_empty(), "Should have code lenses");

    // Find the function lens
    let func_lens = lenses
        .iter()
        .find(|l| l.range.start.line == 0)
        .expect("Should have lens at line 0");

    assert!(func_lens.data.is_some(), "Lens should have data");
    assert_eq!(
        func_lens.data.as_ref().unwrap().kind,
        CodeLensKind::References
    );
}

#[test]
fn test_code_lens_class() {
    let source = "class MyClass {\n  method() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have lenses for both class and method
    assert!(lenses.len() >= 2, "Should have at least 2 code lenses");
}

#[test]
fn test_code_lens_interface() {
    let source = "interface Foo {\n  bar(): void;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have references and implementations lenses for interface
    let interface_lenses: Vec<_> = lenses.iter().filter(|l| l.range.start.line == 0).collect();

    assert!(
        interface_lenses.len() >= 2,
        "Interface should have references and implementations lenses"
    );
}

#[test]
fn test_code_lens_resolve() {
    let source = "function foo() {}\nfoo();\nfoo();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    let func_lens = lenses
        .iter()
        .find(|l| l.range.start.line == 0)
        .expect("Should have function lens");

    // Resolve the lens
    let resolved = provider.resolve_code_lens(root, func_lens);

    assert!(resolved.is_some(), "Should resolve lens");
    let resolved = resolved.unwrap();
    assert!(
        resolved.command.is_some(),
        "Resolved lens should have command"
    );

    let command = resolved.command.unwrap();
    // Should show reference count (2 calls + 1 declaration - 1 = 2 references)
    assert!(
        command.title.contains("reference"),
        "Title should mention references: {}",
        command.title
    );
}

#[test]
fn test_code_lens_enum() {
    let source = "enum Color {\n  Red,\n  Green,\n  Blue\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have a lens for the enum
    let enum_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(enum_lens.is_some(), "Should have lens for enum");
}

#[test]
fn test_code_lens_type_alias() {
    let source = "type MyType = string | number;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have a lens for the type alias
    assert!(!lenses.is_empty(), "Should have lens for type alias");
}

#[test]
fn test_code_lens_empty_file() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Empty file should have no lenses
    assert!(lenses.is_empty(), "Empty file should have no lenses");
}

#[test]
fn test_code_lens_variable_no_lens() {
    let source = "const x = 1;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Variables don't typically get code lenses (too noisy)
    // The lenses should be empty or only contain non-variable lenses
    for lens in &lenses {
        // Verify no lens at the variable position (character 6 is 'x')
        if lens.range.start.character == 6 {
            panic!("Should not have lens for variable");
        }
    }
}
