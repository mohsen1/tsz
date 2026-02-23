use super::*;
use tsz_binder::BinderState;
use tsz_common::position::LineMap;
use tsz_parser::ParserState;

#[test]
fn test_prepare_on_function_declaration() {
    let source = "function foo() {\n  return 1;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "foo" (line 0, col 9)
    let pos = Position::new(0, 9);
    let item = provider.prepare(root, pos);

    assert!(item.is_some(), "Should find call hierarchy item for 'foo'");
    let item = item.unwrap();
    assert_eq!(item.name, "foo");
    assert_eq!(item.kind, SymbolKind::Function);
}

#[test]
fn test_prepare_on_method_declaration() {
    let source = "class Foo {\n  bar() {\n    return 1;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "bar" (line 1, col 2)
    let pos = Position::new(1, 2);
    let item = provider.prepare(root, pos);

    assert!(item.is_some(), "Should find call hierarchy item for 'bar'");
    let item = item.unwrap();
    assert_eq!(item.name, "bar");
    assert_eq!(item.kind, SymbolKind::Method);
}

#[test]
fn test_prepare_on_class_static_block() {
    let source =
        "class C {\nstatic {\n  function foo() { bar(); }\n  function bar() {}\n  foo();\n}\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "static" keyword (line 1, col 1).
    let pos = Position::new(1, 1);
    let item = provider
        .prepare(root, pos)
        .expect("Should find call hierarchy item for static block");

    assert_eq!(item.name, "static {}");
    assert_eq!(item.kind, SymbolKind::Constructor);
    assert_eq!(item.container_name, None);
    assert_eq!(item.selection_range.start, Position::new(1, 0));
    assert_eq!(item.selection_range.end, Position::new(1, 6));
}

#[test]
fn test_prepare_nested_function_in_static_block_has_no_class_container() {
    let source = "class C {\n  static {\n    function bar() {}\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(2, 13);
    let item = provider
        .prepare(root, pos)
        .expect("Should prepare nested function inside static block");

    assert_eq!(item.name, "bar");
    assert_eq!(item.container_name, None);
}

#[test]
fn test_prepare_not_on_function() {
    let source = "const x = 1;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "x" (line 0, col 6) - a variable, not a function
    let pos = Position::new(0, 6);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_none(),
        "Should not find call hierarchy item for variable"
    );
}

#[test]
fn test_prepare_on_export_equals_anonymous_function_uses_module_item() {
    let source = "export = function () {\n  baz();\n}\nfunction baz() {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position inside `function` keyword of `export = function () {}`.
    let pos = Position::new(0, 10);
    let item = provider
        .prepare(root, pos)
        .expect("Should prepare call hierarchy item for export-equals function");

    assert_eq!(item.name, "test.ts");
    assert_eq!(item.kind, SymbolKind::Module);
    assert_eq!(item.range.start, Position::new(0, 0));
    assert_eq!(item.selection_range.start, Position::new(0, 0));
    assert_eq!(item.selection_range.end, Position::new(0, 0));
}

#[test]
fn test_outgoing_calls_from_export_equals_module_selection_span() {
    let source = "export = function () {\n  baz();\n}\nfunction baz() {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Fourslash follow-up call hierarchy requests use the prepare item's selection span.
    let calls = provider.outgoing_calls(root, Position::new(0, 0));

    assert!(
        calls.iter().any(|call| call.to.name == "baz"),
        "Expected outgoing call to `baz` from export-equals module selection span"
    );
}

#[test]
fn test_outgoing_calls_simple() {
    let source = "function greet() {}\nfunction main() {\n  greet();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position inside "main" function name (line 1, col 9)
    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(!calls.is_empty(), "main should have outgoing calls");
    // Should find the call to greet
    let greet_call = calls.iter().find(|c| c.to.name == "greet");
    assert!(greet_call.is_some(), "Should find outgoing call to 'greet'");
    assert!(
        !greet_call.unwrap().from_ranges.is_empty(),
        "Should have at least one call range"
    );
}

#[test]
fn test_outgoing_calls_includes_new_expression_targets() {
    let source = "class Baz {}\nfunction build() {\n  new Baz();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "build" (line 1, col 9)
    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    let baz_call = calls.iter().find(|c| c.to.name == "Baz");
    assert!(
        baz_call.is_some(),
        "Expected outgoing call target for constructor usage 'new Baz()'"
    );
    assert_eq!(
        baz_call.unwrap().from_ranges.len(),
        1,
        "Expected one constructor callsite range"
    );
    assert_eq!(
        baz_call.unwrap().to.kind,
        SymbolKind::Class,
        "Constructor target should be classified as class in call hierarchy"
    );
}

#[test]
fn test_outgoing_calls_includes_new_expression_forward_declared_class() {
    let source = "function bar() {\n  new Baz();\n}\n\nclass Baz {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "bar" (line 0, col 9)
    let pos = Position::new(0, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.to.name == "Baz"),
        "Expected outgoing call target for forward-declared constructor usage 'new Baz()'"
    );
}

#[test]
fn test_prepare_function_range_uses_source_body_end() {
    let source = "function bar() {\n  return 1;\n}\n\nclass Baz {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 9);
    let item = provider
        .prepare(root, pos)
        .expect("Should prepare function declaration item");

    assert_eq!(item.name, "bar");
    assert_eq!(item.range.start, Position::new(0, 0));
    assert_eq!(item.range.end, Position::new(2, 1));
}

#[test]
fn test_outgoing_calls_multiple() {
    let source = "function a() {}\nfunction b() {}\nfunction c() {\n  a();\n  b();\n  a();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "c" function name (line 2, col 9)
    let pos = Position::new(2, 9);
    let calls = provider.outgoing_calls(root, pos);

    // Should find calls to a and b
    assert!(calls.len() >= 2, "Should find at least 2 outgoing targets");

    let a_call = calls.iter().find(|c| c.to.name == "a");
    assert!(a_call.is_some(), "Should find outgoing call to 'a'");
    // 'a' is called twice
    assert_eq!(
        a_call.unwrap().from_ranges.len(),
        2,
        "'a' should be called twice"
    );

    let b_call = calls.iter().find(|c| c.to.name == "b");
    assert!(b_call.is_some(), "Should find outgoing call to 'b'");
}

#[test]
fn test_outgoing_calls_for_static_block_include_only_direct_calls() {
    let source = "class C {\n  static {\n    function foo() {\n      bar();\n    }\n\n    function bar() {}\n    foo();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 3);
    let calls = provider.outgoing_calls(root, pos);

    assert_eq!(
        calls.len(),
        1,
        "Expected only one direct outgoing call from static block body"
    );
    assert_eq!(calls[0].to.name, "foo");
    assert_eq!(calls[0].to.selection_range.start, Position::new(2, 13));
    assert_eq!(calls[0].from_ranges[0].start, Position::new(7, 4));
}

#[test]
fn test_outgoing_calls_for_function_nested_in_static_block_resolve_sibling_declaration() {
    let source = "class C {\n  static {\n    function foo() {\n      bar();\n    }\n\n    function bar() {\n    }\n\n    foo();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at foo declaration name.
    let pos = Position::new(2, 13);
    let calls = provider.outgoing_calls(root, pos);

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].to.name, "bar");
    assert_eq!(calls[0].to.selection_range.start, Position::new(6, 13));
    assert_eq!(calls[0].from_ranges[0].start, Position::new(3, 6));
}

#[test]
fn test_outgoing_calls_no_calls() {
    let source = "function empty() {\n  const x = 1;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "empty" (line 0, col 9)
    let pos = Position::new(0, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.is_empty(),
        "Function with no calls should have no outgoing calls"
    );
}

#[test]
fn test_incoming_calls_simple() {
    let source = "function target() {}\nfunction caller() {\n  target();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);

    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "target" (line 0, col 9)
    let pos = Position::new(0, 9);
    let calls = provider.incoming_calls(root, pos);

    assert!(!calls.is_empty(), "target should have incoming calls");
    let caller_item = calls.iter().find(|c| c.from.name == "caller");
    assert!(
        caller_item.is_some(),
        "Should find incoming call from 'caller'"
    );
}

#[test]
fn test_incoming_calls_include_decorator_references() {
    let source = "@bar\nclass Foo {\n}\n\nfunction bar() {\n  baz();\n}\n\nfunction baz() {\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "bar" declaration name.
    let pos = Position::new(4, 10);
    let calls = provider.incoming_calls(root, pos);

    assert!(
        calls.iter().any(|call| call.from.name == "Foo"),
        "Expected decorator-based incoming call from class 'Foo', got: {calls:?}"
    );
}

#[test]
fn test_incoming_calls_include_tagged_template_references() {
    let source = "function foo() {\n  bar`a${1}b`;\n}\n\nfunction bar(array: TemplateStringsArray, ...args: any[]) {\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "bar" declaration name.
    let pos = Position::new(4, 9);
    let calls = provider.incoming_calls(root, pos);

    assert!(
        calls.iter().any(|call| call.from.name == "foo"),
        "Expected tagged-template incoming call from 'foo', got: {calls:?}"
    );
}

#[test]
fn test_incoming_calls_inside_static_block_report_static_block_caller() {
    let source = "class C {\n  static {\n    function foo() {\n      bar();\n    }\n\n    function bar() {}\n    foo();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "foo" declaration name.
    let pos = Position::new(2, 13);
    let calls = provider.incoming_calls(root, pos);

    assert!(
        calls.iter().any(|call| {
            call.from.name == "static {}" && call.from.kind == SymbolKind::Constructor
        }),
        "Expected static block caller entry for foo(), got: {calls:?}"
    );
}

#[test]
fn test_incoming_calls_no_callers() {
    let source = "function unused() {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "unused" (line 0, col 9)
    let pos = Position::new(0, 9);
    let calls = provider.incoming_calls(root, pos);

    assert!(
        calls.is_empty(),
        "Uncalled function should have no incoming calls"
    );
}

#[test]
fn test_incoming_calls_disambiguates_same_name_symbols() {
    let source = "class A {\n  static sameName() {\n  }\n}\n\nclass B {\n  sameName() {\n    A.sameName();\n  }\n}\n\nconst Obj = {\n  get sameName() {\n    return new B().sameName;\n  }\n};\n\nnamespace Foo {\n  function sameName() {\n    return Obj.sameName;\n  }\n\n  export class C {\n    constructor() {\n      sameName();\n    }\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position on method B.sameName (line 6, col 2)
    let pos = Position::new(6, 2);
    let calls = provider.incoming_calls(root, pos);

    assert!(
        calls.iter().any(|call| call.from.name == "get sameName"),
        "Expected getter incoming reference for B.sameName, got: {calls:?}"
    );
    assert!(
        calls.iter().all(|call| call.from.name != "constructor"),
        "Did not expect unrelated constructor incoming reference for B.sameName, got: {calls:?}"
    );
}

#[test]
fn test_prepare_object_literal_getter_has_variable_container_name() {
    let source = "const Obj = {\n  get sameName() {\n    return 1;\n  }\n};\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 6);
    let item = provider
        .prepare(root, pos)
        .expect("Should prepare getter declaration");

    assert_eq!(item.name, "get sameName");
    assert_eq!(item.container_name.as_deref(), Some("Obj"));
}

#[test]
fn test_incoming_calls_for_object_literal_getter_track_property_access_callers() {
    let source = "class A {\n  static sameName() {\n  }\n}\n\nclass B {\n  sameName() {\n    A.sameName();\n  }\n}\n\nconst Obj = {\n  get sameName() {\n    return new B().sameName;\n  }\n};\n\nnamespace Foo {\n  function sameName() {\n    return Obj.sameName;\n  }\n\n  export class C {\n    constructor() {\n      sameName();\n    }\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position on getter Obj.sameName declaration (line 12, col 6).
    let pos = Position::new(12, 6);
    let calls = provider.incoming_calls(root, pos);

    assert!(
        calls.iter().any(|call| {
            call.from.name == "sameName" && call.from.container_name.as_deref() == Some("Foo")
        }),
        "Expected incoming reference from Foo.sameName to Obj.sameName getter, got: {calls:?}"
    );
}

#[test]
fn test_incoming_calls_for_function_inside_constructor_reports_class_caller() {
    let source = "namespace Foo {\n  function sameName() {\n  }\n\n  export class C {\n    constructor() {\n      sameName();\n    }\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let ctor_pos = Position::new(5, 6);
    let ctor_offset = line_map
        .position_to_offset(ctor_pos, source)
        .expect("constructor position must be valid");
    let ctor_node = crate::utils::find_node_at_offset(arena, ctor_offset);
    let ctor_func = provider
        .find_function_at_or_around(ctor_node)
        .expect("constructor function should resolve");
    assert_eq!(
        provider.get_function_symbol_kind(ctor_func),
        SymbolKind::Constructor
    );
    assert!(
        provider.class_parent_for_constructor(ctor_func).is_some(),
        "constructor should map to containing class for incoming call hierarchy"
    );

    // Position on function Foo.sameName declaration.
    let pos = Position::new(1, 11);
    let calls = provider.incoming_calls(root, pos);

    assert!(
        calls
            .iter()
            .any(|call| call.from.kind == SymbolKind::Class && call.from.name == "C"),
        "Expected class caller for constructor invocation, got: {calls:?}"
    );
    assert!(
        calls
            .iter()
            .all(|call| call.from.kind != SymbolKind::Constructor),
        "Did not expect constructor caller in incoming hierarchy, got: {calls:?}"
    );
}

#[test]
fn test_incoming_calls_do_not_cross_namespace_same_name_functions() {
    let source = "namespace Foo {\n  export function sameName() {\n  }\n\n  export class C {\n    constructor() {\n      sameName();\n    }\n  }\n}\n\nnamespace Foo.Bar {\n  export const sameName = () => new Foo.C();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position on Foo.Bar.sameName declaration.
    let pos = Position::new(12, 15);
    let calls = provider.incoming_calls(root, pos);

    assert!(
        calls.is_empty(),
        "Expected no incoming callers for Foo.Bar.sameName, got: {calls:?}"
    );
}

#[test]
fn test_prepare_method_selection_range_uses_identifier_length() {
    let source = "class A {\n  static sameName() {\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 9);
    let item = provider
        .prepare(root, pos)
        .expect("Should prepare static method call hierarchy item");

    assert_eq!(item.selection_range.start, Position::new(1, 9));
    assert_eq!(item.selection_range.end, Position::new(1, 17));
}

#[test]
fn test_prepare_on_call_expression_resolves_const_function_expression_declaration() {
    let source = "function foo() {\n    bar();\n}\n\nconst bar = function () {\n    baz();\n}\n\nfunction baz() {\n}\n\nbar()\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(11, 0);
    let item = provider
        .prepare(root, pos)
        .expect("Should resolve call expression callee to declaration");

    assert_eq!(item.name, "bar");
    assert_eq!(item.container_name, None);
    assert_eq!(item.selection_range.start, Position::new(4, 6));
    assert_eq!(item.selection_range.end, Position::new(4, 9));
}

#[test]
fn test_call_expression_on_const_function_expression_has_incoming_and_outgoing() {
    let source = "function foo() {\n    bar();\n}\n\nconst bar = function () {\n    baz();\n}\n\nfunction baz() {\n}\n\nbar()\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(11, 0);

    let incoming = provider.incoming_calls(root, pos);
    assert!(
        incoming.iter().any(|call| call.from.name == "foo"),
        "Expected incoming call from 'foo', got: {incoming:?}"
    );
    assert!(
        incoming
            .iter()
            .any(|call| call.from.kind == SymbolKind::File && call.from_ranges.len() == 1),
        "Expected script-level incoming call entry with one callsite, got: {incoming:?}"
    );

    let outgoing = provider.outgoing_calls(root, pos);
    assert!(
        outgoing.iter().any(|call| call.to.name == "baz"),
        "Expected outgoing call to 'baz', got: {outgoing:?}"
    );
}

#[test]
fn test_declaration_name_position_for_const_function_expression_has_incoming_and_outgoing() {
    let source = "function foo() {\n    bar();\n}\n\nconst bar = function () {\n    baz();\n}\n\nfunction baz() {\n}\n\nbar()\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let declaration_pos = Position::new(4, 6);
    let incoming = provider.incoming_calls(root, declaration_pos);
    assert!(
        incoming.iter().any(|call| call.from.name == "foo"),
        "Expected incoming call from 'foo' at declaration position, got: {incoming:?}"
    );

    let outgoing = provider.outgoing_calls(root, declaration_pos);
    assert!(
        outgoing.iter().any(|call| call.to.name == "baz"),
        "Expected outgoing call to 'baz' at declaration position, got: {outgoing:?}"
    );
}

#[test]
fn test_class_property_arrow_function_prepare_and_incoming_calls() {
    let source = "class C {\n    caller = () => {\n        this.callee();\n    };\n\n    callee = () => {\n    };\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let callee_pos = Position::new(5, 8);
    let item = provider
        .prepare(root, callee_pos)
        .expect("Should prepare class property arrow function");
    assert_eq!(item.name, "callee");
    assert_eq!(item.kind, SymbolKind::Function);

    let incoming = provider.incoming_calls(root, callee_pos);
    assert!(
        incoming.iter().any(|call| call.from.name == "caller"),
        "Expected incoming call from class property arrow function 'caller', got: {incoming:?}"
    );
}

#[test]
fn test_interface_method_signature_prepare_and_incoming_calls() {
    let source =
        "interface I {\n    foo(): void;\n}\n\nconst obj: I = { foo() {} };\n\nobj.foo();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let method_pos = Position::new(1, 4);
    let item = provider
        .prepare(root, method_pos)
        .expect("Should prepare call hierarchy item for interface method signature");
    assert_eq!(item.name, "foo");
    assert_eq!(item.kind, SymbolKind::Method);

    let incoming = provider.incoming_calls(root, method_pos);
    assert!(
        incoming
            .iter()
            .any(|call| call.from.kind == SymbolKind::File && !call.from_ranges.is_empty()),
        "Expected script-level incoming call for interface method signature, got: {incoming:?}"
    );

    let outgoing = provider.outgoing_calls(root, method_pos);
    assert!(
        outgoing.is_empty(),
        "Interface method signatures have no body and should not report outgoing calls"
    );
}

#[test]
fn test_call_hierarchy_item_serialization() {
    let item = CallHierarchyItem {
        name: "test".to_string(),
        kind: SymbolKind::Function,
        uri: "file:///test.ts".to_string(),
        range: Range::new(Position::new(0, 0), Position::new(1, 0)),
        selection_range: Range::new(Position::new(0, 9), Position::new(0, 13)),
        container_name: None,
    };

    let json = serde_json::to_string(&item).unwrap();
    assert!(json.contains("\"name\":\"test\""));
    // SymbolKind::Function serializes as "Function" (serde default for enums)
    assert!(
        json.contains("\"kind\":\"Function\"") || json.contains("\"kind\":12"),
        "kind should serialize correctly, got: {json}"
    );

    let deserialized: CallHierarchyItem = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, "test");
    assert_eq!(deserialized.kind, SymbolKind::Function);
}
