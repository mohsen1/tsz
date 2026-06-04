#[test]
fn test_code_lens_class_with_static_property() {
    let source = "class Registry {\n  static instances: Registry[] = [];\n  static count = 0;\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let class_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(
        class_lens.is_some(),
        "Class with static properties should have a code lens"
    );
}

#[test]
fn test_code_lens_interface_with_call_signature() {
    let source = "interface Callable {\n  (x: number): string;\n  (x: string): number;\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    let interface_lenses: Vec<_> = lenses.iter().filter(|l| l.range.start.line == 0).collect();
    assert!(
        interface_lenses.len() >= 2,
        "Interface with call signatures should have refs and impls lenses, got {}",
        interface_lenses.len()
    );
}

#[test]
fn test_code_lens_multiple_type_aliases() {
    let source = "type A = string;\ntype B = number;\ntype C = boolean;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);

    assert!(
        lenses.len() >= 3,
        "Each type alias should have at least one code lens, got {}",
        lenses.len()
    );
}

#[test]
fn test_code_lens_whitespace_only_file() {
    let source = "   \n   \n   ";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        lenses.is_empty(),
        "Whitespace-only file should produce no lenses"
    );
}

#[test]
fn test_code_lens_unicode_function_name() {
    let source = "function grüße() {\n  return 1;\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Unicode function name should produce code lenses"
    );
}

#[test]
fn test_code_lens_class_with_constructor_only() {
    let source = "class Singleton {\n  constructor() {\n    // init\n  }\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Class with constructor should have at least one lens"
    );
}

#[test]
fn test_code_lens_deeply_nested_class() {
    let source = "namespace Outer {\n  namespace Inner {\n    class Deep {\n      method() {}\n    }\n  }\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Deeply nested class should produce code lenses"
    );
}

#[test]
fn test_code_lens_class_with_readonly_property() {
    let source = "class Config {\n  readonly host: string = 'localhost';\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Class with readonly property should have lenses"
    );
}

#[test]
fn test_code_lens_type_alias_conditional() {
    let source = "type IsString<T> = T extends string ? true : false;";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Conditional type alias should produce code lenses"
    );
}

#[test]
fn test_code_lens_type_alias_mapped() {
    let source = "type Partial<T> = {\n  [K in keyof T]?: T[K];\n};";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Mapped type alias should produce code lenses"
    );
}

#[test]
fn test_code_lens_function_with_optional_params() {
    let source = "function greet(name?: string, greeting?: string) {\n  return `Hello`;\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Function with optional params should have lenses"
    );
    let func_lens = lenses.iter().find(|l| l.range.start.line == 0);
    assert!(func_lens.is_some(), "Should have lens at line 0");
}

#[test]
fn test_code_lens_interface_with_generic_params() {
    let source = "interface Repository<T, K extends string> {\n  get(id: K): T;\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    let interface_lenses: Vec<_> = lenses.iter().filter(|l| l.range.start.line == 0).collect();
    assert!(
        interface_lenses.len() >= 2,
        "Generic interface should have refs and impls lenses, got {}",
        interface_lenses.len()
    );
}

#[test]
fn test_code_lens_enum_string_members() {
    let source = "enum Status {\n  Active = 'active',\n  Inactive = 'inactive'\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Enum with string members should produce code lenses"
    );
}

#[test]
fn test_code_lens_multiple_namespaces() {
    let source =
        "namespace A {\n  export function fa() {}\n}\nnamespace B {\n  export function fb() {}\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        lenses.len() >= 2,
        "Multiple namespaces should produce multiple lenses, got {}",
        lenses.len()
    );
}

#[test]
fn test_code_lens_class_with_abstract_property() {
    let source =
        "abstract class Shape {\n  abstract area: number;\n  abstract perimeter(): number;\n}";
    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider = CodeLensProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let lenses = provider.provide_code_lenses(root);
    assert!(
        !lenses.is_empty(),
        "Abstract class with abstract members should produce lenses"
    );
}
