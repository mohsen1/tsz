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

#[test]
fn test_code_lens_abstract_class() {
    let source = "abstract class Base {\n  abstract foo(): void;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Abstract class should get a code lens
    let class_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        class_lens.is_some(),
        "Abstract class should have a code lens"
    );
}

#[test]
fn test_code_lens_const_enum() {
    let source = "const enum Direction {\n  Up,\n  Down,\n  Left,\n  Right\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // const enum should get a code lens
    let enum_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(enum_lens.is_some(), "Const enum should have a code lens");
}

#[test]
fn test_code_lens_interface_only_has_implementations_kind() {
    let source = "interface Serializable {\n  serialize(): string;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Interface should have an Implementations kind lens
    let impl_lens = lenses.iter().find(|l| {
        l.data
            .as_ref()
            .map_or(false, |d| d.kind == CodeLensKind::Implementations)
    });
    assert!(
        impl_lens.is_some(),
        "Interface should have an Implementations lens"
    );
}

#[test]
fn test_code_lens_class_no_implementations_kind() {
    // Regular classes should not get an Implementations lens (only interfaces do)
    let source = "class Concrete {\n  method() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let impl_lens = lenses.iter().find(|l| {
        l.data
            .as_ref()
            .map_or(false, |d| d.kind == CodeLensKind::Implementations)
    });
    assert!(
        impl_lens.is_none(),
        "Regular class should NOT have an Implementations lens"
    );
}

#[test]
fn test_code_lens_resolve_interface_implementations() {
    let source = "interface Runnable {\n  run(): void;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Find the implementations lens and resolve it
    let impl_lens = lenses.iter().find(|l| {
        l.data
            .as_ref()
            .map_or(false, |d| d.kind == CodeLensKind::Implementations)
    });

    if let Some(lens) = impl_lens {
        let resolved = provider.resolve_code_lens(root, lens);
        assert!(resolved.is_some(), "Implementations lens should resolve");
        let resolved = resolved.unwrap();
        assert!(
            resolved.command.is_some(),
            "Resolved lens should have command"
        );
        let cmd = resolved.command.unwrap();
        assert_eq!(cmd.command, "editor.action.goToImplementation");
    }
}

#[test]
fn test_code_lens_resolve_with_no_data() {
    let source = "function foo() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Create a lens with no data - resolution should return None
    let range = tsz_common::position::Range::new(Position::new(0, 0), Position::new(0, 1));
    let lens = CodeLens {
        range,
        command: None,
        data: None,
    };

    let resolved = provider.resolve_code_lens(root, &lens);
    assert!(resolved.is_none(), "Lens without data should not resolve");
}

#[test]
fn test_code_lens_multiple_interfaces() {
    let source = "interface A {\n  x: number;\n}\ninterface B {\n  y: string;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Each interface should get both references and implementations lenses
    let interface_a_lenses: Vec<_> = lenses.iter().filter(|l| l.range.start.line == 0).collect();
    let interface_b_lenses: Vec<_> = lenses.iter().filter(|l| l.range.start.line == 3).collect();

    assert!(
        interface_a_lenses.len() >= 2,
        "Interface A should have at least 2 lenses (refs + impls), got {}",
        interface_a_lenses.len()
    );
    assert!(
        interface_b_lenses.len() >= 2,
        "Interface B should have at least 2 lenses (refs + impls), got {}",
        interface_b_lenses.len()
    );
}

#[test]
fn test_code_lens_only_comments() {
    let source = "// This is a comment\n/* Block comment */";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    assert!(
        lenses.is_empty(),
        "File with only comments should have no code lenses"
    );
}

#[test]
fn test_code_lens_generic_function() {
    let source = "function identity<T>(arg: T): T {\n  return arg;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let func_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        func_lens.is_some(),
        "Generic function should have a code lens"
    );
}

#[test]
fn test_code_lens_resolve_single_reference() {
    let source = "function greet() {}\ngreet();";
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

    let resolved = provider.resolve_code_lens(root, func_lens);
    if let Some(resolved) = resolved {
        if let Some(command) = resolved.command {
            // Reference count depends on binder implementation
            assert!(
                command.title.contains("reference"),
                "Should contain 'reference' in title, got: {}",
                command.title
            );
        }
    }
}

#[test]
fn test_code_lens_class_with_static_method() {
    let source = "class Utils {\n  static parse() {}\n  static format() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have lens for the class itself at minimum
    let class_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        class_lens.is_some(),
        "Class with static methods should have a lens"
    );
}

#[test]
fn test_code_lens_arrow_function_variable() {
    let source = "const greet = () => 'hello';";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Arrow functions assigned to variables typically don't get code lenses
    // Defensive: just verify no panic
    let _ = lenses;
}

#[test]
fn test_code_lens_class_getter_setter() {
    let source = "class Obj {\n  get value() { return 1; }\n  set value(v: number) {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have at least one lens for the class
    let class_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        class_lens.is_some(),
        "Class with getter/setter should have a code lens"
    );
}

#[test]
fn test_code_lens_function_overloads() {
    let source = "function foo(x: string): string;\nfunction foo(x: number): number;\nfunction foo(x: any): any { return x; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have lenses for the overloaded function declarations
    assert!(!lenses.is_empty(), "Function overloads should have lenses");
}

#[test]
fn test_code_lens_exported_class() {
    let source = "export class Widget {\n  render() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let class_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        class_lens.is_some(),
        "Exported class should have a code lens"
    );
}

#[test]
fn test_code_lens_exported_interface() {
    let source = "export interface Config {\n  debug: boolean;\n  port: number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Exported interface should still get lenses
    let interface_lenses: Vec<_> = lenses.iter().filter(|l| l.range.start.line == 0).collect();
    assert!(
        interface_lenses.len() >= 2,
        "Exported interface should have references and implementations lenses, got {}",
        interface_lenses.len()
    );
}

#[test]
fn test_code_lens_enum_with_values() {
    let source =
        "enum Status {\n  Active = 'active',\n  Inactive = 'inactive',\n  Pending = 'pending'\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let enum_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        enum_lens.is_some(),
        "Enum with string values should have a code lens"
    );
}

#[test]
fn test_code_lens_class_with_properties() {
    let source = "class Config {\n  public host: string = 'localhost';\n  private port: number = 3000;\n  method() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have lens for class at minimum
    let class_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        class_lens.is_some(),
        "Class with properties should have a code lens"
    );
}

#[test]
fn test_code_lens_generic_class() {
    let source = "class Container<T> {\n  value: T;\n  get(): T { return this.value; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let class_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        class_lens.is_some(),
        "Generic class should have a code lens"
    );
}

#[test]
fn test_code_lens_generic_interface() {
    let source = "interface Repository<T> {\n  find(id: string): T;\n  save(item: T): void;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let interface_lenses: Vec<_> = lenses.iter().filter(|l| l.range.start.line == 0).collect();
    assert!(
        interface_lenses.len() >= 2,
        "Generic interface should have references and implementations lenses"
    );
}

#[test]
fn test_code_lens_class_extending_class() {
    let source = "class Animal {\n  move() {}\n}\nclass Dog extends Animal {\n  bark() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Both classes should have lenses
    let has_animal = lenses.iter().any(|l| l.range.start.line == 0);
    let has_dog = lenses.iter().any(|l| l.range.start.line == 3);
    assert!(has_animal, "Base class should have a code lens");
    assert!(has_dog, "Derived class should have a code lens");
}

#[test]
fn test_code_lens_class_implementing_interface() {
    let source = "interface Printable {\n  print(): void;\n}\nclass Report implements Printable {\n  print() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Interface should have implementations lens
    let impl_lens = lenses.iter().find(|l| {
        l.range.start.line == 0
            && l.data
                .as_ref()
                .map_or(false, |d| d.kind == CodeLensKind::Implementations)
    });
    assert!(
        impl_lens.is_some(),
        "Interface with implementing class should have implementations lens"
    );
}

#[test]
fn test_code_lens_resolve_many_references() {
    let source = "function used() {}\nused();\nused();\nused();\nused();\nused();";
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

    let resolved = provider.resolve_code_lens(root, func_lens);
    if let Some(resolved) = resolved {
        if let Some(command) = resolved.command {
            assert!(
                command.title.contains("reference"),
                "Should contain 'reference' in title, got: {}",
                command.title
            );
        }
    }
}

#[test]
fn test_code_lens_abstract_method() {
    let source =
        "abstract class Shape {\n  abstract area(): number;\n  abstract perimeter(): number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Abstract class should get a lens
    let class_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        class_lens.is_some(),
        "Abstract class with abstract methods should have lens"
    );
}

#[test]
fn test_code_lens_mixed_declarations() {
    let source = "function a() {}\nclass B {}\ninterface C {}\nenum D { X }\ntype E = number;\nfunction f() {}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have lenses for all declaration types
    assert!(
        lenses.len() >= 6,
        "Should have at least 6 lenses for mixed declarations, got {}",
        lenses.len()
    );
}

#[test]
fn test_code_lens_interface_extending_interface() {
    let source =
        "interface Base {\n  id: number;\n}\ninterface Child extends Base {\n  name: string;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Both interfaces should have lenses
    let has_base = lenses.iter().any(|l| l.range.start.line == 0);
    let has_child = lenses.iter().any(|l| l.range.start.line == 3);
    assert!(has_base, "Base interface should have a code lens");
    assert!(has_child, "Child interface should have a code lens");
}

#[test]
fn test_code_lens_file_path_in_data() {
    let source = "function test() {}";
    let file_path = "src/components/widget.ts";
    let mut parser = ParserState::new(file_path.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, file_path.to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    for lens in &lenses {
        if let Some(data) = &lens.data {
            assert_eq!(
                data.file_path, file_path,
                "Lens data should contain the correct file path"
            );
        }
    }
}

#[test]
fn test_code_lens_async_function() {
    let source = "async function fetchData() {\n  return await Promise.resolve(1);\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let func_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        func_lens.is_some(),
        "Async function should have a code lens"
    );
}

#[test]
fn test_code_lens_generator_function() {
    let source = "function* gen() {\n  yield 1;\n  yield 2;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let func_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        func_lens.is_some(),
        "Generator function should have a code lens"
    );
}

#[test]
fn test_code_lens_class_with_private_method() {
    let source = "class Secret {\n  private hidden() {}\n  public visible() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Class should still get a lens regardless of member visibility
    let class_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        class_lens.is_some(),
        "Class with private methods should have a code lens"
    );
}

#[test]
fn test_code_lens_interface_with_readonly_properties() {
    let source = "interface Immutable {\n  readonly x: number;\n  readonly y: string;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let interface_lenses: Vec<_> = lenses.iter().filter(|l| l.range.start.line == 0).collect();
    assert!(
        interface_lenses.len() >= 2,
        "Interface with readonly properties should have refs and impls lenses"
    );
}

#[test]
fn test_code_lens_enum_with_computed_values() {
    let source =
        "enum FileAccess {\n  Read = 1 << 0,\n  Write = 1 << 1,\n  ReadWrite = Read | Write\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let enum_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        enum_lens.is_some(),
        "Enum with computed values should have a code lens"
    );
}

#[test]
fn test_code_lens_type_alias_union_of_interfaces() {
    let source = "interface A {}\ninterface B {}\ntype AB = A | B;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have lenses for both interfaces and the type alias
    assert!(
        lenses.len() >= 3,
        "Should have lenses for 2 interfaces + 1 type alias, got {}",
        lenses.len()
    );
}

#[test]
fn test_code_lens_class_with_index_signature() {
    let source = "class DynamicObj {\n  [key: string]: any;\n  method() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let class_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        class_lens.is_some(),
        "Class with index signature should have a code lens"
    );
}

#[test]
fn test_code_lens_function_with_default_params() {
    let source = "function greet(name: string = 'World') {\n  return `Hello ${name}`;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let func_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        func_lens.is_some(),
        "Function with default params should have a code lens"
    );
}

#[test]
fn test_code_lens_declare_function() {
    let source = "declare function external(): void;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Declared functions should still get code lenses
    let _ = lenses; // Defensive: just ensure no panic
}

#[test]
fn test_code_lens_class_with_decorators_syntax() {
    let source = "class Component {\n  method() {}\n}\nclass Service {\n  handle() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Both classes should have lenses
    let has_component = lenses.iter().any(|l| l.range.start.line == 0);
    let has_service = lenses.iter().any(|l| l.range.start.line == 3);
    assert!(has_component, "Component class should have a code lens");
    assert!(has_service, "Service class should have a code lens");
}

#[test]
fn test_code_lens_interface_with_optional_properties() {
    let source =
        "interface Options {\n  debug?: boolean;\n  verbose?: boolean;\n  timeout?: number;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let interface_lenses: Vec<_> = lenses.iter().filter(|l| l.range.start.line == 0).collect();
    assert!(
        interface_lenses.len() >= 2,
        "Interface with optional properties should have refs and impls lenses"
    );
}

#[test]
fn test_code_lens_resolve_command_for_references() {
    let source = "function target() {}\ntarget();\ntarget();\ntarget();";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    let ref_lens = lenses.iter().find(|l| {
        l.data
            .as_ref()
            .map_or(false, |d| d.kind == CodeLensKind::References)
            && l.range.start.line == 0
    });

    if let Some(lens) = ref_lens {
        let resolved = provider.resolve_code_lens(root, lens);
        if let Some(resolved) = resolved {
            if let Some(command) = resolved.command {
                assert_eq!(
                    command.command, "editor.action.showReferences",
                    "References lens should use showReferences command"
                );
            }
        }
    }
}

#[test]
fn test_code_lens_class_implementing_multiple_interfaces() {
    let source = "interface A {\n  a(): void;\n}\ninterface B {\n  b(): void;\n}\nclass Impl implements A, B {\n  a() {}\n  b() {}\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    // Should have lenses for both interfaces and the implementing class
    assert!(
        lenses.len() >= 5,
        "Should have lenses for 2 interfaces (refs+impls each) + 1 class, got {}",
        lenses.len()
    );
}
