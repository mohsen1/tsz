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

