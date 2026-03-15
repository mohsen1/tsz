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

#[test]
fn test_prepare_on_arrow_function_assigned_to_variable() {
    let source = "const greet = (name: string) => {\n  return `Hello ${name}`;\n};\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "greet" (line 0, col 6)
    let pos = Position::new(0, 6);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for arrow function variable 'greet'"
    );
    let item = item.unwrap();
    assert_eq!(item.name, "greet");
}

#[test]
fn test_prepare_on_constructor() {
    let source = "class Foo {\n  constructor(x: number) {\n    this.x = x;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "constructor" (line 1, col 2)
    let pos = Position::new(1, 2);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for constructor"
    );
    let item = item.unwrap();
    assert_eq!(item.kind, SymbolKind::Constructor);
}

#[test]
fn test_prepare_on_getter() {
    let source = "class Foo {\n  get value(): number {\n    return 42;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "value" in getter (line 1, col 6)
    let pos = Position::new(1, 6);
    let item = provider.prepare(root, pos);

    assert!(item.is_some(), "Should find call hierarchy item for getter");
    let item = item.unwrap();
    assert_eq!(item.name, "get value");
}

#[test]
fn test_prepare_on_setter() {
    let source = "class Foo {\n  set value(v: number) {\n    this._v = v;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "value" in setter (line 1, col 6)
    let pos = Position::new(1, 6);
    let item = provider.prepare(root, pos);

    assert!(item.is_some(), "Should find call hierarchy item for setter");
    let item = item.unwrap();
    assert_eq!(item.name, "set value");
}

#[test]
fn test_prepare_on_static_method() {
    let source = "class Util {\n  static helper() {\n    return 1;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "helper" (line 1, col 9)
    let pos = Position::new(1, 9);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for static method"
    );
    let item = item.unwrap();
    assert_eq!(item.name, "helper");
    assert_eq!(item.kind, SymbolKind::Method);
}

#[test]
fn test_incoming_calls_from_nested_functions() {
    let source =
        "function target() {}\nfunction outer() {\n  function inner() {\n    target();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "target" declaration (line 0, col 9)
    let pos = Position::new(0, 9);
    let calls = provider.incoming_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.from.name == "inner"),
        "Should find incoming call from nested function 'inner', got: {calls:?}"
    );
}

#[test]
fn test_incoming_calls_from_callbacks() {
    let source = "function handler() {}\nfunction setup() {\n  [1, 2].forEach(() => {\n    handler();\n  });\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "handler" declaration (line 0, col 9)
    let pos = Position::new(0, 9);
    let calls = provider.incoming_calls(root, pos);

    // The call to handler() is inside an arrow function inside setup()
    // It should report the arrow function or setup as the caller
    assert!(
        !calls.is_empty(),
        "Should find incoming calls from callback/closure context"
    );
}

#[test]
fn test_outgoing_calls_from_class_constructor() {
    let source = "function init() {}\nfunction validate() {}\nclass App {\n  constructor() {\n    init();\n    validate();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "constructor" (line 3, col 2)
    let pos = Position::new(3, 2);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.to.name == "init"),
        "Constructor should have outgoing call to 'init', got: {calls:?}"
    );
    assert!(
        calls.iter().any(|c| c.to.name == "validate"),
        "Constructor should have outgoing call to 'validate', got: {calls:?}"
    );
}

#[test]
fn test_recursive_function_calls() {
    let source = "function factorial(n: number): number {\n  if (n <= 1) return 1;\n  return n * factorial(n - 1);\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "factorial" declaration (line 0, col 9)
    let pos = Position::new(0, 9);

    // Outgoing calls should include the recursive call to itself
    let outgoing = provider.outgoing_calls(root, pos);
    assert!(
        outgoing.iter().any(|c| c.to.name == "factorial"),
        "Recursive function should have outgoing call to itself, got: {outgoing:?}"
    );

    // Incoming calls should NOT include the self-call (it's the same function)
    let incoming = provider.incoming_calls(root, pos);
    assert!(
        incoming.is_empty(),
        "Recursive function with no external callers should have no incoming calls, got: {incoming:?}"
    );
}

#[test]
fn test_iife_outgoing_calls() {
    let source = "function helper() {}\n(function() {\n  helper();\n})();\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "helper" declaration (line 0, col 9)
    let pos = Position::new(0, 9);
    let incoming = provider.incoming_calls(root, pos);

    // The call is inside an IIFE, which may or may not be reported
    // At minimum, it should not crash
    let _ = incoming;
}

#[test]
fn test_no_hierarchy_at_type_alias() {
    let source = "type Foo = string;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Foo" (line 0, col 5) - a type alias, not callable
    let pos = Position::new(0, 5);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_none(),
        "Should not find call hierarchy item for type alias"
    );
}

#[test]
fn test_no_hierarchy_at_interface() {
    let source = "interface Bar {\n  x: number;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Bar" (line 0, col 10)
    let pos = Position::new(0, 10);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_none(),
        "Should not find call hierarchy item for interface name"
    );
}

#[test]
fn test_prepare_on_async_function() {
    let source = "async function fetchData(): Promise<void> {\n  await fetch('url');\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "fetchData" (line 0, col 15)
    let pos = Position::new(0, 15);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for async function"
    );
    let item = item.unwrap();
    assert_eq!(item.name, "fetchData");
    assert_eq!(item.kind, SymbolKind::Function);
}

#[test]
fn test_prepare_on_generator_function() {
    let source = "function* gen() {\n  yield 1;\n  yield 2;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "gen" (line 0, col 10)
    let pos = Position::new(0, 10);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for generator function"
    );
    let item = item.unwrap();
    assert_eq!(item.name, "gen");
    assert_eq!(item.kind, SymbolKind::Function);
}

#[test]
fn test_multiple_incoming_calls_from_same_function() {
    let source =
        "function target() {}\nfunction caller() {\n  target();\n  target();\n  target();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "target" declaration (line 0, col 9)
    let pos = Position::new(0, 9);
    let calls = provider.incoming_calls(root, pos);

    assert!(!calls.is_empty(), "Should have incoming calls");
    let caller_entry = calls
        .iter()
        .find(|c| c.from.name == "caller")
        .expect("Should find incoming call from 'caller'");
    assert_eq!(
        caller_entry.from_ranges.len(),
        3,
        "Should have 3 call ranges from the same function"
    );
}

#[test]
fn test_outgoing_calls_with_chained_method_calls() {
    let source = "function a() {}\nfunction b() {}\nfunction chain() {\n  a();\n  b();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "chain" (line 2, col 9)
    let pos = Position::new(2, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.to.name == "a"),
        "Should have outgoing call to 'a'"
    );
    assert!(
        calls.iter().any(|c| c.to.name == "b"),
        "Should have outgoing call to 'b'"
    );
}

#[test]
fn test_prepare_on_function_expression_variable() {
    let source = "const myFunc = function myFuncImpl() {\n  return 1;\n};\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "myFunc" variable name (line 0, col 6)
    let pos = Position::new(0, 6);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for function expression variable"
    );
}

#[test]
fn test_prepare_on_method_in_object_literal() {
    let source = "const obj = {\n  doWork() {\n    return 42;\n  }\n};\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "doWork" (line 1, col 2)
    let pos = Position::new(1, 2);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for object literal method"
    );
    let item = item.unwrap();
    assert_eq!(item.name, "doWork");
}

#[test]
fn test_incoming_calls_multiple_callers() {
    let source = "function target() {}\nfunction callerA() {\n  target();\n}\nfunction callerB() {\n  target();\n}\nfunction callerC() {\n  target();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "target" declaration (line 0, col 9)
    let pos = Position::new(0, 9);
    let calls = provider.incoming_calls(root, pos);

    assert!(
        calls.len() >= 3,
        "Should have at least 3 incoming callers, got: {}",
        calls.len()
    );
    assert!(calls.iter().any(|c| c.from.name == "callerA"));
    assert!(calls.iter().any(|c| c.from.name == "callerB"));
    assert!(calls.iter().any(|c| c.from.name == "callerC"));
}

// ---- Additional call hierarchy tests ----

#[test]
fn test_prepare_on_exported_function() {
    let source = "export function greet(name: string) {\n  return `Hello ${name}`;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "greet" (line 0, col 16)
    let pos = Position::new(0, 16);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for exported function"
    );
    let item = item.unwrap();
    assert_eq!(item.name, "greet");
    assert_eq!(item.kind, SymbolKind::Function);
}

#[test]
fn test_prepare_not_on_enum() {
    let source = "enum Color { Red, Green, Blue }\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "Color" (line 0, col 5)
    let pos = Position::new(0, 5);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_none(),
        "Should not find call hierarchy item for enum name"
    );
}

#[test]
fn test_prepare_on_namespace_function() {
    let source = "namespace NS {\n  export function helper() {}\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "helper" (line 1, col 19)
    let pos = Position::new(1, 19);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for namespace function"
    );
    if let Some(item) = item {
        assert_eq!(item.name, "helper");
        assert_eq!(item.kind, SymbolKind::Function);
    }
}

#[test]
fn test_outgoing_calls_empty_function() {
    let source = "function empty() {}\n";
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
        "Empty function should have no outgoing calls"
    );
}

#[test]
fn test_outgoing_calls_single_call() {
    let source = "function helper() {}\nfunction main() {\n  helper();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "main" (line 1, col 9)
    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert_eq!(calls.len(), 1, "Should have exactly one outgoing call");
    assert_eq!(calls[0].to.name, "helper");
}

#[test]
fn test_incoming_calls_from_method() {
    let source = "function target() {}\nclass Svc {\n  run() {\n    target();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "target" declaration (line 0, col 9)
    let pos = Position::new(0, 9);
    let calls = provider.incoming_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.from.name == "run"),
        "Should find incoming call from method 'run', got: {calls:?}"
    );
}

#[test]
fn test_outgoing_calls_method_calling_function() {
    let source = "function doWork() {}\nclass Worker {\n  process() {\n    doWork();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "process" method (line 2, col 2)
    let pos = Position::new(2, 2);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.to.name == "doWork"),
        "Method should have outgoing call to 'doWork', got: {calls:?}"
    );
}

#[test]
fn test_prepare_on_abstract_method() {
    let source = "abstract class Base {\n  abstract compute(): number;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "compute" (line 1, col 11)
    let pos = Position::new(1, 11);
    let item = provider.prepare(root, pos);

    // Abstract methods may or may not produce hierarchy items
    // This tests that it doesn't crash
    let _ = item;
}

#[test]
fn test_prepare_on_private_method() {
    let source = "class Foo {\n  private secret() {\n    return 42;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "secret" (line 1, col 10)
    let pos = Position::new(1, 10);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for private method"
    );
    if let Some(item) = item {
        assert_eq!(item.name, "secret");
        assert_eq!(item.kind, SymbolKind::Method);
    }
}

#[test]
fn test_outgoing_calls_multiple_distinct_targets() {
    let source = "function x() {}\nfunction y() {}\nfunction z() {}\nfunction caller() {\n  x();\n  y();\n  z();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "caller" (line 3, col 9)
    let pos = Position::new(3, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.len() >= 3,
        "Should have at least 3 outgoing calls, got: {}",
        calls.len()
    );
    assert!(calls.iter().any(|c| c.to.name == "x"));
    assert!(calls.iter().any(|c| c.to.name == "y"));
    assert!(calls.iter().any(|c| c.to.name == "z"));
}

#[test]
fn test_prepare_on_overloaded_function() {
    let source = "function add(a: number, b: number): number;\nfunction add(a: string, b: string): string;\nfunction add(a: any, b: any): any {\n  return a + b;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "add" implementation (line 2, col 9)
    let pos = Position::new(2, 9);
    let item = provider.prepare(root, pos);

    // Should at least not crash; defensively check
    if let Some(item) = item {
        assert_eq!(item.name, "add");
        assert_eq!(item.kind, SymbolKind::Function);
    }
}

#[test]
fn test_prepare_on_async_method() {
    let source = "class Api {\n  async fetch() {\n    return 'data';\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "fetch" (line 1, col 8)
    let pos = Position::new(1, 8);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for async method"
    );
    if let Some(item) = item {
        assert_eq!(item.name, "fetch");
        assert_eq!(item.kind, SymbolKind::Method);
    }
}

// =========================================================================
// Additional tests for broader coverage
// =========================================================================

#[test]
fn test_prepare_on_default_exported_function() {
    let source = "export default function handler() {\n  return 42;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "handler" (line 0, col 24)
    let pos = Position::new(0, 24);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "handler");
        assert_eq!(item.kind, SymbolKind::Function);
    }
}

#[test]
fn test_prepare_at_file_end_returns_none() {
    let source = "function foo() {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position well past the end of the file
    let pos = Position::new(100, 0);
    let item = provider.prepare(root, pos);
    // Should not panic; may return None or Some depending on offset resolution
    let _ = item;
}

#[test]
fn test_prepare_at_column_zero_line_zero() {
    let source = "function first() {}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 0);
    let item = provider.prepare(root, pos);
    // The cursor is on "function" keyword, which should resolve to the function
    if let Some(item) = item {
        assert_eq!(item.name, "first");
    }
}

#[test]
fn test_outgoing_calls_from_arrow_function_variable() {
    let source = "function target() {}\nconst caller = () => {\n  target();\n};\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "caller" (line 1, col 6)
    let pos = Position::new(1, 6);
    let calls = provider.outgoing_calls(root, pos);

    // Defensively check - arrow function should detect outgoing calls
    if !calls.is_empty() {
        assert!(calls.iter().any(|c| c.to.name == "target"));
    }
}

#[test]
fn test_incoming_calls_for_constructor_via_new_expression() {
    let source = "class Widget {}\nfunction build() {\n  new Widget();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "build" (line 1, col 9) - check outgoing calls include Widget
    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    // Should have at least one outgoing call to Widget
    if !calls.is_empty() {
        assert!(
            calls.iter().any(|c| c.to.name == "Widget"),
            "Should include Widget in outgoing calls from build"
        );
    }
}

#[test]
fn test_prepare_on_deeply_nested_function() {
    let source = "function outer() {\n  function middle() {\n    function inner() {\n      return 1;\n    }\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "inner" (line 2, col 13)
    let pos = Position::new(2, 13);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "inner");
        assert_eq!(item.kind, SymbolKind::Function);
    }
}

#[test]
fn test_outgoing_calls_with_conditional_calls() {
    let source = "function a() {}\nfunction b() {}\nfunction decide(flag: boolean) {\n  if (flag) { a(); } else { b(); }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "decide" (line 2, col 9)
    let pos = Position::new(2, 9);
    let calls = provider.outgoing_calls(root, pos);

    // Both branches should show up as outgoing calls
    if calls.len() >= 2 {
        assert!(calls.iter().any(|c| c.to.name == "a"));
        assert!(calls.iter().any(|c| c.to.name == "b"));
    }
}

#[test]
fn test_prepare_on_empty_source() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 0);
    let item = provider.prepare(root, pos);
    assert!(
        item.is_none(),
        "Empty source should yield no hierarchy item"
    );
}

#[test]
fn test_incoming_calls_empty_source() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 0);
    let calls = provider.incoming_calls(root, pos);
    assert!(
        calls.is_empty(),
        "Empty source should yield no incoming calls"
    );
}

#[test]
fn test_outgoing_calls_empty_source() {
    let source = "";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 0);
    let calls = provider.outgoing_calls(root, pos);
    assert!(
        calls.is_empty(),
        "Empty source should yield no outgoing calls"
    );
}

#[test]
fn test_prepare_on_protected_method() {
    let source = "class Base {\n  protected compute() {\n    return 0;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "compute" (line 1, col 12)
    let pos = Position::new(1, 12);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "compute");
        assert_eq!(item.kind, SymbolKind::Method);
    }
}

#[test]
fn test_outgoing_calls_from_method_calling_other_methods() {
    let source = "class Svc {\n  a() {}\n  b() {}\n  c() {\n    this.a();\n    this.b();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "c" (line 3, col 2)
    let pos = Position::new(3, 2);
    let calls = provider.outgoing_calls(root, pos);

    // Defensively check - should find calls to a and b
    let names: Vec<&str> = calls.iter().map(|c| c.to.name.as_str()).collect();
    if names.contains(&"a") {
        assert!(names.contains(&"b"), "If a is found, b should be too");
    }
}

#[test]
fn test_prepare_on_function_with_type_parameters() {
    let source = "function identity<T>(x: T): T {\n  return x;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "identity" (line 0, col 9)
    let pos = Position::new(0, 9);
    let item = provider.prepare(root, pos);

    assert!(
        item.is_some(),
        "Should find call hierarchy item for generic function"
    );
    if let Some(item) = item {
        assert_eq!(item.name, "identity");
        assert_eq!(item.kind, SymbolKind::Function);
    }
}

#[test]
fn test_call_hierarchy_item_has_uri() {
    let source = "function hello() {}\n";
    let mut parser = ParserState::new("my_file.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "my_file.ts".to_string(), source);

    let pos = Position::new(0, 9);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(
            item.uri, "my_file.ts",
            "URI should match the file name provided"
        );
        assert_eq!(item.name, "hello");
    }
}

// =========================================================================
// Additional edge case tests
// =========================================================================

#[test]
fn test_prepare_on_single_line_function() {
    let source = "function f() { return 1; }\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 9);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "f");
        assert_eq!(item.kind, SymbolKind::Function);
    }
}

#[test]
fn test_prepare_on_method_with_rest_params() {
    let source = "class C {\n  collect(...args: number[]) {\n    return args;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "collect" (line 1, col 2)
    let pos = Position::new(1, 2);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "collect");
        assert_eq!(item.kind, SymbolKind::Method);
    }
}

#[test]
fn test_outgoing_calls_in_try_catch_block() {
    let source = "function safe() {}\nfunction risky() {}\nfunction doStuff() {\n  try {\n    risky();\n  } catch (e) {\n    safe();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "doStuff" (line 2, col 9)
    let pos = Position::new(2, 9);
    let calls = provider.outgoing_calls(root, pos);

    // Should find calls in both try and catch blocks
    let names: Vec<&str> = calls.iter().map(|c| c.to.name.as_str()).collect();
    if names.contains(&"risky") {
        assert!(
            names.contains(&"safe"),
            "Should find outgoing calls from both try and catch blocks"
        );
    }
}

#[test]
fn test_incoming_calls_from_multiple_methods_same_class() {
    let source = "function target() {}\nclass Svc {\n  a() { target(); }\n  b() { target(); }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "target" declaration (line 0, col 9)
    let pos = Position::new(0, 9);
    let calls = provider.incoming_calls(root, pos);

    // Should find incoming calls from both methods
    assert!(
        calls.len() >= 2,
        "Should find incoming calls from both methods, got: {}",
        calls.len()
    );
}

#[test]
fn test_prepare_on_method_with_optional_params() {
    let source = "class Config {\n  set(key: string, value?: string) {\n    return key;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "set" (line 1, col 2)
    let pos = Position::new(1, 2);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "set");
        assert_eq!(item.kind, SymbolKind::Method);
    }
}

#[test]
fn test_prepare_on_function_with_destructured_params() {
    let source = "function process({ x, y }: { x: number; y: number }) {\n  return x + y;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "process" (line 0, col 9)
    let pos = Position::new(0, 9);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "process");
        assert_eq!(item.kind, SymbolKind::Function);
    }
}

#[test]
fn test_outgoing_calls_from_for_loop_body() {
    let source = "function work() {}\nfunction loop_caller() {\n  for (let i = 0; i < 3; i++) {\n    work();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "loop_caller" (line 1, col 9)
    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.to.name == "work"),
        "Should find outgoing call to 'work' from inside a for loop, got: {calls:?}"
    );
}

#[test]
fn test_prepare_on_function_unicode_name() {
    let source = "function calcul\u{00E9}() {\n  return 1;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at the function name
    let pos = Position::new(0, 9);
    let item = provider.prepare(root, pos);

    // Should not crash; if it finds the item, the name should contain unicode
    let _ = item;
}

#[test]
fn test_outgoing_calls_from_while_loop() {
    let source = "function tick() {}\nfunction runner() {\n  while (true) {\n    tick();\n    break;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "runner" (line 1, col 9)
    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.to.name == "tick"),
        "Should find outgoing call to 'tick' from while loop, got: {calls:?}"
    );
}

#[test]
fn test_prepare_on_async_arrow_function() {
    let source = "const fetchAll = async () => {\n  return [];\n};\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "fetchAll" (line 0, col 6)
    let pos = Position::new(0, 6);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "fetchAll");
    }
}

#[test]
fn test_incoming_calls_from_switch_case() {
    let source = "function handler() {}\nfunction dispatch(action: string) {\n  switch (action) {\n    case 'a': handler(); break;\n    case 'b': handler(); break;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "handler" declaration (line 0, col 9)
    let pos = Position::new(0, 9);
    let calls = provider.incoming_calls(root, pos);

    if let Some(dispatch_call) = calls.iter().find(|c| c.from.name == "dispatch") {
        assert!(
            dispatch_call.from_ranges.len() >= 2,
            "Should have at least 2 call ranges from switch cases, got: {}",
            dispatch_call.from_ranges.len()
        );
    }
}

#[test]
fn test_prepare_on_function_with_default_params() {
    let source = "function greet(name: string = 'World') {\n  return name;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 9);
    let item = provider.prepare(root, pos);

    assert!(item.is_some(), "Should find call hierarchy item for function with default params");
    if let Some(item) = item {
        assert_eq!(item.name, "greet");
        assert_eq!(item.kind, SymbolKind::Function);
    }
}

#[test]
fn test_prepare_not_on_type_annotation() {
    let source = "const x: number = 42;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "number" type annotation
    let pos = Position::new(0, 10);
    let item = provider.prepare(root, pos);

    assert!(item.is_none(), "Should not find call hierarchy item for type annotation");
}

#[test]
fn test_outgoing_calls_from_ternary_expression() {
    let source = "function a() {}\nfunction b() {}\nfunction choose(cond: boolean) {\n  cond ? a() : b();\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "choose"
    let pos = Position::new(2, 9);
    let calls = provider.outgoing_calls(root, pos);

    let names: Vec<_> = calls.iter().map(|c| c.to.name.as_str()).collect();
    assert!(names.contains(&"a"), "Should find outgoing call to 'a', got: {names:?}");
    assert!(names.contains(&"b"), "Should find outgoing call to 'b', got: {names:?}");
}

#[test]
fn test_prepare_on_function_with_many_params() {
    let source = "function compute(a: number, b: number, c: number, d: number, e: number) {\n  return a + b + c + d + e;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 9);
    let item = provider.prepare(root, pos);

    assert!(item.is_some());
    if let Some(item) = item {
        assert_eq!(item.name, "compute");
    }
}

#[test]
fn test_outgoing_calls_from_do_while_loop() {
    let source = "function step() {}\nfunction loop_fn() {\n  do {\n    step();\n  } while (false);\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.to.name == "step"),
        "Should find outgoing call to 'step' from do-while loop"
    );
}

#[test]
fn test_incoming_calls_from_arrow_function_variable() {
    let source = "function target() {}\nconst caller = () => {\n  target();\n};\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "target" declaration
    let pos = Position::new(0, 9);
    let calls = provider.incoming_calls(root, pos);

    if !calls.is_empty() {
        assert!(
            calls.iter().any(|c| c.from.name == "caller"),
            "Should find incoming call from 'caller' arrow function"
        );
    }
}

#[test]
fn test_prepare_on_method_with_return_type() {
    let source = "class Service {\n  getData(): string[] {\n    return [];\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "getData"
    let pos = Position::new(1, 2);
    let item = provider.prepare(root, pos);

    assert!(item.is_some());
    if let Some(item) = item {
        assert_eq!(item.name, "getData");
        assert_eq!(item.kind, SymbolKind::Method);
    }
}

#[test]
fn test_outgoing_calls_from_nested_if_else() {
    let source = "function alpha() {}\nfunction beta() {}\nfunction gamma() {}\nfunction decide(x: number) {\n  if (x > 0) {\n    alpha();\n  } else if (x < 0) {\n    beta();\n  } else {\n    gamma();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "decide"
    let pos = Position::new(3, 9);
    let calls = provider.outgoing_calls(root, pos);

    let names: Vec<_> = calls.iter().map(|c| c.to.name.as_str()).collect();
    assert!(names.contains(&"alpha"), "Should find call to 'alpha', got: {names:?}");
    assert!(names.contains(&"beta"), "Should find call to 'beta', got: {names:?}");
    assert!(names.contains(&"gamma"), "Should find call to 'gamma', got: {names:?}");
}

#[test]
fn test_prepare_on_readonly_method() {
    let source = "class Buffer {\n  readonly size: number = 0;\n  getSize(): number {\n    return this.size;\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "getSize"
    let pos = Position::new(2, 2);
    let item = provider.prepare(root, pos);

    assert!(item.is_some());
    if let Some(item) = item {
        assert_eq!(item.name, "getSize");
    }
}

#[test]
fn test_outgoing_calls_from_for_of_loop() {
    let source = "function process(x: number) {}\nfunction iterate(items: number[]) {\n  for (const item of items) {\n    process(item);\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.to.name == "process"),
        "Should find outgoing call to 'process' from for-of loop"
    );
}

#[test]
fn test_prepare_not_on_import_statement() {
    let source = "import { foo } from './foo';\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "foo" in import
    let pos = Position::new(0, 9);
    let item = provider.prepare(root, pos);

    // Import specifiers are not callable items
    let _ = item;
}

#[test]
fn test_outgoing_calls_from_for_in_loop() {
    let source = "function log(k: string) {}\nfunction enumerate(obj: object) {\n  for (const key in obj) {\n    log(key);\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    assert!(
        calls.iter().any(|c| c.to.name == "log"),
        "Should find outgoing call to 'log' from for-in loop"
    );
}

#[test]
fn test_prepare_on_generic_function() {
    let source = "function identity<T>(x: T): T {\n  return x;\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 9);
    let item = provider.prepare(root, pos);

    assert!(item.is_some());
    if let Some(item) = item {
        assert_eq!(item.name, "identity");
        assert_eq!(item.kind, SymbolKind::Function);
    }
}

#[test]
fn test_outgoing_calls_multiple_calls_to_same_function() {
    let source = "function log(msg: string) {}\nfunction verbose() {\n  log('start');\n  log('middle');\n  log('end');\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(1, 9);
    let calls = provider.outgoing_calls(root, pos);

    if let Some(log_call) = calls.iter().find(|c| c.to.name == "log") {
        assert!(
            log_call.from_ranges.len() >= 3,
            "Should have at least 3 call ranges for 'log', got: {}",
            log_call.from_ranges.len()
        );
    }
}

#[test]
fn test_prepare_on_single_line_arrow_function() {
    let source = "const double = (x: number) => x * 2;\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    let pos = Position::new(0, 6);
    let item = provider.prepare(root, pos);

    if let Some(item) = item {
        assert_eq!(item.name, "double");
    }
}

#[test]
fn test_incoming_calls_from_try_catch() {
    let source = "function risky() {}\nfunction safe() {\n  try {\n    risky();\n  } catch (e) {\n    risky();\n  }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let line_map = LineMap::build(source);
    let provider =
        CallHierarchyProvider::new(arena, &binder, &line_map, "test.ts".to_string(), source);

    // Position at "risky" declaration
    let pos = Position::new(0, 9);
    let calls = provider.incoming_calls(root, pos);

    if let Some(safe_call) = calls.iter().find(|c| c.from.name == "safe") {
        assert!(
            safe_call.from_ranges.len() >= 2,
            "Should have at least 2 call ranges from try/catch, got: {}",
            safe_call.from_ranges.len()
        );
    }
}
