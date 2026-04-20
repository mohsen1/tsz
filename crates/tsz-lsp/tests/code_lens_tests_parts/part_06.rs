#[test]
fn test_code_lens_async_generator_function() {
    let source = "async function* streamData() {\n  yield 1;\n  yield 2;\n}";
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
        "Async generator function should have a code lens"
    );
}

#[test]
fn test_code_lens_class_with_protected_method() {
    let source = "class Base {\n  protected init() {}\n  protected cleanup() {}\n}";
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
        "Class with protected methods should have a code lens"
    );
}

#[test]
fn test_code_lens_enum_single_member() {
    let source = "enum Singleton {\n  Instance\n}";
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
        "Enum with single member should have a code lens"
    );
}

#[test]
fn test_code_lens_function_with_rest_params() {
    let source = "function collect(...args: number[]) {\n  return args;\n}";
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
        "Function with rest params should have a code lens"
    );
}

#[test]
fn test_code_lens_class_with_static_property() {
    let source = "class Registry {\n  static instances: Registry[] = [];\n  static count = 0;\n}";
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
        "Class with static properties should have a code lens"
    );
}

#[test]
fn test_code_lens_interface_with_call_signature() {
    let source = "interface Callable {\n  (x: number): string;\n  (x: string): number;\n}";
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
        "Interface with call signatures should have refs and impls lenses, got {}",
        interface_lenses.len()
    );
}

#[test]
fn test_code_lens_multiple_type_aliases() {
    let source = "type A = string;\ntype B = number;\ntype C = boolean;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
        "Whitespace-only file should produce no lenses"
    );
}

#[test]
fn test_code_lens_unicode_function_name() {
    let source = "function grüße() {\n  return 1;\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
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

