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

#[test]
fn test_code_lens_multiple_functions() {
    let source = "function foo() {}\nfunction bar() {}\nfunction baz() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have one lens per function (3 total)
    assert!(
        lenses.len() >= 3,
        "Should have at least 3 code lenses for 3 functions, got {}",
        lenses.len()
    );

    // Each function should get a lens on its respective line
    let lines: Vec<u32> = lenses.iter().map(|l| l.range.start.line).collect();
    assert!(lines.contains(&0), "Should have lens for foo at line 0");
    assert!(lines.contains(&1), "Should have lens for bar at line 1");
    assert!(lines.contains(&2), "Should have lens for baz at line 2");
}

#[test]
fn test_code_lens_nested_functions() {
    let source = "function outer() {\n  function inner() {}\n  return inner();\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have lenses for both outer and inner functions
    // The provider iterates all nodes, so inner function declarations get lenses too
    assert!(
        lenses.len() >= 2,
        "Should have at least 2 code lenses (outer + inner), got {}",
        lenses.len()
    );

    let has_outer = lenses.iter().any(|l| l.range.start.line == 0);
    assert!(has_outer, "Should have lens for outer function at line 0");

    let has_inner = lenses.iter().any(|l| l.range.start.line == 1);
    assert!(has_inner, "Should have lens for inner function at line 1");
}

#[test]
fn test_code_lens_interface_method() {
    let source = "interface Greeter {\n  greet(name: string): string;\n  wave(): void;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Interface should get references + implementations lenses
    let interface_lenses: Vec<_> = lenses.iter().filter(|l| l.range.start.line == 0).collect();
    assert!(
        interface_lenses.len() >= 2,
        "Interface should have references and implementations lenses, got {}",
        interface_lenses.len()
    );

    let has_refs = interface_lenses
        .iter()
        .any(|l| l.data.as_ref().unwrap().kind == CodeLensKind::References);
    let has_impls = interface_lenses
        .iter()
        .any(|l| l.data.as_ref().unwrap().kind == CodeLensKind::Implementations);
    assert!(has_refs, "Interface should have references lens");
    assert!(has_impls, "Interface should have implementations lens");
}

#[test]
fn test_code_lens_class_with_constructor_and_methods() {
    let source = "class Animal {\n  constructor(public name: string) {}\n  speak() {\n    return this.name;\n  }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have lens for class declaration at minimum
    let class_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        class_lens.is_some(),
        "Should have lens for class declaration"
    );

    // Should have lenses for methods too
    assert!(
        lenses.len() >= 2,
        "Should have lenses for class and at least one method, got {}",
        lenses.len()
    );
}

#[test]
fn test_code_lens_multiple_classes() {
    let source = "class A {\n  foo() {}\n}\nclass B {\n  bar() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have lenses for both classes and their methods
    assert!(
        lenses.len() >= 4,
        "Should have at least 4 lenses (2 classes + 2 methods), got {}",
        lenses.len()
    );

    let has_class_a = lenses.iter().any(|l| l.range.start.line == 0);
    let has_class_b = lenses.iter().any(|l| l.range.start.line == 3);
    assert!(has_class_a, "Should have lens for class A at line 0");
    assert!(has_class_b, "Should have lens for class B at line 3");
}

#[test]
fn test_code_lens_exported_function() {
    let source = "export function greet() {}\nexport function farewell() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Exported functions should still get lenses
    assert!(
        lenses.len() >= 2,
        "Should have lenses for exported functions, got {}",
        lenses.len()
    );
}

#[test]
fn test_code_lens_resolve_zero_references() {
    let source = "function unused() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(!lenses.is_empty(), "Should have a lens for the function");

    let func_lens = &lenses[0];
    let resolved = provider.resolve_code_lens(root, func_lens);

    assert!(resolved.is_some(), "Should resolve lens");
    let resolved = resolved.unwrap();
    let command = resolved.command.unwrap();
    assert!(
        command.title.contains("0 references"),
        "Unused function should show 0 references, got: {}",
        command.title
    );
}

#[test]
fn test_code_lens_all_have_data() {
    let source = "function a() {}\nclass B {}\ninterface C {}\nenum D { X }\ntype E = string;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // All unresolved lenses should have data with file_path set
    for lens in &lenses {
        assert!(lens.data.is_some(), "All lenses should have data");
        let data = lens.data.as_ref().unwrap();
        assert_eq!(
            data.file_path, "test.ts",
            "Lens data should have correct file path"
        );
    }
}
